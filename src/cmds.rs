use crate::agent::CilRuntime;
use crate::task::TaskRecord;
use anyhow::Result;

/// Coding commands for LingShu CIL
#[derive(Debug)]
pub enum CodingCommand {
    Help,
    Project(String),
    Open(String),
    Search(String),
    Edit { path: String, old: String, new: String },
    Explain(String),
    Run(String),
    Shell(String),
    Cargo(String),
    Git(String),
    Memory(String),
    Task(String),
    RunTask(String),
    Resume(String),
    Diagnose,
    Fix(String),
    Commit(String),
    Status,
    Exit,
    Invalid(String),
}

impl CodingCommand {
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        if !input.starts_with('/') {
            return CodingCommand::Invalid(input.to_string());
        }
        let rest = &input[1..];
        let parts: Vec<&str> = rest.splitn(2, |c: char| c.is_whitespace()).collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd.as_str() {
            "help" | "h" | "?" => CodingCommand::Help,
            "project" | "p" | "dir" | "cd" => CodingCommand::Project(arg.to_string()),
            "open" | "o" => CodingCommand::Open(arg.to_string()),
            "search" | "s" | "grep" => CodingCommand::Search(arg.to_string()),
            "edit" | "e" => {
                let parts: Vec<&str> = arg.splitn(5, '\n').collect();
                if parts.len() >= 3 {
                    CodingCommand::Edit { path: parts[0].trim().to_string(), old: parts[1].to_string(), new: parts[2].to_string() }
                } else {
                    CodingCommand::Invalid("Usage: /edit <path> <old_string> +++ <new_string>".to_string())
                }
            }
            "explain" | "ex" => CodingCommand::Explain(arg.to_string()),
            "run" | "r" => CodingCommand::Run(arg.to_string()),
            "shell" | "sh" | "!" => CodingCommand::Shell(arg.to_string()),
            "cargo" | "c" => CodingCommand::Cargo(arg.to_string()),
            "git" | "g" => CodingCommand::Git(arg.to_string()),
            "memory" | "mem" => CodingCommand::Memory(arg.to_string()),
            "task" | "t" => CodingCommand::Task(arg.to_string()),
            "run-task" | "rt" => CodingCommand::RunTask(arg.to_string()),
            "resume" | "load" => CodingCommand::Resume(arg.to_string()),
            "diagnose" | "diag" | "check" => CodingCommand::Diagnose,
            "fix" => CodingCommand::Fix(arg.to_string()),
            "commit" | "cm" => CodingCommand::Commit(arg.to_string()),
            "status" | "st" => CodingCommand::Status,
            "exit" | "quit" | "q" => CodingCommand::Exit,
            _ => CodingCommand::Invalid(format!("Unknown command: /{}", cmd)),
        }
    }

    pub fn execute(self, cil: &mut CilRuntime) -> Result<String> {
        match self {
            CodingCommand::Help => Ok(help_text()),
            CodingCommand::Project(path) => cil.cmd_project(&path),
            CodingCommand::Open(path) => cil.cmd_open(&path),
            CodingCommand::Search(pattern) => cil.cmd_search(&pattern),
            CodingCommand::Edit { path, old, new } => cil.cmd_edit(&path, &old, &new),
            CodingCommand::Explain(query) => cil.cmd_explain(&query),
            CodingCommand::Run(cmd) => cil.cmd_run(&cmd),
            CodingCommand::Shell(cmd) => cil.cmd_shell(&cmd),
            CodingCommand::Cargo(args) => cil.cmd_cargo(&args),
            CodingCommand::Git(args) => cil.cmd_git(&args),
            CodingCommand::Memory(args) => cil.cmd_memory(&args),
            CodingCommand::Task(args) => cil.cmd_task(&args),
            CodingCommand::RunTask(desc) => cmd_run_task(cil, &desc),
            CodingCommand::Resume(id) => cmd_resume(cil, &id),
            CodingCommand::Diagnose => cil.cmd_diagnose(),
            CodingCommand::Fix(query) => cil.cmd_fix(&query),
            CodingCommand::Commit(msg) => cil.cmd_commit(&msg),
            CodingCommand::Status => cil.cmd_status(),
            CodingCommand::Exit => { cil.should_exit = true; Ok("Bye!".to_string()) }
            CodingCommand::Invalid(msg) => Ok(format!("{}\nType /help for available commands.", msg)),
        }
    }
}

