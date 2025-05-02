use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::Span,
    widgets::{
        Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Table, TableState,
    },
    Frame,
};

use super::{domain_model::DomainModel, explorer::ExplorerAction, utils::Utils};

pub struct FilterFrame {
    table_state: TableState,
    area: Rect,
}

impl FilterFrame {
    pub fn new() -> Self {
        Self {
            table_state: {
                let mut state = TableState::default();
                state.select(Some(0));
                state
            },
            area: Rect::default(),
        }
    }

    pub fn set_area(&mut self, area: Rect) {
        self.area = area;
    }

    pub fn set_selected(&mut self, new_selected: usize) {
        self.table_state.select(Some(new_selected));
    }

    pub fn draw(&mut self, model: &DomainModel, f: &mut Frame, area: Rect, is_focused: bool) {
        self.set_area(area);

        let header = Row::new(vec!["Column", "Type", "Filter"])
            .style(Style::default().fg(Color::Black).bg(Color::Gray).bold());

        let rows = model.current_filters().iter().map(|f| {
            Row::new(vec![
                Span::raw(f.col_name),
                Span::raw(f.type_name),
                Span::raw(f.filter_text.to_owned()),
            ])
        });

        let total_rows = rows.len();

        let block = Utils::new_frame_block(is_focused, "Filters");

        let constraints = vec![
            Constraint::Length(15),
            Constraint::Length(10),
            Constraint::Min(10),
        ];

        let table = Table::new(rows, constraints)
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().fg(Color::Yellow))
            .highlight_symbol("» ");

        f.render_stateful_widget(table, area, &mut self.table_state);

        // Draw scrollbar
        let visible_rows = self.visible_rows();
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

    pub fn handle_key(&mut self, model: &DomainModel, key: KeyEvent) -> Option<ExplorerAction> {
        let total_rows = model.current_filters().len();
        let visible_rows = self.visible_rows();

        match key.code {
            KeyCode::Char('x') => {
                let selected = self.table_state.selected();
                match selected {
                    Some(selected) => {
                        if selected <= model.current_filters().len() {
                            Some(ExplorerAction::DeleteFilter(selected))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            }
            KeyCode::Char('e') => {
                let selected = self.table_state.selected();
                match selected {
                    Some(selected) => {
                        if selected <= model.current_filters().len() {
                            Some(ExplorerAction::ShowEditFilter(selected))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            }
            _ => {
                Utils::handle_table_state_keys(
                    &mut self.table_state,
                    total_rows,
                    visible_rows,
                    key,
                );
                None
            }
        }
    }

    fn visible_rows(&self) -> usize {
        self.area.height.saturating_sub(3) as usize
    }
}
