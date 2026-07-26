use ratatui::prelude::*;
use ratatui_textarea::TextArea;

pub fn render(f: &mut Frame, area: Rect, input: &TextArea) {
    f.render_widget(input, area);
}
