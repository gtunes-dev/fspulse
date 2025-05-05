use ratatui::{
    buffer::Buffer,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Span, Text},
    widgets::{
        Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Table,
        TableState, Widget,
    },
};

use crate::query::columns::ColType;

use super::{
    domain_model::{ColInfo, DomainModel, TypeSelection},
    explorer::ExplorerAction,
    utils::Utils,
};

pub struct GridFrame {
    pub columns: Vec<String>,
    pub col_infos: Vec<ColInfo>,
    pub rows: Vec<Row<'static>>,
    pub column_constraints: Vec<Constraint>,
    pub table_state: TableState,
    pub table_area: Rect,
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
            table_area: Rect::default(),
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
                ColType::Val | ColType::ItemType | ColType::ChangeType => {
                    Constraint::Length(col_size(col_name, 1) as u16)
                }
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
        self.table_area.height.saturating_sub(1) as usize // subtract the header row
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
        // ---- 1. Draw the outer frame ---------------------------------------------------------
        let frame_title = GridFrame::frame_title(self.model.current_type());
        let block = Utils::new_frame_block_with_title(self.has_focus, frame_title);

        // Work only inside the borders
        let inner = block.inner(area);
        if inner.width < 2 || inner.height < 1 {
            return; // not enough space
        }

        block.render(area, buf);

        // ---- 2. Split inner area: [table | 1‑col scrollbar] ----------------------------------
        let [table_area, bar_area] = Layout::horizontal([
            Constraint::Min(0),    // table takes remaining width
            Constraint::Length(1), // 1 char for scrollbar
        ])
        .areas(inner);

        self.frame.table_area = table_area;

        // ---- 3. Stateful table ---------------------------------------------------------------
        let header = Row::new(self.frame.columns.iter().map(|h| Span::raw(h.clone())))
            .style(Style::default().bg(Color::DarkGray).bold());

        let highlight_style = if self.has_focus {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default()
        };

        let table = Table::new(
            self.frame.rows.clone(),
            self.frame.column_constraints.clone(),
        )
        .header(header)
        .highlight_symbol("» ")
        .row_highlight_style(highlight_style);

        StatefulWidget::render(table, table_area, buf, &mut self.frame.table_state);
        // ---- 4. Vertical scrollbar -----------------------------------------------------------
        let total_rows = self.frame.rows.len();
        let viewport_rows = table_area.height as usize;

        if total_rows > viewport_rows {
            if let Some(selected) = self.frame.table_state.selected() {
                let mut sb_state = ScrollbarState::new(total_rows)
                    .viewport_content_length(viewport_rows)
                    .position(selected);

                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .thumb_symbol("▐")
                    .track_symbol(Some(" "))
                    .render(bar_area, buf, &mut sb_state);
            }
        }
    }
}
