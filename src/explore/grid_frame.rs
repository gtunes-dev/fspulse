use ratatui::{
    crossterm::event::KeyEvent,
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::{Span, Text},
    widgets::{
        Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Table, TableState
    },
    Frame,
};

use crate::query::columns::ColType;

use super::{domain_model::ColInfo, explorer::ExplorerAction, utils::Utils};

pub struct GridFrame {
    pub columns: Vec<String>,
    pub col_infos: Vec<ColInfo>,
    pub rows: Vec<Row<'static>>,
    pub column_constraints: Vec<Constraint>,
    pub table_state: TableState,
    pub area: Rect,
}

impl GridFrame {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            col_infos: Vec::new(),
            rows: Vec::new(),
            column_constraints: Vec::new(),
            table_state: {
                let mut state = TableState::default();
                state.select(Some(0));
                state
            },
            area: Rect::default(),
        }
    }

    pub fn set_data(
        &mut self,
        columns: Vec<String>,
        col_infos: Vec<ColInfo>,
        raw_rows: Vec<Vec<String>>,
    ) {
        self.columns = columns;
        self.col_infos = col_infos;

        // Build aligned Rows once, store as static (owned Strings are fine)
        self.rows = raw_rows
            .into_iter()
            .map(|row| {
                let cells = row
                    .into_iter()
                    .zip(&self.col_infos)
                    .map(|(value, col_info)| {
                        let text = Text::from(value).alignment(col_info.col_align);
                        Cell::from(text)
                    });
                Row::new(cells)
            })
            .collect();

        // Build constraints based on column type and header length
        fn col_size(header: &str, default: usize) -> usize {
            header.len().max(default)
        }

        self.column_constraints = self
            .columns
            .iter()
            .zip(&self.col_infos)
            .map(|(col_name, col_info)| match col_info.col_type {
                ColType::Id => Constraint::Length(col_size(col_name, 9) as u16),
                ColType::Int => Constraint::Length(col_size(col_name, 8) as u16),
                ColType::Val | ColType::ItemType | ColType::ChangeType => Constraint::Length(col_size(col_name, 1) as u16),
                ColType::Bool => Constraint::Length(col_size(col_name, 1) as u16),
                ColType::Date => Constraint::Length(col_size(col_name, 10) as u16),
                ColType::Path => Constraint::Min(30),
                ColType::String => Constraint::Min(15),
            })
            .collect();

        // Reset selection
        self.table_state = {
            let mut state = TableState::default();
            state.select(Some(0));
            state
        };
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, is_focused: bool) {
        self.area = area;

        // Set up for drawing
        let total_rows = self.rows.len();
        let visible_rows = self.visible_rows();

        let header = Row::new(self.columns.iter().map(|h| Span::raw(h.clone())))
            .style(Style::default().fg(Color::Black).bg(Color::Gray).bold());

        let block = Utils::new_frame_block(is_focused, "Data Grid");

        let table = Table::new(self.rows.clone(), self.column_constraints.clone())
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().fg(Color::Yellow))
            .highlight_symbol("» ");

        // Draw table
        f.render_stateful_widget(table, area, &mut self.table_state);

        // Draw scrollbar
        if total_rows > visible_rows {
            if let Some(selected) = self.table_state.selected() {
                let mut scrollbar_state = ScrollbarState::new(total_rows)
                    .viewport_content_length(visible_rows)
                    .position(selected);

                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .thumb_symbol("▐")
                    .track_symbol(Some(" "))
                    .render(area, f.buffer_mut(), &mut scrollbar_state);
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        let total_rows = self.rows.len();
        let visible_rows = self.visible_rows();
        Utils::handle_table_state_keys(&mut self.table_state, total_rows, visible_rows, key);

        None
    }

    pub fn visible_rows(&self) -> usize {
        self.area.height.saturating_sub(3) as usize
    }
}
