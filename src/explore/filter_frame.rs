use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{
        Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Table,
        TableState, Widget,
    },
};

use super::{
    domain_model::{DomainModel, TypeSelection},
    explorer::ExplorerAction,
    utils::{StylePalette, Utils},
};

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

impl FilterFrameView<'_> {
    fn render_empty(&self, area: Rect, buf: &mut Buffer) {
        let [_, para_area, _] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .flex(Flex::Center) // even distribution above & below
        .areas(area);

        let style = if self.has_focus {
            StylePalette::style(&StylePalette::TextFocus)
        } else {
            Style::default()
        };

        let text = vec![
            Line::from(
                Span::from("No Filters").style(style)),
            Line::from("(select a column and press 'f' to add a filter)"),
        ];

        Paragraph::new(text)
            .alignment(Alignment::Center)
            .render(para_area, buf);
    }

    fn render_filters(&mut self, area: Rect, buf: &mut Buffer) {
        let header_style = StylePalette::TableHeader.style();

        let header = Row::new(vec!["Column", "Type", "Filter"]).style(header_style);

        let rows = self.model.current_filters().iter().map(|f| {
            Row::new(vec![
                Span::raw(f.col_name),
                Span::raw(f.type_name),
                Span::raw(f.filter_text.to_owned()),
            ])
        });

        let total_rows = rows.len();

        let constraints = vec![
            Constraint::Length(15),
            Constraint::Length(10),
            Constraint::Min(10),
        ];

        let highlight_style = if self.has_focus {
            StylePalette::TableRowHighlight.style()
        } else {
            Style::default()
        };

        let table = Table::new(rows, constraints)
            .header(header)
            //  .block(block)
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

impl Widget for FilterFrameView<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        self.frame.set_area(area);

        if self.model.current_filters().is_empty() {
            self.render_empty(area, buf);
        } else {
            self.render_filters(area, buf);
        }
    }
}
