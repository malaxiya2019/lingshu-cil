use super::{Tool, ToolOutput};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// A structured compilation error
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticError {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub error_code: String,
    pub message: String,
    pub suggestion: String,
}

/// Parse cargo check/clippy output into structured errors
pub fn parse_diagnostics(stderr: &str) -> Vec<DiagnosticError> {
    let mut errors = Vec::new();
    let mut current_file = String::new();
    let mut current_line = 0usize;
    let mut current_col = 0usize;
    let mut current_code = String::new();
    let mut current_msg = String::new();
    let mut current_suggestion = String::new();
    let mut in_error = false;
    let mut in_suggestion = false;

    for line in stderr.lines() {
        if line.contains("error[") || line.contains("error:") {
            if in_error && !current_code.is_empty() {
                errors.push(DiagnosticError {
                    file: current_file.clone(),
                    line: current_line,
                    column: current_col,
                    error_code: current_code.clone(),
                    message: current_msg.clone(),
                    suggestion: current_suggestion.clone(),
                });
            }

            in_error = true;
            in_suggestion = false;
            current_suggestion.clear();

            if let Some(start) = line.find("error[") {
                if let Some(end) = line.find(']') {
                    current_code = line[start + 6..end].to_string();
                }
            } else {
                current_code = "general".to_string();
            }
            current_msg = line.to_string();

        } else if in_error && line.trim().starts_with("--> ") {
            let loc = line.trim().strip_prefix("--> ").unwrap_or("");
            let parts: Vec<&str> = loc.rsplitn(3, ':').collect();
            if parts.len() >= 3 {
                current_file = parts[2].trim().to_string();
                current_line = parts[1].parse::<usize>().unwrap_or(0);
                current_col = parts[0].parse::<usize>().unwrap_or(0);
            } else if parts.len() == 2 {
                current_file = parts[1].trim().to_string();
                current_line = parts[0].parse::<usize>().unwrap_or(0);
                current_col = 1;
            }

        } else if in_error && line.contains("help:") {
            in_suggestion = true;
            if let Some(help_idx) = line.find("help:") {
                current_suggestion.push_str(line[help_idx + 5..].trim());
            }

        } else if in_error && in_suggestion {
            if line.starts_with("  ") || line.starts_with("   ") {
                current_suggestion.push(' ');
                current_suggestion.push_str(line.trim());
            } else {
                in_suggestion = false;
            }

        } else if in_error && !current_code.is_empty() && line.trim().starts_with("=") {
            current_msg.push(' ');
            current_msg.push_str(line.trim());
        }
    }

    if in_error && !current_code.is_empty() {
        errors.push(DiagnosticError {
            file: current_file,
            line: current_line,
            column: current_col,
            error_code: current_code,
            message: current_msg,
            suggestion: current_suggestion,
        });
    }

    errors
}

pub struct DiagnoseTool;
impl Tool for DiagnoseTool {
    fn name(&self) -> &str { "diagnose" }
    fn description(&self) -> &str { "Run cargo check and report structured compilation errors" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "full": {"type": "boolean", "description": "Show full raw output instead of structured"}
            },
            "required": []
        })
    }
    fn execute(&self, input: Value, project_dir: &Path) -> ToolOutput {
        let full = input.get("full").and_then(|v| v.as_bool()).unwrap_or(false);

        let check_args: &[&str] = if Command::new("cargo")
            .args(["watch", "--help"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        { &["watch", "-x", "check", "--no-color"] }
        else { &["check", "--color", "never"] };

        let output = Command::new("cargo")
            .args(check_args)
            .current_dir(project_dir)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);

                if full {
                    let mut result = String::new();
                    result.push_str("=== cargo check ===\n");
                    if !stdout.is_empty() { result.push_str(&stdout); }
                    if !stderr.is_empty() { result.push_str(&stderr); }
                    if out.status.success() {
                        result.push_str("\nNo errors.");
                    } else {
                        let count = stderr.lines().filter(|l| l.contains("error[") || l.contains("error:")).count();
                        result.push_str(&format!("\n{} error(s) found.", count));
                    }
                    ToolOutput { output: result, is_error: !out.status.success() }
                } else {
                    let errors = parse_diagnostics(&stderr);
                    if errors.is_empty() {
                        ToolOutput {
                            output: serde_json::to_string_pretty(&serde_json::json!({
                                "success": true,
                                "errors": [],
                                "message": "No compilation errors."
                            })).unwrap_or_else(|_| "No errors.".to_string()),
                            is_error: false,
                        }
                    } else {
                        let error_count = errors.len();
                        let max_preview = 5;
                        let preview: Vec<_> = errors.iter().take(max_preview).collect();
                        let has_more = error_count > max_preview;

                        let result = serde_json::json!({
                            "success": false,
                            "error_count": error_count,
                            "errors": preview.iter().map(|e| serde_json::json!({
                                "file": e.file,
                                "line": e.line,
                                "column": e.column,
                                "error_code": e.error_code,
                                "message": e.message,
                                "suggestion": e.suggestion,
                            })).collect::<Vec<_>>(),
                            "note": if has_more { format!("{} more errors (use full mode)", error_count - max_preview) } else { String::new() },
                        });

                        ToolOutput {
                            output: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| format!("{} error(s) found.", error_count)),
                            is_error: true,
                        }
                    }
                }
            }
            Err(e) => ToolOutput {
                output: format!("Diagnose error: {}", e),
                is_error: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_error() {
        let stderr = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n   |\n10 |     let x: i32 = \"hello\";\n   |         ^ expected i32, found &str\n   |\n   = note: expected type `i32`\n   = note: found type `&str`\nhelp: try using a different type\n";
        let errors = parse_diagnostics(stderr);
        assert!(!errors.is_empty());
        assert_eq!(errors[0].error_code, "E0308");
        assert!(errors[0].message.contains("mismatched types"));
    }

    #[test]
    fn test_parse_with_file_location() {
        let stderr = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n";
        let errors = parse_diagnostics(stderr);
        assert!(!errors.is_empty());
        assert_eq!(errors[0].file, "src/main.rs");
        assert_eq!(errors[0].line, 10);
        assert_eq!(errors[0].column, 5);
    }

    #[test]
    fn test_parse_suggestion() {
        let stderr = "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n   |\nhelp: try this\n";
        let errors = parse_diagnostics(stderr);
        assert!(!errors.is_empty());
        assert!(errors[0].suggestion.contains("try this"));
    }

    #[test]
    fn test_parse_no_errors() {
        let errors = parse_diagnostics("Compilation successful");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_parse_general_error() {
        let stderr = "error: could not compile `lingshu-cil` due to previous error\n";
        let errors = parse_diagnostics(stderr);
        assert!(!errors.is_empty());
        assert_eq!(errors[0].error_code, "general");
    }

    #[test]
    fn test_parse_multiple_errors() {
        let stderr = "error[E0308]: first error\n  --> src/main.rs:1:1\nerror[E0425]: second error\n  --> src/lib.rs:5:10\n";
        let errors = parse_diagnostics(stderr);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].error_code, "E0308");
        assert_eq!(errors[1].error_code, "E0425");
    }
}
