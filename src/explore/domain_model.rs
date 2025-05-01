use ratatui::layout::Alignment;

use crate::query::{
    columns::{self, ColType},
    ColMap,
};

#[derive(Debug, Clone, Copy)]
pub enum TypeSelection {
    Roots,
    Scans,
    Items,
    Changes,
}

impl TypeSelection {
    pub fn all_types() -> &'static [TypeSelection] {
        &[
            TypeSelection::Roots,
            TypeSelection::Scans,
            TypeSelection::Items,
            TypeSelection::Changes,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            TypeSelection::Roots => "Roots",
            TypeSelection::Scans => "Scans",
            TypeSelection::Items => "Items",
            TypeSelection::Changes => "Changes",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            TypeSelection::Roots => 0,
            TypeSelection::Scans => 1,
            TypeSelection::Items => 2,
            TypeSelection::Changes => 3,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColInfo {
    pub col_align: Alignment,
    pub col_type: ColType,
}

pub struct ColumnOption {
    pub name: &'static str,
    pub selected: bool,
    pub col_info: ColInfo,
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub col_name: &'static str,
    pub type_name: &'static str,
    pub filter_text: String,
}

impl Filter {
    pub fn new(col_name: &'static str, type_name: &'static str, filter_text: String) -> Self {
        Filter {
            col_name,
            type_name,
            filter_text,
        }
    }
}

struct DomainState {
    pub columns: Vec<ColumnOption>,
    pub filters: Vec<Filter>,
}

impl DomainState {
    fn new(columns: Vec<ColumnOption>) -> Self {
        DomainState {
            columns,
            filters: Vec::new(),
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

    pub fn current_columns(&self) -> &Vec<ColumnOption> {
        &self.current_state().columns
    }

    pub fn current_columns_mut(&mut self) -> &mut Vec<ColumnOption> {
        self.current_state_mut().columns.as_mut()
    }

    pub fn current_filters(&self) -> &Vec<Filter> {
        &self.current_state().filters
    }

    pub fn current_filters_mut(&mut self) -> &mut Vec<Filter> {
        self.current_state_mut().filters.as_mut()
    }

    fn column_options_from_map(col_map: &ColMap) -> Vec<ColumnOption> {
        col_map
            .entries()
            .map(|(col_name, col_spec)| ColumnOption {
                name: col_name,
                selected: col_spec.is_default,
                col_info: ColInfo {
                    col_align: col_spec.col_align.to_ratatui(),
                    col_type: col_spec.col_type,
                },
            })
            .collect()
    }
}
