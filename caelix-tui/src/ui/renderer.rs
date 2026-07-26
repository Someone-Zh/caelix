use ratatui::{
    prelude::*,
    widgets::{Block, Borders, LineGauge, List, ListItem, Paragraph},
};

use crate::domain::{UI_INPUT_AREA_HEIGHT, UI_SIDEBAR_WIDTH_PERCENT, UI_STATUS_BAR_HEIGHT};

use super::app::{AppMode, Focus, MenuItem, TuiApp};
use super::theme::Theme;
use super::widgets::{chat_area, input_area, sidebar, splash};
use super::widgets::splash::render_logo;

pub fn render(f: &mut Frame, app: &mut TuiApp) {
    let area = f.area();

    match app.mode {
        AppMode::Splash => {
            splash::render(f, area, app.splash_light_x);
        }
        AppMode::Input => {
            render_input_mode(f, area, app);
        }
        AppMode::Chat => {
            render_chat_mode(f, area, app);
        }
    }

    if app.menu_open {
        render_menu(f, area, app);
    }
}

fn split_with_status(area: Rect) -> (Rect, Rect) {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(UI_STATUS_BAR_HEIGHT),
        ])
        .split(area);
    (v[0], v[1])
}

fn render_input_mode(f: &mut Frame, area: Rect, app: &TuiApp) {
    let (main_area, status_area) = split_with_status(area);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(7),
            Constraint::Min(1),
            Constraint::Length(10),
            Constraint::Length(4),
            Constraint::Min(1),
        ])
        .split(main_area);

    render_logo(f, outer[1]);

    let input_width = (main_area.width as f32 * 0.6) as u16;
    let input_center = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(input_width),
            Constraint::Min(1),
        ])
        .split(outer[3]);

    input_area::render(f, input_center[1], &app.input);

    let hint = Paragraph::new(" Enter 换行 · Ctrl+Enter 发送 · Esc 菜单 ")
        .style(Theme::muted())
        .alignment(Alignment::Center);
    f.render_widget(hint, outer[4]);

    render_status_bar(f, status_area, app);
}

fn render_chat_mode(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let (main_area, status_area) = split_with_status(area);

    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(100 - UI_SIDEBAR_WIDTH_PERCENT),
            Constraint::Percentage(UI_SIDEBAR_WIDTH_PERCENT),
        ])
        .split(main_area);

    let left_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(UI_INPUT_AREA_HEIGHT),
        ])
        .split(main_layout[0]);

    chat_area::render(
        f,
        left_layout[0],
        &app.service,
        app.chat_selected_msg,
        app.focus == Focus::Chat,
        app.chat_auto_scroll,
        app.chat_cursor_line,
        app.chat_cursor_col,
        &mut app.chat_scroll_y,
        &mut app.chat_viewport_rows,
    );
    input_area::render(f, left_layout[1], &app.input);
    sidebar::render(f, main_layout[1], &app.service);

    render_status_bar(f, status_area, app);
}

