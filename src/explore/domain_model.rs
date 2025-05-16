use ratatui::{layout::Alignment, text::Line};

use crate::query::columns::{self, ColType, ColTypeInfo};

use super::view::SavedView;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DomainType {
    Alerts,
    Items,
    Changes,
    Scans,
    Roots,
}

impl DomainType {
    pub fn all_types() -> &'static [DomainType] {
        &[
            DomainType::Alerts,
            DomainType::Items,
            DomainType::Changes,
            DomainType::Scans,
            DomainType::Roots,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DomainType::Alerts => "Alerts",
            DomainType::Items => "Items",
            DomainType::Changes => "Changes",
            DomainType::Scans => "Scans",
            DomainType::Roots => "Roots",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            DomainType::Alerts => 0,
            DomainType::Items => 1,
            DomainType::Changes => 2,
            DomainType::Scans => 3,
            DomainType::Roots => 4,
        }
    }

    pub fn as_title(&self) -> Line<'static> {
        match self {
            DomainType::Alerts => Line::from("🔔 Alerts (A)"),
            DomainType::Items => Line::from("Items (I)"),
            DomainType::Changes => Line::from("Changes (C)"),
            DomainType::Scans => Line::from("Scans (S)"),
            DomainType::Roots => Line::from("Roots (R)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderDirection {
    Ascend,
    Descend,
    None,
}

impl OrderDirection {
    pub fn to_display(self) -> &'static str {
        match self {
            OrderDirection::Ascend => "↑",
            OrderDirection::Descend => "↓",
            OrderDirection::None => " ",
        }
    }

    pub fn to_query_term(self) -> &'static str {
        match self {
            OrderDirection::Ascend => "ASC",
            OrderDirection::Descend => "DESC",
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColSelect {
    ForceSelect,
    Selected,
    NotSelected,
}

#[derive(Debug, Clone, Copy)]
pub struct ColumnInfo {
    pub name_db: &'static str,
    pub name_display: &'static str,
    pub col_align: Alignment,
    pub col_type: ColType,
    pub selected: ColSelect,
    pub order_direction: OrderDirection,
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub col_name: &'static str,
    pub type_name: &'static str,
    pub col_type_info: ColTypeInfo,
    pub filter_text: String,
}

impl Filter {
    pub fn new(
        col_name: &'static str,
        type_name: &'static str,
        col_type_info: ColTypeInfo,
        filter_text: String,
    ) -> Self {
        Filter {
            col_name,
            type_name,
            col_type_info,
            filter_text,
        }
    }
}

struct DomainState {
    pub view: Option<&'static SavedView>,
    pub columns: Vec<ColumnInfo>,
    pub filters: Vec<Filter>,
    pub limit: String,
}

impl DomainState {
    fn new(columns: Vec<ColumnInfo>) -> Self {
        DomainState {
            view: None,
            columns,
            filters: Vec::new(),
            limit: "100".to_owned(),
        }
    }
}

pub struct DomainModel {
    current_type: DomainType,
    alerts_state: DomainState,
    roots_state: DomainState,
    scans_state: DomainState,
    items_state: DomainState,
    changes_state: DomainState,
}

impl DomainModel {
    pub fn new() -> Self {
        DomainModel {
            current_type: DomainType::Alerts,
            alerts_state: DomainState::new(Self::column_options_from_map(DomainType::Alerts)),
            roots_state: DomainState::new(Self::column_options_from_map(DomainType::Roots)),
            scans_state: DomainState::new(Self::column_options_from_map(DomainType::Scans)),
            items_state: DomainState::new(Self::column_options_from_map(DomainType::Items)),
            changes_state: DomainState::new(Self::column_options_from_map(DomainType::Changes)),
        }
    }

    fn current_state(&self) -> &DomainState {
        match self.current_type {
            DomainType::Alerts => &self.alerts_state,
            DomainType::Roots => &self.roots_state,
            DomainType::Scans => &self.scans_state,
            DomainType::Items => &self.items_state,
            DomainType::Changes => &self.changes_state,
        }
    }

    fn current_state_mut(&mut self) -> &mut DomainState {
        match self.current_type {
            DomainType::Alerts => &mut self.alerts_state,
            DomainType::Roots => &mut self.roots_state,
            DomainType::Scans => &mut self.scans_state,
            DomainType::Items => &mut self.items_state,
            DomainType::Changes => &mut self.changes_state,
        }
    }

    pub fn current_type(&self) -> DomainType {
        self.current_type
    }

    pub fn set_current_type(&mut self, current_type: DomainType) {
        self.current_type = current_type;
    }

    pub fn current_view(&self) -> &Option<&SavedView> {
        &self.current_state().view
    }

    pub fn set_current_view(&mut self, view: Option<&'static SavedView>) {
        self.current_state_mut().view = view
    }

    pub fn current_columns(&self) -> &Vec<ColumnInfo> {
        &self.current_state().columns
    }

    pub fn current_columns_mut(&mut self) -> &mut Vec<ColumnInfo> {
        self.current_state_mut().columns.as_mut()
    }

    pub fn current_filters(&self) -> &Vec<Filter> {
        &self.current_state().filters
    }

    pub fn current_filters_mut(&mut self) -> &mut Vec<Filter> {
        self.current_state_mut().filters.as_mut()
    }

    pub fn current_limit(&self) -> String {
        self.current_state().limit.clone()
    }

    pub fn set_current_limit(&mut self, new_limit: String) {
        self.current_state_mut().limit = new_limit;
    }

    pub fn reset_current_columns(&mut self) {
        match self.current_type {
            DomainType::Alerts => {
                self.alerts_state.columns = Self::column_options_from_map(DomainType::Alerts);

                let alert_col = self
                    .alerts_state
                    .columns
                    .iter_mut()
                    .find(|col_info| col_info.name_db == "alert_id");
                if let Some(col_info) = alert_col {
                    col_info.selected = ColSelect::ForceSelect;
                }
            }
            DomainType::Roots => {
                self.roots_state.columns = Self::column_options_from_map(DomainType::Roots)
            }
            DomainType::Scans => {
                self.scans_state.columns = Self::column_options_from_map(DomainType::Scans)
            }
            DomainType::Items => {
                self.items_state.columns = Self::column_options_from_map(DomainType::Items)
            }
            DomainType::Changes => {
                self.changes_state.columns = Self::column_options_from_map(DomainType::Changes)
            }
        }
    }

    fn column_options_from_map(type_selection: DomainType) -> Vec<ColumnInfo> {
        let col_map = match type_selection {
            DomainType::Alerts => &columns::ALERTS_QUERY_COLS,
            DomainType::Roots => &columns::ROOTS_QUERY_COLS,
            DomainType::Scans => &columns::SCANS_QUERY_COLS,
            DomainType::Items => &columns::ITEMS_QUERY_COLS,
            DomainType::Changes => &columns::CHANGES_QUERY_COLS,
        };

        col_map
            .entries()
            .map(|(col_name, col_spec)| ColumnInfo {
                name_db: col_name,
                name_display: col_spec.name_display,
                selected: match col_spec.is_default {
                    true => ColSelect::Selected,
                    false => ColSelect::NotSelected,
                },
                col_align: col_spec.col_align.to_ratatui(),
                col_type: col_spec.col_type,
                order_direction: OrderDirection::None,
            })
            .collect()
    }
}
