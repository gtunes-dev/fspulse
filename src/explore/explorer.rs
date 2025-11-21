use crate::alerts::{AlertStatus, Alerts};
use crate::query::columns::{ColType, ColTypeInfo};
use crate::query::QueryProcessor;
use crate::{database::Database, error::FsPulseError};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, Clear, StatefulWidget, Tabs};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Borders, Paragraph},
    Terminal,
};
use ratatui::{border, symbols, Frame};
use std::io;

use super::column_frame::ColumnFrameView;
use super::domain_model::{ColSelect, ColumnInfo, DomainModel, DomainType, Filter, OrderDirection};
use super::filter_frame::{FilterFrame, FilterFrameView};
use super::filter_popup::{FilterPopupState, FilterPopupWidget};
use super::grid_frame::GridFrameView;
use super::input_box::{InputBoxState, InputBoxWidget};
use super::limit_widget::LimitWidget;
use super::message_box::{MessageBoxState, MessageBoxType};
use super::path_popup::{PathPopupState, PathPopupWidget};
use super::utils::{StylePalette, Utils};
use super::view::{SavedView, ViewsListState, ViewsListWidget, RECENT_ALERTS};
use super::{column_frame::ColumnFrame, grid_frame::GridFrame};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Focus {
    Tabs,
    Filters,
    DataGrid,
    ColumnSelector,
    Limit,
}

