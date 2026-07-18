use std::path::Path;
use std::process::Command;

/// Verify the project compiles and tests pass
pub struct Verifier;

#[derive(Debug)]
pub struct VerifyResult {
    pub success: bool,
    pub errors: Vec<String>,
}

impl Verifier {
    pub fn new() -> Self {
        Self
    }

    /// Run cargo check and collect errors
    pub fn verify(&self, project_dir: &Path) -> VerifyResult {
        let mut errors = Vec::new();

        // Step 1: cargo check
        match Command::new("cargo")
            .args(["check", "--color", "never"])
            .current_dir(project_dir)
            .output()
        {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if !out.status.success() {
                    for line in stderr.lines() {
                        if line.contains("error[") || line.contains("error:") {
                            errors.push(line.to_string());
                        }
                    }
                }
            }
            Err(e) => {
                errors.push(format!("cargo check failed: {}", e));
            }
        }

        // Step 2: cargo test (only if check passed)
        if errors.is_empty() {
            match Command::new("cargo")
                .args(["test", "--color", "never"])
                .current_dir(project_dir)
                .output()
            {
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if !out.status.success() {
                        for line in stderr.lines() {
                            if line.contains("FAILED") || line.contains("error[") {
                                errors.push(line.to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("cargo test failed: {}", e));
                }
            }
        }

        VerifyResult {
            success: errors.is_empty(),
            errors,
        }
    }

    /// Decide whether verification should run based on recent tool activity
    pub fn should_verify(&self, messages: &[crate::model::LlmMessage]) -> bool {
        // Check if any recent messages involved file edits or shell commands
        let recent = messages.iter().rev().take(6);
        for msg in recent {
            if msg.role == "tool" && (msg.content.contains("Written") || msg.content.contains("Edited")) {
                return true;
            }
        }
        false
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LlmMessage;

    #[test]
    fn test_should_verify_with_edits() {
        let v = Verifier::new();
        let msgs = vec![
            LlmMessage::tool("Written 100 bytes to src/main.rs", "call_123"),
        ];
        assert!(v.should_verify(&msgs));
    }

    #[test]
    fn test_should_verify_without_edits() {
        let v = Verifier::new();
        let msgs = vec![
            LlmMessage::tool("Found 5 matches for pattern", "call_123"),
        ];
        assert!(!v.should_verify(&msgs));
    }

    #[test]
    fn test_should_verify_empty() {
        let v = Verifier::new();
        assert!(!v.should_verify(&[]));
    }
}
