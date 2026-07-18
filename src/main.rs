#![allow(dead_code)]
mod agent;
mod cmds;
mod context;
mod llm;
mod logging;
mod mcp;
mod model;
mod session;
mod task;
mod tools;
mod verifier;
mod patch;
mod checkpoint;
mod context_selector;

use anyhow::Result;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use agent::CilRuntime;
use cmds::CodingCommand;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // ── MCP Server Mode ──
    if args.len() > 1 && (args[1] == "--mcp" || args[1] == "mcp") {
        eprintln!("[lingshu-cil] Starting MCP server on stdio transport...");
        let server = mcp::McpServer::new();
        return server.run();
    }

    // ── Print version ──
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-v") {
        println!("lingshu-cil v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // ── Direct command mode ──
    if args.len() > 2 && (args[1] == "--cmd" || args[1] == "-c") {
        let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut cil = CilRuntime::new(workspace)?;
        let cmd = CodingCommand::parse(&args[2..].join(" "));
        let output = cmd.execute(&mut cil)?;
        println!("{}", output);
        return Ok(());
    }

    // ── Interactive REPL Mode ──
    let workspace = if args.len() > 1 && !args[1].starts_with('-') {
        PathBuf::from(&args[1])
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let mut cil = CilRuntime::new(workspace)?;

    println!("╔══════════════════════════════════════════╗");
    println!("║    LingShu CIL v{} — AI Coding Assistant  ║", env!("CARGO_PKG_VERSION"));
    println!("║    Type /help for commands               ║");
    println!("║    Type /exit to quit                    ║");
    println!("╚══════════════════════════════════════════╝");
    println!(" Project: {}", cil.project_dir.display());
    println!(" Model: {}", cil.model.name);
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    while !cil.should_exit {
        print!("> ");
        stdout.flush()?;

        let mut input = String::new();
        match stdin.lock().read_line(&mut input) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => { eprintln!("Input error: {}", e); break; }
        }

        let input = input.trim();
        if input.is_empty() { continue; }

        let cmd = CodingCommand::parse(input);
        match cmd.execute(&mut cil) {
            Ok(output) => println!("{}", output),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    println!("\nSession ended. {} task(s) recorded.", cil.tasks.len());
    Ok(())
}
