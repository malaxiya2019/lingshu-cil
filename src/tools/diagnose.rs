use super::{Tool, ToolOutput};
use serde_json::Value;
use std::path::PathBuf;

pub struct DiagnoseTool;
impl Tool for DiagnoseTool {
    fn name(&self) -> &str { "diagnose" }
    fn description(&self) -> &str { "Run cargo check and report compilation errors" }
    fn input_schema(&self) -> Value { serde_json::json!({"type": "object","properties": {},"required": []}) }
    fn execute(&self, _input: Value, project_dir: &PathBuf) -> ToolOutput {
        let output = std::process::Command::new("cargo")
            .args(["check", "--color", "never"])
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
