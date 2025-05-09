use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::Stylize,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use super::{
    domain_model::{OrderDirection, TypeSelection},
    explorer::ExplorerAction,
    utils::StylePalette,
};

pub const SAVED_VIEWS: &[SavedView] = &[INVALID_ITEMS, CHANGED_TO_INVALID];

#[derive(Debug)]
pub struct FilterSpec {
    pub col_name: &'static str,
    pub filter_text: &'static str,
}

#[derive(Debug)]
pub struct ColumnSpec {
    pub col_name: &'static str,
    pub show_col: bool,
    pub order_direction: OrderDirection,
}

#[derive(Clone, Copy, Debug)]
pub struct SavedView {
    pub name: &'static str,
    pub type_selection: TypeSelection,
    pub filters: &'static [FilterSpec],
    pub columns: &'static [ColumnSpec],
}

// Helpers

const fn f(col_name: &'static str, filter_text: &'static str) -> FilterSpec {
    FilterSpec {
        col_name,
        filter_text,
    }
}

const fn c(col_name: &'static str, show_col: bool, order_direction: OrderDirection) -> ColumnSpec {
    ColumnSpec {
        col_name,
        show_col,
        order_direction,
    }
}

const fn sv(
    name: &'static str,
    type_selection: TypeSelection,
    filters: &'static [FilterSpec],
    columns: &'static [ColumnSpec],
) -> SavedView {
    SavedView {
        name,
        type_selection,
        filters,
        columns,
    }
}

// Invalid Items Filter
const INVALID_ITEMS_FILTERS: &[FilterSpec] = &[f("val", "I"), f("is_ts", "false")];
const INVALID_ITEMS_COLUMNS: &[ColumnSpec] = &[
    c("root_id", false, OrderDirection::None),
    c("item_path", true, OrderDirection::Ascend),
    c("last_scan", false, OrderDirection::None),
    c("item_type", false, OrderDirection::None),
    c("is_ts", false, OrderDirection::None),
    c("val_error", true, OrderDirection::None),
];

const INVALID_ITEMS: SavedView = sv(
    "Invalid Items",
    TypeSelection::Items,
    INVALID_ITEMS_FILTERS,
    INVALID_ITEMS_COLUMNS,
);

const CHANGED_TO_INVALID_FILTERS: &[FilterSpec] = &[
    f("val_change", "T"),
    f("val_old", "I, N, U"),
    f("val_new", "I"),
];

const CHANGE_TO_INVALID_COLUMNS: &[ColumnSpec] = &[
    c("item_path", true, OrderDirection::Ascend),
    c("val_error", true, OrderDirection::None),
];
const CHANGED_TO_INVALID: SavedView = sv(
    "Changed to Invalid",
    TypeSelection::Changes,
    CHANGED_TO_INVALID_FILTERS,
    CHANGE_TO_INVALID_COLUMNS,
);

pub struct ViewsState {
    pub list_state: ListState,
}

#[derive(Debug)]
pub struct ViewsWidget;

impl StatefulWidget for ViewsWidget {
    type State = ViewsState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list_block = Block::default()
            .borders(Borders::ALL)
            .style(StylePalette::PopUp.style());
        let inner_area = list_block.inner(area);

        let [header_area, list_area] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(5)]).areas(inner_area);

        list_block.render(area, buf);

        self.render_header(header_area, buf);

        let list_items = SAVED_VIEWS
            .iter()
            .map(|view| ListItem::from(view.name))
            .collect::<Vec<ListItem>>();

        let list = List::new(list_items)
            .highlight_style(StylePalette::TableRowHighlight.style())
            .highlight_symbol("» ");

        StatefulWidget::render(list, list_area, buf, &mut state.list_state);
    }
}

impl ViewsWidget {
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        Paragraph::new("Views").bold().centered().render(area, buf);
    }
}

impl ViewsState {
    pub fn new() -> Self {
        ViewsState {
            list_state: {
                let mut list_state = ListState::default();
                list_state.select(Some(0));
                list_state
            },
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        match key.code {
            KeyCode::Esc => Some(ExplorerAction::Dismiss),
            KeyCode::Up => {
                self.list_state.select_previous();
                None
            }
            KeyCode::Down => {
                self.list_state.select_next();
                None
            }
            KeyCode::Enter => self
                .list_state
                .selected()
                .and_then(|index| SAVED_VIEWS.get(index))
                .map(|saved_view| ExplorerAction::ApplyView(*saved_view)),
            _ => None,
        }
    }
}
