use crate::checkpoint::CheckpointManager;
use crate::context::ContextEngine;
use crate::context_selector::ContextSelector;
use crate::llm;
use crate::model::{LlmMessage, ModelConfig, PermissionMode, Task, TaskStatus, ToolCall};
use crate::patch::PatchEngine;
use crate::tools::ToolRegistry;
use crate::verifier::Verifier;

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Receiver;

/// CIL Runtime — the core coding agent loop
#[allow(dead_code)]
pub struct CilRuntime {
    pub project_dir: PathBuf,
    pub context: ContextEngine,
    pub context_selector: ContextSelector,
    pub model: ModelConfig,
    pub mode: PermissionMode,
    pub should_exit: bool,
    pub tasks: Vec<Task>,
    pub memory: HashMap<String, String>,
    pub log_path: String,
    pub tool_registry: ToolRegistry,
    pub patch_engine: PatchEngine,
    pub checkpoint_manager: CheckpointManager,
    pub verifier: Verifier,
    pub current_checkpoint: Option<crate::checkpoint::Checkpoint>,
    pub current_patch: Option<crate::patch::PatchSet>,
}

impl CilRuntime {
    pub fn new(workspace: PathBuf) -> Result<Self> {
        let log_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lingshu")
            .join("logs");
        std::fs::create_dir_all(&log_dir).ok();
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let log_path = log_dir.join(format!("lingshu-cil_{}.log", timestamp));

        let mut context = ContextEngine::new(workspace.clone());
        let _ = context.scan_workspace();

        Ok(Self {
            project_dir: workspace.clone(),
            context,
            context_selector: ContextSelector::new(workspace.clone()),
            model: ModelConfig::builtins().into_iter().find(|m| m.name == "deepseek-coder")
                .unwrap_or_else(|| ModelConfig::builtins()[0].clone()),
            mode: PermissionMode::Normal,
            should_exit: false,
            tasks: Vec::new(),
            memory: HashMap::new(),
            log_path: log_path.display().to_string(),
            tool_registry: ToolRegistry::new(),
            patch_engine: PatchEngine::new(workspace.clone()),
            checkpoint_manager: CheckpointManager::new(workspace),
            verifier: Verifier::new(),
            current_checkpoint: None,
            current_patch: None,
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
        // Use cargo-watch for auto-rechecking when available
        let cargo_args: Vec<&str> = if args.is_empty() {
            if Self::has_cargo_watch() { vec!["watch", "-x", "check"] } else { vec!["check"] }
        } else {
            args.split_whitespace().collect()
        };
        let output = Command::new("cargo").args(&cargo_args).current_dir(&self.project_dir).output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = stdout.to_string();
                if !stderr.is_empty() { result.push_str(&stderr); }
                if out.status.success() { result.push_str("\nCargo succeeded."); }
                else { result.push_str("\nCargo failed."); }
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
            if self.memory.is_empty() { return Ok("Memory is empty.".to_string()); }
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
            if self.tasks.is_empty() { return Ok("No tasks.".to_string()); }
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
            id, description: args.to_string(),
            status: TaskStatus::Pending,
            created_at: chrono::Utc::now().format("%H:%M:%S").to_string(),
        };
        self.tasks.push(task);
        Ok(format!("Task added: {}", args))
    }

    /// Check if cargo-watch is installed
    fn has_cargo_watch() -> bool {
        std::process::Command::new("cargo")
            .args(["watch", "--help"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub fn cmd_diagnose(&self) -> Result<String> {
        let check_args: Vec<&str> = if Self::has_cargo_watch() {
            vec!["watch", "-x", "check", "--no-color"]
        } else {
            vec!["check", "--color", "never"]
        };
        let output = Command::new("cargo")
            .args(&check_args)
            .current_dir(&self.project_dir)
            .output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = "cargo check diagnostics\n────────────────────────\n".to_string();
                if !stdout.is_empty() { result.push_str(&stdout); }
                if !stderr.is_empty() { result.push_str(&stderr); }
                if out.status.success() { result.push_str("\nNo errors. Clean compilation."); }
                else { result.push_str(&format!("\n{} error(s).", stderr.lines().filter(|l| l.contains("error")).count())); }
                Ok(result)
            }
            Err(e) => Ok(format!("Diagnose error: {}", e)),
        }
    }

    pub fn cmd_fix(&mut self, query: &str) -> Result<String> {
        if query.is_empty() {
            return Ok("Usage: /fix <description of what to fix>".to_string());
        }
        let diag = Command::new("cargo").args(if Self::has_cargo_watch() { vec!["watch", "-x", "check", "--no-color"] } else { vec!["check", "--color", "never"] }).current_dir(&self.project_dir).output();
        let errors = match diag {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.success() { "No compilation errors.".to_string() } else { stderr.to_string() }
            }
            Err(_) => "Could not run cargo check.".to_string(),
        };
        let ctx = self.context.scan_workspace().ok();
        let ctx_summary = ctx.as_ref().map(|c| c.summary.clone()).unwrap_or_default();
        let user_msg = format!("I need to fix this issue: {}\n\nCompilation errors:\n{}\n\nPlease analyze and provide the fix.", query, errors);
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
            let output = Command::new("git").args(["commit", "-m", msg]).current_dir(&self.project_dir).output();
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
            let stats = Command::new("git").args(["diff", "--stat"]).current_dir(&self.project_dir).output()
                .ok().and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
                .unwrap_or_else(|| "Could not get diff".to_string());
            let diff = Command::new("git").args(["diff", "--no-color"]).current_dir(&self.project_dir).output()
                .ok().and_then(|o| if o.status.success() {
                    let d = String::from_utf8_lossy(&o.stdout);
                    Some(if d.len() > 2000 { format!("{}... (truncated)", &d[..2000]) } else { d.to_string() })
                } else { None })
                .unwrap_or_default();
            Ok(format!("Changes to commit:\n{}\n\n{}\n\nUse: /commit \"your message\"", stats, diff))
        }
    }

