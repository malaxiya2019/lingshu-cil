use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// A single file change in a patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: ChangeType,
    pub lines_added: usize,
    pub lines_removed: usize,
    /// The unified diff content (git diff format)
    pub diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    Create,
    Modify,
    Delete,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Create => write!(f, "CREATE"),
            ChangeType::Modify => write!(f, "MODIFY"),
            ChangeType::Delete => write!(f, "DELETE"),
        }
    }
}

/// A complete patch set (multiple file changes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchSet {
    pub id: String,
    pub description: String,
    pub changes: Vec<FileChange>,
    pub created_at: String,
    pub applied: bool,
    pub rolled_back: bool,
}

impl PatchSet {
    pub fn new(description: &str) -> Self {
        Self {
            id: format!("patch_{}", chrono::Utc::now().timestamp()),
            description: description.to_string(),
            changes: Vec::new(),
            created_at: chrono::Utc::now().format("%H:%M:%S").to_string(),
            applied: false,
            rolled_back: false,
        }
    }

    pub fn summary(&self) -> String {
        if self.changes.is_empty() {
            return "No changes.".to_string();
        }
        let mut result = format!("Patch #{}: {}\n", self.id, self.description);
        result.push_str(&format!("  Status: {}\n", if self.applied { "Applied" } else { "Pending" }));
        if self.rolled_back { result.push_str("  Rolled back\n"); }
        result.push_str("  Files:\n");
        for change in &self.changes {
            result.push_str(&format!(
                "    {} {} (+{} -{})\n",
                change.change_type, change.path, change.lines_added, change.lines_removed
            ));
        }
        result
    }
}

/// The patch engine — generates, reviews, applies and rolls back patches
pub struct PatchEngine {
    pub project_dir: PathBuf,
}

#[allow(dead_code)]
impl PatchEngine {
    pub fn new(project_dir: PathBuf) -> Self {
        Self { project_dir }
    }

    /// Generate a unified diff patch between original content and new content
    pub fn generate_patch(
        &self,
        file_path: &str,
        original_content: &str,
        new_content: &str,
    ) -> Result<String, String> {
        let _full_path = self.project_dir.join(file_path);
        let relative_path = file_path;

        // Use temp files for diff generation
        let tmp_old = format!("/tmp/_lingshu_old_{}", std::process::id());
        let tmp_new = format!("/tmp/_lingshu_new_{}", std::process::id());

        std::fs::write(&tmp_old, original_content).map_err(|e| e.to_string())?;
        std::fs::write(&tmp_new, new_content).map_err(|e| e.to_string())?;

        let output = Command::new("diff")
            .args(["-u", "--label", &format!("a/{}", relative_path), "--label", &format!("b/{}", relative_path), &tmp_old, &tmp_new])
            .output()
            .map_err(|e| format!("diff command failed: {}", e))?;

        let _ = std::fs::remove_file(&tmp_old);
        let _ = std::fs::remove_file(&tmp_new);

        // diff returns exit code 0 for identical files, 1 for different, 2 for error
        if output.status.success() {
            return Ok(String::new()); // No changes
        }

        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if diff.is_empty() {
            return Err("Empty diff generated".to_string());
        }

        Ok(diff)
    }

