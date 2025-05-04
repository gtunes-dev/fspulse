use ratatui::{
    buffer::Buffer, crossterm::event::KeyEvent, layout::{Constraint, Rect}, style::{Color, Style, Stylize}, text::{Span, Text}, widgets::{
        Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Table, TableState, Widget
    },
};

use crate::query::columns::ColType;

use super::{domain_model::{ColInfo, DomainModel, TypeSelection}, explorer::ExplorerAction, utils::Utils};

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
        reset_selection: bool,
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
        if reset_selection {
            self.table_state = {
                let mut state = TableState::default();
                state.select(Some(0));
                state
            };
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

    pub fn frame_title(type_selection: TypeSelection) -> &'static str {
        match type_selection {
            TypeSelection::Items => "Items Data",
            TypeSelection::Changes => "Changes Data",
            TypeSelection::Scans => "Scans Data",
            TypeSelection::Roots => "Roots Data",
        }
    }
}

pub struct GridFrameView<'a> {
    frame: &'a mut GridFrame,
    model: &'a DomainModel,
    has_focus: bool,
}

impl<'a> GridFrameView<'a> {
    pub fn new(frame: &'a mut GridFrame, model: &'a DomainModel, has_focus: bool) -> Self {
        Self {
            frame,
            model,
            has_focus,
        }
    }
}

impl Widget for GridFrameView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.frame.area = area;

        // Set up for drawing
        let total_rows = self.frame.rows.len();
        let visible_rows = self.frame.visible_rows();

        let header = Row::new(self.frame.columns.iter().map(|h| Span::raw(h.clone())))
            .style(Style::default().fg(Color::Black).bg(Color::Gray).bold());

        let frame_title = GridFrame::frame_title(self.model.current_type());
        let block = Utils::new_frame_block_with_title(self.has_focus, frame_title);

        let mut highlight_style = Style::default();
        if self.has_focus {
            highlight_style = highlight_style.fg(Color::Yellow);
        }

        let table = Table::new(self.frame.rows.clone(), self.frame.column_constraints.clone())
            .header(header)
            .block(block)
            .row_highlight_style(highlight_style)
            .highlight_symbol("» ");

        <Table as StatefulWidget>::render(table, area, buf, &mut self.frame.table_state);

        // Draw scrollbar
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