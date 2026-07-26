use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui_textarea::TextArea;

use crate::application::AppService;
use crate::domain::SPLASH_DURATION_MS;

use super::event::UiEvent;
use super::widgets::markdown::render_markdown_plain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Splash,
    Input,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    SessionList = 0,
    ModelList = 1,
    Quit = 2,
}

impl MenuItem {
    pub fn label(&self) -> &'static str {
        match self {
            MenuItem::SessionList => "📋 会话列表",
            MenuItem::ModelList => "🤖 模型列表",
            MenuItem::Quit => "🚪 退出",
        }
    }

    pub fn all() -> [MenuItem; 3] {
        [MenuItem::SessionList, MenuItem::ModelList, MenuItem::Quit]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Chat,
    Input,
}

impl Focus {
    pub fn label(&self) -> &'static str {
        match self {
            Focus::Chat => "Chat",
            Focus::Input => "Editor",
        }
    }
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct TuiApp<'a> {
    pub service: AppService,
    pub mode: AppMode,
    pub input: TextArea<'a>,
    pub splash_start: Instant,
    pub splash_light_x: f32,
    pub running: bool,
    pub needs_redraw: bool,
    pub menu_open: bool,
    pub menu_selected: usize,
    pub focus: Focus,
    pub chat_selected_msg: usize,
    pub chat_scroll_y: i32,
    pub chat_auto_scroll: bool,
    pub chat_viewport_rows: u16,
    pub chat_cursor_line: usize,
    pub chat_cursor_col: usize,
    pub spinner_idx: usize,
    pub workdir: String,
    pub alert: Option<String>,
}