    /// Generate patch from current workspace state using git diff
    pub fn generate_workspace_patch(&self) -> Result<PatchSet, String> {
        let output = Command::new("git")
            .args(["diff", "--no-color"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git diff failed: {}", e))?;

        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if diff.is_empty() {
            return Err("No uncommitted changes".to_string());
        }

        let mut patch = PatchSet::new("workspace changes");
        let changes = self.parse_diff_stats()?;
        patch.changes = changes;
        patch.applied = true; // Already applied in workspace

        Ok(patch)
    }

    /// Parse a unified diff output into FileChange entries
    pub fn parse_diff(&self, diff: &str) -> Vec<FileChange> {
        let mut changes = Vec::new();
        let mut current_file: Option<String> = None;
        let mut current_diff = String::new();
        let mut added = 0;
        let mut removed = 0;

        for line in diff.lines() {
            if line.starts_with("--- a/") || line.starts_with("--- "){
                // Previous file's diff done
                if let Some(path) = current_file.take() {
                    changes.push(FileChange {
                        path,
                        change_type: ChangeType::Modify,
                        lines_added: added,
                        lines_removed: removed,
                        diff: current_diff.clone(),
                    });
                    current_diff.clear();
                    added = 0;
                    removed = 0;
                }
                current_diff.push_str(line);
                current_diff.push('\n');
            } else if let Some(stripped) = line.strip_prefix("+++ b/") {
                current_file = Some(stripped.to_string());
                current_diff.push_str(line);
                current_diff.push('\n');
            } else if line.starts_with("--- /dev/null") {
                // New file
                current_diff.push_str(line);
                current_diff.push('\n');
            } else if line.starts_with("+++ /dev/null") {
                // Deleted file
                current_diff.push_str(line);
                current_diff.push('\n');
            } else if line.starts_with("@@") {
                current_diff.push_str(line);
                current_diff.push('\n');
                // Parse @@ -a,b +c,d @@ to get line counts
                if let Some(counts) = Self::parse_hunk_header(line) {
                    added += counts.0;
                    removed += counts.1;
                }
            } else if line.starts_with('+') && !line.starts_with("+++") {
                current_diff.push_str(line);
                current_diff.push('\n');
                added += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                current_diff.push_str(line);
                current_diff.push('\n');
                removed += 1;
            } else {
                current_diff.push_str(line);
                current_diff.push('\n');
            }
        }

        // Don't forget the last file
        if let Some(path) = current_file {
            changes.push(FileChange {
                path,
                change_type: ChangeType::Modify,
                lines_added: added,
                lines_removed: removed,
                diff: current_diff,
            });
        }

        changes
    }

    fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
        // @@ -a,b +c,d @@
        if let Some(rest) = line.strip_prefix("@@") {
            if let Some(segments) = rest.split("@@").next() {
                let parts: Vec<&str> = segments.split_whitespace().collect();
                if parts.len() >= 2 {
                    let new_part = parts[1]; // +c,d
                    let inner = new_part.trim_start_matches('+');
                    let counts: Vec<&str> = inner.split(',').collect();
                    let added = counts.first().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                    let removed = counts.get(1).and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(1);
                    return Some((added, removed));
                }
            }
        }
        None
    }

    /// Parse git diff --stat output
    fn parse_diff_stats(&self) -> Result<Vec<FileChange>, String> {
        let output = Command::new("git")
            .args(["diff", "--no-color", "--stat"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git diff --stat failed: {}", e))?;

        let stats = String::from_utf8_lossy(&output.stdout).to_string();
        let mut changes = Vec::new();

        for line in stats.lines() {
            if line.trim().is_empty() || line.contains("file changed") {
                continue;
            }
            // Parse lines like: "src/main.rs | 10 +++++-----"
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 2 {
                let path = parts[0].trim().to_string();
                let stat_part = parts[1].trim();
                let added = stat_part.chars().filter(|&c| c == '+').count();
                let removed = stat_part.chars().filter(|&c| c == '-').count();

                let change_type = ChangeType::Modify;

                changes.push(FileChange {
                    path,
                    change_type,
                    lines_added: added,
                    lines_removed: removed,
                    diff: String::new(),
                });
            }
        }

        Ok(changes)
    }

    /// Apply a patch to the workspace using git apply
    pub fn apply_patch(&self, patch_content: &str) -> Result<String, String> {
        let tmp_file = format!("/tmp/_lingshu_patch_{}", std::process::id());
        std::fs::write(&tmp_file, patch_content).map_err(|e| e.to_string())?;

        let output = Command::new("git")
            .args(["apply", "--whitespace=fix", &tmp_file])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git apply failed: {}", e))?;

        let _ = std::fs::remove_file(&tmp_file);

        if output.status.success() {
            Ok("Patch applied successfully.".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Patch apply failed: {}", stderr))
        }
    }

    /// Rollback to the last checkpoint using git checkout
    pub fn rollback(&self, commit_hash: &str) -> Result<String, String> {
        let output = Command::new("git")
            .args(["checkout", commit_hash, "--", "."])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git checkout failed: {}", e))?;

        if output.status.success() {
            Ok(format!("Rolled back to checkpoint {}", &commit_hash[..std::cmp::min(8, commit_hash.len())]))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Rollback failed: {}", stderr))
        }
    }

    /// Show the diff for review (safe, read-only)
    pub fn review_diff(&self) -> Result<String, String> {
        let output = Command::new("git")
            .args(["diff", "--no-color"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git diff failed: {}", e))?;

        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if diff.is_empty() {
            return Ok("No changes to review.".to_string());
        }

        // Also get stat
        let stat_output = Command::new("git")
            .args(["diff", "--no-color", "--stat"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| e.to_string())?;
        let stat = String::from_utf8_lossy(&stat_output.stdout);

        let mut result = String::new();
        result.push_str("=== Changes to Review ===\n\n");
        result.push_str(&stat);

        // Calculate risk
        let added = diff.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
        let removed = diff.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
        result.push_str(&format!("\n+{} / -{} lines\n", added, removed));

        if added > 100 || removed > 50 {
            result.push_str("\u{26a0}\u{fe0f}  Large change — consider reviewing carefully.\n");
        }
        if diff.contains("unsafe") {
            result.push_str("\u{26a0}\u{fe0f}  Contains `unsafe` code.\n");
        }

        if diff.len() <= 5000 {
            result.push_str("\n---\n");
            result.push_str(&diff);
        } else {
            result.push_str(&format!("\n--- Diff too large ({} bytes), showing first 3000 chars ---\n", diff.len()));
            result.push_str(&diff[..3000]);
            result.push_str("\n... (truncated)");
        }

        Ok(result)
    }

    /// Get a succinct file-level summary of changes (for agent use)
    pub fn get_change_summary(&self) -> Result<String, String> {
        let output = Command::new("git")
            .args(["diff", "--no-color", "--stat"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git diff --stat failed: {}", e))?;

        let stats = String::from_utf8_lossy(&output.stdout).to_string();
        if stats.is_empty() {
            Ok("No changes.".to_string())
        } else {
            let lines: Vec<&str> = stats.lines().collect();
            let file_count = lines.iter().filter(|l| !l.contains("file changed")).count();
            Ok(format!("{} file(s) changed\n{}", file_count, stats))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_type_display() {
        assert_eq!(ChangeType::Create.to_string(), "CREATE");
        assert_eq!(ChangeType::Modify.to_string(), "MODIFY");
        assert_eq!(ChangeType::Delete.to_string(), "DELETE");
    }

    #[test]
    fn test_patch_set_new() {
        let patch = PatchSet::new("test patch");
        assert!(patch.id.starts_with("patch_"));
        assert_eq!(patch.description, "test patch");
        assert!(!patch.applied);
        assert!(!patch.rolled_back);
        assert!(patch.changes.is_empty());
    }

    #[test]
    fn test_patch_set_summary_empty() {
        let patch = PatchSet::new("empty");
        assert_eq!(patch.summary(), "No changes.");
    }

    #[test]
    fn test_patch_set_summary_with_changes() {
        let mut patch = PatchSet::new("test");
        patch.changes.push(FileChange {
            path: "src/main.rs".to_string(),
            change_type: ChangeType::Modify,
            lines_added: 5,
            lines_removed: 2,
            diff: "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n".to_string(),
        });
        let summary = patch.summary();
        assert!(summary.contains("MODIFY"));
        assert!(summary.contains("src/main.rs"));
        assert!(summary.contains("+5"));
        assert!(summary.contains("-2"));
    }

    #[test]
    fn test_parse_hunk_header() {
        let (added, removed) = PatchEngine::parse_hunk_header("@@ -10,6 +10,7 @@").unwrap();
        assert_eq!(added, 10);
        assert_eq!(removed, 7);
    }

    #[test]
    fn test_parse_hunk_header_single() {
        let (added, removed) = PatchEngine::parse_hunk_header("@@ -1 +1 @@").unwrap();
        assert_eq!(added, 1);
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_parse_diff_simple() {
        let diff = "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n-foo\n+bar\n+baz\n old_line\n";
        let engine = PatchEngine::new(PathBuf::from("."));
        let changes = engine.parse_diff(diff);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "src/main.rs");
    }

    #[test]
    fn test_parse_diff_empty() {
        let engine = PatchEngine::new(PathBuf::from("."));
        let changes = engine.parse_diff("");
        assert!(changes.is_empty());
    }
}
