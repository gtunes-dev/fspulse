use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Row, StatefulWidget, Table, TableState, Widget},
};

use super::{
    domain_model::{ColumnInfo, DomainModel, OrderDirection, TypeSelection}, explorer::ExplorerAction, filter_popup::FilterPopupState, utils::{StylePalette, Utils}
};

pub struct ColumnFrame {
    table_state: TableState,
    area: Rect,
}

impl ColumnFrame {
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

    pub fn handle_key(&mut self, model: &mut DomainModel, key: KeyEvent) -> Option<ExplorerAction> {
        let mut explorer_action = None;

        match key.code {
            KeyCode::Char('+') => {
                if let Some(selected) = self.table_state.selected() {
                    if selected > 0 {
                        let current_cols = model.current_columns_mut();
                        current_cols.swap(selected, selected - 1);

                        // maintain the current selection
                        self.table_state.select(Some(selected - 1));

                        explorer_action = Some(ExplorerAction::RefreshQuery(false))
                    }
                }
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                if let Some(selected) = self.table_state.selected() {
                    let current_cols = model.current_columns_mut();
                    if selected < (current_cols.len() - 1) {
                        current_cols.swap(selected, selected + 1);

                        // maintain the current selection
                        self.table_state.select(Some(selected + 1));

                        explorer_action = Some(ExplorerAction::RefreshQuery(false))
                    }
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                explorer_action = self.set_order_direction(model, OrderDirection::Ascend)
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                explorer_action = self.set_order_direction(model, OrderDirection::Descend)
            }
            KeyCode::Right => {
                explorer_action = self.next_order_direction(model);
            }
            KeyCode::Left => explorer_action = self.prev_order_direction(model),
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(selected) = self.table_state.selected() {
                    if let Some(col) = model.current_columns_mut().get_mut(selected) {
                        col.order_direction = OrderDirection::None;

                        explorer_action = Some(ExplorerAction::RefreshQuery(true))
                    }
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(selected) = self.table_state.selected() {
                    if let Some(col) = model.current_columns_mut().get_mut(selected) {
                        col.selected = !col.selected;

                        explorer_action = Some(ExplorerAction::RefreshQuery(false))
                    }
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                if let Some(selected) = self.table_state.selected() {
                    if let Some(col_option) = model.current_columns().get(selected) {
                        explorer_action = Some(ExplorerAction::ShowAddFilter(
                            FilterPopupState::new_add_filter_popup(
                                col_option.name_db,
                                col_option.col_type.info(),
                            ),
                        ));
                    };
                }
            }
            _ => {
                let total_rows = model.current_columns().len();
                let visible_rows = self.visible_rows();
                Utils::handle_table_state_keys(
                    &mut self.table_state,
                    total_rows,
                    visible_rows,
                    key,
                );
            }
        }

        explorer_action
    }

    fn selected_column<'a>(&self, model: &'a mut DomainModel) -> Option<&'a mut ColumnInfo> {
        if let Some(selected) = self.table_state.selected() {
            return model.current_columns_mut().get_mut(selected);
        }
        None
    }

    fn set_order_direction(
        &mut self,
        model: &mut DomainModel,
        order_direction: OrderDirection,
    ) -> Option<ExplorerAction> {
        let mut action = None;

        if let Some(col) = self.selected_column(model) {
            if col.selected && col.order_direction != order_direction {
                col.order_direction = order_direction;

                action = Some(ExplorerAction::RefreshQuery(true))
            }
        };

        action
    }

    fn next_order_direction(&mut self, model: &mut DomainModel) -> Option<ExplorerAction> {
        let mut action = None;

        if let Some(col) = self.selected_column(model) {
            if col.selected {
                let new_order_direction = match col.order_direction {
                    OrderDirection::Ascend => OrderDirection::Descend,
                    OrderDirection::Descend => OrderDirection::None,
                    OrderDirection::None => OrderDirection::Ascend,
                };

                action = self.set_order_direction(model, new_order_direction)
            }
        }

        action
    }

    fn prev_order_direction(&mut self, model: &mut DomainModel) -> Option<ExplorerAction> {
        let mut action = None;

        if let Some(col) = self.selected_column(model) {
            if col.selected {
                let new_order_direction = match col.order_direction {
                    OrderDirection::Ascend => OrderDirection::None,
                    OrderDirection::Descend => OrderDirection::Ascend,
                    OrderDirection::None => OrderDirection::Descend,
                };

                action = self.set_order_direction(model, new_order_direction)
            }
        }

        action
    }

    fn visible_rows(&self) -> usize {
        self.area.height.saturating_sub(2) as usize
    }

    pub fn set_area(&mut self, new_area: Rect) {
        self.area = new_area;
    }

    pub fn frame_title(type_selection: TypeSelection) -> &'static str {
        match type_selection {
            TypeSelection::Items => "Items Columns",
            TypeSelection::Changes => "Changes Columns",
            TypeSelection::Scans => "Scans Columns",
            TypeSelection::Roots => "Roots Columns",
        }
    }
}

pub struct ColumnFrameView<'a> {
    frame: &'a mut ColumnFrame,
    model: &'a DomainModel,
    has_focus: bool,
}

impl<'a> ColumnFrameView<'a> {
    pub fn new(frame: &'a mut ColumnFrame, model: &'a DomainModel, has_focus: bool) -> Self {
        Self {
            frame,
            model,
            has_focus,
        }
    }
}

impl Widget for ColumnFrameView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.frame.set_area(area);
        let mut rows = Vec::new();

        for col in self.model.current_columns() {
            let checked = if col.selected { "[x]" } else { "[ ]" };
            let text = format!("{checked} {:<20}", col.name_db);

            let row = Row::new(vec![
                text,
                col.order_direction.to_display().to_owned(),
                String::default(),
            ]);

            rows.push(row);
        }

        let widths = [
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ];

        let highlight_style = if self.has_focus {
            StylePalette::TableRowHighlight.style()
        } else {
            Style::default()
        };

        let table = Table::new(rows, widths)
            .row_highlight_style(highlight_style)
            .highlight_symbol("» ");
        <Table as StatefulWidget>::render(table, area, buf, &mut self.frame.table_state);

        //f.render_widget(paragraph, area);
    }
}
