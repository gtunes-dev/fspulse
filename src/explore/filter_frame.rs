use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Rect},
    style::{Color, Style, Stylize},
    text::Span,
    widgets::{
        Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Table, TableState,
        Widget,
    },
};

use super::{domain_model::{DomainModel, TypeSelection}, explorer::ExplorerAction, utils::Utils};

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

    pub fn handle_key(&mut self, model: &DomainModel, key: KeyEvent) -> Option<ExplorerAction> {
        let total_rows = model.current_filters().len();
        let visible_rows = self.visible_rows();

        match key.code {
            KeyCode::Delete | KeyCode::Backspace => {
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
            KeyCode::Char(' ') | KeyCode::Enter => {
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

    pub fn frame_title(type_selection: TypeSelection) -> &'static str {
        match type_selection {
            TypeSelection::Items => "Items Filters",
            TypeSelection::Changes => "Changes Filters",
            TypeSelection::Scans => "Scans Filters",
            TypeSelection::Roots => "Roots Filters",
        }
    }
}

pub struct FilterFrameView<'a> {
    frame: &'a mut FilterFrame,
    model: &'a DomainModel,
    has_focus: bool,
}

impl<'a> FilterFrameView<'a> {
    pub fn new(frame: &'a mut FilterFrame, model: &'a DomainModel, has_focus: bool) -> Self {
        Self {
            frame,
            model,
            has_focus,
        }
    }
}

impl Widget for FilterFrameView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.frame.set_area(area);

        let header = Row::new(vec!["Column", "Type", "Filter"])
            .style(Style::default().fg(Color::Black).bg(Color::Gray).bold());

        let rows = self.model.current_filters().iter().map(|f| {
            Row::new(vec![
                Span::raw(f.col_name),
                Span::raw(f.type_name),
                Span::raw(f.filter_text.to_owned()),
            ])
        });

        let total_rows = rows.len();

        let title = FilterFrame::frame_title(self.model.current_type());
        let block = Utils::new_frame_block_with_title(self.has_focus, title);

        let constraints = vec![
            Constraint::Length(15),
            Constraint::Length(10),
            Constraint::Min(10),
        ];

        let mut highlight_style = Style::default();
        if self.has_focus {
            highlight_style = highlight_style.fg(Color::Yellow);
        }

        let table = Table::new(rows, constraints)
            .header(header)
            .block(block)
            .row_highlight_style(highlight_style)
            .highlight_symbol("» ");

        <Table as StatefulWidget>::render(table, area, buf, &mut self.frame.table_state);

        // Draw scrollbar
        let visible_rows = self.frame.visible_rows();
        if total_rows > visible_rows {
            if let Some(selected) = self.frame.table_state.selected() {
                let mut scrollbar_state = ScrollbarState::new(total_rows)
                    .viewport_content_length(visible_rows)
                    .position(selected);

                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .thumb_symbol("▐")
                    .track_symbol(Some(" "))
                    .render(area, buf, &mut scrollbar_state);
            }
        }
    }
}
