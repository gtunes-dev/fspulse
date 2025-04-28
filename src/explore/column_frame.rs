use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::query::{columns, ColMap};

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

pub struct ColumnOption {
    pub name: &'static str,
    pub selected: bool,
}

pub struct ColumnFrame {
    pub selected_type: TypeSelection,
    pub cursor_position: usize,
    pub dropdown_open: bool,
    pub dropdown_cursor: usize,
    pub scroll_offset: usize,
    pub area: Rect,

    pub roots_columns: Vec<ColumnOption>,
    pub scans_columns: Vec<ColumnOption>,
    pub items_columns: Vec<ColumnOption>,
    pub changes_columns: Vec<ColumnOption>,
}

impl ColumnFrame {
    pub fn current_columns(&self) -> &Vec<ColumnOption> {
        match self.selected_type {
            TypeSelection::Roots => &self.roots_columns,
            TypeSelection::Scans => &self.scans_columns,
            TypeSelection::Items => &self.items_columns,
            TypeSelection::Changes => &self.changes_columns,
        }
    }

    pub fn current_columns_mut(&mut self) -> &mut Vec<ColumnOption> {
        match self.selected_type {
            TypeSelection::Roots => &mut self.roots_columns,
            TypeSelection::Scans => &mut self.scans_columns,
            TypeSelection::Items => &mut self.items_columns,
            TypeSelection::Changes => &mut self.changes_columns,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down => {
                if self.dropdown_open {
                    if self.dropdown_cursor + 1 < TypeSelection::all_types().len() {
                        self.dropdown_cursor += 1;
                    }
                } else {
                    self.move_down();
                }
            }
            KeyCode::Up => {
                if self.dropdown_open {
                    if self.dropdown_cursor > 0 {
                        self.dropdown_cursor -= 1;
                    }
                } else {
                    self.move_up();
                }
            }
            KeyCode::Char('+') => {
                if !self.dropdown_open && self.cursor_position >= 1 {
                    let idx = self.cursor_position - 1;
                    if idx > 0 {
                        let current_cols = self.current_columns_mut();
                        let item = current_cols.remove(idx);
                        current_cols.insert(idx - 1, item);

                        // maintain the current selection
                        self.cursor_position -= 1;
                    }
                }
            }
            KeyCode::Char('-') => {
                if !self.dropdown_open && self.cursor_position >= 1 {
                    let idx = self.cursor_position - 1;
                    let current_cols = self.current_columns_mut();
                    if idx < current_cols.len() - 1 {
                        let item = current_cols.remove(idx);
                        current_cols.insert(idx + 1, item);

                        // maintain the current selection
                        self.cursor_position += 1;
                    }
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if self.dropdown_open {
                    // Select the highlighted type
                    self.selected_type = TypeSelection::all_types()[self.dropdown_cursor];
                    self.dropdown_open = false;
                    self.cursor_position = 0; // return cursor to Type line
                } else if self.cursor_position == 0 {
                    // Open dropdown if cursor is on Type
                    self.dropdown_open = true;
                    self.dropdown_cursor = self.selected_type.index();
                } else {
                    let idx = self.cursor_position - 1;
                    if let Some(col) = self.current_columns_mut().get_mut(idx) {
                        col.selected = !col.selected;
                    }
                }
            }
            KeyCode::Esc => {
                if self.dropdown_open {
                    // Cancel dropdown
                    self.dropdown_open = false;
                }
            }
            _ => {}
        }
    }
}

impl ColumnFrame {
    pub fn new() -> Self {
        Self {
            selected_type: TypeSelection::Roots,
            cursor_position: 0,
            dropdown_open: false,
            dropdown_cursor: 0,
            scroll_offset: 0,
            area: Rect::default(),
            roots_columns: Self::column_options_from_map(&columns::ROOTS_QUERY_COLS),
            scans_columns: Self::column_options_from_map(&columns::SCANS_QUERY_COLS),
            items_columns: Self::column_options_from_map(&columns::ITEMS_QUERY_COLS),
            changes_columns: Self::column_options_from_map(&columns::CHANGES_QUERY_COLS),
        }
    }

    pub fn column_options_from_map(col_map: &ColMap) -> Vec<ColumnOption> {
        col_map
            .entries()
            .map(|(col_name, col_spec)| ColumnOption {
                name: col_name,
                selected: col_spec.is_default,
            })
            .collect()
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, is_focused: bool) {
        self.set_area(area);
        let mut lines = Vec::new();

        // Always draw the collapsed Type selector line
        let type_display = format!(" {} ▼ ", self.selected_type.name()); // Add spaces around name
        let mut type_line = Line::from(vec![
            "Type: ".into(),
            type_display.clone().bg(Color::Blue).fg(Color::White),
        ]);

        if self.cursor_position == 0 && is_focused && !self.dropdown_open {
            type_line = type_line.style(Style::default().fg(Color::Yellow).bold());
        }

        lines.push(type_line);

        // Spacer
        lines.push(Line::from(" "));

        let visible_rows = self.visible_rows();
        for (i, col) in self
            .current_columns()
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_rows)
        {
            let checked = if col.selected { "[x]" } else { "[ ]" };

            let text = format!("{checked} {:<20}", col.name);

            let mut line = Line::from(text);

            if self.cursor_position == i + 1 && is_focused && !self.dropdown_open {
                line = line.style(Style::default().fg(Color::Yellow).bold());
            }

            lines.push(line);
        }

        let block = if is_focused {
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title("Type & Columns")
        } else {
            Block::default()
                .borders(Borders::ALL)
                .title("Type & Columns")
        };

        let paragraph = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left);

        f.render_widget(paragraph, area);
    }

    pub fn move_up(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;

            if self.cursor_position >= 1 {
                let column_idx = self.cursor_position - 1;
                if column_idx < self.scroll_offset {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_position < self.current_columns().len() {
            self.cursor_position += 1;

            if self.cursor_position >= 1 {
                let column_idx = self.cursor_position - 1;
                let visible_rows = self.visible_rows();

                if column_idx >= self.scroll_offset + visible_rows {
                    self.scroll_offset += 1;
                }
            }
        }
    }

    pub fn draw_dropdown(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = TypeSelection::all_types()
            .iter()
            .map(|ty| ListItem::new(ty.name()))
            .collect();

        let mut state = ListState::default();
        state.select(Some(self.dropdown_cursor));

        let list = List::new(items)
            .block(
                Block::default()
                    .title("Select Type")
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Double),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut state);
    }

    fn visible_rows(&self) -> usize {
        self.area.height.saturating_sub(4) as usize
    }

    pub fn set_area(&mut self, new_area: Rect) {
        if self.area.height == new_area.height {
            self.area = new_area;
        } else {
            let old_area = self.area;

            self.area = new_area;
            let new_visible_rows = self.visible_rows();

            if old_area.height != new_area.height {
                self.correct_scroll_for_resize(new_visible_rows);
            }
        }
    }

    fn correct_scroll_for_resize(&mut self, new_visible_rows: usize) {
        if self.cursor_position >= 1 {
            let column_idx = self.cursor_position - 1;
    
            if column_idx < self.scroll_offset {
                self.scroll_offset = column_idx;
            } else if column_idx >= self.scroll_offset + new_visible_rows {
                self.scroll_offset = column_idx.saturating_sub(new_visible_rows.saturating_sub(1));
            } else {
                let max_scroll_offset = column_idx.saturating_sub(new_visible_rows.saturating_sub(1));
                if self.scroll_offset > max_scroll_offset {
                    self.scroll_offset = max_scroll_offset;
                }
            }
        }
    }
}
