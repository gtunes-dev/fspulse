use ratatui::{
    buffer::Buffer,
    crossterm::event::{Event, KeyCode, KeyEvent},
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget},
};
use tui_input::{backend::crossterm::EventHandler, Input};

use super::{explorer::ExplorerAction, utils::{StylePalette, Utils}};

pub struct InputBoxState {
    pub cursor_pos: Option<(u16, u16)>,
}

impl InputBoxState {
    pub fn new() -> Self {
        InputBoxState { cursor_pos: None }
    }
}

#[derive(Debug, Clone)]
pub struct InputBox {
    pub prompt: String,
    pub input: Input,
}

impl InputBox {
    pub fn new(prompt: String, input_value: Option<String>) -> Self {
        let input = match input_value {
            Some(input_value) => Input::default().with_value(input_value),
            None => Input::default(),
        };

        InputBox { prompt, input }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        match key.code {
            KeyCode::Esc => {
                return Some(ExplorerAction::Dismiss);
            }
            KeyCode::Enter => {
                return Some(ExplorerAction::SetLimit(self.input.value().trim().to_owned()));
            }
            KeyCode::Char(c) if c.is_ascii_digit() && key.modifiers.is_empty() => {
                let _ = self.input.handle_event(&Event::Key(key));
            }
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Backspace
            | KeyCode::Delete => {
                self.input.handle_event(&Event::Key(key));
            }
            _ => { /* ignore */ }
        }

        None
    }
}

impl StatefulWidget for InputBox {
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
                Constraint::Length(3), // 2 - Input box
                Constraint::Length(1), // 3 - Spacer
                Constraint::Length(2), // 4 - Help
            ])
            .split(inner_block);

        outer_block.render(area, buf);
        Line::from(self.prompt).render(inner_layout[0], buf);

        // keep 2 width for border and 1 for cursor
        let input_area = inner_layout[2];
        let width = input_area.width.max(3) - 3;
        let scroll = self.input.visual_scroll(width as usize);
        Paragraph::new(self.input.value())
            .style(Style::default())
            .scroll((0, scroll as u16))
            .block(Block::bordered())
            .render(input_area, buf);

        Utils::render_popup_help( "Esc: Cancel  |  Enter: Save", inner_layout[4], buf);

        // Ratatui hides the cursor unless it's explicitly set. Position the  cursor past the
        // end of the input text and one line down from the border to the input line
        let x = self.input.visual_cursor().max(scroll) - scroll + 1;
        state.cursor_pos = Some((input_area.x + x as u16, input_area.y + 1));
    }
}
