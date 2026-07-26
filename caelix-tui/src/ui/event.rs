use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

use crate::domain::TICK_RATE_MS;

#[derive(Debug, Clone)]
pub enum UiEvent {
    Key(KeyEvent),
    Tick,
    Resize(u16, u16),
    SendMessage,
}

pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new() -> Self {
        Self {
            tick_rate: Duration::from_millis(TICK_RATE_MS),
        }
    }

    pub fn next(&self) -> Result<UiEvent, std::io::Error> {
        if event::poll(self.tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

                    if is_ctrl && key.code == KeyCode::Enter {
                        Ok(UiEvent::SendMessage)
                    } else {
                        Ok(UiEvent::Key(key))
                    }
                }
                Event::Resize(w, h) => Ok(UiEvent::Resize(w, h)),
                _ => Ok(UiEvent::Tick),
            }
        } else {
            Ok(UiEvent::Tick)
        }
    }
}
