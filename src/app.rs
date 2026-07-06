use ratatui::prelude::Stylize;
use crate::commands::SlashCommand;
use crate::context::ContextEngine;
use crate::logging::Logger;
use crate::model::{Message, ModelConfig, PermissionMode};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;

const TIPS: &[&str] = &[
    "Tip: Use /model <name> to switch AI models",
    "Tip: Use /dir <path> to change workspace",
    "Tip: Use /yolo on to enable autonomous mode",
    "Tip: Use /tree to see workspace file tree",
    "Tip: Use /help for all available commands",
    "Tip: Tab-complete file paths in /dir commands",
    "Tip: Pipe output with 2> cil.log for logging",
];

#[derive(Debug)]
pub struct AppConfig {
    pub current_model: ModelConfig,
    pub permission_mode: PermissionMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            current_model: ModelConfig::builtins()
                .into_iter()
                .find(|m| m.name == "deepseek-coder")
                .unwrap_or_else(|| ModelConfig::builtins()[0].clone()),
            permission_mode: PermissionMode::FullContext,
        }
    }
}

pub struct App {
    pub messages: Vec<Message>,
    pub input: String,
    pub cursor_pos: usize,
    pub should_exit: bool,
    pub config: AppConfig,
    pub context_engine: ContextEngine,
    pub logger: Logger,
    pub scroll_offset: usize,
    pub is_streaming: bool,
    pub stream_buffer: String,
    pub tip_index: usize,
    pub input_history: Vec<String>,
    pub history_pos: Option<usize>,
    pub log_path: String,
}

impl App {
    pub fn new(workspace: PathBuf) -> Result<Self> {
        let logger = Logger::new("lingshu-cil")?;
        let log_path = logger.path().display().to_string();

        let mut context_engine = ContextEngine::new(workspace.clone());
        let _ = context_engine.scan_workspace();

        let mut app = Self {
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            should_exit: false,
            config: AppConfig::default(),
            context_engine,
            logger,
            scroll_offset: 0,
            is_streaming: false,
            stream_buffer: String::new(),
            tip_index: 0,
            input_history: Vec::new(),
            history_pos: None,
            log_path,
        };

        let ctx = app.context_engine.scan_workspace().ok();
        if let Some(c) = ctx {
            app.messages.push(Message::new(
                "system",
                &format!(
                    "LingShu CIL v0.2.1-ds ready.\n📁 Workspace: {}\n📄 {} files | {} lines\n🤖 Model: {} | Mode: {}",
                    c.working_dir.display(),
                    c.files.len(),
                    c.total_lines,
                    app.config.current_model.name,
                    app.config.permission_mode,
                ),
            ));
            app.logger.info("system", &c.summary);
        }

        Ok(app)
    }

    pub fn handle_input(&mut self) -> Result<()> {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return Ok(());
        }

        self.input_history.push(input.clone());
        self.history_pos = None;

        self.messages.push(Message::new("user", &input));
        self.logger.info("user", &input);

        if input.starts_with('/') {
            let cmd = SlashCommand::parse(&input);
            match cmd.execute(self) {
                Ok(Some(output)) => {
                    self.messages.push(Message::new("system", &output));
                }
                Ok(None) => {}
                Err(e) => {
                    self.messages
                        .push(Message::new("system", &format!("Error: {}", e)));
                }
            }
        } else {
            self.is_streaming = true;
            self.stream_buffer.clear();

            let ctx_prompt = self.context_engine.build_context_prompt(&input);
            let response = self.generate_response(&input, &ctx_prompt);

            self.messages.push(Message::new("assistant", &response));
            self.is_streaming = false;
            self.logger.info("assistant", &response);
        }

        self.input.clear();
        self.cursor_pos = 0;
        self.scroll_offset = 0;

