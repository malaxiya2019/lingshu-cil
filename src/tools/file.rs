use super::{Tool, ToolOutput};
use serde_json::Value;
use std::path::PathBuf;

pub struct ReadFileTool;
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read the contents of a file in the project" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative path to file"}
            },
            "required": ["path"]
        })
    }
    fn execute(&self, input: Value, project_dir: &PathBuf) -> ToolOutput {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let full_path = project_dir.join(path);
        match std::fs::read_to_string(&full_path) {
            Ok(content) => ToolOutput { output: content, is_error: false },
            Err(e) => ToolOutput {
                output: format!("Error reading {}: {}", full_path.display(), e),
                is_error: true,
            },
        }
    }
}

pub struct WriteFileTool;
impl Tool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Write content to a file (creates or overwrites)" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative path to file"},
                "content": {"type": "string", "description": "File content"}
            },
            "required": ["path", "content"]
        })
    }
    fn execute(&self, input: Value, project_dir: &PathBuf) -> ToolOutput {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let full_path = project_dir.join(path);
        if let Some(parent) = full_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&full_path, content) {
            Ok(_) => ToolOutput {
                output: format!("Written {} bytes to {}", content.len(), full_path.display()),
                is_error: false,
            },
            Err(e) => ToolOutput {
                output: format!("Error writing {}: {}", full_path.display(), e),
                is_error: true,
            },
        }
    }
}

pub struct EditFileTool;
impl Tool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str { "Apply a search-and-replace edit to a file" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "old_string": {"type": "string", "description": "Text to replace"},
                "new_string": {"type": "string", "description": "Replacement text"}
            },
            "required": ["path", "old_string", "new_string"]
        })
    }
    fn execute(&self, input: Value, project_dir: &PathBuf) -> ToolOutput {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let old = input.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
        let new = input.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
        let full_path = project_dir.join(path);

        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => return ToolOutput {
                output: format!("Error reading {}: {}", full_path.display(), e),
                is_error: true,
            },
        };

        if !content.contains(old) {
            return ToolOutput {
                output: format!("Error: Could not find the specified text in {}", path),
                is_error: true,
            };
        }

        let new_content = content.replace(old, new);
        match std::fs::write(&full_path, &new_content) {
            Ok(_) => ToolOutput {
                output: format!("Edited {}: replaced `{}` with `{}`", path,
                    &old[..std::cmp::min(old.len(), 40)], &new[..std::cmp::min(new.len(), 40)]),
                is_error: false,
            },
            Err(e) => ToolOutput {
                output: format!("Error writing {}: {}", full_path.display(), e),
                is_error: true,
            },
        }
    }
}

pub struct ListDirTool;
impl Tool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str { "List files in a directory" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative directory path"}
            },
            "required": ["path"]
        })
    }
    fn execute(&self, input: Value, project_dir: &PathBuf) -> ToolOutput {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let full_path = project_dir.join(path);

        let output = std::process::Command::new("ls")
            .arg("-la")
            .arg(&full_path)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                ToolOutput { output: stdout.to_string(), is_error: false }
            }
            Err(e) => ToolOutput {
                output: format!("Error listing {}: {}", full_path.display(), e),
                is_error: true,
            },
        }
    }
}
