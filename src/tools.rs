use crate::model::{ToolCall, ToolDefinition, ToolResult};
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

/// Get all available tool definitions for the LLM
pub fn all_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read_file".into(),
            description: "Read the contents of a file in the project".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative path to file"}
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "write_file".into(),
            description: "Write content to a file (creates or overwrites)".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative path to file"},
                    "content": {"type": "string", "description": "File content"}
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "edit_file".into(),
            description: "Apply a search-and-replace edit to a file".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old_string": {"type": "string", "description": "Text to replace"},
                    "new_string": {"type": "string", "description": "Replacement text"}
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        ToolDefinition {
            name: "run_shell".into(),
            description: "Run a shell command in the project directory".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to run"},
                    "description": {"type": "string", "description": "What this command does"}
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "search_code".into(),
            description: "Search for text patterns in project files".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Text or regex pattern"},
                    "file_pattern": {"type": "string", "description": "Optional file glob (e.g. *.rs)"}
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "list_dir".into(),
            description: "List files in a directory".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Relative directory path"}
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "git_status".into(),
            description: "Show git status of the project".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "git_diff".into(),
            description: "Show git diff (unstaged changes)".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "diagnose".into(),
            description: "Run cargo check and report compilation errors".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    ]
}

/// Execute a tool call and return the result
pub fn execute_tool(tc: &ToolCall, project_dir: &PathBuf) -> ToolResult {
    let result = match tc.name.as_str() {
        "read_file" => execute_read_file(tc, project_dir),
        "write_file" => execute_write_file(tc, project_dir),
        "edit_file" => execute_edit_file(tc, project_dir),
        "run_shell" => execute_run_shell(tc, project_dir),
        "search_code" => execute_search_code(tc, project_dir),
        "list_dir" => execute_list_dir(tc, project_dir),
        "git_status" => execute_git(project_dir, &["status", "--short"]),
        "git_diff" => execute_git(project_dir, &["diff", "--no-color"]),
        "diagnose" => execute_diagnose(project_dir),
        _ => ToolResult {
            call_id: tc.id.clone(),
            name: tc.name.clone(),
            output: format!("Unknown tool: {}", tc.name),
            is_error: true,
        },
    };
    result
}

fn execute_read_file(tc: &ToolCall, project_dir: &PathBuf) -> ToolResult {
    let path = tc.arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let full_path = project_dir.join(path);
    match std::fs::read_to_string(&full_path) {
        Ok(content) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: content, is_error: false,
        },
        Err(e) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Error reading {}: {}", full_path.display(), e), is_error: true,
        },
    }
}

fn execute_write_file(tc: &ToolCall, project_dir: &PathBuf) -> ToolResult {
    let path = tc.arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let content = tc.arguments.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let full_path = project_dir.join(path);
    // Create parent dirs
    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&full_path, content) {
        Ok(_) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Written {} bytes to {}", content.len(), full_path.display()),
            is_error: false,
        },
        Err(e) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Error writing {}: {}", full_path.display(), e), is_error: true,
        },
    }
}

fn execute_edit_file(tc: &ToolCall, project_dir: &PathBuf) -> ToolResult {
    let path = tc.arguments.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let old = tc.arguments.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
    let new = tc.arguments.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
    let full_path = project_dir.join(path);

    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => return ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Error reading {}: {}", full_path.display(), e), is_error: true,
        },
    };

    if !content.contains(old) {
        return ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Error: Could not find the specified text in {}", path), is_error: true,
        };
    }

    let new_content = content.replace(old, new);
    match std::fs::write(&full_path, &new_content) {
        Ok(_) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Edited {}: replaced `{}` with `{}`", path,
                &old[..std::cmp::min(old.len(), 40)], &new[..std::cmp::min(new.len(), 40)]),
            is_error: false,
        },
        Err(e) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Error writing {}: {}", full_path.display(), e), is_error: true,
        },
    }
}

fn execute_run_shell(tc: &ToolCall, project_dir: &PathBuf) -> ToolResult {
    let cmd = tc.arguments.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let desc = tc.arguments.get("description").and_then(|v| v.as_str()).unwrap_or("run command");

    let output = Command::new("sh")
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
            // Truncate if too long
            if result.len() > 50000 {
                result.truncate(50000);
                result.push_str("
... (truncated)");
            }
            ToolResult {
                call_id: tc.id.clone(), name: tc.name.clone(),
                output: if result.is_empty() { format!("[{}: completed, exit=0]", desc) } else { result },
                is_error: !success,
            }
        }
        Err(e) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Shell error: {}", e), is_error: true,
        },
    }
}

fn execute_search_code(tc: &ToolCall, project_dir: &PathBuf) -> ToolResult {
    let pattern = tc.arguments.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    let file_pattern = tc.arguments.get("file_pattern").and_then(|v| v.as_str()).unwrap_or("");

    let mut cmd = vec!["grep", "-rn", "--binary-files=without-match"];
    if !file_pattern.is_empty() {
        cmd.extend_from_slice(&["--include", file_pattern]);
    }
    cmd.push(pattern);
    cmd.push(".");

    let output = Command::new("grep")
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
            if result.len() > 30000 { result.truncate(30000); result.push_str("
... (truncated)"); }
            ToolResult { call_id: tc.id.clone(), name: tc.name.clone(), output: result, is_error: !out.status.success() }
        }
        Err(e) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Search error: {}", e), is_error: true,
        },
    }
}

fn execute_list_dir(tc: &ToolCall, project_dir: &PathBuf) -> ToolResult {
    let path = tc.arguments.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let full_path = project_dir.join(path);

    let output = Command::new("ls")
        .arg("-la")
        .arg(&full_path)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            ToolResult { call_id: tc.id.clone(), name: tc.name.clone(), output: stdout.to_string(), is_error: false }
        }
        Err(e) => ToolResult {
            call_id: tc.id.clone(), name: tc.name.clone(),
            output: format!("Error listing {}: {}", full_path.display(), e), is_error: true,
        },
    }
}

fn execute_git(project_dir: &PathBuf, args: &[&str]) -> ToolResult {
    let output = Command::new("git")
        .args(args)
        .current_dir(project_dir)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let mut result = stdout.to_string();
            if !stderr.is_empty() { result.push_str(&stderr); }
            ToolResult {
                call_id: String::new(), name: "git".into(),
                output: result, is_error: !out.status.success(),
            }
        }
        Err(e) => ToolResult {
            call_id: String::new(), name: "git".into(),
            output: format!("Git error: {}", e), is_error: true,
        },
    }
}

fn execute_diagnose(project_dir: &PathBuf) -> ToolResult {
    let output = Command::new("cargo")
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
                result.push_str("\n✅ No errors.");
            } else {
                result.push_str("\n❌ Compilation errors found.");
            }
            ToolResult {
                call_id: String::new(), name: "diagnose".into(),
                output: result, is_error: !out.status.success(),
            }
        }
        Err(e) => ToolResult {
            call_id: String::new(), name: "diagnose".into(),
            output: format!("Diagnose error: {}", e), is_error: true,
        },
    }
}
