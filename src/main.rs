mod app;
mod commands;
mod context;
mod llm;
mod markdown;
mod logging;
mod mcp;
mod model;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{event, execute};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self};
use std::path::PathBuf;
use std::time::Duration;

use app::App;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // ── MCP Server Mode ──
    if args.len() > 1 && (args[1] == "--mcp" || args[1] == "mcp") {
        eprintln!("[lingshu-cil] Starting MCP server on stdio transport...");
        eprintln!("[lingshu-cil] Resources: deepseek://models, deepseek://config, deepseek://usage");
        let server = mcp::McpServer::new();
        return server.run();
    }

    // ── Print version info ──
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-v") {
        println!("lingshu-cil v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // ── Print usage ──
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("LingShu CIL v{}", env!("CARGO_PKG_VERSION"));
        println!();
        println!("USAGE:");
        println!("  lingshu-cil [directory]        Start TUI in the given directory");
        println!("  lingshu-cil --mcp               Start MCP server on stdio transport");
        println!("  lingshu-cil --version / -v      Print version");
        println!("  lingshu-cil --help / -h         Print this help");
        println!();
        println!("MCP RESOURCES:");
        println!("  deepseek://models   Model catalog with V3.2 pricing");
        println!("  deepseek://config   Server configuration (masked)");
        println!("  deepseek://usage    Real-time usage & session metrics");
        println!();
        println!("ENVIRONMENT:");
        println!("  DEEPSEEK_API_KEY    API key for DeepSeek models");
        println!("  OPENAI_API_KEY      API key for OpenAI models");
        println!("  ANTHROPIC_API_KEY   API key for Anthropic models");
        println!("  QWEN_API_KEY        API key for Qwen (DashScope) models");
        println!("  GEMINI_API_KEY      API key for Gemini models");
        println!("  MOONSHOT_API_KEY    API key for Moonshot models");
        return Ok(());
    }

    // ── TUI Mode (default) ──
    let workspace = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let result = run_tui(workspace);

    let _ = disable_raw_mode();
    let _ = execute!(io::stderr(), LeaveAlternateScreen);

    result
}

fn run_tui(workspace: PathBuf) -> Result<()> {
    let mut stderr = io::stderr();
    enable_raw_mode()?;
    execute!(stderr, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new(workspace)?;
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();

    while !app.should_exit {
        terminal.draw(|frame| app.render(frame))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                        if app.is_streaming {
                            app.cancel_streaming("interrupted by user");
                        } else {
                            app.should_exit = true;
                            break;
                        }
                    }
                    app.handle_key_event(key)?;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse_event(mouse.kind, mouse.row);
                }
                Event::Resize(_w, _h) => {}
                _ => {}
            }
        }

        // Check for streaming updates
        app.poll_stream();

        if last_tick.elapsed() >= Duration::from_secs(5) {
            app.advance_tip();
            last_tick = std::time::Instant::now();
        }
    }

    terminal.clear()?;
    disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen)?;

    println!(
        "LingShu CIL session ended. {} messages logged.",
        app.messages.len()
    );

    Ok(())
}
