use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    pub const BG: Color = Color::Rgb(30, 30, 30);
    pub const FG: Color = Color::Rgb(212, 212, 212);
    pub const MUTED: Color = Color::Rgb(133, 133, 133);

    pub const ACCENT: Color = Color::Rgb(86, 156, 214);
    pub const ACCENT_BRIGHT: Color = Color::Rgb(120, 190, 255);
    pub const SUCCESS: Color = Color::Rgb(78, 201, 176);
    pub const WARNING: Color = Color::Rgb(220, 160, 70);
    pub const ERROR: Color = Color::Rgb(220, 90, 90);
    pub const PURPLE: Color = Color::Rgb(197, 134, 192);

    pub const USER_MSG: Color = Color::Rgb(78, 201, 176);
    pub const ASSISTANT_MSG: Color = Color::Rgb(212, 212, 212);
    pub const SYSTEM_MSG: Color = Color::Rgb(197, 134, 192);

    pub const SIDEBAR_BG: Color = Color::Rgb(37, 37, 37);
    pub const INPUT_BG: Color = Color::Rgb(37, 37, 37);
    pub const BORDER: Color = Color::Rgb(60, 60, 60);
    pub const BORDER_ACTIVE: Color = Color::Rgb(86, 156, 214);

    pub const SPLASH_GLOW: Color = Color::Rgb(120, 190, 255);

    pub fn base() -> Style {
        Style::default().fg(Self::FG).bg(Self::BG)
    }

    pub fn accent() -> Style {
        Style::default().fg(Self::ACCENT)
    }

    pub fn success() -> Style {
        Style::default().fg(Self::SUCCESS)
    }

    pub fn warning() -> Style {
        Style::default().fg(Self::WARNING)
    }

    pub fn error() -> Style {
        Style::default().fg(Self::ERROR)
    }

    pub fn muted() -> Style {
        Style::default().fg(Self::MUTED)
    }

    pub fn bold() -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    pub fn user_msg() -> Style {
        Style::default().fg(Self::USER_MSG)
    }

    pub fn assistant_msg() -> Style {
        Style::default().fg(Self::ASSISTANT_MSG)
    }
}
