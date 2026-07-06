mod app;
mod commands;
mod context;
mod logging;
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
                        app.should_exit = true;
                        break;
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