    pub fn cmd_status(&mut self) -> Result<String> {
        let ctx = self.context.scan_workspace().ok();
        let ctx_info = ctx.as_ref().map(|c| format!("{} files, {} lines", c.files.len(), c.total_lines))
            .unwrap_or_else(|| "not scanned".to_string());
        let has_checkpoint = if self.current_checkpoint.is_some() { "active" } else { "none" };
        Ok(format!(
            "LingShu CIL\n             Model: {}\n             Mode: {}\n             Project: {}\n             Context: {}\n             Tasks: {}\n             Checkpoint: {}\n             Log: {}",
            self.model.name, self.mode, self.project_dir.display(), ctx_info, self.tasks.len(), has_checkpoint, self.log_path
        ))
    }

    pub fn cmd_diff(&self) -> Result<String> {
        match self.patch_engine.review_diff() {
            Ok(review) => Ok(review),
            Err(e) => Ok(format!("Diff error: {}", e)),
        }
    }

    pub fn cmd_apply(&mut self) -> Result<String> {
        // Create checkpoint first
        let task_id = format!("apply_{}", chrono::Utc::now().timestamp());
        match self.checkpoint_manager.create_checkpoint(&task_id, "manual apply") {
            Ok(ck) => {
                self.current_checkpoint = Some(ck);
                println!("[Checkpoint created]");
            }
            Err(e) => eprintln!("[Checkpoint warning: {}]", e),
        }

        // Generate patch from workspace
        match self.patch_engine.generate_workspace_patch() {
            Ok(patch) => {
                self.current_patch = Some(patch.clone());
                let summary = patch.summary();
                Ok(format!("Patch applied:\n{}", summary))
            }
            Err(e) => Ok(format!("No changes to apply: {}", e)),
        }
    }

