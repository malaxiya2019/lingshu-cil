use super::{Tool, ToolOutput};
use serde_json::Value;
use std::path::Path;

pub struct DiagnoseTool;
impl Tool for DiagnoseTool {
    fn name(&self) -> &str { "diagnose" }
    fn description(&self) -> &str { "Run cargo check and report compilation errors" }
    fn input_schema(&self) -> Value { serde_json::json!({"type": "object","properties": {},"required": []}) }
    fn execute(&self, _input: Value, project_dir: &Path) -> ToolOutput {
        // Use cargo-watch for auto-rechecking if available
        let check_args: &[&str] = if std::process::Command::new("cargo")
            .args(["watch", "--help"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        { &["watch", "-x", "check", "--no-color"] }
        else { &["check", "--color", "never"] };
        let output = std::process::Command::new("cargo")
            .args(check_args)
            .current_dir(project_dir)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = String::new();
                result.push_str("=== cargo check ===\n");
                if !stdout.is_empty() { result.push_str(&stdout); }
                if !stderr.is_empty() { result.push_str(&stderr); }
                if out.status.success() {
                    result.push_str("\nNo errors.");
                } else {
                    result.push_str("\nCompilation errors found.");
                }
                ToolOutput { output: result, is_error: !out.status.success() }
            }
            Err(e) => ToolOutput {
                output: format!("Diagnose error: {}", e),
                is_error: true,
            },
        }
    }
}