fn cmd_run_task(cil: &mut CilRuntime, description: &str) -> Result<String> {
    if description.is_empty() {
        return Ok("Usage: /run-task <task description>\n\nExample: /run-task fix all cargo warnings in the project".to_string());
    }

    println!("\n=== Agent Task: {} ===\n", description);

    let verifier = crate::verifier::Verifier::new();
    let result = cil.run_ai_task(description, Some(&verifier))?;

    // Save task record
    if let Some(task) = cil.tasks.last() {
        let record = TaskRecord::from_task(task, &cil.project_dir.display().to_string());
        match record.save() {
            Ok(path) => println!("\n[Session saved to {}]", path),
            Err(e) => eprintln!("\n[Session save error: {}]", e),
        }
    }

    Ok(result)
}

fn cmd_resume(cil: &mut CilRuntime, id: &str) -> Result<String> {
    if id.is_empty() {
        // List available sessions
        let sessions = TaskRecord::list_all().unwrap_or_default();
        if sessions.is_empty() {
            return Ok("No saved sessions found in ~/.lingshu/sessions/".to_string());
        }
        let mut output = "Saved sessions:\n".to_string();
        for s in sessions {
            if let Ok(record) = TaskRecord::load(&s) {
                output.push_str(&format!("  {} | {} | {}\n", record.id, record.project, record.description));
            }
        }
        output.push_str("\nUse: /resume <id> to restore a session");
        return Ok(output);
    }

    match TaskRecord::load(id) {
        Ok(record) => {
            // Restore project directory
            let project = std::path::PathBuf::from(&record.project);
            if project.exists() {
                cil.project_dir = project;
                cil.context.set_workspace(cil.project_dir.clone());
                let _ = cil.context.scan_workspace();
            }
            Ok(format!(
                "Resumed session: {}\n  Project: {}\n  Task: {}\n  Status: {}\n  Created: {}",
                record.id, record.project, record.description, record.status, record.created_at
            ))
        }
        Err(e) => Ok(format!("Cannot load session {}: {}", id, e)),
    }
}

