use serde::{Deserialize, Serialize};
// unused import
use std::process::Command;

/// A git checkpoint — saves state before agent modifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub task_id: String,
    pub commit_hash: String,
    pub branch: String,
    pub timestamp: String,
    pub description: String,
    pub files_snapshot: Vec<String>,
}

impl Checkpoint {
    pub fn new(task_id: &str, description: &str) -> Self {
        Self {
            id: format!("ck_{}", chrono::Utc::now().timestamp()),
            task_id: task_id.to_string(),
            commit_hash: String::new(),
            branch: String::new(),
            timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(),
            description: description.to_string(),
            files_snapshot: Vec::new(),
        }
    }
}

/// Manages git checkpoints for safe agent operations
pub struct CheckpointManager {
    pub project_dir: std::path::PathBuf,
}

impl CheckpointManager {
    pub fn new(project_dir: std::path::PathBuf) -> Self {
        Self { project_dir }
    }

    /// Check if the project is a git repository
    pub fn is_git_repo(&self) -> bool {
        let output = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&self.project_dir)
            .output();
        matches!(output, Ok(out) if out.status.success())
    }

    /// Get current branch name
    pub fn current_branch(&self) -> Result<String, String> {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git failed: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err("Not a git repository".to_string())
        }
    }

    /// Get HEAD commit hash
    pub fn head_hash(&self) -> Result<String, String> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git failed: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err("Not a git repository".to_string())
        }
    }

    /// Create a checkpoint (stash changes before agent modifies files)
    /// Returns the checkpoint and whether any changes were stashed
    pub fn create_checkpoint(&self, task_id: &str, description: &str) -> Result<Checkpoint, String> {
        if !self.is_git_repo() {
            return Err("Not a git repository".to_string());
        }

        let mut checkpoint = Checkpoint::new(task_id, description);
        checkpoint.branch = self.current_branch().unwrap_or_default();

        // Check for uncommitted changes
        let status_output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git status failed: {}", e))?;

        let status = String::from_utf8_lossy(&status_output.stdout).to_string();
        let has_changes = !status.trim().is_empty();

        if has_changes {
            // Get list of changed files
            for line in status.lines() {
                if line.len() > 3 {
                    let file = line[3..].trim().to_string();
                    checkpoint.files_snapshot.push(file);
                }
            }

            // Create a temporary commit or stash to serve as checkpoint
            // We use git stash create which creates a commit without applying it
            let stash_output = Command::new("git")
                .args(["stash", "create"])
                .current_dir(&self.project_dir)
                .output()
                .map_err(|e| format!("git stash create failed: {}", e))?;

            let stash_hash = String::from_utf8_lossy(&stash_output.stdout).trim().to_string();

            if !stash_hash.is_empty() {
                // We created a stash commit; apply it back to keep working
                checkpoint.commit_hash = stash_hash;
                let _ = Command::new("git")
                    .args(["stash", "apply"])
                    .current_dir(&self.project_dir)
                    .output();
            } else {
                // No changes to stash; record HEAD as checkpoint
                checkpoint.commit_hash = self.head_hash().unwrap_or_default();
            }
        } else {
            // No changes; just record HEAD
            checkpoint.commit_hash = self.head_hash().unwrap_or_default();
        }

        Ok(checkpoint)
    }

    /// Rollback to a checkpoint
    pub fn rollback(&self, checkpoint: &Checkpoint) -> Result<String, String> {
        if !self.is_git_repo() {
            return Err("Not a git repository".to_string());
        }

        // Restore files from the stash commit
        if !checkpoint.commit_hash.is_empty() {
            let output = Command::new("git")
                .args(["checkout", &checkpoint.commit_hash, "--", "."])
                .current_dir(&self.project_dir)
                .output()
                .map_err(|e| format!("git checkout failed: {}", e))?;

            if output.status.success() {
                Ok(format!(
                    "Rolled back to checkpoint {} ({} files restored)",
                    &checkpoint.commit_hash[..std::cmp::min(8, checkpoint.commit_hash.len())],
                    checkpoint.files_snapshot.len()
                ))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Rollback failed: {}", stderr))
            }
        } else {
            Err("Checkpoint has no commit hash".to_string())
        }
    }

    /// Verify the workspace is in a clean state (no uncommitted changes)
    pub fn is_clean(&self) -> bool {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.project_dir)
            .output();
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).trim().is_empty(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_checkpoint_new() {
        let ck = Checkpoint::new("task_123", "test checkpoint");
        assert!(ck.id.starts_with("ck_"));
        assert_eq!(ck.task_id, "task_123");
        assert_eq!(ck.description, "test checkpoint");
        assert!(ck.commit_hash.is_empty());
    }

    #[test]
    fn test_checkpoint_is_git_repo() {
        // The project itself should be a git repo
        let mgr = CheckpointManager::new(PathBuf::from("."));
        // This will be true when running in the lingshu-cil repo
        let result = mgr.is_git_repo();
        // We can't assert true/false since it depends on test context
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_checkpoint_create_in_git_repo() {
        let mgr = CheckpointManager::new(PathBuf::from("."));
        if mgr.is_git_repo() {
            let result = mgr.create_checkpoint("test_task", "test");
            // May succeed or fail depending on git state
            // Just verify it doesn't panic
            let _ = result;
        }
    }
}
