//! Simple markdown renderer for TUI with syntax highlighting
//!
//! Parses markdown text and produces styled ratatui Lines.
//! Uses `syntect` for code block syntax highlighting.

use ratatui::prelude::Stylize;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxSet, SyntaxReference};
use syntect::easy::HighlightLines;
use std::sync::OnceLock;

/// A rendered markdown block
#[derive(Debug)]
pub enum MarkdownBlock {
    /// Plain text line
    Paragraph(Line<'static>),
    /// Code block with syntax highlighting
    CodeBlock(Vec<Line<'static>>),
    /// Horizontal rule
    HorizontalRule,
    /// Section header
    Heading(u8, Line<'static>),
}

/// Lazy-initialized syntect resources
struct SyntaxHighlighter {
    ss: SyntaxSet,
    ts: ThemeSet,
}

static HIGHLIGHTER: OnceLock<SyntaxHighlighter> = OnceLock::new();

fn get_highlighter() -> &'static SyntaxHighlighter {
    HIGHLIGHTER.get_or_init(|| {
        SyntaxHighlighter {
            ss: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
        }
    })
}

/// Highlight a code block using syntect
fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let h = get_highlighter();
    let syntax: Option<&SyntaxReference> = if lang.is_empty() {
        None
    } else {
        h.ss.find_syntax_by_token(lang)
    };
    let syntax = syntax.unwrap_or_else(|| h.ss.find_syntax_plain_text());

    let theme = &h.ts.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut result = Vec::new();

    for line in code.lines() {
        match highlighter.highlight_line(line, &h.ss) {
            Ok(highlighted) => {
                let spans: Vec<Span<'static>> = highlighted
                    .into_iter()
                    .map(|(style, text)| {
                        let fg = style.foreground;
                        let ratatui_style = Style::default().fg(Color::Rgb(
                            fg.r.saturating_mul(255) as u8,
                            fg.g.saturating_mul(255) as u8,
                            fg.b.saturating_mul(255) as u8,
                        ));
                        Span::styled(text.to_string(), ratatui_style)
                    })
                    .collect();
                result.push(Line::from(spans));
            }
            Err(_) => {
                result.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Green),
                )));
            }
        }
    }

    result
}

/// Render plain text with inline markdown styling
fn render_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Bold: **text**
        if let Some(start) = remaining.find("**") {
            let before = &remaining[..start];
            if !before.is_empty() {
                spans.push(Span::raw(before.to_string()));
            }
            let after = &remaining[start + 2..];
            if let Some(end) = after.find("**") {
                let bold_text = &after[..end];
                spans.push(Span::styled(
                    bold_text.to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
                remaining = &after[end + 2..];
            } else {
                spans.push(Span::raw("**".to_string()));
                remaining = after;
            }
        }
        // Italic: *text* (but not **)
        else if let Some(start) = remaining.find('*') {
            // Check it's not **
            if remaining[start..].starts_with("**") {
                spans.push(Span::raw("*".to_string()));
                remaining = &remaining[start + 1..];
            } else {
                let before = &remaining[..start];
                if !before.is_empty() {
                    spans.push(Span::raw(before.to_string()));
                }
                let after = &remaining[start + 1..];
                if let Some(end) = after.find('*') {
                    let italic_text = &after[..end];
                    spans.push(Span::styled(
                        italic_text.to_string(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::ITALIC),
                    ));
                    remaining = &after[end + 1..];
                } else {
                    spans.push(Span::raw("*".to_string()));
                    remaining = after;
                }
            }
        }
        // Inline code: `code`
        else if let Some(start) = remaining.find('`') {
            let before = &remaining[..start];
            if !before.is_empty() {
                spans.push(Span::raw(before.to_string()));
            }
            let after = &remaining[start + 1..];
            if let Some(end) = after.find('`') {
                let code_text = &after[..end];
                spans.push(Span::styled(
                    code_text.to_string(),
                    Style::default()
                        .fg(Color::Cyan)
                        .bg(Color::Rgb(30, 30, 30)),
                ));
                remaining = &after[end + 1..];
            } else {
                spans.push(Span::raw("`".to_string()));
                remaining = after;
            }
        } else {
            spans.push(Span::raw(remaining.to_string()));
            break;
        }
    }

    spans
}

/// Parse markdown text and return rendered blocks
pub fn render_markdown(text: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Code block
        if line.trim_start().starts_with("```") {
            let lang = line.trim_start().trim_start_matches("```").trim().to_string();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            // Skip closing ```
            if i < lines.len() {
                i += 1;
            }
            let code = code_lines.join("\n");
            let highlighted = highlight_code(&code, &lang);
            blocks.push(MarkdownBlock::CodeBlock(highlighted));
            continue;
        }

        // Horizontal rule
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "___" || trimmed == "***" {
            blocks.push(MarkdownBlock::HorizontalRule);
            i += 1;
            continue;
        }

        // Headers
        if line.starts_with('#') {
            let level = line.chars().take_while(|&c| c == '#').count() as u8;
            let content = line[level as usize..].trim();
            let rendered = render_inline(content);
            let header_style = match level {
                1 => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                2 => Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD),
                3 => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(Color::Cyan),
            };
            let styled_line = Line::from(
                rendered
                    .into_iter()
                    .map(|s| Span::styled(s.content.clone(), header_style))
                    .collect::<Vec<Span>>(),
            );
            blocks.push(MarkdownBlock::Heading(level, styled_line));
            i += 1;
            continue;
        }

        // Blockquote
        if trimmed.starts_with('>') {
            let content = trimmed.trim_start_matches('>').trim();
            let rendered = render_inline(content);
            let styled: Vec<Span> = rendered
                .into_iter()
                .map(|s| {
                    Span::styled(
                        s.content,
                        Style::default().fg(Color::DarkGray).italic(),
                    )
                })
                .collect();
            let mut spans = vec![Span::styled("▎", Style::default().fg(Color::Gray))];
            spans.extend(styled);
            blocks.push(MarkdownBlock::Paragraph(Line::from(spans)));
            i += 1;
            continue;
        }

        // Regular paragraph (including lists)
        if !line.is_empty() {
            let rendered = render_inline(line);
            blocks.push(MarkdownBlock::Paragraph(Line::from(rendered)));
        } else {
            // Empty line - add a blank paragraph for spacing
            blocks.push(MarkdownBlock::Paragraph(Line::from("")));
        }

        i += 1;
    }

    blocks
}

/// Convert MarkdownBlocks to ratatui Lines for display
pub fn blocks_to_lines<'a>(blocks: &'a [MarkdownBlock]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for block in blocks {
        match block {
            MarkdownBlock::Paragraph(line) => {
                lines.push(line.clone());
            }
            MarkdownBlock::Heading(_, line) => {
                lines.push(line.clone());
            }
            MarkdownBlock::CodeBlock(code_lines) => {
                // Add subtle background for code blocks
                lines.push(Line::from(Span::styled(
                    " ┌─ code ───────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
                for cl in code_lines {
                    // Create a modifiable line from the code
                    let mut line_with_bg = Vec::new();
                    for span in &cl.spans {
                        line_with_bg.push(Span::styled(
                            span.content.clone(),
                            span.style.clone(),
                        ));
                    }
                    lines.push(Line::from(line_with_bg));
                }
                lines.push(Line::from(Span::styled(
                    " └──────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            MarkdownBlock::HorizontalRule => {
                lines.push(Line::from(Span::styled(
                    " ────────────────────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    lines
}
