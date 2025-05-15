
use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use super::{
    domain_model::{OrderDirection, DomainType},
    explorer::ExplorerAction,
    utils::{StylePalette, Utils},
};

pub const SAVED_VIEWS: &[SavedView] = &[RECENT_ALERTS, INVALID_ITEMS, CHANGED_TO_INVALID];

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
    pub type_selection: DomainType,
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
    type_selection: DomainType,
    filters: &'static [FilterSpec],
    columns: &'static [ColumnSpec],
) -> SavedView {
    SavedView {
        name,
        desc,
        type_selection,
        filters,
        columns,
    }
}

pub const RECENT_ALERTS: SavedView = sv (
    "Recent Alerts",
    "Alerts (open) ordered by recency",
    DomainType::Alerts,
    RECENT_ALERTS_F,
    RECENT_ALERTS_C,
);

const INVALID_ITEMS: SavedView = sv(
    "Invalid Items",
    "Items with a validity state of 'Invalid'",
    DomainType::Items,
    INVALID_ITEMS_F,
    INVALID_ITEMS_C,
);
const CHANGED_TO_INVALID: SavedView = sv(
    "Changed to Invalid",
    "Changes in which the item's state transitioned to 'Invalid'",
    DomainType::Changes,
    CHANGED_TO_INVALID_F,
    CHANGE_TO_INVALID_C,
);

const RECENT_ALERTS_F: &[FilterSpec] = &[f("alert_status", "O")];
const RECENT_ALERTS_C: &[ColumnSpec] = &[
    c("created_at", true, OrderDirection::Descend),
];

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
    pub name_len: usize,
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
                let pad = format!("{:<width$}", ":", width = (state.name_len + 2) - view.name.len());
                ListItem::new(Line::from(vec![
                    Span::styled(view.name, Style::default().bold()),
                    Span::raw(pad),
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
        let longest = SAVED_VIEWS.iter().map(|v| v.name.len()).max().unwrap_or(0);

        ViewsListState {
            name_len: longest,
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
                .map(ExplorerAction::ApplyView),
            _ => None,
        }
    }
}
