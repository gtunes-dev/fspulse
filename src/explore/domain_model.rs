use ratatui::{layout::Alignment, text::Line};

use crate::query::{
    columns::{self, ColType, ColTypeInfo},
    ColMap,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeSelection {
    Items,
    Changes,
    Scans,
    Roots,
}

impl TypeSelection {
    pub fn all_types() -> &'static [TypeSelection] {
        &[
            TypeSelection::Items,
            TypeSelection::Changes,
            TypeSelection::Scans,
            TypeSelection::Roots,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            TypeSelection::Items => "Items",
            TypeSelection::Changes => "Changes",
            TypeSelection::Scans => "Scans",
            TypeSelection::Roots => "Roots",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            TypeSelection::Items => 0,
            TypeSelection::Changes => 1,
            TypeSelection::Scans => 2,
            TypeSelection::Roots => 3,
        }
    }

    pub fn title(&self) -> Line<'static> {
        match self {
            TypeSelection::Items => Line::from("Items (I)"),
            TypeSelection::Changes => Line::from("Changes (C)"),
            TypeSelection::Scans => Line::from("Scans (S)"),
            TypeSelection::Roots => Line::from("Roots (R)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderDirection {
    Ascend,
    Descend,
    None
}

impl OrderDirection {
    pub fn to_display(&self) -> &'static str {
        match self {
            OrderDirection::Ascend => "↑",
            OrderDirection::Descend => "↓",
            OrderDirection::None => " ",
        }
    }

    pub fn to_query_term(&self) -> &'static str {
        match self {
            OrderDirection::Ascend => "ASC",
            OrderDirection::Descend => "DESC",
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColumnInfo {
    pub name_db: &'static str,
    pub name_display: &'static str,
    pub col_align: Alignment,
    pub col_type: ColType,
    pub selected: bool,
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
    pub columns: Vec<ColumnInfo>,
    pub filters: Vec<Filter>,
    pub limit: String,
}

impl DomainState {
    fn new(columns: Vec<ColumnInfo>) -> Self {
        DomainState {
            columns,
            filters: Vec::new(),
            limit: "100".to_owned(),
        }
    }
}

pub struct DomainModel {
    current_type: TypeSelection,
    roots_state: DomainState,
    scans_state: DomainState,
    items_state: DomainState,
    changes_state: DomainState,
}

impl DomainModel {
    pub fn new() -> Self {
        DomainModel {
            current_type: TypeSelection::Items,
            roots_state: DomainState::new(Self::column_options_from_map(
                &columns::ROOTS_QUERY_COLS,
            )),
            scans_state: DomainState::new(Self::column_options_from_map(
                &columns::SCANS_QUERY_COLS,
            )),
            items_state: DomainState::new(Self::column_options_from_map(
                &columns::ITEMS_QUERY_COLS,
            )),
            changes_state: DomainState::new(Self::column_options_from_map(
                &columns::CHANGES_QUERY_COLS,
            )),
        }
    }

    fn current_state(&self) -> &DomainState {
        match self.current_type {
            TypeSelection::Roots => &self.roots_state,
            TypeSelection::Scans => &self.scans_state,
            TypeSelection::Items => &self.items_state,
            TypeSelection::Changes => &self.changes_state,
        }
    }

    fn current_state_mut(&mut self) -> &mut DomainState {
        match self.current_type {
            TypeSelection::Roots => &mut self.roots_state,
            TypeSelection::Scans => &mut self.scans_state,
            TypeSelection::Items => &mut self.items_state,
            TypeSelection::Changes => &mut self.changes_state,
        }
    }

    pub fn current_type(&self) -> TypeSelection {
        self.current_type
    }

    pub fn set_current_type(&mut self, current_type: TypeSelection) {
        self.current_type = current_type;
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

    fn column_options_from_map(col_map: &ColMap) -> Vec<ColumnInfo> {
        col_map
            .entries()
            .map(|(col_name, col_spec)| ColumnInfo {
                name_db: col_name,
                name_display: col_spec.name_display,
                selected: col_spec.is_default,
                col_align: col_spec.col_align.to_ratatui(),
                col_type: col_spec.col_type,
                order_direction: OrderDirection::None,
            })
            .collect()
    }
}
