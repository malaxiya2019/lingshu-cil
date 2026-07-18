use super::{Tool, ToolOutput};
use serde_json::Value;
use std::path::PathBuf;

pub struct RunShellTool;
impl Tool for RunShellTool {
    fn name(&self) -> &str { "run_shell" }
    fn description(&self) -> &str { "Run a shell command in the project directory" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell command to run"},
                "description": {"type": "string", "description": "What this command does"}
            },
            "required": ["command"]
        })
    }
    fn execute(&self, input: Value, project_dir: &PathBuf) -> ToolOutput {
        let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let desc = input.get("description").and_then(|v| v.as_str()).unwrap_or("run command");

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(project_dir)
            .output();

        match output {
            Ok(out) => {
                let mut result = String::new();
                if !out.stdout.is_empty() {
                    result.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    if !result.is_empty() { result.push('\n'); }
                    result.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                let success = out.status.success();
                if result.len() > 50000 {
                    result.truncate(50000);
                    result.push_str("\n... (truncated)");
                }
                ToolOutput {
                    output: if result.is_empty() { format!("[{}: completed, exit=0]", desc) } else { result },
                    is_error: !success,
                }
            }
            Err(e) => ToolOutput {
                output: format!("Shell error: {}", e),
                is_error: true,
            },
        }
    }
}
