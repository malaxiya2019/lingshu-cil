use crate::agent::CilRuntime;
use anyhow::Result;

/// Coding commands for LingShu CIL
/// Parses user input and delegates to CilRuntime tools
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
    Diagnose,
    Fix(String),
    Commit(String),
    Status,
    Exit,
    Invalid(String),
}

impl CodingCommand {
    /// Parse a command from user input
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
                let parts: Vec<&str> = arg.splitn(5, |c| c == '\n').collect();
                if parts.len() >= 3 {
                    CodingCommand::Edit {
                        path: parts[0].trim().to_string(),
                        old: parts[1].to_string(),
                        new: parts[2].to_string(),
                    }
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
            "diagnose" | "diag" | "check" => CodingCommand::Diagnose,
            "fix" => CodingCommand::Fix(arg.to_string()),
            "commit" | "cm" => CodingCommand::Commit(arg.to_string()),
            "status" | "st" => CodingCommand::Status,
            "exit" | "quit" | "q" => CodingCommand::Exit,
            _ => CodingCommand::Invalid(format!("Unknown command: /{}", cmd)),
        }
    }

    /// Execute the command and return output
    /// Only parses commands — delegates all logic to CilRuntime
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
            CodingCommand::Diagnose => cil.cmd_diagnose(),
            CodingCommand::Fix(query) => cil.cmd_fix(&query),
            CodingCommand::Commit(msg) => cil.cmd_commit(&msg),
            CodingCommand::Status => cil.cmd_status(),
            CodingCommand::Exit => { cil.should_exit = true; Ok("Bye!".to_string()) }
            CodingCommand::Invalid(msg) => Ok(format!("{}\nType /help for available commands.", msg)),
        }
    }
}

fn help_text() -> String {
    r#"
       LingShu CIL — AI Coding Assistant

 P0 — Core Coding Tools
  /project <dir>   Set project directory
  /open <file>     Open/read a file
  /search <pat>    Search code in project
  /edit <p> <o> <n>  Edit file (search-replace)
  /explain <q>     Explain code/ask AI
  /run <cmd>       Run a command (output only)
  /shell <cmd>     Interactive shell command
  /cargo <args>    Run cargo commands
  /git <args>      Run git commands
  /diagnose        Run cargo check diagnostics
  /fix <desc>      AI auto-fix compilation errors
  /commit <msg>    Git commit with AI message

 P1 — Task & Memory
  /task <desc>     Create/manage coding tasks
  /memory <k=v>    Store/recall context

 System
  /status          Show current state
  /help            Show this help
  /exit /quit      Exit CIL
"#.to_string()
}
