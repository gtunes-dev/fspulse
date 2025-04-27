use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Alignment,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

#[derive(Debug, Clone, Copy)]
pub enum TypeSelection {
    Roots,
    Scans,
    Items,
    Changes,
}

impl TypeSelection {
    pub fn all_types() -> &'static [TypeSelection] {
        &[
            TypeSelection::Roots,
            TypeSelection::Scans,
            TypeSelection::Items,
            TypeSelection::Changes,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            TypeSelection::Roots => "Roots",
            TypeSelection::Scans => "Scans",
            TypeSelection::Items => "Items",
            TypeSelection::Changes => "Changes",
        }
    }
}

pub struct ColumnFrame {
    pub selected_type: TypeSelection,
    pub available_columns: Vec<&'static str>,
    pub checked_columns: Vec<&'static str>,
    pub cursor_position: usize,
    pub dropdown_open: bool,
}

impl ColumnFrame {
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down => {
                self.move_down();
            }
            KeyCode::Up => {
                self.move_up();
            }
            _ => {}
        }
    }
}

impl ColumnFrame {
    pub fn new() -> Self {
        Self {
            selected_type: TypeSelection::Roots,
            available_columns: vec!["Name", "Path", "Size", "Date Modified"],
            checked_columns: vec!["Name", "Size"],
            cursor_position: 0, // Start at Type selector
            dropdown_open: false,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: ratatui::layout::Rect, is_focused: bool) {
        let mut lines = Vec::new();

        // First line: Type selector
        let type_text = format!("Type: [{}] v", self.selected_type.name());
        let mut type_line = Line::from(type_text);

        if self.cursor_position == 0 && is_focused{
            type_line = type_line.style(Style::default().fg(Color::Yellow).bold());
        }

        lines.push(type_line);

        // Spacer
        lines.push(Line::from(" "));

        // Columns
        for (i, &col) in self.available_columns.iter().enumerate() {
            let checked = if self.checked_columns.contains(&col) {
                "[x]"
            } else {
                "[ ]"
            };

            let text = format!("{checked} {:<22}", col);

            let mut line = Line::from(text);

            if self.cursor_position == i + 1 && is_focused {
                line = line.style(Style::default().fg(Color::Yellow).bold());
            }

            lines.push(line);
        }

        let block = if is_focused {
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title("Type & Columns")
        } else {
            Block::default()
                .borders(Borders::ALL)
                .title("Type & Columns")
        };

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left);

        f.render_widget(paragraph, area);
    }

    pub fn move_up(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_position < self.available_columns.len() {
            self.cursor_position += 1;
        }
    }
}
