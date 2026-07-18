use crate::context::ContextEngine;
use crate::llm;
use crate::logging::Logger;
use crate::model::{LlmMessage, ModelConfig, PermissionMode, Task, TaskStatus, ToolCall};
use crate::tools::ToolRegistry;

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Receiver;

/// CIL Runtime — the core coding agent loop
pub struct CilRuntime {
    pub project_dir: PathBuf,
    pub context: ContextEngine,
    pub logger: Logger,
    pub model: ModelConfig,
    pub mode: PermissionMode,
    pub should_exit: bool,
    pub tasks: Vec<Task>,
    pub memory: HashMap<String, String>,
    pub log_path: String,
    pub tool_registry: ToolRegistry,
}

impl CilRuntime {
    pub fn new(workspace: PathBuf) -> Result<Self> {
        let logger = Logger::new("lingshu-cil")?;
        let log_path = logger.path().display().to_string();
        let mut context = ContextEngine::new(workspace.clone());
        let _ = context.scan_workspace();

        Ok(Self {
            project_dir: workspace,
            context,
            logger,
            model: ModelConfig::builtins().into_iter().find(|m| m.name == "deepseek-coder")
                .unwrap_or_else(|| ModelConfig::builtins()[0].clone()),
            mode: PermissionMode::Normal,
            should_exit: false,
            tasks: Vec::new(),
            memory: HashMap::new(),
            log_path,
            tool_registry: ToolRegistry::new(),
        })
    }

    // ── Project commands ──

    pub fn cmd_project(&mut self, path: &str) -> Result<String> {
        let target = if path.is_empty() {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
        } else {
            let p = PathBuf::from(shellexpand(path));
            if p.is_absolute() { p } else { self.project_dir.join(&p) }
        };
        if target.exists() {
            self.project_dir = target.clone();
            self.context.set_workspace(target.clone());
            match self.context.scan_workspace() {
                Ok(ctx) => Ok(format!("Project: {}\n   {} files, {} lines", target.display(), ctx.files.len(), ctx.total_lines)),
                Err(e) => Ok(format!("Project: {} (scan: {})", target.display(), e)),
            }
        } else {
            Ok(format!("Directory not found: {}", target.display()))
        }
    }

