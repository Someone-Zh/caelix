use ratatui::{
    prelude::*,
    widgets::Paragraph,
};

use super::super::theme::Theme;

const LOGO_LINES: &[&str] = &[
    "   ██████╗ █████╗ ███████╗██╗     ██╗██╗  ██╗",
    "  ██╔════╝██╔══██╗██╔════╝██║     ██║╚██╗██╔╝",
    "  ██║     ███████║█████╗  ██║     ██║ ╚███╔╝ ",
    "  ██║     ██╔══██║██╔══╝  ██║     ██║ ██╔██╗ ",
    "  ╚██████╗██║  ██║███████╗███████╗██║██╔╝ ██╗",
    "   ╚═════╝╚═╝  ╚═╝╚══════╝╚══════╝╚═╝╚═╝  ╚═╝",
];

const SUBTITLE: &str = "~  Terminal AI Assistant  ~";

pub fn logo_height() -> u16 {
    LOGO_LINES.len() as u16
}

pub fn logo_width() -> u16 {
    LOGO_LINES[0].chars().count() as u16
}

pub fn render_logo(f: &mut Frame, area: Rect) {
    if area.width < logo_width() || area.height < logo_height() {
        return;
    }
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(logo_height()),
            Constraint::Min(1),
        ])
        .split(area);
    let h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(logo_width()),
            Constraint::Min(1),
        ])
        .split(v[1]);
    let logo_area = h[1];

    for (i, line) in LOGO_LINES.iter().enumerate() {
        let line_y = logo_area.y + i as u16;
        let line_area = Rect {
            x: logo_area.x,
            y: line_y,
            width: logo_area.width,
            height: 1,
        };
        let p = Paragraph::new(line.to_string()).style(
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(p, line_area);
    }
}

pub fn render(f: &mut Frame, area: Rect, light_x: f32) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(6),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);

    let logo_v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(logo_height()),
            Constraint::Min(1),
        ])
        .split(vertical[1]);

    let logo_h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(logo_width()),
            Constraint::Min(1),
        ])
        .split(logo_v[1]);

    let logo_area = logo_h[1];

    for (i, line) in LOGO_LINES.iter().enumerate() {
        let line_y = logo_area.y + i as u16;
        if line_y >= area.y + area.height {
            break;
        }
        let line_area = Rect {
            x: logo_area.x,
            y: line_y,
            width: logo_area.width,
            height: 1,
        };
        let styled_line = apply_light_effect(line, light_x, i);
        let p = Paragraph::new(styled_line).style(
            Style::default()
                .fg(Theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(p, line_area);
    }

    let sub = Paragraph::new(SUBTITLE)
        .style(Style::default().fg(Theme::MUTED).add_modifier(Modifier::ITALIC))
        .alignment(Alignment::Center);
    f.render_widget(sub, vertical[2]);

    let hint = Paragraph::new(" 按任意键继续... ")
        .style(Style::default().fg(Theme::MUTED))
        .alignment(Alignment::Center);
    f.render_widget(hint, vertical[3]);
}

fn apply_light_effect(line: &str, light_x: f32, row: usize) -> Line<'static> {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len() as f32;
    let wave_offset = (row as f32) * 0.05;
    let effective_x = (light_x + wave_offset) % 1.5;
    let light_center = effective_x * len;
    let light_radius = len * 0.15;

    let mut spans = Vec::new();
    for (i, ch) in chars.iter().enumerate() {
        let dist = (i as f32 - light_center).abs();
        let intensity = if dist < light_radius {
            1.0 - (dist / light_radius)
        } else {
            0.0
        };

        let style = if intensity > 0.0 {
            let bright = (intensity * 255.0) as u8;
            let r = (120u16 + bright as u16 * 135 / 255) as u8;
            let g = (190u16 + bright as u16 * 65 / 255) as u8;
            let b = 255u8;
            Style::default().fg(Color::Rgb(r, g, b)).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD)
        };

        spans.push(Span::styled(ch.to_string(), style));
    }

    Line::from(spans)
}