#[derive(Debug)]
pub enum ExplorerAction {
    Dismiss,
    RefreshQuery(bool),
    ShowAddFilter(&'static str, ColTypeInfo),
    ShowMessage(MessageBoxState),
    AddFilter(Filter),
    ShowEditFilter(usize),
    DeleteFilter(usize),
    UpdateFilter(usize, String),
    ShowLimit,
    SetLimit(String),
    ApplyView(&'static SavedView),
    SetAlertStatus(AlertStatus),
    AddRoot(String),
}

enum ActivePopup {
    Filter(FilterPopupState),
    InputBox(InputBoxState),
    MessageBox(MessageBoxState),
    Path(PathPopupState),
    ViewsList(ViewsListState),
}

pub struct Explorer {
    model: DomainModel,
    focus: Focus,
    needs_query_refresh: bool,
    query_resets_selection: bool,
    column_frame: ColumnFrame,
    grid_frame: GridFrame,
    filter_frame: FilterFrame,
    filter_frame_collapsed: bool,
    active_popups: Vec<ActivePopup>,
}

impl Explorer {
    pub fn new() -> Self {
        let mut explorer = Self {
            model: DomainModel::new(),
            focus: Focus::DataGrid,
            needs_query_refresh: true,
            query_resets_selection: true,
            column_frame: ColumnFrame::new(),
            grid_frame: GridFrame::new(),
            filter_frame: FilterFrame::new(),
            filter_frame_collapsed: false,
            active_popups: Vec::new(),
        };

        explorer.apply_view(&RECENT_ALERTS);

        explorer
    }

    fn draw(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), FsPulseError> {
        terminal.draw(|f| {
            let full_area = f.area(); // updated here

            // Create layouts and area rects
            let [_, tabs_area, _, view_area, filters_area, center_area, help_area] = Layout::new(
                Direction::Vertical,
                [
                    Constraint::Length(1),                          // Spacer
                    Constraint::Length(1),                          // Tabs
                    Constraint::Length(1),                          // Spacer
                    Constraint::Length(2),                          // View
                    Constraint::Length(self.filter_frame_height()), // Filters
                    Constraint::Min(0),                             // Main content
                    Constraint::Length(1),                          // Help/status
                ],
            )
            .areas(full_area);

            let [left, data] = Layout::new(
                Direction::Horizontal,
                [
                    Constraint::Length(30), // Left (Type + Columns)
                    Constraint::Min(0),     // Right (Data Grid)
                ],
            )
            .areas(center_area);

            // layout the left chunk
            let [limit, columns] = Layout::new(
                Direction::Vertical,
                [Constraint::Length(2), Constraint::Fill(1)],
            )
            .areas(left);

            // -- Borders
            // View

            let view_block_set = symbols::border::Set {
                bottom_left: symbols::line::NORMAL.bottom_left,
                bottom_right: symbols::line::NORMAL.bottom_right,
                ..symbols::border::PLAIN
            };
            let view_block = Block::default()
                .border_set(view_block_set)
                .borders(border!(TOP, LEFT, RIGHT))
                .title("View");
            f.render_widget(&view_block, view_area);

            // Filter
            let filter_block_set = symbols::border::Set {
                top_left: symbols::line::NORMAL.vertical_right,
                top_right: symbols::line::NORMAL.vertical_left,
                bottom_left: symbols::line::NORMAL.bottom_left,
                bottom_right: symbols::line::NORMAL.bottom_right,
                ..symbols::border::PLAIN
            };
            let filter_block = Block::default()
                .border_set(filter_block_set)
                .borders(border!(TOP, LEFT, RIGHT))
                .title(FilterFrame::frame_title(self.model.current_type()));
            f.render_widget(&filter_block, filters_area);

            // Limit
            let limit_block_set = symbols::border::Set {
                top_left: symbols::line::NORMAL.vertical_right,
                top_right: symbols::line::NORMAL.horizontal_down,
                bottom_left: symbols::line::NORMAL.vertical_right,
                ..symbols::border::PLAIN
            };
            let limit_block = Block::default()
                .border_set(limit_block_set)
                .borders(border!(TOP, LEFT))
                .title("Row Limit");

            f.render_widget(&limit_block, limit);

            // Columns
            let columns_block_set = symbols::border::Set {
                top_left: symbols::line::NORMAL.vertical_right,
                top_right: symbols::line::NORMAL.vertical_left,
                ..symbols::border::PLAIN
            };
            let columns_block = Block::default()
                .border_set(columns_block_set)
                .borders(border!(TOP, LEFT, BOTTOM))
                .title(ColumnFrame::frame_title(self.model.current_type()));
            f.render_widget(&columns_block, columns);

            // Data
            let data_border_set = symbols::border::Set {
                top_left: symbols::line::NORMAL.horizontal_down,
                top_right: symbols::line::NORMAL.vertical_left,
                ..symbols::border::PLAIN
            };
            let data_block = Block::default()
                .border_set(data_border_set)
                .borders(border!(ALL))
                //       .border_type(BorderType::Plain)
                .title(GridFrame::frame_title(self.model.current_type()));

            f.render_widget(&data_block, data);

            let titles = DomainType::all_types().iter().map(|t| t.as_title());

            let mut tabs_width: u16 = DomainType::all_types()
                .iter()
                .map(|l| l.as_title().width() as u16) // Line::width() is in ratatui >=0.25
                .sum::<u16>()
                + ((titles.len().saturating_sub(1)) as u16);

            tabs_width += 3 * 3; // " * " in between each tab

            let tabs_rect: Rect = Utils::center_rect(
                tabs_area,
                Constraint::Length(tabs_width),
                Constraint::Length(tabs_area.height),
            );

            let tabs_highlight = match self.focus {
                Focus::Tabs => StylePalette::TabFocusHighlight.style(),
                _ => StylePalette::TabHighlight.style(),
            };

            let tabs_block = Block::default();

            let tabs = Tabs::new(titles)
                .block(tabs_block)
                .style(StylePalette::Tab.style())
                .highlight_style(tabs_highlight)
                .divider(symbols::DOT)
                .select(self.model.current_type().index());
            f.render_widget(tabs, tabs_rect);

            self.render_view_frame(view_block.inner(view_area), f);

            self.render_filter_frame(filter_block.inner(filters_area), f);

            let limit_widget = LimitWidget::new(
                self.model.current_limit(),
                matches!(self.focus, Focus::Limit),
            );
            f.render_widget(limit_widget, limit_block.inner(limit));

            let column_frame_view = ColumnFrameView::new(
                &mut self.column_frame,
                &self.model,
                matches!(self.focus, Focus::ColumnSelector),
            );
            f.render_widget(column_frame_view, columns_block.inner(columns));

            // Draw right (data grid)
            let grid_frame_view = GridFrameView::new(
                &mut self.grid_frame,
                &self.model,
                matches!(self.focus, Focus::DataGrid),
            );
            f.render_widget(grid_frame_view, data_block.inner(data));

            let help_paragraph = Paragraph::new(self.help_text())
                .style(Style::default().bg(Color::Blue).fg(Color::White));
            //.block(help_block);
            f.render_widget(help_paragraph, help_area);

            self.render_popups(f);
        })?;

        Ok(())
    }

    fn render_popups(&mut self, f: &mut Frame) {
        for popup in self.active_popups.iter_mut() {
            Self::render_popup(f, popup);
        }
    }

    fn render_popup(f: &mut Frame, active_popup: &mut ActivePopup) {
        match active_popup {
            ActivePopup::Filter(ref mut state) => {
                render_centered_popup(f, FilterPopupWidget, state, 80, 12);
            }
            ActivePopup::InputBox(ref mut state) => {
                render_centered_popup(f, InputBoxWidget, state, 80, 10);
            }
            ActivePopup::MessageBox(ref mut state) => {
                let input_rect = Utils::center_rect(
                    f.area(),
                    Constraint::Percentage(60),
                    Constraint::Length(10),
                );
                f.render_widget(Clear, input_rect);
                state.draw(f);
            }
            ActivePopup::Path(ref mut state) => {
                render_centered_popup(f, PathPopupWidget, state, 80, 10);
            }
            ActivePopup::ViewsList(ref mut state) => {
                render_centered_popup(f, ViewsListWidget, state, 80, 12);
            }
        }
    }

    pub fn filter_frame_height(&self) -> u16 {
        match self.filter_frame_collapsed {
            true => 2,
            false => 8,
        }
    }

    pub fn render_view_frame(&mut self, area: Rect, f: &mut Frame) {
        let desc = if let Some(view) = self.model.current_view() {
            view.desc
        } else {
            "No Current View"
        };

        let view_desc = format!("View (V): {desc}");

        let p = Paragraph::new(view_desc).alignment(Alignment::Center);
        f.render_widget(p, area);
    }

    pub fn render_filter_frame(&mut self, area: Rect, f: &mut Frame) {
        if self.filter_frame_collapsed {
            let count_str = if self.model.current_filters().is_empty() {
                "No Filters".to_owned()
            } else {
                format!("{} Hidden", self.model.current_filters().len())
            };

            let p = Paragraph::new(format!("Filters - {count_str} (ctrl-f to expand)")).centered();
            f.render_widget(p, area);
        } else {
            let filter_frame_view = FilterFrameView::new(
                &mut self.filter_frame,
                &self.model,
                matches!(self.focus, Focus::Filters),
            );
            f.render_widget(filter_frame_view, area);
        }
    }

    pub fn explore(&mut self) -> Result<(), FsPulseError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            if self.needs_query_refresh {
                self.refresh_query();
            }
            self.draw(&mut terminal)?;

            // Handle input
            if event::poll(std::time::Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    if self.popup_handle_key(key) {
                        continue;
                    }

                    match (key.code, key.modifiers) {
                        (KeyCode::Char('!'), _) => {
                            self.active_popups
                                .push(ActivePopup::Path(PathPopupState::new(
                                    "Choose a path:".into(),
                                    None,
                                )))
                        }
                        (KeyCode::Char('a'), _) | (KeyCode::Char('A'), _) => {
                            self.set_current_type(DomainType::Alerts)
                        }
                        (KeyCode::Char('i'), _) | (KeyCode::Char('I'), _) => {
                            self.set_current_type(DomainType::Items)
                        }
                        (KeyCode::Char('c'), _) | (KeyCode::Char('C'), _) => {
                            self.set_current_type(DomainType::Changes)
                        }
                        (KeyCode::Char('s'), _) | (KeyCode::Char('S'), _) => {
                            self.set_current_type(DomainType::Scans)
                        }
                        (KeyCode::Char('r'), _) | (KeyCode::Char('R'), _) => {
                            self.set_current_type(DomainType::Roots)
                        }
                        (KeyCode::Char('v'), _) | (KeyCode::Char('V'), _) => {
                            self.show_views();
                        }
                        (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                            self.filter_frame_collapsed = !self.filter_frame_collapsed;
                            if self.focus == Focus::Filters {
                                self.focus = self.next_focus(self.focus)
                            }
                        }
                        // Show Limit Input
                        (KeyCode::Char('l'), _) | (KeyCode::Char('L'), _) => {
                            self.show_limit_input();
                        }
                        // Quit
                        (KeyCode::Char('q'), _) | (KeyCode::Char('Q'), _) => {
                            break;
                        }
                        (KeyCode::Tab, _) => {
                            self.focus = self.next_focus(self.focus);
                        }
                        (KeyCode::BackTab, _) => {
                            self.focus = self.prev_focus(self.focus);
                        }
                        _ => {
                            self.dispatch_key_to_active_frame(key);
                        }
                    }
                }
            }
        }

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    fn popup_handle_key(&mut self, key: KeyEvent) -> bool {
        let mut handled = true;

        let action = match self.popups_last_mut() {
            Some(ActivePopup::Filter(state)) => state.handle_key(key),
            Some(ActivePopup::InputBox(state)) => state.handle_key(key),
            Some(ActivePopup::MessageBox(state)) => state.handle_key(key),
            Some(ActivePopup::Path(state)) => state.handle_key(key),
            Some(ActivePopup::ViewsList(state)) => state.handle_key(key),
            None => {
                handled = false;
                None
            }
        };

        match action {
            Some(ExplorerAction::Dismiss) => self.popups_pop(),
            Some(ExplorerAction::ShowMessage(message_box)) => {
                self.active_popups
                    .push(ActivePopup::MessageBox(message_box));
            }
            Some(ExplorerAction::AddFilter(filter)) => {
                self.model.current_filters_mut().push(filter);
                self.popups_pop();
                self.filter_frame
                    .set_selected(self.model.current_filters().len());

                self.needs_query_refresh = true;
                self.query_resets_selection = true;
            }
            Some(ExplorerAction::UpdateFilter(filter_index, new_filter_text)) => {
                if let Some(filter) = self.model.current_filters_mut().get_mut(filter_index) {
                    if filter.filter_text != new_filter_text {
                        filter.filter_text = new_filter_text;
                        self.needs_query_refresh = true;
                        self.query_resets_selection = true;
                    }
                }
                self.popups_pop();
            }
            Some(ExplorerAction::SetLimit(new_limit)) => {
                self.model.set_current_limit(new_limit);
                self.popups_pop();

                self.needs_query_refresh = true;
                self.query_resets_selection = false;
            }
            Some(ExplorerAction::ApplyView(saved_view)) => {
                self.popups_pop();
                self.apply_view(saved_view);
            }
            Some(ExplorerAction::AddRoot(_path)) => {}

            _ => {}
        };

        handled
    }

