use ratatui::prelude::*;

use super::super::theme::Theme;

pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();

        if trimmed.starts_with("```") {
            if in_code_block {
                in_code_block = false;
                code_lang.clear();
            } else {
                in_code_block = true;
                code_lang = trimmed.trim_start_matches("```").to_string();
                lines.push(Line::from(Span::styled(
                    format!(" ── {} ──", if code_lang.is_empty() { "code" } else { &code_lang }),
                    Style::default().fg(Theme::PURPLE).add_modifier(Modifier::ITALIC),
                )));
            }
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                format!("  {}", raw_line),
                Style::default().fg(Theme::ACCENT_BRIGHT).bg(Color::Rgb(25, 25, 25)),
            )));
            continue;
        }

        if let Some(heading) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                heading.to_string(),
                Style::default().fg(Theme::ACCENT_BRIGHT).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(heading) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                heading.to_string(),
                Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED),
            )));
        } else if let Some(heading) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                heading.to_string(),
                Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED),
            )));
        } else if let Some(quote) = trimmed.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(Theme::PURPLE)),
                Span::styled(quote.to_string(), Style::default().fg(Theme::MUTED).add_modifier(Modifier::ITALIC)),
            ]));
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            let styled = inline_format(item);
            let mut spans = vec![Span::styled("• ", Style::default().fg(Theme::ACCENT))];
            spans.extend(styled);
            lines.push(Line::from(spans));
        } else if trimmed.is_empty() {
            lines.push(Line::from(""));
        } else {
            let styled = inline_format(raw_line);
            lines.push(Line::from(styled));
        }
    }

    lines
}

pub fn render_markdown_plain(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();

        if trimmed.starts_with("```") {
            if in_code_block {
                in_code_block = false;
            } else {
                in_code_block = true;
                let code_lang = trimmed.trim_start_matches("```");
                lines.push(format!(
                    " ── {} ──",
                    if code_lang.is_empty() { "code" } else { code_lang }
                ));
            }
            continue;
        }

        if in_code_block {
            lines.push(format!("  {}", raw_line));
            continue;
        }

        if let Some(heading) = trimmed.strip_prefix("### ") {
            lines.push(heading.to_string());
        } else if let Some(heading) = trimmed.strip_prefix("## ") {
            lines.push(heading.to_string());
        } else if let Some(heading) = trimmed.strip_prefix("# ") {
            lines.push(heading.to_string());
        } else if let Some(quote) = trimmed.strip_prefix("> ") {
            lines.push(format!("│ {}", quote));
        } else if let Some(item) = trimmed.strip_prefix("- ") {
            lines.push(format!("• {}", inline_format_plain(item)));
        } else if trimmed.is_empty() {
            lines.push(String::new());
        } else {
            lines.push(inline_format_plain(raw_line));
        }
    }

    lines
}

fn inline_format_plain(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '`' && i + 1 < chars.len() {
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                result.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }

        if c == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                result.push(chars[i]);
                i += 1;
            }
            i += 2;
            continue;
        }

        if c == '*' {
            i += 1;
            while i < chars.len() && chars[i] != '*' {
                result.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }

        result.push(c);
        i += 1;
    }

    result
}

fn inline_format(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '`' && i + 1 < chars.len() {
            if !buffer.is_empty() {
                spans.push(Span::styled(buffer.clone(), Style::default().fg(Theme::FG)));
                buffer.clear();
            }

            let mut code = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                code.push(chars[i]);
                i += 1;
            }
            i += 1;

            spans.push(Span::styled(
                code,
                Style::default().fg(Theme::ACCENT_BRIGHT).bg(Color::Rgb(25, 25, 25)),
            ));
            continue;
        }

        if c == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            if !buffer.is_empty() {
                spans.push(Span::styled(buffer.clone(), Style::default().fg(Theme::FG)));
                buffer.clear();
            }

            let mut bold = String::new();
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                bold.push(chars[i]);
                i += 1;
            }
            i += 2;

            spans.push(Span::styled(
                bold,
                Style::default().fg(Theme::FG).add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if c == '*' {
            if !buffer.is_empty() {
                spans.push(Span::styled(buffer.clone(), Style::default().fg(Theme::FG)));
                buffer.clear();
            }

            let mut italic = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '*' {
                italic.push(chars[i]);
                i += 1;
            }
            i += 1;

            spans.push(Span::styled(
                italic,
                Style::default().fg(Theme::FG).add_modifier(Modifier::ITALIC),
            ));
            continue;
        }

        buffer.push(c);
        i += 1;
    }

    if !buffer.is_empty() {
        spans.push(Span::styled(buffer, Style::default().fg(Theme::FG)));
    }

    spans
}
