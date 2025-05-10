use ratatui::{
    buffer::Buffer, crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Direction, Layout, Rect}, style::Style, text::Line, widgets::{Block, Borders, StatefulWidget, Widget}
};
use tui_textarea::TextArea;

use super::{explorer::ExplorerAction, utils::{StylePalette, Utils}};

#[derive(Debug)]
pub struct InputBoxState {
    pub prompt: String,
    pub text_area: TextArea<'static>,
}

impl InputBoxState {
    pub fn new(prompt: String, input_value: Option<String>) -> Self {
        let text_area = match input_value {
            Some(input_value) => {
                TextArea::new(vec![input_value])
            }
            None => TextArea::default()
        };

        InputBoxState { prompt, text_area }
    }

}

impl InputBoxState {
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        match key.code {
            KeyCode::Esc => {
                return Some(ExplorerAction::Dismiss);
            }
            KeyCode::Enter => {
                let text_val = self.text_area.lines()[0].trim().to_owned();
                return Some(ExplorerAction::SetLimit(text_val));
            }
            KeyCode::Char(c) if c.is_ascii_digit() && key.modifiers.is_empty() => {
                self.text_area.input(key);
            }
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Backspace
            | KeyCode::Delete => {
                self.text_area.input(key);
            }
            _ => { /* ignore */ }
        }

        None
    }
}

pub struct InputBoxWidget;

impl StatefulWidget for InputBoxWidget {
    type State = InputBoxState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let outer_block = Block::default().borders(Borders::ALL)
        .style(StylePalette::PopUp.style());
        let inner_block = outer_block.inner(area);

        // Total vertical height is the border plus all of the sections below:
        // 2 + 8 = 10

        let inner_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // 0 - Spacer
                Constraint::Length(1), // 1 - Prompt
                Constraint::Length(3), // 2 - TextArea
                Constraint::Length(1), // 3 - Spacer
                Constraint::Length(2), // 4 - Help
            ])
            .split(inner_block);

        outer_block.render(area, buf);
        Line::from(state.prompt.to_owned()).render(inner_layout[0], buf);

        state.text_area.set_style(StylePalette::PopUp.style());
        state.text_area.set_cursor_line_style(Style::default());
        state.text_area.set_block(Block::default().borders(Borders::ALL));
        state.text_area.render(inner_layout[2], buf);

        Utils::render_popup_help( "Esc: Cancel  |  Enter: Save", inner_layout[4], buf);
    }
}