    fn dispatch_key_to_active_frame(&mut self, key: KeyEvent) {
        let action = match self.focus {
            Focus::Tabs => self.handle_tab_section_key(key),
            Focus::Limit => LimitWidget::handle_key(key),
            Focus::ColumnSelector => self.column_frame.handle_key(&mut self.model, key),
            Focus::DataGrid => self.grid_frame.handle_key(key),
            Focus::Filters => self.filter_frame.handle_key(&self.model, key),
        };

        if let Some(action) = action {
            match action {
                ExplorerAction::RefreshQuery(reset_selection) => {
                    self.needs_query_refresh = true;
                    self.query_resets_selection = reset_selection;
                }
                ExplorerAction::ShowMessage(message_box) => {
                    self.popups_push(ActivePopup::MessageBox(message_box));
                }
                ExplorerAction::ShowLimit => self.show_limit_input(),
                ExplorerAction::ShowAddFilter(name_db, col_type_info) => {
                    let filter_popup =
                        FilterPopupState::new_add_filter_popup(name_db, col_type_info);

                    self.popups_push(ActivePopup::Filter(filter_popup));
                }
                ExplorerAction::DeleteFilter(filter_index) => {
                    self.model.current_filters_mut().remove(filter_index);
                    if filter_index > self.model.current_filters().len() {
                        self.filter_frame.set_selected(filter_index - 1);
                    }

                    self.needs_query_refresh = true;
                    self.query_resets_selection = true;
                }
                ExplorerAction::ShowEditFilter(filter_index) => {
                    if let Some(filter) = self.model.current_filters().get(filter_index) {
                        let edit_filter_popup = FilterPopupState::new_edit_filter_popup(
                            filter.col_name,
                            filter_index,
                            filter.col_type_info,
                            filter.filter_text.to_owned(),
                        );
                        self.popups_push(ActivePopup::Filter(edit_filter_popup));
                    }
                }
                ExplorerAction::SetAlertStatus(new_status) => {
                    self.set_alert_status(new_status);
                }
                _ => unreachable!(),
            }
        }
    }

