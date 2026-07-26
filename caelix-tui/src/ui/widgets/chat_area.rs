use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::application::AppService;
use crate::domain::MessageRole;

use super::super::theme::Theme;
use super::markdown::{render_markdown, render_markdown_plain};

const MSG_GAP: u16 = 1;
const MSG_PAD_V: u16 = 1;

struct MsgLayout {
    top_row: i32,
    height: u16,
}

pub fn render(
    f: &mut Frame,
    area: Rect,
    service: &AppService,
    selected_msg: usize,
    focused: bool,
    auto_scroll: bool,
    cursor_line_in_msg: usize,
    cursor_col_in_msg: usize,
    scroll_y: &mut i32,
    viewport_rows: &mut u16,
) {
    let messages = service.get_messages();

    let border_style = if focused {
        Style::default().fg(Theme::ACCENT)
    } else {
        Style::default().fg(Theme::BORDER)
    };

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = outer.inner(area);
    f.render_widget(outer, area);

    if messages.is_empty() {
        let empty = Paragraph::new("还没有消息，开始输入吧！")
            .style(Theme::muted())
            .alignment(Alignment::Center);
        f.render_widget(empty, inner);
        return;
    }

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let avail_height = inner.height;
    *viewport_rows = avail_height;

    let msg_pad_x = inner.x + 1;
    let avail_width = inner.width.saturating_sub(3);

    let msg_heights: Vec<u16> = messages
        .iter()
        .map(|msg| {
            let cursor = if msg.is_streaming { "▌" } else { "" };
            let content = format!("{}{}", msg.content, cursor);
            let lines: Vec<Line> = render_markdown(&content);
            (lines.len() as u16 + MSG_PAD_V * 2).max(3)
        })
        .collect();

    let len = messages.len();
    let mut layouts: Vec<MsgLayout> = Vec::with_capacity(len);
    let mut y_acc: i32 = 0;
    for &h in msg_heights.iter() {
        layouts.push(MsgLayout {
            top_row: y_acc,
            height: h,
        });
        y_acc += h as i32 + MSG_GAP as i32;
    }

    let total_rows = y_acc - MSG_GAP as i32;
    let max_scroll = (total_rows - avail_height as i32).max(0);

    if auto_scroll {
        *scroll_y = max_scroll;
    }

    if focused {
        let sel = selected_msg.min(len.saturating_sub(1));
        let sel_layout = &layouts[sel];
        let raw_cur_line = sel_layout.top_row + MSG_PAD_V as i32 + cursor_line_in_msg as i32;

        let margin = 2;
        if raw_cur_line - *scroll_y < margin {
            *scroll_y = (raw_cur_line - margin).max(0);
        }
        let view_bottom = *scroll_y + avail_height as i32;
        if raw_cur_line + margin >= view_bottom {
            *scroll_y = (raw_cur_line + margin + 1 - avail_height as i32).max(0);
        }
    }

    *scroll_y = (*scroll_y).max(0).min(max_scroll);

    let content_top = inner.y as i32 - *scroll_y;

    let sb_x = inner.x + inner.width.saturating_sub(1);
    let sb_area = Rect::new(sb_x, inner.y, 1, inner.height.saturating_sub(1));
    if max_scroll > 0 {
        let mut sb_state = ScrollbarState::new(max_scroll as usize)
            .position(*scroll_y as usize)
            .viewport_content_length(avail_height as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .style(Style::default().fg(Theme::BORDER))
                .thumb_style(Style::default().fg(Theme::ACCENT)),
            sb_area,
            &mut sb_state,
        );
    }

    for (i, msg) in messages.iter().enumerate() {
        let layout = &layouts[i];
        let mh = layout.height;
        let msg_top = content_top + layout.top_row;
        let msg_bottom = msg_top + mh as i32;

        if msg_bottom < inner.y as i32 {
            continue;
        }
        if msg_top >= (inner.y + inner.height) as i32 {
            break;
        }

        let is_user = msg.role == MessageRole::User;
        let cursor_suffix = if msg.is_streaming { "▌" } else { "" };
        let content_with_cursor = format!("{}{}", msg.content, cursor_suffix);

        let lines: Vec<Line> = render_markdown(&content_with_cursor);
        let text = ratatui::text::Text::from(lines);

        let actual_y = msg_top.max(inner.y as i32) as u16;
        let remaining_bottom = (inner.y + inner.height).saturating_sub(actual_y);
        let actual_h = mh.min(remaining_bottom);

        if actual_h < 2 {
            continue;
        }

        let (bg_color, border_color) = if is_user {
            (Color::Rgb(35, 55, 50), Theme::USER_MSG)
        } else {
            (Color::Rgb(37, 37, 37), Theme::ACCENT)
        };

        let msg_area = Rect::new(msg_pad_x, actual_y, avail_width, actual_h);

        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(bg_color));

        let inner_msg = block.inner(msg_area);

        f.render_widget(block, msg_area);

        if inner_msg.height > 0 && inner_msg.width > 0 {
            let msg_scroll_y = (*scroll_y - layout.top_row - MSG_PAD_V as i32).max(0) as u16;
            let para = Paragraph::new(text.clone()).scroll((msg_scroll_y, 0));
            f.render_widget(para, inner_msg);
        }

        if focused && i == selected_msg && inner_msg.height > 0 && inner_msg.width > 0 {
            let rendered_lines = render_markdown_plain(&msg.content);
            if !rendered_lines.is_empty() {
                let cl = cursor_line_in_msg.min(rendered_lines.len().saturating_sub(1));
                let cur_chars: Vec<char> = rendered_lines[cl].chars().collect();
                let cc = if cur_chars.is_empty() {
                    0
                } else {
                    cursor_col_in_msg.min(cur_chars.len())
                };

                let cursor_y = (content_top + layout.top_row + MSG_PAD_V as i32 + cl as i32) as u16;
                let cursor_x = inner_msg.x + cc as u16;

                if cursor_y >= inner.y
                    && cursor_y < inner.y + inner.height
                    && cursor_x >= inner.x
                    && cursor_x < inner.x + inner.width
                {
                    let cursor_span = Span::styled(
                        "█".to_string(),
                        Style::default()
                            .fg(border_color)
                            .bg(bg_color)
                            .add_modifier(Modifier::BOLD),
                    );
                    let cursor_para = Paragraph::new(Line::from(cursor_span));
                    let cursor_area = Rect::new(cursor_x, cursor_y, 1, 1);
                    f.render_widget(cursor_para, cursor_area);
                }
            }
        }
    }
}
