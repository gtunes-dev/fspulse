use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::Span,
    widgets::{Block, Borders, Row, Table},
    Frame,
};

pub struct GridFrame {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub cursor_row: usize,
    pub scroll_offset: usize,
    pub area: Rect,
    pub loaded: bool,
}

impl GridFrame {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            cursor_row: 0,
            scroll_offset: 0,
            area: Rect::default(),
            loaded: false,
        }
    }

    pub fn set_data(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) {
        self.columns = columns;
        self.rows = rows;
        self.cursor_row = 0;
        self.scroll_offset = 0;
    }

    pub fn set_area(&mut self, new_area: Rect) {
        self.area = new_area;
        // We can later add scroll adjustment logic here if needed
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, is_focused: bool) {
        self.area = area;
        let visible_rows = self.visible_rows();

        let start = self.scroll_offset;
        let end = (self.scroll_offset + visible_rows).min(self.rows.len());

        let displayed_rows = self.rows[start..end].iter().enumerate().map(|(i, row)| {
            let mut ratatui_row = Row::new(row.clone());

            // Highlight the selected row if focused
            if self.cursor_row == start + i && is_focused {
                ratatui_row = ratatui_row.style(Style::default().fg(Color::Yellow).bold());
            }

            ratatui_row
        });

        let header = Row::new(self.columns.iter().map(|h| Span::raw(h.clone())))
            .style(Style::default().bold().underlined());

        let block = if is_focused {
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title("Data Grid")
        } else {
            Block::default().borders(Borders::ALL).title("Data Grid")
        };

        let column_constraints = vec![Constraint::Min(10); self.columns.len()];

        let table = Table::new(displayed_rows, column_constraints)
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().fg(Color::Yellow));

        f.render_widget(table, area);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                // we'll add scrolling logic here later
            }
            KeyCode::Down => {
                // we'll add scrolling logic here later
            }
            _ => {}
        }
    }

    pub fn visible_rows(&self) -> usize {
        self.area.height.saturating_sub(2) as usize // 1 for header + 1 for borders
    }
}
