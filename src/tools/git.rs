use super::{Tool, ToolOutput};
use serde_json::Value;
use std::path::PathBuf;

pub struct GitStatusTool;
impl Tool for GitStatusTool {
    fn name(&self) -> &str { "git_status" }
    fn description(&self) -> &str { "Show git status of the project" }
    fn input_schema(&self) -> Value { serde_json::json!({"type": "object","properties": {},"required": []}) }
    fn execute(&self, _input: Value, project_dir: &PathBuf) -> ToolOutput {
        exec_git(project_dir, &["status", "--short"])
    }
}

pub struct GitDiffTool;
impl Tool for GitDiffTool {
    fn name(&self) -> &str { "git_diff" }
    fn description(&self) -> &str { "Show git diff (unstaged changes)" }
    fn input_schema(&self) -> Value { serde_json::json!({"type": "object","properties": {},"required": []}) }
    fn execute(&self, _input: Value, project_dir: &PathBuf) -> ToolOutput {
        exec_git(project_dir, &["diff", "--no-color"])
    }
}

fn exec_git(project_dir: &PathBuf, args: &[&str]) -> ToolOutput {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(project_dir)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let mut result = stdout.to_string();
            if !stderr.is_empty() { result.push_str(&stderr); }
            ToolOutput { output: result, is_error: !out.status.success() }
        }
        Err(e) => ToolOutput {
            output: format!("Git error: {}", e),
            is_error: true,
        },
    }
}
