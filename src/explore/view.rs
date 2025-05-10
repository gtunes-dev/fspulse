use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use super::{
    domain_model::{OrderDirection, TypeSelection},
    explorer::ExplorerAction,
    utils::{StylePalette, Utils},
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
    pub desc: &'static str,
    pub desc_long: &'static str,
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
    desc: &'static str,
    desc_long: &'static str,
    type_selection: TypeSelection,
    filters: &'static [FilterSpec],
    columns: &'static [ColumnSpec],
) -> SavedView {
    SavedView {
        name,
        desc,
        desc_long,
        type_selection,
        filters,
        columns,
    }
}

const INVALID_ITEMS: SavedView = sv(
    "Invalid Items",
    "Items with a validity state of 'Invalid'",
    "blah blah",
    TypeSelection::Items,
    INVALID_ITEMS_F,
    INVALID_ITEMS_C,
);
const CHANGED_TO_INVALID: SavedView = sv(
    "Changed to Invalid",
    "Changes in which the item's state transitioned to 'Invalid'",
    "blah blah",
    TypeSelection::Changes,
    CHANGED_TO_INVALID_F,
    CHANGE_TO_INVALID_C,
);

const INVALID_ITEMS_F: &[FilterSpec] = &[f("val", "I"), f("is_ts", "false")];
const INVALID_ITEMS_C: &[ColumnSpec] = &[
    c("root_id", false, OrderDirection::None),
    c("item_path", true, OrderDirection::Ascend),
    c("last_scan", false, OrderDirection::None),
    c("item_type", false, OrderDirection::None),
    c("is_ts", false, OrderDirection::None),
    c("val_error", true, OrderDirection::None),
];

const CHANGED_TO_INVALID_F: &[FilterSpec] = &[
    f("val_change", "T"),
    f("val_old", "I, N, U"),
    f("val_new", "I"),
];
const CHANGE_TO_INVALID_C: &[ColumnSpec] = &[
    c("item_path", true, OrderDirection::Ascend),
    c("val_error", true, OrderDirection::None),
];

pub struct ViewsListState {
    pub list_state: ListState,
}

#[derive(Debug)]
pub struct ViewsListWidget;

impl StatefulWidget for ViewsListWidget {
    type State = ViewsListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list_block = Block::default()
            .borders(Borders::ALL)
            .style(StylePalette::PopUp.style());
        let inner_area = list_block.inner(area);

        let [header_area, list_area, _, help_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .areas(inner_area);

        list_block.render(area, buf);

        self.render_header(header_area, buf);

        let list_items = SAVED_VIEWS
            .iter()
            .map(|view| {
                ListItem::new(Line::from(vec![
                    Span::styled(view.name, Style::default().bold()),
                    Span::raw(": "),
                    Span::raw(view.desc),
                ]))
            })
            .collect::<Vec<ListItem>>();

        let list = List::new(list_items)
            .highlight_style(StylePalette::TableRowHighlight.style())
            .highlight_symbol("» ");

        StatefulWidget::render(list, list_area, buf, &mut state.list_state);

        Utils::render_popup_help("Esc: Cancel  |  Enter: Set Filter", help_area, buf);
    }
}

impl ViewsListWidget {
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        Paragraph::new("Views").bold().centered().render(area, buf);
    }
}

impl ViewsListState {
    pub fn new() -> Self {
        ViewsListState {
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
