pub mod domain;
pub mod infrastructure;
pub mod application;
pub mod ui;

use std::sync::Arc;

use caelix_service::CaelixApiImpl;

pub async fn run_tui(_api: Arc<CaelixApiImpl>) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    use ratatui::{Terminal, backend::CrosstermBackend};

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = ui::TuiApp::new().await;
    let events = ui::EventHandler::new();

    while app.is_running() {
        terminal.draw(|f| ui::render(f, &mut app))?;

        let event = events.next()?;
        app.handle_event(event).await;
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