    pub fn cmd_open(&self, path: &str) -> Result<String> {
        let full_path = self.project_dir.join(path);
        match std::fs::read_to_string(&full_path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let preview: String = if lines.len() > 100 {
                    lines[..100].join("\n") + &format!("\n... ({} more lines)", lines.len() - 100)
                } else { content.clone() };
                Ok(format!("{} ({} lines)\n\n{}", path, lines.len(), preview))
            }
            Err(e) => Ok(format!("Cannot open {}: {}", path, e)),
        }
    }

    pub fn cmd_search(&self, pattern: &str) -> Result<String> {
        let output = Command::new("grep")
            .args(["-rn", "--binary-files=without-match", "--color=never", pattern, "."])
            .current_dir(&self.project_dir)
            .output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if stdout.is_empty() {
                    Ok(format!("No matches for `{}`", pattern))
                } else {
                    let count = stdout.lines().count();
                    Ok(format!("{} matches for `{}`:\n\n{}", count, pattern, stdout))
                }
            }
            Err(e) => Ok(format!("Search error: {}", e)),
        }
    }

    pub fn cmd_edit(&self, path: &str, old: &str, new: &str) -> Result<String> {
        let full_path = self.project_dir.join(path);
        if !full_path.exists() {
            return Ok(format!("File not found: {}", path));
        }
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => return Ok(format!("Read error: {}", e)),
        };
        if !content.contains(old) {
            return Ok(format!("Could not find specified text in {}", path));
        }
        let new_content = content.replace(old, new);
        match std::fs::write(&full_path, &new_content) {
            Ok(_) => Ok(format!("Edited {}", path)),
            Err(e) => Ok(format!("Write error: {}", e)),
        }
    }

    pub fn cmd_explain(&mut self, query: &str) -> Result<String> {
        if query.is_empty() {
            return Ok("Usage: /explain <question about code>".to_string());
        }
        let ctx = self.context.scan_workspace().ok();
        let ctx_summary = ctx.as_ref().map(|c| c.summary.clone()).unwrap_or_default();
        let messages = vec![
            LlmMessage::system(&format!("{} Project: {}", llm::build_system_prompt(), ctx_summary)),
            LlmMessage::user(query),
        ];

        match self.stream_to_string(&messages) {
            Ok(response) => Ok(response),
            Err(e) => Ok(format!("LLM error: {}\n\nFalling back to shell.", e)),
        }
    }

    pub fn cmd_run(&self, cmd: &str) -> Result<String> {
        if cmd.is_empty() { return Ok("Usage: /run <command>".to_string()); }
        let output = Command::new("sh").arg("-c").arg(cmd).current_dir(&self.project_dir).output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = String::new();
                if !stdout.is_empty() { result.push_str(&stdout); }
                if !stderr.is_empty() { result.push_str(&stderr); }
                if result.len() > 10000 { result.truncate(10000); result.push_str("\n... (truncated)"); }
                Ok(if result.is_empty() { format!("Command completed (exit={})", out.status.code().unwrap_or(-1)) } else { result })
            }
            Err(e) => Ok(format!("Error: {}", e)),
        }
    }

    pub fn cmd_shell(&self, cmd: &str) -> Result<String> {
        self.cmd_run(cmd)
    }

    pub fn cmd_cargo(&self, args: &str) -> Result<String> {
        let cargo_args: Vec<&str> = if args.is_empty() { vec!["check"] } else { args.split_whitespace().collect() };
        let output = Command::new("cargo").args(&cargo_args).current_dir(&self.project_dir).output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = stdout.to_string();
                if !stderr.is_empty() { result.push_str(&stderr); }
                if out.status.success() {
                    result.push_str("\nCargo succeeded.");
                } else {
                    result.push_str("\nCargo failed.");
                }
                Ok(result)
            }
            Err(e) => Ok(format!("Cargo error: {}", e)),
        }
    }

    pub fn cmd_git(&self, args: &str) -> Result<String> {
        let git_args: Vec<&str> = if args.is_empty() { vec!["status", "--short"] } else { args.split_whitespace().collect() };
        let output = Command::new("git").args(&git_args).current_dir(&self.project_dir).output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = stdout.to_string();
                if !stderr.is_empty() { result.push_str(&stderr); }
                Ok(result)
            }
            Err(e) => Ok(format!("Git error: {}", e)),
        }
    }

    pub fn cmd_memory(&mut self, args: &str) -> Result<String> {
        if args.is_empty() {
            if self.memory.is_empty() {
                return Ok("Memory is empty.".to_string());
            }
            let mut result = "Memory:\n".to_string();
            for (k, v) in &self.memory {
                result.push_str(&format!("  {} = {}\n", k, &v[..std::cmp::min(v.len(), 80)]));
            }
            return Ok(result);
        }
        if let Some(eq_pos) = args.find('=') {
            let key = args[..eq_pos].trim();
            let val = args[eq_pos + 1..].trim();
            self.memory.insert(key.to_string(), val.to_string());
            Ok(format!("Stored: {}", key))
        } else if self.memory.contains_key(args) {
            Ok(format!("{} = {}", args, self.memory[args]))
        } else {
            Ok(format!("No key '{}' in memory", args))
        }
    }

    pub fn cmd_task(&mut self, args: &str) -> Result<String> {
        if args.is_empty() || args == "list" || args == "ls" {
            if self.tasks.is_empty() {
                return Ok("No tasks.".to_string());
            }
            let mut result = "Tasks:\n".to_string();
            for (i, task) in self.tasks.iter().enumerate() {
                result.push_str(&format!("  {}. [{}] {}\n", i + 1, task.status, task.description));
            }
            return Ok(result);
        }
        if args == "done" || args == "clear" {
            self.tasks.clear();
            return Ok("All tasks cleared.".to_string());
        }
        let id = format!("task_{}", chrono::Utc::now().timestamp());
        let task = Task {
            id,
            description: args.to_string(),
            status: TaskStatus::Pending,
            created_at: chrono::Utc::now().format("%H:%M:%S").to_string(),
        };
        self.tasks.push(task);
        Ok(format!("Task added: {}", args))
    }

    pub fn cmd_diagnose(&self) -> Result<String> {
        let output = Command::new("cargo")
            .args(["check", "--color", "never"])
            .current_dir(&self.project_dir)
            .output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = String::new();
                result.push_str("cargo check diagnostics\n");
                result.push_str("────────────────────────\n");
                if !stdout.is_empty() { result.push_str(&stdout); }
                if !stderr.is_empty() { result.push_str(&stderr); }
                if out.status.success() {
                    result.push_str("\nNo errors. Clean compilation.");
                } else {
                    let error_count = stderr.lines().filter(|l| l.contains("error")).count();
                    result.push_str(&format!("\n{} compilation error(s).", error_count));
                }
                Ok(result)
            }
            Err(e) => Ok(format!("Diagnose error: {}", e)),
        }
    }

    pub fn cmd_fix(&mut self, query: &str) -> Result<String> {
        if query.is_empty() {
            return Ok("Usage: /fix <description of what to fix>\nor: /fix (auto-detect from cargo check)".to_string());
        }
        let diag_output = Command::new("cargo")
            .args(["check", "--color", "never"])
            .current_dir(&self.project_dir)
            .output();
        let errors = match diag_output {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.success() {
                    "No compilation errors.".to_string()
                } else {
                    stderr.to_string()
                }
            }
            Err(_) => "Could not run cargo check.".to_string(),
        };

        let ctx = self.context.scan_workspace().ok();
        let ctx_summary = ctx.as_ref().map(|c| c.summary.clone()).unwrap_or_default();
        let user_msg = format!(
            "I need to fix this issue: {}\n\nCompilation errors:\n{}\n\nPlease analyze and provide the fix.",
            query, errors
        );
        let messages = vec![
            LlmMessage::system(&format!("{} Project: {}", llm::build_system_prompt(), ctx_summary)),
            LlmMessage::user(&user_msg),
        ];

        match self.stream_to_string(&messages) {
            Ok(response) => Ok(format!("Fix Analysis:\n\n{}", response)),
            Err(e) => Ok(format!("LLM error: {}", e)),
        }
    }

    pub fn cmd_commit(&self, msg: &str) -> Result<String> {
        if !msg.is_empty() {
            let output = Command::new("git")
                .args(["commit", "-m", msg])
                .current_dir(&self.project_dir)
                .output();
            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let mut result = stdout.to_string();
                    if !stderr.is_empty() { result.push_str(&stderr); }
                    Ok(result)
                }
                Err(e) => Ok(format!("Git error: {}", e)),
            }
        } else {
            let output = Command::new("git")
                .args(["diff", "--stat"])
                .current_dir(&self.project_dir)
                .output();
            let stats = match output {
                Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
                Err(_) => "Could not get diff".to_string(),
            };
            let output2 = Command::new("git")
                .args(["diff", "--no-color"])
                .current_dir(&self.project_dir)
                .output();
            let diff = match output2 {
                Ok(out) => {
                    let d = String::from_utf8_lossy(&out.stdout);
                    if d.len() > 2000 {
                        format!("{}... (truncated)", &d[..2000])
                    } else { d.to_string() }
                }
                Err(_) => String::new(),
            };
            Ok(format!(
                "Changes to commit:\n{}\n\n{}\n\nUse: /commit \"your message\"",
                stats, diff
            ))
        }
    }

    pub fn cmd_status(&mut self) -> Result<String> {
        let ctx = self.context.scan_workspace().ok();
        let ctx_info = ctx.as_ref()
            .map(|c| format!("{} files, {} lines", c.files.len(), c.total_lines))
            .unwrap_or_else(|| "not scanned".to_string());
        Ok(format!(
            "LingShu CIL\n             Model: {}\n             Mode: {}\n             Project: {}\n             Context: {}\n             Tasks: {}\n             Log: {}",
            self.model.name, self.mode, self.project_dir.display(), ctx_info, self.tasks.len(), self.log_path
        ))
    }

    // ── AI Helpers ──

    fn stream_to_string(&self, messages: &[LlmMessage]) -> Result<String, String> {
        let rx = llm::chat_stream(&self.model, messages, None)
            .map_err(|e| format!("LLM error: {}", e))?;
        collect_stream(rx)
    }

    /// LLM-powered coding task with tool calling
    pub fn run_ai_task(&mut self, task_description: &str) -> Result<String> {
        let ctx = self.context.scan_workspace().ok();
        let ctx_summary = ctx.as_ref().map(|c| c.summary.clone()).unwrap_or_default();
        let tools = self.tool_registry.to_definitions();

        let mut messages = vec![
            LlmMessage::system(&format!(
                "{} Project: {} - {}",
                llm::build_system_prompt(), self.project_dir.display(), ctx_summary
            )),
            LlmMessage::user(task_description),
        ];

        let task_id = format!("task_{}", chrono::Utc::now().timestamp());
        self.tasks.push(Task {
            id: task_id.clone(),
            description: task_description.to_string(),
            status: TaskStatus::InProgress,
            created_at: chrono::Utc::now().format("%H:%M:%S").to_string(),
        });

        for _iteration in 0..15 {
            let rx = match llm::chat_stream(&self.model, &messages, Some(&tools)) {
                Ok(r) => r,
                Err(e) => return Ok(format!("LLM error: {}", e)),
            };

            let mut content = String::new();
            let mut tool_calls_map: HashMap<usize, ToolCall> = HashMap::new();

            for event in rx {
                match event {
                    llm::StreamEvent::Chunk(chunk) => content.push_str(&chunk),
                    llm::StreamEvent::ToolCallDelta(tc) => {
                        let entry = tool_calls_map.entry(tc.index).or_insert_with(|| ToolCall {
                            id: tc.id.clone().unwrap_or_default(),
                            name: String::new(),
                            arguments: serde_json::Value::Null,
                        });
                        if let Some(ref id) = tc.id { entry.id = id.clone(); }
                        if let Some(ref func) = tc.function {
                            if let Some(ref name) = func.name { entry.name = name.clone(); }
                            if let Some(ref args) = func.arguments {
                                let current = entry.arguments.as_str().unwrap_or("").to_string() + args;
                                entry.arguments = serde_json::from_str(&current).unwrap_or(serde_json::Value::String(current));
                            }
                        }
                    }
                    llm::StreamEvent::Done => break,
                    llm::StreamEvent::Error(e) => return Ok(format!("Error: {}", e)),
                }
            }

            let tool_calls: Vec<ToolCall> = {
                let mut tcs: Vec<_> = tool_calls_map.into_values().collect();
                tcs.sort_by_key(|tc| tc.id.as_str().len());
                tcs
            };

            messages.push(LlmMessage::assistant(&content, if tool_calls.is_empty() { None } else { Some(tool_calls.clone()) }));

            if tool_calls.is_empty() {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
                    task.status = TaskStatus::Done;
                }
                return Ok(content);
            }

            for tc in &tool_calls {
                let result = self.tool_registry.execute(
                    &tc.name,
                    tc.arguments.clone(),
                    &self.project_dir,
                );
                messages.push(LlmMessage::tool(&result.output, &tc.id));
            }
        }

        Ok("Max iterations reached. Task may be incomplete.".to_string())
    }
}

fn collect_stream(rx: Receiver<llm::StreamEvent>) -> Result<String, String> {
    let mut result = String::new();
    for event in rx {
        match event {
            llm::StreamEvent::Chunk(chunk) => result.push_str(&chunk),
            llm::StreamEvent::Done => break,
            llm::StreamEvent::ToolCallDelta(_) => {}
            llm::StreamEvent::Error(e) => return Err(e),
        }
    }
    Ok(result)
}

fn shellexpand(s: &str) -> String {
    if s.starts_with("~/") {
        dirs::home_dir().map(|h| h.join(&s[2..]).display().to_string()).unwrap_or_else(|| s.to_string())
    } else if s == "~" {
        dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}