    fn next_focus(&self, current_focus: Focus) -> Focus {
        let mut next = match current_focus {
            Focus::Tabs => Focus::Filters,
            Focus::Filters => Focus::Limit,
            Focus::Limit => Focus::ColumnSelector,
            Focus::ColumnSelector => Focus::DataGrid,
            Focus::DataGrid => Focus::Tabs,
        };

        // if necessary, recurse until we find a focusable frame
        if next == Focus::Filters && self.filter_frame_collapsed {
            next = self.next_focus(next);
        }

        next
    }

    fn prev_focus(&self, current_focus: Focus) -> Focus {
        let mut prev = match current_focus {
            Focus::Tabs => Focus::DataGrid,
            Focus::DataGrid => Focus::ColumnSelector,
            Focus::ColumnSelector => Focus::Limit,
            Focus::Limit => Focus::Filters,
            Focus::Filters => Focus::Tabs,
        };

        // if necessary, recurse until we find a focusable frame
        if prev == Focus::Filters && self.filter_frame_collapsed {
            prev = self.prev_focus(prev)
        }

        prev
    }

    // Help String Pattern (for all sections):
    //
    // - Each section’s help string uses the same structure and order of keybindings.
    // - The pattern is:
    //
    //   [Section-specific keys]  |  Tab: Next Section  |  [Global shortcuts]  |  Q: Quit
    //
    // - Section-specific keys:
    //     - Navigation (e.g., ↑↓, ←→, PgUp/PgDn, Home/End)
    //     - Actions (e.g., Enter, Space, Del, +/-)
    // - Global shortcuts:
    //     - Typed letters that invoke app-wide actions (e.g., I/C/S/R to change type, L to edit limit)
    //
    // - All keys use uppercase letters (e.g., Q, L) for consistency.
    // - Delimit sections with " | ".
    // - Avoid including the name of the focused frame unless necessary (focus is indicated visually).
    fn help_text(&self) -> &'static str {
        match self.focus {
            Focus::Tabs => "← →: Switch Type  |  Tab: Next Section  |  I/C/S/R: Switch to Items/Changes/Scans/Roots  |  q: Quit",
            Focus::Limit => "Space/Enter: Edit Limit  |  Tab: Next Section  |  L: Edit Limit  |  q: Quit",
            Focus::Filters => "↑↓: Navigate  |  Space/Enter: Edit  |  Del: Delete  |  Tab: Next Section  |  Q: Quit",
            Focus::ColumnSelector => "↑↓: Navigate  |  Space/Enter: Toggle  |  + / -: Reorder  |  F: Add Filter  |  Tab: Next Section  |  Q: Quit",
            Focus::DataGrid => "↑↓: Scroll  |  PgUp/PgDn: Page  |  Home/End: Top/Bottom  |  Tab: Next Section  |  Q: Quit",
        }
    }

    fn build_query_and_columns(&mut self) -> Result<(String, Vec<ColumnInfo>), FsPulseError> {
        let mut cols = Vec::new();

        // Build the new query
        let mut query = self.model.current_type().as_str().to_ascii_lowercase();

        // Build the where clause
        let mut first_filter = true;
        for filter in self.model.current_filters() {
            if first_filter {
                query.push_str(" where ");
                first_filter = false;
            } else {
                query.push_str(", ");
            }
            query.push_str(filter.col_name);
            query.push_str(":(");
            query.push_str(&filter.filter_text);
            query.push(')')
        }

        // Build the "show" clause and cols vector
        // It's by design that we allow a query with no visible columns
        // We'll display an empty result with an explanatory message
        // in the data grid
        let mut first_col = true;
        for col in self.model.current_columns() {
            if col.selected == ColSelect::Selected || col.selected == ColSelect::ForceSelect {
                match first_col {
                    true => {
                        query.push_str(" show ");
                        first_col = false
                    }
                    false => query.push_str(", "),
                }

                cols.push(*col);

                query.push_str(col.name_db);
                // append format specifiers
                match col.col_type {
                    ColType::AlertStatus | ColType::AlertType => query.push_str("@full"),
                    _ => {}
                }
            }
        }

        // Implement Order By.
        // TODO - there's a caveat to this implemenation which is that the user
        // can put an order-by directive on a hidden column. Since SQL requires
        // order by to only be on columns in the select list, we current just skip
        // hidden columns in this traversal. Consider doing something smarter like
        // including the hidden columns in the query but not displaying them in the
        // UI. Not sure this is worth it, though
        first_col = true;
        for col in self.model.current_columns() {
            if (col.selected == ColSelect::Selected || col.selected == ColSelect::ForceSelect)
                && col.order_direction != OrderDirection::None
            {
                match first_col {
                    true => {
                        query.push_str(" order by ");
                        first_col = false;
                    }
                    false => query.push_str(", "),
                }
                query.push_str(&format!(
                    "{} {}",
                    col.name_db,
                    col.order_direction.to_query_term()
                ));
            }
        }

        // Build the Limit clause
        let limit = self.model.current_limit();
        if !limit.is_empty() {
            query.push_str(" limit ");
            query.push_str(&limit);
        }

        Ok((query, cols))
    }

    fn refresh_query(&mut self) {
        match self.refresh_query_impl() {
            Ok(()) => {}
            Err(err) => self.popups_push(ActivePopup::MessageBox(MessageBoxState::new(
                MessageBoxType::Error,
                err.to_string(),
            ))),
        }

        self.needs_query_refresh = false;
        self.query_resets_selection = false;
    }

    fn refresh_query_impl(&mut self) -> Result<(), FsPulseError> {
        let (query_str, columns) = self.build_query_and_columns()?;

        match QueryProcessor::execute_query(&query_str) {
            Ok((rows, _column_headers, _alignments)) => {
                // Note: We use columns from build_query_and_columns, not _column_headers or _alignments
                // because TUI needs full column metadata, not just display names and alignments
                self.grid_frame
                    .set_data(self.query_resets_selection, columns, rows);
                //self.error_message = None;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn handle_tab_section_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        let action = None;

        match key.code {
            KeyCode::Left => {
                let new_type = match self.model.current_type() {
                    DomainType::Alerts => DomainType::Roots,
                    DomainType::Items => DomainType::Alerts,
                    DomainType::Changes => DomainType::Items,
                    DomainType::Scans => DomainType::Changes,
                    DomainType::Roots => DomainType::Scans,
                };
                self.set_current_type(new_type);
            }
            KeyCode::Right => {
                let new_type = match self.model.current_type() {
                    DomainType::Alerts => DomainType::Items,
                    DomainType::Items => DomainType::Changes,
                    DomainType::Changes => DomainType::Scans,
                    DomainType::Scans => DomainType::Roots,
                    DomainType::Roots => DomainType::Alerts,
                };
                self.set_current_type(new_type);
            }
            _ => {}
        }

        action
    }

    fn show_limit_input(&mut self) {
        self.popups_push(ActivePopup::InputBox(InputBoxState::new(
            "Choose a new limit".into(),
            Some(self.model.current_limit()),
        )));
    }

    fn show_views(&mut self) {
        self.popups_push(ActivePopup::ViewsList(ViewsListState::new()));
    }

    fn set_current_type(&mut self, new_type: DomainType) {
        self.model.set_current_type(new_type);
        self.needs_query_refresh = true;
        self.query_resets_selection = false;
    }

    fn apply_view(&mut self, saved_view: &'static SavedView) {
        self.model.set_current_type(saved_view.type_selection);
        self.model.set_current_view(Some(saved_view));

        // Setting a view replaces existing filters
        self.model.current_filters_mut().clear();

        for filter_spec in saved_view.filters {
            let col_info = self
                .model
                .current_columns()
                .iter()
                .find(|col| col.name_db == filter_spec.col_name);
            if let Some(col_info) = col_info {
                let filter = Filter::new(
                    filter_spec.col_name,
                    col_info.col_type.info().type_name,
                    col_info.col_type.info(),
                    filter_spec.filter_text.to_owned(),
                );
                self.model.current_filters_mut().push(filter);
            }
        }

        self.model.reset_current_columns();

        for column_spec in saved_view.columns {
            if let Some(col_info) = self
                .model
                .current_columns_mut()
                .iter_mut()
                .find(|col_info| col_info.name_db == column_spec.col_name)
            {
                col_info.selected = match column_spec.show_col {
                    true => ColSelect::Selected,
                    false => ColSelect::NotSelected,
                };
                col_info.order_direction = column_spec.order_direction;
            }
        }

        self.needs_query_refresh = true;
        self.query_resets_selection = true;
    }

    fn set_alert_status(&mut self, new_status: AlertStatus) {
        let Some(alert_index) = self.grid_frame.table_state.selected() else {
            return;
        };

        let Some(row) = self.grid_frame.raw_rows.get_mut(alert_index) else {
            return;
        };

        let Some(alert_id_col) = self
            .grid_frame
            .columns
            .iter()
            .position(|col| col.name_db == "alert_id")
        else {
            return;
        };

        let Some(status_col) = self
            .grid_frame
            .columns
            .iter()
            .position(|col| col.name_db == "alert_status")
        else {
            return;
        };

        let Some(alert_id_str) = row.get(alert_id_col) else {
            return;
        };

        let Ok(alert_id) = alert_id_str.parse::<i64>() else {
            return;
        };

        let conn = match Database::get_connection() {
            Ok(c) => c,
            Err(e) => {
                self.popups_push(ActivePopup::MessageBox(MessageBoxState::new(
                    MessageBoxType::Error,
                    format!("Database Error: Failed to get database connection: {}", e),
                )));
                return;
            }
        };

        match Alerts::set_alert_status(&conn, alert_id, new_status) {
            Ok(()) => row[status_col] = new_status.short_name().to_owned(),
            Err(err) => self.popups_push(ActivePopup::MessageBox(MessageBoxState::new(
                MessageBoxType::Error,
                err.to_string(),
            ))),
        }
    }

    fn popups_last_mut(&mut self) -> Option<&mut ActivePopup> {
        self.active_popups.last_mut()
    }

    fn popups_push(&mut self, popup: ActivePopup) {
        self.active_popups.push(popup);
    }

    fn popups_pop(&mut self) {
        self.active_popups.pop();
    }
}

fn render_centered_popup<W: StatefulWidget>(
    f: &mut Frame,
    widget: W,
    state: &mut W::State,
    width: u16,
    height: u16,
) {
    let rect = Utils::center(f.area(), width, height);
    f.render_widget(Clear, rect);
    f.render_stateful_widget(widget, rect, state);
}
