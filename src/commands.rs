use crate::app::App;
use crate::model::PermissionMode;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug)]
pub enum SlashCommand {
    Help,
    Model(String),
    Dir(String),
    Yolo(String),
    Context,
    Tree,
    Clear,
    Save,
    Load(String),
    Exit,
    Env,
    Status,
    Unknown(String),
}

impl SlashCommand {
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return SlashCommand::Unknown(trimmed.to_string());
        }

        let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd.as_str() {
            "/help" | "/h" | "/?" => SlashCommand::Help,
            "/model" | "/m" => SlashCommand::Model(arg.to_string()),
            "/dir" | "/cd" | "/d" => SlashCommand::Dir(arg.to_string()),
            "/yolo" | "/permissions" | "/mode" => SlashCommand::Yolo(arg.to_string()),
            "/context" | "/ctx" => SlashCommand::Context,
            "/tree" | "/ls" => SlashCommand::Tree,
            "/clear" | "/cls" => SlashCommand::Clear,
            "/save" | "/export" => SlashCommand::Save,
            "/load" | "/import" => SlashCommand::Load(arg.to_string()),
            "/exit" | "/quit" | "/q" => SlashCommand::Exit,
            "/env" => SlashCommand::Env,
            "/status" => SlashCommand::Status,
            _ => SlashCommand::Unknown(trimmed.to_string()),
        }
    }

    pub fn execute(self, app: &mut App) -> Result<Option<String>> {
        match self {
            SlashCommand::Help => {
                let help = r#"╔══════════════════════════════════════════╗
║           LingShu CIL Commands          ║
╠══════════════════════════════════════════╣
║ /help  /h  /?   Show this help          ║
║ /model <name>   Switch AI model         ║
║ /dir   <path>   Set workspace directory ║
║ /yolo  on|off   Toggle permission mode  ║
║ /context /ctx   Show workspace context   ║
║ /tree  /ls      Show file tree          ║
║ /clear /cls     Clear conversation      ║
║ /save  /export  Export conversation     ║
║ /load  /import  Import conversation     ║
║ /status         Show current status     ║
║ /env            Show environment info   ║
║ /exit  /quit /q Exit CIL                ║
╚══════════════════════════════════════════╝"#;
                Ok(Some(help.to_string()))
            }

            SlashCommand::Model(name) => {
                if name.is_empty() {
                    let current = &app.config.current_model;
                    let builtins = crate::model::ModelConfig::builtins();
                    let models: Vec<String> = builtins.iter().map(|m| m.name.clone()).collect();
                    return Ok(Some(format!(
                        "Current model: {}\nAvailable: {}",
                        current,
                        models.join(", ")
                    )));
                }
                let builtins = crate::model::ModelConfig::builtins();
                if let Some(m) = builtins.into_iter().find(|m| m.name == name) {
                    app.config.current_model = m;
                    app.logger.info("model", &format!("Switched to model: {}", name));
                    Ok(Some(format!("✓ Switched to model: {}", name)))
                } else if name.contains('/') || name.contains(':') {
                    app.config.current_model.name = name.clone();
                    app.logger.info("model", &format!("Switched to custom model: {}", name));
                    Ok(Some(format!("✓ Set custom model: {}", name)))
                } else {
                    Ok(Some(format!(
                        "✗ Unknown model: {}\nAvailable: deepseek-chat, deepseek-coder, qwen-plus, qwen-turbo, gpt-4o, claude-3-sonnet, gemini-2.0-flash, gemini-2.5-pro, moonshot-v1, moonshot-v1-32k",
                        name
                    )))
                }
            }

            SlashCommand::Dir(path) => {
                let target = if path.is_empty() {
                    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
                } else {
                    let p = PathBuf::from(shellexpand(&path));
                    if p.is_absolute() {
                        p
                    } else {
                        let base = app.context_engine.workspace().clone();
                        base.join(&p)
                    }
                };

                if target.exists() {
                    app.context_engine.set_workspace(target.clone());
                    app.logger.info("dir", &format!("Changed directory to: {}", target.display()));
                    match app.context_engine.scan_workspace() {
                        Ok(ctx) => {
                            app.logger.info("context", &ctx.summary);
                            Ok(Some(format!(
                                "✓ Workspace: {}\n  {} files, {} lines",
                                target.display(),
                                ctx.files.len(),
                                ctx.total_lines
                            )))
                        }
                        Err(e) => Ok(Some(format!(
                            "✓ Workspace: {}\n  (scan error: {})",
                            target.display(),
                            e
                        ))),
                    }
                } else {
                    Ok(Some(format!("✗ Directory not found: {}", target.display())))
                }
            }

            SlashCommand::Yolo(mode) => {
                let new_mode = match mode.to_lowercase().as_str() {
                    "on" | "true" | "yolo" | "1" => PermissionMode::Yolo,
                    "suggest" => PermissionMode::SuggestOnly,
                    "off" | "false" | "context" | "0" => PermissionMode::FullContext,
                    "" => app.config.permission_mode.next(),
                    _ => return Ok(Some(format!(
                        "✗ Unknown mode: {}. Use: on|off|suggest",
                        mode
                    ))),
                };
                app.config.permission_mode = new_mode;
                app.logger.info("yolo", &format!("Permission mode: {}", new_mode));
                Ok(Some(format!("✓ Permission mode: {}", new_mode)))
            }

            SlashCommand::Context => {
                let ctx = app.context_engine.scan_workspace()?;
                Ok(Some(format!(
                    "📁 {}\n  {} files | {} total lines\n  Depth: {} levels",
                    ctx.working_dir.display(),
                    ctx.files.len(),
                    ctx.total_lines,
                    8,
                )))
            }

            SlashCommand::Tree => {
                let tree = app.context_engine.tree_view();
                Ok(Some(tree))
            }

            SlashCommand::Clear => {
                app.messages.clear();
                app.input.clear();
                app.scroll_offset = 0;
                app.logger.info("system", "Conversation cleared");
                Ok(Some("Conversation cleared.".to_string()))
            }

            SlashCommand::Save => {
                let path = app.export_conversation();
                match path {
                    Ok(p) => Ok(Some(format!("✓ Conversation saved to: {}", p))),
                    Err(e) => Ok(Some(format!("✗ Failed to save: {}", e))),
                }
            }

            SlashCommand::Load(path_arg) => {
                let path = if path_arg.is_empty() {
                    app.import_conversation()
                } else {
                    app.import_conversation_from(&path_arg)
                };
                match path {
                    Ok(count) => Ok(Some(format!("✓ Loaded {} messages from conversation.", count))),
                    Err(e) => Ok(Some(format!("✗ Failed to load: {}", e))),
                }
            }

            SlashCommand::Exit => {
                app.should_exit = true;
                Ok(None)
            }

            SlashCommand::Env => {
                let info = format!(
                    "Shell: {}\nUser: {}\nHome: {}\nCWD: {}\nOS: Android/Termux\nRust: {}",
                    std::env::var("SHELL").unwrap_or_default(),
                    std::env::var("USER").unwrap_or_default(),
                    dirs::home_dir().map(|p| p.display().to_string()).unwrap_or_default(),
                    app.context_engine.workspace().display(),
                    "1.96.1",
                );
                Ok(Some(info))
            }

            SlashCommand::Status => {
                let ctx = app.context_engine.scan_workspace().ok();
                let ctx_info = ctx
                    .map(|c| format!("{} files, {} lines", c.files.len(), c.total_lines))
                    .unwrap_or_else(|| "not scanned".to_string());
                Ok(Some(format!(
                    "Model: {}\nMode: {}\nWorkspace: {}\nContext: {}\nMessages: {}",
                    app.config.current_model.name,
                    app.config.permission_mode,
                    app.context_engine.workspace().display(),
                    ctx_info,
                    app.messages.len(),
                )))
            }

            SlashCommand::Unknown(raw) => {
                if raw.starts_with('/') {
                    Ok(Some(format!(
                        "✗ Unknown command: {}\n  Type /help for available commands.",
                        raw
                    )))
                } else {
                    Ok(None)
                }
            }
        }
    }
}

fn shellexpand(s: &str) -> String {
    if s.starts_with("~/") {
        dirs::home_dir()
            .map(|h| h.join(&s[2..]).display().to_string())
            .unwrap_or_else(|| s.to_string())
    } else if s == "~" {
        dirs::home_dir()
            .map(|h| h.display().to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}
