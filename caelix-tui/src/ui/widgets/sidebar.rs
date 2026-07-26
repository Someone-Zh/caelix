use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::application::AppService;
use crate::domain::{NotificationLevel, TaskStatus};

use super::super::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, service: &AppService) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .split(area);

    render_tasks(f, layout[0], service);
    render_notifications(f, layout[1], service);
    render_progress(f, layout[2], service);
}

fn render_tasks(f: &mut Frame, area: Rect, service: &AppService) {
    let block = Block::default()
        .title(" 待办任务 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Theme::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = service
        .get_tasks()
        .iter()
        .take(5)
        .map(|task| {
            let status_icon = match task.status {
                TaskStatus::Running => "⏳",
                TaskStatus::Completed => "✅",
                TaskStatus::Failed => "❌",
                TaskStatus::Pending => "📋",
                TaskStatus::Cancelled => "🚫",
            };

            let style = match task.status {
                TaskStatus::Running => Theme::accent(),
                TaskStatus::Completed => Theme::success(),
                TaskStatus::Failed => Theme::error(),
                _ => Theme::muted(),
            };

            let line = if let Some(p) = &task.progress {
                format!(" {} {} ({:.0}%)", status_icon, task.title, p.percentage() * 100.0)
            } else {
                format!(" {} {}", status_icon, task.title)
            };

            ListItem::new(line).style(style)
        })
        .collect();

    if items.is_empty() {
        let empty = Paragraph::new("暂无任务").style(Theme::muted());
        f.render_widget(empty, inner);
    } else {
        let list = List::new(items).block(Block::default());
        f.render_widget(list, inner);
    }
}

fn render_notifications(f: &mut Frame, area: Rect, service: &AppService) {
    let block = Block::default()
        .title(" 通知 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Theme::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = service
        .get_notifications()
        .iter()
        .take(5)
        .map(|n| {
            let (icon, style) = match n.level {
                NotificationLevel::Info => ("ℹ️ ", Theme::accent()),
                NotificationLevel::Success => ("✅ ", Theme::success()),
                NotificationLevel::Warning => ("⚠️ ", Theme::warning()),
                NotificationLevel::Error => ("❌ ", Theme::error()),
            };
            let content = format!("{}{}", icon, n.content);
            ListItem::new(content).style(style)
        })
        .collect();

    if items.is_empty() {
        let empty = Paragraph::new("暂无通知").style(Theme::muted());
        f.render_widget(empty, inner);
    } else {
        let list = List::new(items).block(Block::default());
        f.render_widget(list, inner);
    }
}

fn render_progress(f: &mut Frame, area: Rect, service: &AppService) {
    let block = Block::default()
        .title(" 任务进度 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Theme::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let tasks_with_progress: Vec<_> = service
        .get_tasks()
        .iter()
        .filter(|t| t.progress.is_some())
        .collect();

    if tasks_with_progress.is_empty() {
        let empty = Paragraph::new("暂无进度").style(Theme::muted());
        f.render_widget(empty, inner);
        return;
    }

    let bar_width = inner.width.saturating_sub(4) as usize;
    let lines: Vec<Line> = tasks_with_progress
        .iter()
        .take(3)
        .map(|task| {
            let p = task.progress.as_ref().unwrap();
            let percent = p.percentage();
            let filled = (percent * bar_width as f32) as usize;
            let bar: String = "█".repeat(filled) + &"░".repeat(bar_width.saturating_sub(filled));

            let style = if percent >= 1.0 {
                Theme::success()
            } else {
                Theme::accent()
            };

            Line::from(vec![
                Span::styled(format!(" {:.<20} ", task.title), Theme::muted()),
                Span::styled(format!("{:>3.0}% ", percent * 100.0), style),
                Span::styled(bar, style),
            ])
        })
        .collect();

    let p = Paragraph::new(lines);
    f.render_widget(p, inner);
}
