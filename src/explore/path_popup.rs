use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, StatefulWidget, Widget},
};
use std::{borrow::Cow, fs};
use tui_textarea::{CursorMove, TextArea};

use super::{
    explorer::ExplorerAction,
    utils::{StylePalette, Utils},
};

#[derive(Debug)]
pub struct PathPopupState {
    pub prompt: String,
    pub text_area: TextArea<'static>,
    pub status: String,
    pub valid: bool,
}

impl PathPopupState {
    pub fn new(prompt: String, input_value: Option<String>) -> Self {
        let text_area = match input_value {
            Some(input_value) => TextArea::new(vec![input_value]),
            None => TextArea::default(),
        };
        PathPopupState {
            prompt,
            text_area,
            status: String::new(),
            valid: false,
        }
    }

    /// Check if the current input is an existing directory, and update status accordingly.
    pub fn validate(&mut self) {
        let path_str = self.text_area.lines()[0].trim();
        if path_str.is_empty() {
            self.status = "Enter a directory path".to_string();
            self.valid = false;
        } else if let Ok(md) = fs::metadata(path_str) {
            if md.is_dir() {
                self.status = "✓ Directory exists".to_string();
                self.valid = true;
            } else {
                self.status = "✗ Path exists but is not a directory".to_string();
                self.valid = false;
            }
        } else {
            self.status = "✗ Directory does not exist".to_string();
            self.valid = false;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        match key.code {
            KeyCode::Esc => {
                return Some(ExplorerAction::Dismiss);
            }
            KeyCode::Enter => {
                self.validate();
                if self.valid {
                    let path_val = self.text_area.lines()[0].trim().to_owned();
                    return Some(ExplorerAction::AddRoot(path_val));
                }
            }
            KeyCode::Tab => {
                let input = self.text_area.lines()[0].trim();
                if let Some((completed, matches)) = self.autocomplete_path(input) {
                    if matches.len() == 1 {
                        // Only one match: replace input and show autocomplete
                        self.text_area = TextArea::new(vec![completed]);
                        self.text_area.move_cursor(CursorMove::End);
                        self.validate();
                    } else if matches.len() > 1 {
                        self.status = format!(
                            "Matches: {}",
                            matches
                                .iter()
                                .map(String::as_str)
                                .collect::<Vec<_>>()
                                .join("  ")
                        );
                    } else {
                        self.status = "No completions found".to_string();
                    }
                }
                // Do NOT call self.validate() here, or you'll overwrite self.status!
                return None;
            }
            _ => {
                self.text_area.input(key);
                self.validate();
            }
        }
        None
    }

    fn autocomplete_path(&self, input: &str) -> Option<(String, Vec<String>)> {
        use std::path::{Path, PathBuf};
        let input = input.trim();
        let path = Path::new(input);

        let (parent, partial): (&Path, Cow<str>) = if input.ends_with('/') || input.is_empty() {
            (Path::new(input), Cow::Borrowed(""))
        } else {
            (
                path.parent().unwrap_or(Path::new("/")),
                path.file_name().unwrap_or_default().to_string_lossy(),
            )
        };

        if let Ok(entries) = std::fs::read_dir(parent) {
            let mut matches = vec![];
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&*partial) && entry.path().is_dir() {
                    matches.push(name);
                }
            }
            matches.sort();
            if matches.is_empty() {
                return None;
            }
            if matches.len() == 1 {
                // Complete with a slash for further expansion
                let mut new_path = PathBuf::from(parent);
                new_path.push(&matches[0]);
                let mut completed = new_path.to_string_lossy().to_string();
                if !completed.ends_with('/') {
                    completed.push('/');
                }
                return Some((completed, matches));
            } else {
                return Some((input.to_string(), matches));
            }
        }
        None
    }
}

pub struct PathPopupWidget;

impl StatefulWidget for PathPopupWidget {
    type State = PathPopupState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .style(StylePalette::PopUp.style());
        let inner_block = outer_block.inner(area);

        let inner_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // 0 - Prompt
                Constraint::Length(3), // 1 - TextArea
                Constraint::Length(1), // 2 - Status Line
                Constraint::Length(2), // 3 - Help
            ])
            .split(inner_block);

        outer_block.render(area, buf);

        Line::from(state.prompt.to_owned()).render(inner_layout[0], buf);

        state.text_area.set_style(StylePalette::PopUp.style());
        state.text_area.set_cursor_line_style(Style::default());
        state
            .text_area
            .set_block(Block::default().borders(Borders::ALL));
        state.text_area.render(inner_layout[1], buf);

        // Render status line (in color depending on validity)
        let status_line = if state.valid {
            Line::from(vec![Span::styled(
                &state.status,
                Style::default()
                    .fg(ratatui::style::Color::Green)
                    .add_modifier(Modifier::BOLD),
            )])
        } else {
            Line::from(vec![Span::styled(
                &state.status,
                Style::default().fg(ratatui::style::Color::Red),
            )])
        };
        status_line.render(inner_layout[2], buf);

        Utils::render_popup_help(
            "Esc: Cancel  |  Enter: Save (if valid)",
            inner_layout[3],
            buf,
        );
    }
}
