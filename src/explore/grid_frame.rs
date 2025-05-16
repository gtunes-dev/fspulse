use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{
        Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
        Table, TableState, Widget,
    },
};

use crate::{alerts::AlertStatus, query::columns::ColType};

use super::{
    domain_model::{ColumnInfo, DomainModel, DomainType},
    explorer::ExplorerAction,
    utils::{StylePalette, Utils},
};

pub struct GridFrame {
    pub columns: Vec<ColumnInfo>,
    pub raw_rows: Vec<Vec<String>>,
    pub column_constraints: Vec<Constraint>,
    pub table_state: TableState,
    pub table_area: Rect,
}

impl GridFrame {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            raw_rows: Vec::new(),
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
        columns: Vec<ColumnInfo>,
        raw_rows: Vec<Vec<String>>,
    ) {
        self.columns = columns;
        self.raw_rows = raw_rows;

        // Build constraints based on column type and header length
        fn col_size(header: &str, default: usize) -> usize {
            header.len().max(default)
        }

        self.column_constraints = self
            .columns
            .iter()
            .map(|col| match col.col_type {
                ColType::Id => Constraint::Length(col_size(col.name_display, 9) as u16),
                ColType::Int => Constraint::Length(col_size(col.name_display, 8) as u16),
                ColType::Val
                | ColType::ItemType
                | ColType::ChangeType
                | ColType::AlertType
                | ColType::AlertStatus => Constraint::Length(col_size(col.name_display, 4) as u16),
                ColType::Bool => Constraint::Length(col_size(col.name_display, 1) as u16),
                ColType::Date => Constraint::Length(col_size(col.name_display, 10) as u16),
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
        match key.code {
            KeyCode::Char('o') | KeyCode::Char('O') => {
                match self.selected_alert_id() {
                    Some(alert_id) => Some(ExplorerAction::SetAlertStatus(alert_id, AlertStatus::Open)),
                    None => None
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
               match self.selected_alert_id() {
                    Some(alert_id) => Some(ExplorerAction::SetAlertStatus(alert_id, AlertStatus::Flagged)),
                    None => None
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                match self.selected_alert_id() {
                    Some(alert_id) => Some(ExplorerAction::SetAlertStatus(alert_id, AlertStatus::Dismissed)),
                    None => None
                }
            }
            _ => {
                let total_rows = self.raw_rows.len();
                let visible_rows = self.visible_rows();
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

    pub fn visible_rows(&self) -> usize {
        self.table_area.height.saturating_sub(1) as usize // subtract the header row
    }

    pub fn frame_title(type_selection: DomainType) -> &'static str {
        match type_selection {
            DomainType::Alerts => "Alerts Data",
            DomainType::Items => "Items Data",
            DomainType::Changes => "Changes Data",
            DomainType::Scans => "Scans Data",
            DomainType::Roots => "Roots Data",
        }
    }

    fn selected_alert_id(&self) -> Option<i64> {
        // Traverse: selected row → get row → find alert_id column → get cell → parse to i64
        self.table_state
            .selected()
            .and_then(|alert_index| self.raw_rows.get(alert_index))
            .and_then(|row| {
                self.columns
                    .iter()
                    .position(|col_info| col_info.name_db == "alert_id")
                    .and_then(|col_index| row.get(col_index))
            })
            .and_then(|alert_id_str| alert_id_str.parse::<i64>().ok())
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

impl GridFrameView<'_> {
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

        let reason = if self.frame.columns.is_empty() {
            Line::from("(all columns are hidden)")
        } else {
            Line::from(format!("(no {} found)", self.model.current_type().as_str()))
        };

        let text = vec![
            Line::from(Span::from("No Data to Display").style(style)),
            reason,
        ];

        Paragraph::new(text)
            .alignment(Alignment::Center)
            .render(para_area, buf);
    }

    fn render_table(&mut self, area: Rect, buf: &mut Buffer) {
        // ---- 2. Split inner area: [table | 1‑col scrollbar] ----------------------------------
        let [table_area, bar_area] = Layout::horizontal([
            Constraint::Min(0),    // table takes remaining width
            Constraint::Length(1), // 1 char for scrollbar
        ])
        .areas(area);

        self.frame.table_area = table_area;

        // ---- 3. Stateful table ---------------------------------------------------------------
        let header = Row::new(
            self.frame
                .columns
                .iter()
                .map(|col| Span::raw(col.name_display)),
        )
        .style(StylePalette::TableHeader.style());

        let highlight_style = if self.has_focus {
            StylePalette::TableRowHighlight.style()
        } else {
            Style::default()
        };

        let rows = self.frame.raw_rows.iter().map(|row| {
            let cells = row.iter().zip(&self.frame.columns).map(|(value, col)| {
                let text = Text::from(value.as_str()).alignment(col.col_align);
                Cell::from(text)
            });
            Row::new(cells)
        });

        let table = Table::new(rows, self.frame.column_constraints.clone())
            .header(header)
            .highlight_symbol("» ")
            .row_highlight_style(highlight_style);

        StatefulWidget::render(table, table_area, buf, &mut self.frame.table_state);

        // ---- 4. Vertical scrollbar -----------------------------------------------------------
        if bar_area.width > 0 && bar_area.height > 0 {
            let total_rows = self.frame.raw_rows.len();
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
}

impl Widget for GridFrameView<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        if self.frame.columns.is_empty() || self.frame.raw_rows.is_empty() {
            self.render_empty(area, buf);
        } else {
            self.render_table(area, buf);
        }
    }
}