impl<'a> TuiApp<'a> {
    pub async fn new() -> Self {
        let mut service = AppService::new_mock();
        let _ = service.refresh_tasks().await;
        let _ = service.refresh_notifications().await;

        let mut input = TextArea::default();
        input.set_cursor_line_style(ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::UNDERLINED));
        input.set_block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title(" 输入消息 (Enter 换行, Ctrl+Enter 发送, Esc 菜单) "),
        );

        Self {
            service,
            mode: AppMode::Splash,
            input,
            splash_start: Instant::now(),
            splash_light_x: 0.0,
            running: true,
            needs_redraw: true,
            menu_open: false,
            menu_selected: 0,
            focus: Focus::Input,
            chat_selected_msg: 0,
            chat_scroll_y: 0,
            chat_auto_scroll: true,
            chat_viewport_rows: 20,
            chat_cursor_line: 0,
            chat_cursor_col: 0,
            spinner_idx: 0,
            workdir: "/home/user/projects/caelix".to_string(),
            alert: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.spinner_idx % SPINNER_FRAMES.len()]
    }

    pub fn is_streaming(&self) -> bool {
        self.service.get_messages().iter().any(|m| m.is_streaming)
    }

    fn clamp_selected(&mut self) {
        let len = self.service.get_messages().len();
        if len == 0 {
            self.chat_selected_msg = 0;
        } else {
            self.chat_selected_msg = self.chat_selected_msg.min(len - 1);
        }
    }

    fn selected_msg_lines(&self) -> Vec<String> {
        let msgs = self.service.get_messages();
        if msgs.is_empty() {
            return vec![String::new()];
        }
        let idx = self.chat_selected_msg.min(msgs.len() - 1);
        render_markdown_plain(&msgs[idx].content)
    }

    pub fn set_viewport_rows(&mut self, rows: u16) {
        self.chat_viewport_rows = rows.max(1);
    }

    pub async fn handle_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Tick => {
                self.handle_tick();
            }
            UiEvent::Resize(_, _) => {
                self.needs_redraw = true;
            }
            UiEvent::SendMessage => {
                if !self.menu_open {
                    self.handle_send().await;
                }
            }
            UiEvent::Key(key) => {
                self.handle_key(key).await;
            }
        }
    }

    fn handle_tick(&mut self) {
        self.needs_redraw = true;
        self.spinner_idx = self.spinner_idx.wrapping_add(1);

        match self.mode {
            AppMode::Splash => {
                self.splash_light_x += 0.008;
                if self.splash_light_x > 1.5 {
                    self.splash_light_x = -0.5;
                }
                if self.splash_start.elapsed().as_millis() as u64 >= SPLASH_DURATION_MS {
                    self.mode = AppMode::Input;
                }
            }
            AppMode::Input | AppMode::Chat => {
                if !self.menu_open {
                    let had_update = self.service.tick_stream();
                    if had_update {
                        self.needs_redraw = true;
                        self.clamp_selected();
                        if self.chat_auto_scroll {
                            let len = self.service.get_messages().len();
                            if len > 0 {
                                self.chat_selected_msg = len - 1;
                                self.chat_cursor_line = 0;
                                self.chat_cursor_col = 0;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn handle_send(&mut self) {
        match self.mode {
            AppMode::Splash => {
                self.mode = AppMode::Input;
            }
            AppMode::Input | AppMode::Chat => {
                let content: String = self.input.lines().join("\n");
                if !content.trim().is_empty() {
                    let _ = self.service.send_user_message(content).await;
                    self.input = TextArea::default();
                    self.input.set_cursor_line_style(
                        ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::UNDERLINED),
                    );
                    self.input.set_block(
                        ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .title(" 输入消息 (Enter 换行, Ctrl+Enter 发送, Esc 菜单) "),
                    );
                    self.mode = AppMode::Chat;
                    self.chat_auto_scroll = true;
                    self.clamp_selected();
                    let len = self.service.get_messages().len();
                    if len > 0 {
                        self.chat_selected_msg = len - 1;
                        self.chat_cursor_line = 0;
                        self.chat_cursor_col = 0;
                    }
                }
            }
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if self.menu_open {
            self.handle_menu_key(key);
            return;
        }

        if key.code == KeyCode::BackTab {
            self.cycle_focus();
            return;
        }

        let is_shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if is_shift && key.code == KeyCode::Tab {
            self.cycle_focus();
            return;
        }

        if key.code == KeyCode::Esc {
            match self.mode {
                AppMode::Splash => {
                    self.mode = AppMode::Input;
                }
                AppMode::Input | AppMode::Chat => {
                    self.menu_open = true;
                    self.menu_selected = 0;
                    self.needs_redraw = true;
                }
            }
            return;
        }

        if self.focus == Focus::Chat {
            let len = self.service.get_messages().len();
            match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    if len > 0 {
                        let lines = self.selected_msg_lines();
                        if self.chat_cursor_col > 0 {
                            self.chat_cursor_col -= 1;
                        } else if self.chat_cursor_line > 0 {
                            self.chat_cursor_line -= 1;
                            let prev: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                            self.chat_cursor_col = prev.len().saturating_sub(1).max(0);
                        } else if self.chat_selected_msg > 0 {
                            self.chat_selected_msg -= 1;
                            let prev_lines = self.selected_msg_lines();
                            self.chat_cursor_line = prev_lines.len().saturating_sub(1);
                            let last: Vec<char> = prev_lines[self.chat_cursor_line].chars().collect();
                            self.chat_cursor_col = last.len().saturating_sub(1).max(0);
                            self.chat_auto_scroll = false;
                        }
                    }
                    self.needs_redraw = true;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if len > 0 {
                        let lines = self.selected_msg_lines();
                        let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                        let cur_len = cur.len();
                        if self.chat_cursor_col + 1 < cur_len {
                            self.chat_cursor_col += 1;
                        } else if self.chat_cursor_line + 1 < lines.len() {
                            self.chat_cursor_line += 1;
                            self.chat_cursor_col = 0;
                        } else if self.chat_selected_msg + 1 < len {
                            self.chat_selected_msg += 1;
                            self.chat_cursor_line = 0;
                            self.chat_cursor_col = 0;
                            if self.chat_selected_msg < len - 1 {
                                self.chat_auto_scroll = false;
                            }
                        }
                    }
                    self.needs_redraw = true;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if len > 0 {
                        let lines = self.selected_msg_lines();
                        if self.chat_cursor_line + 1 < lines.len() {
                            self.chat_cursor_line += 1;
                            let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                            self.chat_cursor_col = self.chat_cursor_col.min(cur.len().saturating_sub(1).max(0));
                        } else if self.chat_selected_msg + 1 < len {
                            self.chat_selected_msg += 1;
                            self.chat_cursor_line = 0;
                            self.chat_cursor_col = 0;
                            if self.chat_selected_msg < len - 1 {
                                self.chat_auto_scroll = false;
                            }
                        }
                    }
                    self.needs_redraw = true;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if len > 0 {
                        if self.chat_cursor_line > 0 {
                            self.chat_cursor_line -= 1;
                            let lines = self.selected_msg_lines();
                            let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                            self.chat_cursor_col = self.chat_cursor_col.min(cur.len().saturating_sub(1).max(0));
                        } else if self.chat_selected_msg > 0 {
                            self.chat_selected_msg -= 1;
                            let lines = self.selected_msg_lines();
                            self.chat_cursor_line = lines.len().saturating_sub(1);
                            let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                            self.chat_cursor_col = cur.len().saturating_sub(1).max(0);
                            self.chat_auto_scroll = false;
                        }
                    }
                    self.needs_redraw = true;
                }
                KeyCode::Char('0') => {
                    self.chat_cursor_col = 0;
                    self.needs_redraw = true;
                }
                KeyCode::Char('$') => {
                    let lines = self.selected_msg_lines();
                    let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                    self.chat_cursor_col = cur.len().saturating_sub(1).max(0);
                    self.needs_redraw = true;
                }
                KeyCode::PageDown => {
                    if len > 0 {
                        let step = (self.chat_viewport_rows as usize).saturating_sub(2).max(1);
                        let lines = self.selected_msg_lines();
                        let max_line = lines.len().saturating_sub(1);
                        if self.chat_cursor_line + step < max_line {
                            self.chat_cursor_line += step;
                            let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                            self.chat_cursor_col = self.chat_cursor_col.min(cur.len().saturating_sub(1).max(0));
                        } else if self.chat_selected_msg + 1 < len {
                            self.chat_selected_msg += 1;
                            self.chat_cursor_line = 0;
                            self.chat_cursor_col = 0;
                        } else {
                            self.chat_cursor_line = max_line;
                        }
                        self.chat_auto_scroll = self.chat_selected_msg >= len - 1
                            && self.chat_cursor_line >= self.selected_msg_lines().len().saturating_sub(1);
                    }
                    self.needs_redraw = true;
                }
                KeyCode::PageUp => {
                    let step = (self.chat_viewport_rows as usize).saturating_sub(2).max(1);
                    if self.chat_cursor_line >= step {
                        self.chat_cursor_line -= step;
                        let lines = self.selected_msg_lines();
                        let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                        self.chat_cursor_col = self.chat_cursor_col.min(cur.len().saturating_sub(1).max(0));
                    } else if self.chat_selected_msg > 0 {
                        self.chat_selected_msg -= 1;
                        let prev_lines = self.selected_msg_lines();
                        self.chat_cursor_line = prev_lines.len().saturating_sub(1);
                        let cur: Vec<char> = prev_lines[self.chat_cursor_line].chars().collect();
                        self.chat_cursor_col = cur.len().saturating_sub(1).max(0);
                    } else {
                        self.chat_cursor_line = 0;
                    }
                    self.chat_auto_scroll = false;
                    self.needs_redraw = true;
                }
                KeyCode::End | KeyCode::Char('G') => {
                    if len > 0 {
                        self.chat_selected_msg = len - 1;
                        let lines = self.selected_msg_lines();
                        self.chat_cursor_line = lines.len().saturating_sub(1);
                        let cur: Vec<char> = lines[self.chat_cursor_line].chars().collect();
                        self.chat_cursor_col = cur.len().saturating_sub(1).max(0);
                        self.chat_auto_scroll = true;
                    }
                    self.needs_redraw = true;
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    self.chat_selected_msg = 0;
                    self.chat_cursor_line = 0;
                    self.chat_cursor_col = 0;
                    self.chat_auto_scroll = false;
                    self.needs_redraw = true;
                }
                _ => {}
            }
            return;
        }

        match self.mode {
            AppMode::Splash => {
                self.mode = AppMode::Input;
            }
            AppMode::Input | AppMode::Chat => {
                if self.focus == Focus::Input {
                    let _ = self.input.input(key);
                    self.needs_redraw = true;
                }
            }
        }
    }

    fn cycle_focus(&mut self) {
        let prev = self.focus;
        self.focus = match self.focus {
            Focus::Chat => Focus::Input,
            Focus::Input => Focus::Chat,
        };
        if prev == Focus::Input && self.focus == Focus::Chat {
            let msgs = self.service.get_messages();
            if !msgs.is_empty() {
                let last = msgs.len() - 1;
                self.chat_selected_msg = last;
                self.chat_cursor_line = 0;
                self.chat_cursor_col = 0;
                self.chat_auto_scroll = true;
            }
        }
        self.needs_redraw = true;
    }

    fn handle_menu_key(&mut self, key: KeyEvent) {
        let total = MenuItem::all().len();
        match key.code {
            KeyCode::Esc => {
                self.menu_open = false;
                self.needs_redraw = true;
            }
            KeyCode::Up => {
                if self.menu_selected > 0 {
                    self.menu_selected -= 1;
                } else {
                    self.menu_selected = total - 1;
                }
                self.needs_redraw = true;
            }
            KeyCode::Down => {
                if self.menu_selected + 1 < total {
                    self.menu_selected += 1;
                } else {
                    self.menu_selected = 0;
                }
                self.needs_redraw = true;
            }
            KeyCode::Enter => {
                match MenuItem::all()[self.menu_selected] {
                    MenuItem::SessionList => {}
                    MenuItem::ModelList => {}
                    MenuItem::Quit => {
                        self.running = false;
                    }
                }
                self.menu_open = false;
                self.needs_redraw = true;
            }
            _ => {}
        }
    }
}
