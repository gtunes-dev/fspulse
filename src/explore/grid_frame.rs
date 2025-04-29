use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::{Span, Text},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};

use crate::query::columns::ColType;

use super::column_frame::ColInfo;

pub struct GridFrame {
    pub columns: Vec<String>,
    pub col_infos: Vec<ColInfo>,
    pub rows: Vec<Vec<String>>,
    pub cursor_row: usize,
    pub scroll_offset: usize,
    pub area: Rect,
    pub table_state: TableState,
}

impl GridFrame {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            col_infos: Vec::new(),
            rows: Vec::new(),
            cursor_row: 0,
            scroll_offset: 0,
            area: Rect::default(),
            table_state: {
                let mut state = TableState::default();
                state.select(Some(0));
                state
            },
        }
    }

    pub fn set_data(
        &mut self,
        columns: Vec<String>,
        col_infos: Vec<ColInfo>,
        rows: Vec<Vec<String>>,
    ) {
        self.columns = columns;
        self.col_infos = col_infos;
        self.rows = rows;
        self.cursor_row = 0;
        self.scroll_offset = 0;
        self.table_state.select(Some(0));
    }

    pub fn set_area(&mut self, new_area: Rect) {
        self.area = new_area;
        // We can later add scroll adjustment logic here if needed
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, is_focused: bool) {
        self.set_area(area);
        let visible_rows = self.visible_rows();

        let start = self.scroll_offset;
        let end = (self.scroll_offset + visible_rows).min(self.rows.len());

        let displayed_rows = self.rows[start..end].iter().enumerate().map(|(i, row)| {
            let cells = row.iter().zip(&self.col_infos).map(|(value, col_info)| {
                let text = Text::from(value.clone()).alignment(col_info.col_align);
                Cell::from(text)
            });

            let mut ratatui_row = Row::new(cells);

            if self.cursor_row == start + i && is_focused {
                ratatui_row = ratatui_row.style(Style::default().fg(Color::Yellow).bold());
            }

            ratatui_row
        });
        let header = Row::new(self.columns.iter().map(|h| Span::raw(h.clone())))
            .style(Style::default().fg(Color::Black).bg(Color::Gray).bold());

        let block = if is_focused {
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title("Data Grid")
        } else {
            Block::default().borders(Borders::ALL).title("Data Grid")
        };

        let column_constraints = self
            .col_infos
            .iter()
            .map(|col_info| match col_info.col_type {
                ColType::Id | ColType::Int => Constraint::Min(8),
                ColType::Bool => Constraint::Length(3),
                ColType::Date => Constraint::Length(12),
                ColType::Path => Constraint::Min(20),
                ColType::Enum => Constraint::Min(3),
                ColType::String => Constraint::Min(15),
            })
            .collect::<Vec<_>>();

        let table = Table::new(displayed_rows, column_constraints)
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().fg(Color::Yellow));

        f.render_stateful_widget(table, area, &mut self.table_state);
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                    self.table_state.select(Some(self.cursor_row));
                }
            }
            KeyCode::Down => {
                if self.cursor_row + 1 < self.rows.len() {
                    self.cursor_row += 1;
                    self.table_state.select(Some(self.cursor_row));
                }
            }
            _ => {}
        }
    }

    pub fn visible_rows(&self) -> usize {
        self.area.height.saturating_sub(2) as usize // 1 for header + 1 for borders
    }
}
