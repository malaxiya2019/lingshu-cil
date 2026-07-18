use super::{Tool, ToolOutput};
use serde_json::Value;
use std::path::Path;

pub struct SearchCodeTool;
impl Tool for SearchCodeTool {
    fn name(&self) -> &str { "search_code" }
    fn description(&self) -> &str { "Search for text patterns in project files" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "Text or regex pattern"},
                "file_pattern": {"type": "string", "description": "Optional file glob (e.g. *.rs)"}
            },
            "required": ["pattern"]
        })
    }
    fn execute(&self, input: Value, project_dir: &Path) -> ToolOutput {
        let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let file_pattern = input.get("file_pattern").and_then(|v| v.as_str()).unwrap_or("");

        let mut cmd = vec!["grep", "-rn", "--binary-files=without-match"];
        if !file_pattern.is_empty() {
            cmd.extend_from_slice(&["--include", file_pattern]);
        }
        cmd.push(pattern);
        cmd.push(".");

        // Skip first element since we hardcoded "grep"
        let output = std::process::Command::new("grep")
            .args(&cmd[1..])
            .current_dir(project_dir)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = stdout.to_string();
                if !stderr.is_empty() { result.push_str(&stderr); }
                if result.is_empty() { result = format!("No matches for `{}`", pattern); }
                if result.len() > 30000 { result.truncate(30000); result.push_str("\n... (truncated)"); }
                ToolOutput { output: result, is_error: !out.status.success() }
            }
            Err(e) => ToolOutput {
                output: format!("Search error: {}", e),
                is_error: true,
            },
        }
    }
}