fn shorten_path(path: &str, max_chars: usize) -> String {
    if path.len() <= max_chars {
        return path.to_string();
    }
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return path.to_string();
    }
    if parts.len() == 1 {
        return format!("/{}", parts[0]);
    }
    let last = parts[parts.len() - 1];
    let second_last = parts[parts.len() - 2];
    let two = format!("/{}/{}", second_last, last);
    if two.len() <= max_chars {
        return two;
    }
    format!("/{}", last)
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &TuiApp) {
    let bg = Style::default().bg(Color::Rgb(28, 28, 28)).fg(Theme::FG);
    let alert_bg = Style::default().bg(Color::Rgb(120, 20, 20)).fg(Color::White);

    let block = Block::default().style(bg);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let alert_width = if app.alert.is_some() { 20 } else { 0 };
    let path_len = if inner.width > 80 { 24 } else { 12 };

    let left_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(alert_width),
            Constraint::Length(14),
            Constraint::Length(path_len),
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Length(14),
            Constraint::Min(1),
        ])
        .split(inner);

    let spinner_char = if app.is_streaming() {
        app.spinner_frame()
    } else {
        "●"
    };
    let spinner_color = if app.is_streaming() { Theme::ACCENT } else { Theme::SUCCESS };
    let spinner = Paragraph::new(format!(" {} ", spinner_char))
        .style(Style::default().fg(spinner_color).bg(Color::Rgb(28, 28, 28)));
    f.render_widget(spinner, left_layout[0]);

    if let Some(alert_text) = &app.alert {
        let display = if alert_text.len() > 14 {
            format!(" ⚠ {} ", &alert_text[..14])
        } else {
            format!(" ⚠ {} ", alert_text)
        };
        let alert = Paragraph::new(display)
            .style(alert_bg.add_modifier(Modifier::BOLD));
        f.render_widget(alert, left_layout[1]);
    }

    let focus_text = format!(" Focus: {} ", app.focus.label());
    let focus_style = Style::default()
        .fg(Theme::ACCENT)
        .bg(Color::Rgb(28, 28, 28))
        .add_modifier(Modifier::BOLD);
    let focus_para = Paragraph::new(focus_text).style(focus_style);
    f.render_widget(focus_para, left_layout[2]);

    let short_dir = shorten_path(&app.workdir, 22);
    let dir_text = format!(" 📂 {} ", short_dir);
    let dir = Paragraph::new(dir_text)
        .style(Style::default().fg(Theme::MUTED).bg(Color::Rgb(28, 28, 28)));
    f.render_widget(dir, left_layout[3]);

    let agent = Paragraph::new(" Caelix ")
        .style(Style::default().fg(Theme::FG).bg(Color::Rgb(28, 28, 28)).add_modifier(Modifier::BOLD));
    f.render_widget(agent, left_layout[4]);

    let model = Paragraph::new(" deepseek-v3 ")
        .style(Style::default().fg(Theme::MUTED).bg(Color::Rgb(28, 28, 28)));
    f.render_widget(model, left_layout[5]);

    let gauge_ratio: f64 = 0.37;
    let gauge = LineGauge::default()
        .ratio(gauge_ratio)
        .filled_style(Style::default().fg(Theme::ACCENT).bg(Color::Rgb(60, 60, 60)))
        .label(" 3.7k ");
    f.render_widget(gauge, left_layout[6]);

    let keys = Paragraph::new(" Ctrl+C 停止  ·  Shift+Tab 切换窗口  ·  Esc 菜单 ")
        .style(Style::default().fg(Theme::MUTED).bg(Color::Rgb(28, 28, 28)))
        .alignment(Alignment::Right);
    f.render_widget(keys, left_layout[7]);
}

fn render_menu(f: &mut Frame, area: Rect, app: &TuiApp) {
    let menu_width = 36;
    let menu_height = 10;
    let x = (area.width - menu_width) / 2;
    let y = (area.height - menu_height) / 2;
    let menu_area = Rect::new(x, y, menu_width, menu_height);

    let shadow = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(shadow, menu_area);

    let block = Block::default()
        .title(" 菜单 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Theme::ACCENT))
        .style(Style::default().bg(Color::Rgb(30, 30, 30)));
    let inner = block.inner(menu_area);
    f.render_widget(block, menu_area);

    let items: Vec<ListItem> = MenuItem::all()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == app.menu_selected;
            let prefix = if is_selected { "▶ " } else { "  " };
            let text = format!("{}{}", prefix, item.label());
            let style = if is_selected {
                Style::default()
                    .fg(Theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Theme::FG)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default());
    f.render_widget(list, inner);

    let hint_area = Rect::new(
        menu_area.x,
        menu_area.y + menu_area.height - 1,
        menu_area.width,
        1,
    );
    let hint = Paragraph::new(" ↑/↓ 选择 · Enter 确认 · Esc 关闭 ")
        .style(Theme::muted())
        .alignment(Alignment::Center);
    f.render_widget(hint, hint_area);
}