fn help_text() -> String {
    r#"
       LingShu CIL — AI Coding Assistant

 Agent
  /run-task <desc>  Run AI coding task with tool loop
  /resume [id]      Resume a saved task session

 P0 — Core Tools
  /project <dir>    Set project directory
  /open <file>      Open/read a file
  /search <pat>     Search code in project
  /edit <p> <o> <n> Edit file (search-replace)
  /explain <q>      Explain code/ask AI
  /run <cmd>        Run a command
  /shell <cmd>      Interactive shell command
  /cargo <args>     Run cargo commands
  /git <args>       Run git commands
  /diagnose         Run cargo check diagnostics
  /fix <desc>       AI auto-fix compilation errors
  /commit <msg>     Git commit with AI message

 P1 — Task & Memory
  /task <desc>      Create/manage coding tasks
  /memory <k=v>     Store/recall context

 System
  /status           Show current state
  /help             Show this help
  /exit /quit       Exit CIL
"#.to_string()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_help() {
        assert!(matches!(CodingCommand::parse("/help"), CodingCommand::Help));
        assert!(matches!(CodingCommand::parse("/h"), CodingCommand::Help));
        assert!(matches!(CodingCommand::parse("/?"), CodingCommand::Help));
    }

    #[test]
    fn test_parse_exit() {
        assert!(matches!(CodingCommand::parse("/exit"), CodingCommand::Exit));
        assert!(matches!(CodingCommand::parse("/quit"), CodingCommand::Exit));
        assert!(matches!(CodingCommand::parse("/q"), CodingCommand::Exit));
    }

    #[test]
    fn test_parse_open() {
        assert!(matches!(CodingCommand::parse("/open src/main.rs"), CodingCommand::Open(_)));
        assert!(matches!(CodingCommand::parse("/o src/main.rs"), CodingCommand::Open(_)));
    }

    #[test]
    fn test_parse_search() {
        assert!(matches!(CodingCommand::parse("/search foo"), CodingCommand::Search(_)));
        assert!(matches!(CodingCommand::parse("/s foo"), CodingCommand::Search(_)));
        assert!(matches!(CodingCommand::parse("/grep foo"), CodingCommand::Search(_)));
    }

    #[test]
    fn test_parse_run() {
        assert!(matches!(CodingCommand::parse("/run cargo check"), CodingCommand::Run(_)));
        assert!(matches!(CodingCommand::parse("/r cargo check"), CodingCommand::Run(_)));
    }

    #[test]
    fn test_parse_shell() {
        assert!(matches!(CodingCommand::parse("/shell ls"), CodingCommand::Shell(_)));
        assert!(matches!(CodingCommand::parse("/sh ls"), CodingCommand::Shell(_)));
        assert!(matches!(CodingCommand::parse("/! ls"), CodingCommand::Shell(_)));
    }

    #[test]
    fn test_parse_cargo() {
        assert!(matches!(CodingCommand::parse("/cargo check"), CodingCommand::Cargo(_)));
        assert!(matches!(CodingCommand::parse("/c check"), CodingCommand::Cargo(_)));
    }

    #[test]
    fn test_parse_git() {
        assert!(matches!(CodingCommand::parse("/git status"), CodingCommand::Git(_)));
        assert!(matches!(CodingCommand::parse("/g status"), CodingCommand::Git(_)));
    }

    #[test]
    fn test_parse_diagnose() {
        assert!(matches!(CodingCommand::parse("/diagnose"), CodingCommand::Diagnose));
        assert!(matches!(CodingCommand::parse("/diag"), CodingCommand::Diagnose));
        assert!(matches!(CodingCommand::parse("/check"), CodingCommand::Diagnose));
    }

    #[test]
    fn test_parse_edit() {
        // /edit expects newlines as separators: /edit <path>\n<old>\n<new>
        match CodingCommand::parse("/edit src/main.rs\nfn main()\nfn test()") {
            CodingCommand::Edit { path, old, new } => {
                assert_eq!(path, "src/main.rs");
                assert_eq!(old, "fn main()");
                assert_eq!(new, "fn test()");
            }
            _ => panic!("Expected Edit command"),
        }
    }

    #[test]
    fn test_parse_run_task() {
        assert!(matches!(CodingCommand::parse("/run-task fix warnings"), CodingCommand::RunTask(_)));
        assert!(matches!(CodingCommand::parse("/rt fix warnings"), CodingCommand::RunTask(_)));
    }

    #[test]
    fn test_parse_resume() {
        assert!(matches!(CodingCommand::parse("/resume task_123"), CodingCommand::Resume(_)));
        assert!(matches!(CodingCommand::parse("/load task_123"), CodingCommand::Resume(_)));
    }

    #[test]
    fn test_parse_invalid() {
        assert!(matches!(CodingCommand::parse("hello"), CodingCommand::Invalid(_)));
        assert!(matches!(CodingCommand::parse("/unknown"), CodingCommand::Invalid(_)));
    }

    #[test]
    fn test_parse_project() {
        assert!(matches!(CodingCommand::parse("/project /tmp"), CodingCommand::Project(_)));
        assert!(matches!(CodingCommand::parse("/p /tmp"), CodingCommand::Project(_)));
        assert!(matches!(CodingCommand::parse("/cd /tmp"), CodingCommand::Project(_)));
    }

    #[test]
    fn test_parse_status() {
        assert!(matches!(CodingCommand::parse("/status"), CodingCommand::Status));
        assert!(matches!(CodingCommand::parse("/st"), CodingCommand::Status));
    }

    #[test]
    fn test_parse_task() {
        assert!(matches!(CodingCommand::parse("/task fix bug"), CodingCommand::Task(_)));
        assert!(matches!(CodingCommand::parse("/t fix bug"), CodingCommand::Task(_)));
    }

    #[test]
    fn test_parse_memory() {
        assert!(matches!(CodingCommand::parse("/memory key=value"), CodingCommand::Memory(_)));
    }

    #[test]
    fn test_parse_commit() {
        assert!(matches!(CodingCommand::parse("/commit fix bug"), CodingCommand::Commit(_)));
    }

    #[test]
    fn test_parse_explain() {
        assert!(matches!(CodingCommand::parse("/explain code"), CodingCommand::Explain(_)));
        assert!(matches!(CodingCommand::parse("/ex code"), CodingCommand::Explain(_)));
    }

    #[test]
    fn test_parse_fix() {
        assert!(matches!(CodingCommand::parse("/fix error"), CodingCommand::Fix(_)));
    }

    #[test]
    fn test_parse_empty_edit() {
        match CodingCommand::parse("/edit") {
            CodingCommand::Invalid(msg) => assert!(msg.contains("Usage")),
            _ => panic!("Expected Invalid for /edit without args"),
        }
    }
}