    pub fn cmd_rollback(&mut self) -> Result<String> {
        match &self.current_checkpoint {
            Some(ck) => {
                match self.checkpoint_manager.rollback(ck) {
                    Ok(msg) => {
                        self.current_checkpoint = None;
                        self.current_patch = None;
                        Ok(msg)
                    }
                    Err(e) => Ok(format!("Rollback failed: {}", e)),
                }
            }
            None => Ok("No checkpoint to rollback to. Use /apply first or run a task.".to_string()),
        }
    }

    // ── AI Helpers ──

    fn stream_to_string(&self, messages: &[LlmMessage]) -> Result<String, String> {
        let rx = llm::chat_stream(&self.model, messages, None)
            .map_err(|e| format!("LLM error: {}", e))?;
        collect_stream(rx)
    }

    /// Run an AI-powered coding task with full agent loop (LLM → Tool → LLM loop)
    /// Creates a git checkpoint before making changes, supports rollback.
    pub fn run_ai_task(&mut self, task_description: &str, verifier: Option<&crate::verifier::Verifier>) -> Result<String> {
        // Create git checkpoint before starting
        let task_id = format!("rt_{}", chrono::Utc::now().timestamp());
        if self.checkpoint_manager.is_git_repo() {
            if let Ok(ck) = self.checkpoint_manager.create_checkpoint(&task_id, task_description) {
                self.current_checkpoint = Some(ck);
            }
        }
        let ctx = self.context.scan_workspace().ok();
        let ctx_summary = ctx.as_ref().map(|c| c.summary.clone()).unwrap_or_default();
        let tools = self.tool_registry.to_definitions();

        let mut messages = vec![
            LlmMessage::system(&format!(
                "{} You are an AI coding assistant. Use the available tools to complete the task.\nProject: {} - {}",
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

        let mut final_output = String::new();
        let mut iteration = 0;
        const MAX_ITERATIONS: usize = 20;

        loop {
            if iteration >= MAX_ITERATIONS {
                final_output.push_str("\n[Max iterations reached. Task may be incomplete.]");
                break;
            }
            iteration += 1;

            // Step 1: Call LLM
            let rx = match llm::chat_stream(&self.model, &messages, Some(&tools)) {
                Ok(r) => r,
                Err(e) => return Ok(format!("LLM error: {}", e)),
            };

            // Step 2: Collect response and tool calls
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

            // Step 3: Sort tool calls
            let tool_calls: Vec<ToolCall> = {
                let mut tcs: Vec<_> = tool_calls_map.into_values().collect();
                tcs.sort_by_key(|tc| tc.id.as_str().len());
                tcs
            };

            // Step 4: Add assistant message
            if !content.is_empty() {
                final_output.push_str(&format!("\n[LLM] {}", &content[..std::cmp::min(content.len(), 500)]));
            }
            messages.push(LlmMessage::assistant(&content, if tool_calls.is_empty() { None } else { Some(tool_calls.clone()) }));

            // Step 5: If no tool calls, task is done
            if tool_calls.is_empty() {
                final_output.push_str(&format!("\n\n{}", content));
                break;
            }

            // Step 6: Execute each tool call
            for tc in &tool_calls {
                final_output.push_str(&format!("\n[Tool] {}...", &tc.name[..std::cmp::min(tc.name.len(), 30)]));
                let result = self.tool_registry.execute(
                    &tc.name, tc.arguments.clone(), &self.project_dir,
                );
                messages.push(LlmMessage::tool(&result.output, &tc.id));
            }

            // Step 7: Auto-verify if verifier is available
            if let Some(v) = verifier {
                if v.should_verify(&messages) {
                    let v_result = v.verify(&self.project_dir);
                    if !v_result.success {
                        final_output.push_str(&format!("\n[Verify] Failed: {} errors", v_result.errors.len()));
                        messages.push(LlmMessage::user(&format!(
                            "Verification failed after the changes. Please fix these issues:\n{}",
                            v_result.errors.join("\n")
                        )));
                    } else {
                        final_output.push_str("\n[Verify] Passed");
                    }
                }
            }
        }

        // Mark task complete
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Done;
        }

        Ok(final_output)
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