        Ok(())
    }

    fn generate_response(&mut self, query: &str, _ctx_prompt: &str) -> String {
        let ctx = self.context_engine.scan_workspace().ok();
        let file_count = ctx.as_ref().map(|c| c.files.len()).unwrap_or(0);
        let model_name = &self.config.current_model.name;
        let mode = self.config.permission_mode.as_str();

        let mut response = String::new();

        if file_count > 0 {
            response.push_str(&format!(
                "🤖 **{}** (Mode: {})\n\n",
                model_name, mode
            ));

            if query.contains("file") || query.contains("code") || query.contains("struct") {
                response.push_str("I've scanned your workspace. Here's what I found:\n\n");
                if let Some(c) = &ctx {
                    for f in c.files.iter().take(5) {
                        response.push_str(&format!(
                            "  📄 `{}` ({} lines, {} KB)\n",
                            f.relative_path,
                            f.lines,
                            f.size_bytes / 1024
                        ));
                    }
                }
                response.push_str(&format!(
                    "\n[Context-aware analysis would go here based on query: {}]",
                    query
                ));
            } else {
                response.push_str(&format!(
                    "Context: {} files scanned in workspace.\n\n{}",
                    file_count,
                    self.simulate_reasoning(query)
                ));
            }
        } else {
            response.push_str(&format!(
                "🤖 **{}** (Mode: {})\n\n{}",
                model_name,
                mode,
                self.simulate_reasoning(query)
            ));
        }

        response
    }

    fn simulate_reasoning(&self, query: &str) -> String {
        let lower = query.to_lowercase();
        if lower.contains("hello") || lower.contains("hi") || lower.contains("你好") {
            return "Hello! I'm LingShu CIL, your context-aware AI assistant.\n\nI'm currently connected to your workspace. Try asking me questions about your code, or use /help to see what I can do.".to_string();
        }
        if lower.contains("docker") || lower.contains("dockerfile") {
            return "**Dockerfile Optimization Tips:**\n\n1. Use multi-stage builds to reduce image size\n2. Combine `RUN` commands to minimize layers\n3. Use `.dockerignore` to exclude unnecessary files\n4. Pin base image versions for reproducibility\n5. Use `--no-cache` for package managers\n\nI can analyze your specific Dockerfile if you reference it!".to_string();
        }
        if lower.contains("security") || lower.contains("漏洞") || lower.contains("vuln") {
            return "**Security Analysis:**\n\nI can help audit your code for:\n- SQL injection points\n- Hardcoded credentials or secrets\n- Unsafe deserialization\n- Path traversal vulnerabilities\n- Outdated dependency versions\n\nRun `/context` first to make sure your code is loaded, then ask specific questions!".to_string();
        }
        if lower.contains("rust") || lower.contains("编译") || lower.contains("build") {
            return "**Build Tips:**\n\nFor Rust projects in Termux:\n- Use `cargo build --release` for optimized builds\n- Check `~/.cargo/config.toml` for linker settings\n- Consider `CARGO_BUILD_JOBS=4` to limit parallel jobs\n- For Android NDK issues, try `rustup target add aarch64-linux-android`".to_string();
        }

        format!(
            "I understand your query: \"{}\"\n\nI'm a context-aware terminal AI assistant. I can:\n- Analyze your codebase\n- Answer questions about your project\n- Help with debugging and optimization\n- Provide security insights\n\nTry `/model deepseek-chat` to switch models, or `/dir <path>` to scan a different directory.",
            query
        )
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        if self.is_streaming {
            return Ok(());
        }

        match key.code {
            KeyCode::Enter => {
                self.handle_input()?;
            }
            KeyCode::Char(c) => {
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT {
                    self.input.insert(self.cursor_pos, c);
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.input.len() {
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.input.len();
            }
            KeyCode::Up => {
                if !self.input_history.is_empty() {
                    let pos = match self.history_pos {
                        Some(p) if p > 0 => p - 1,
                        Some(_) => 0,
                        None => self.input_history.len() - 1,
                    };
                    self.history_pos = Some(pos);
                    self.input = self.input_history[pos].clone();
                    self.cursor_pos = self.input.len();
                }
            }
            KeyCode::Down => {
                if let Some(pos) = self.history_pos {
                    if pos + 1 < self.input_history.len() {
                        let new_pos = pos + 1;
                        self.history_pos = Some(new_pos);
                        self.input = self.input_history[new_pos].clone();
                    } else {
                        self.history_pos = None;
                        self.input.clear();
                    }
                    self.cursor_pos = self.input.len();
                }
            }
            KeyCode::Tab => {
                if self.input.starts_with("/dir ") || self.input.starts_with("/cd ") {
                    let path_part = self.input
                        .trim_start_matches("/dir ")
                        .trim_start_matches("/cd ")
                        .trim();
                    if let Some(completed) = self.tab_complete_path(path_part) {
                        let prefix = if self.input.trim().starts_with("/dir") { "/dir " } else { "/cd " };
                        self.input = format!("{}{}", prefix, completed);
                        self.cursor_pos = self.input.len();
                    }
                }
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
            }
            KeyCode::Esc => {
                self.input.clear();
                self.cursor_pos = 0;
            }
            _ => {}
        }

        Ok(())
    }

    fn tab_complete_path(&self, partial: &str) -> Option<String> {
        let base = self.context_engine.workspace();
        let search_path = if partial.is_empty() || partial == "." {
            base.clone()
        } else if partial.starts_with('/') {
            PathBuf::from(partial)
        } else {
            base.join(partial)
        };

        let parent = search_path.parent()?;
        let prefix = search_path.file_name()?.to_string_lossy().to_string();

        let entries: Vec<_> = std::fs::read_dir(parent)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                name.starts_with(&prefix.to_lowercase())
            })
            .collect();

        if entries.len() == 1 {
            let entry = &entries[0];
            let name = entry.file_name().into_string().ok()?;
            let is_dir = entry.file_type().ok()?.is_dir();

            let full = if partial.is_empty() || partial == "." {
                name
            } else if partial.starts_with('/') {
                let p = parent.join(&name);
                p.to_string_lossy().to_string()
            } else {
                let partial_path = PathBuf::from(partial);
                let parent_dir = partial_path.parent()?;
                let parent_display = if parent_dir.to_string_lossy().is_empty() {
                    String::new()
                } else {
                    format!("{}/", parent_dir.display())
                };
                format!("{}{}{}", parent_display, name, if is_dir { "/" } else { "" })
            };
            Some(full)
        } else {
            None
        }
    }

    pub fn handle_mouse_event(&mut self, kind: MouseEventKind, _row: u16) {
        match kind {
            MouseEventKind::ScrollDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(3);
            }
            MouseEventKind::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            _ => {}
        }
    }

    pub fn advance_tip(&mut self) {
        self.tip_index = (self.tip_index + 1) % TIPS.len();
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let vertical = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ]);
        let [header_area, tip_area, chat_area, input_area, footer_area] = vertical.areas(area);

        frame.render_widget(self.render_header(), header_area);
        frame.render_widget(self.render_tip(), tip_area);
        frame.render_widget(self.render_chat(), chat_area);
        frame.render_widget(self.render_input(), input_area);

        let cursor_x = input_area.x + 2 + self.cursor_pos as u16;
        let cursor_y = input_area.y + 1;
        frame.set_cursor_position((cursor_x.min(area.right().saturating_sub(1)), cursor_y));

        frame.render_widget(self.render_footer(), footer_area);
    }

    fn render_header(&self) -> Paragraph<'static> {
        let model = &self.config.current_model.name;
        let dir = self.context_engine.workspace().display().to_string();
        let mode = self.config.permission_mode;

        let header_text = Text::from(
            Line::from(vec![
                Span::styled(" ╭─ ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    ">_ LingShu CIL",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" (v0.2.1-ds) ", Style::default().fg(Color::Gray)),
                Span::styled("│", Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled("model:", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!(" {} ", model),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                Span::styled("│", Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled("dir:", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!(" {} ", &dir),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled("│", Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled("mode:", Style::default().fg(Color::Gray)),
                match mode {
                    PermissionMode::Yolo => Span::styled(
                        format!(" {} ", mode),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    _ => Span::styled(
                        format!(" {} ", mode),
                        Style::default().fg(Color::Blue),
                    ),
                },
                Span::styled("\n ╰─", Style::default().fg(Color::Cyan)),
            ]),
        );

        Paragraph::new(header_text)
            .style(Style::default().bg(Color::Black))
    }

    fn render_tip(&self) -> Paragraph<'static> {
        let tip = TIPS[self.tip_index % TIPS.len()];

        Paragraph::new(Line::from(vec![
            Span::styled(" 💡 ", Style::default().fg(Color::Yellow)),
            Span::styled(tip, Style::default().fg(Color::DarkGray).italic()),
        ]))
        .style(Style::default().bg(Color::Black))
    }

    fn render_chat(&self) -> Paragraph<'static> {
        let mut lines = Vec::new();

        for msg in &self.messages {
            let style = if msg.is_user() {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if msg.is_system() {
                Style::default().fg(Color::DarkGray).italic()
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if msg.is_user() { "› " }
            else if msg.is_system() { "· " }
            else { "  " };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}[{}] ", prefix, msg.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                if msg.is_user() {
                    Span::styled("You: ", Style::default().fg(Color::Cyan))
                } else {
                    Span::styled("", Style::default())
                },
                Span::styled(msg.content.clone(), style),
            ]));

            lines.push(Line::from(""));
        }

        if self.is_streaming {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    "‧‧‧ thinking ‧‧‧",
                    Style::default().fg(Color::Green).italic(),
                ),
            ]));
        }

        if self.scroll_offset > 0 {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" ↑ {} more lines above ", self.scroll_offset),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(Color::Cyan))
                    .border_type(BorderType::Plain),
            )
            .style(Style::default().bg(Color::Black))
            .scroll((self.scroll_offset as u16, 0))
    }

    fn render_input(&self) -> Paragraph<'static> {
        let mode_indicator = match self.config.permission_mode {
            PermissionMode::FullContext => Span::styled("░", Style::default().fg(Color::Blue)),
            PermissionMode::SuggestOnly => Span::styled("░", Style::default().fg(Color::Yellow)),
            PermissionMode::Yolo => Span::styled("⚡", Style::default().fg(Color::Red)),
        };

        let input_content = if self.input.is_empty() {
            "Type a message or /help...".to_string()
        } else {
            self.input.clone()
        };

        let input_style = if self.input.is_empty() {
            Style::default().fg(Color::DarkGray).italic()
        } else {
            Style::default().fg(Color::White)
        };

        Paragraph::new(Line::from(vec![
            mode_indicator,
            Span::raw(" "),
            Span::styled(input_content, input_style),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .border_type(BorderType::Rounded),
        )
        .style(Style::default().bg(Color::Black))
    }

    fn render_footer(&self) -> Paragraph<'static> {
        let model = self.config.current_model.name.clone();
        let dir = self.context_engine.workspace().display().to_string();
        let mode = self.config.permission_mode;
        let msg_count = self.messages.len();
        let log_path = self.log_path.clone();

        let mode_style = if mode == PermissionMode::Yolo {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Blue)
        };

        let spans = vec![
            Span::styled(model, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" active · "),
            Span::styled(dir, Style::default().fg(Color::Yellow)),
            Span::raw(" · "),
            Span::styled(format!("{} msgs", msg_count), Style::default().fg(Color::DarkGray)),
            Span::raw(" · "),
            Span::styled(mode.to_string(), mode_style),
            Span::raw(" · "),
            Span::styled(log_path, Style::default().fg(Color::DarkGray)),
        ];

        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::Black))
            .alignment(Alignment::Left)
    }
}
