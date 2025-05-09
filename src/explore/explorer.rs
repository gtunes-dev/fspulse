use crate::query::QueryProcessor;
use crate::{database::Database, error::FsPulseError};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::widgets::{Block, Clear, Tabs};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Borders, Paragraph},
    Terminal,
};
use ratatui::{border, symbols};
use std::io;

use super::column_frame::ColumnFrameView;
use super::domain_model::{ColumnInfo, DomainModel, Filter, OrderDirection, TypeSelection};
use super::filter_frame::{FilterFrame, FilterFrameView};
use super::filter_popup::FilterPopup;
use super::grid_frame::GridFrameView;
use super::input_box::{InputBox, InputBoxState};
use super::limit_widget::LimitWidget;
use super::message_box::{MessageBox, MessageBoxType};
use super::utils::{StylePalette, Utils};
use super::view::{SavedView, ViewsState, ViewsWidget};
use super::{column_frame::ColumnFrame, grid_frame::GridFrame};

enum Focus {
    Tabs,
    Filters,
    DataGrid,
    ColumnSelector,
    Limit,
}

pub enum ExplorerAction {
    Dismiss,
    RefreshQuery(bool),
    ShowAddFilter(FilterPopup),
    ShowMessage(MessageBox),
    AddFilter(Filter),
    ShowEditFilter(usize),
    DeleteFilter(usize),
    UpdateFilter(usize, String),
    ShowLimit,
    SetLimit(String),
    ApplyView(SavedView),
}

pub struct Explorer {
    model: DomainModel,
    focus: Focus,
    needs_query_refresh: bool,
    query_resets_selection: bool,
    column_frame: ColumnFrame,
    grid_frame: GridFrame,
    filter_frame: FilterFrame,
    filter_popup: Option<FilterPopup>,
    message_box: Option<MessageBox>,
    input_box: Option<InputBox>,
    views_state: Option<ViewsState>,
}

impl Explorer {
    pub fn new() -> Self {
        Self {
            model: DomainModel::new(),
            focus: Focus::DataGrid,
            needs_query_refresh: true,
            query_resets_selection: true,
            column_frame: ColumnFrame::new(),
            grid_frame: GridFrame::new(),
            filter_frame: FilterFrame::new(),
            filter_popup: None,
            message_box: None,
            input_box: None,
            views_state: None,
        }
    }

    fn draw(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), FsPulseError> {
        terminal.draw(|f| {
            let full_area = f.area(); // updated here

            // Create layouts and area rects
            let [tabs, filters, center, help] = Layout::new(
                Direction::Vertical,
                [
                    Constraint::Length(3), // Tabs
                    Constraint::Length(8), // Filters
                    Constraint::Min(0),    // Main content
                    Constraint::Length(1), // Help/status
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
            .areas(center);

            // layout the left chunk
            let [limit, columns] = Layout::new(
                Direction::Vertical,
                [Constraint::Length(2), Constraint::Fill(1)],
            )
            .areas(left);

            // -- Borders
            // Filter
            let filter_block_set = symbols::border::Set {
                bottom_left: symbols::line::NORMAL.bottom_left,
                bottom_right: symbols::line::NORMAL.bottom_right,
                ..symbols::border::PLAIN
            };
            let filter_block = Block::default()
                .border_set(filter_block_set)
                .borders(border!(TOP, LEFT, RIGHT))
                //    .border_type(BorderType::Plain)
                .title(FilterFrame::frame_title(self.model.current_type()));
            f.render_widget(&filter_block, filters);

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

            let titles = TypeSelection::all_types().iter().map(|t| t.title());

            let mut tabs_width: u16 = TypeSelection::all_types()
                .iter()
                .map(|l| l.title().width() as u16) // Line::width() is in ratatui >=0.25
                .sum::<u16>()
                + ((titles.len().saturating_sub(1)) as u16);

            tabs_width += 3 * 3; // " * " in between each tab

            let tabs_rect =
                Utils::center(tabs, Constraint::Length(tabs_width), Constraint::Length(1));

            let tabs_highlight = match self.focus {
                Focus::Tabs => StylePalette::TabHighlight.style(),
                _ => StylePalette::Tab.style(), // .add_modifier(Modifier::UNDERLINED),
            };

            let tabs = Tabs::new(titles)
                .highlight_style(tabs_highlight)
                .divider(symbols::DOT)
                .select(self.model.current_type().index());
            f.render_widget(tabs, tabs_rect);

            let filter_frame_view = FilterFrameView::new(
                &mut self.filter_frame,
                &self.model,
                matches!(self.focus, Focus::Filters),
            );
            f.render_widget(filter_frame_view, filter_block.inner(filters));

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
            f.render_widget(help_paragraph, help);

            if let Some(ref mut filter_popup) = self.filter_popup {
                let is_top_window = self.message_box.is_none();
                filter_popup.draw(f, is_top_window);
            }

            if let Some(views_state) = self.views_state.as_mut() {
                let views_rect =
                    Utils::center(f.area(), Constraint::Percentage(60), Constraint::Length(10));
                f.render_widget(Clear, views_rect);

                let views_popup = ViewsWidget;
                f.render_stateful_widget(views_popup, views_rect, views_state);
            }

            // Draw the message box if needed
            if let Some(ref message_box) = self.message_box {
                message_box.draw(f);
                let input_rect =
                    Utils::center(f.area(), Constraint::Percentage(60), Constraint::Length(10));
                f.render_widget(Clear, input_rect);
            }

            if let Some(ref input_box) = self.input_box {
                let input_rect =
                    Utils::center(f.area(), Constraint::Percentage(60), Constraint::Length(10));
                f.render_widget(Clear, input_rect);
                let mut input_state = InputBoxState::new();
                f.render_stateful_widget(input_box.clone(), input_rect, &mut input_state);
                if let Some((x, y)) = input_state.cursor_pos {
                    f.set_cursor_position((x, y));
                }
            }
        })?;

        Ok(())
    }

    pub fn explore(&mut self, db: &Database) -> Result<(), FsPulseError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            if self.needs_query_refresh {
                self.refresh_query(db);
            }
            self.draw(&mut terminal)?;

            // Handle input
            if event::poll(std::time::Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    if self.modal_input_handled(key) {
                        continue;
                    }

                    match key.code {
                        // Switch Type
                        KeyCode::Char('i') | KeyCode::Char('I') => {
                            self.set_current_type(TypeSelection::Items)
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            self.set_current_type(TypeSelection::Changes)
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            self.set_current_type(TypeSelection::Scans)
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            self.set_current_type(TypeSelection::Roots)
                        }
                        KeyCode::Char('v') | KeyCode::Char('V') => {
                            self.show_views();
                        }

                        // Show Limit Input
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            self.show_limit_input();
                        }

                        // Quit
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            break;
                        }
                        KeyCode::Tab => {
                            self.focus = self.next_focus();
                        }
                        KeyCode::BackTab => {
                            self.focus = self.prev_focus();
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

    fn modal_input_handled(&mut self, key: KeyEvent) -> bool {
        if let Some(ref message_box) = self.message_box {
            if message_box.is_dismiss_event(key) {
                self.message_box = None;
            }
            // skip all handling below
            return true;
        }

        if let Some(views_state) = self.views_state.as_mut() {
            let action = views_state.handle_key(key);
            if let Some(action) = action {
                match action {
                    ExplorerAction::Dismiss => self.views_state = None,
                    ExplorerAction::ApplyView(saved_view) => {
                        self.views_state = None;
                        self.apply_view(&saved_view);
                    }
                    _ => {}
                }
            }

            return true;
        }

        if let Some(ref mut filter_popup) = self.filter_popup {
            let action = filter_popup.handle_key(key);
            if let Some(action) = action {
                match action {
                    ExplorerAction::Dismiss => self.filter_popup = None,
                    ExplorerAction::ShowMessage(message_box) => {
                        self.message_box = Some(message_box)
                    }
                    ExplorerAction::AddFilter(filter) => {
                        self.model.current_filters_mut().push(filter);
                        self.filter_popup = None;
                        self.filter_frame
                            .set_selected(self.model.current_filters().len());

                        self.needs_query_refresh = true;
                        self.query_resets_selection = true;
                    }
                    ExplorerAction::UpdateFilter(filter_index, new_filter_text) => {
                        if let Some(filter) = self.model.current_filters_mut().get_mut(filter_index)
                        {
                            if filter.filter_text != new_filter_text {
                                filter.filter_text = new_filter_text;
                                self.needs_query_refresh = true;
                                self.query_resets_selection = true;
                            }
                        }

                        self.filter_popup = None;
                    }
                    _ => {}
                }
            }

            return true;
        }

        if let Some(ref mut input_box) = self.input_box {
            let action = input_box.handle_key(key);
            if let Some(action) = action {
                match action {
                    ExplorerAction::Dismiss => self.input_box = None,
                    ExplorerAction::SetLimit(new_limit) => {
                        self.model.set_current_limit(new_limit);
                        self.input_box = None;

                        self.needs_query_refresh = true;
                        self.query_resets_selection = false;
                    }
                    _ => {}
                }
            }

            return true;
        }

        false
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
                ExplorerAction::ShowMessage(message_box) => self.message_box = Some(message_box),
                ExplorerAction::ShowLimit => self.show_limit_input(),
                ExplorerAction::ShowAddFilter(filter_popup) => {
                    self.filter_popup = Some(filter_popup)
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
                        let edit_filter_popup = FilterPopup::new_edit_filter_popup(
                            filter.col_name,
                            filter_index,
                            filter.col_type_info,
                            filter.filter_text.to_owned(),
                        );
                        self.filter_popup = Some(edit_filter_popup);
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    fn next_focus(&self) -> Focus {
        match self.focus {
            Focus::Tabs => Focus::Filters,
            Focus::Filters => Focus::Limit,
            Focus::Limit => Focus::ColumnSelector,
            Focus::ColumnSelector => Focus::DataGrid,
            Focus::DataGrid => Focus::Tabs,
        }
    }

    fn prev_focus(&self) -> Focus {
        match self.focus {
            Focus::Tabs => Focus::DataGrid,
            Focus::DataGrid => Focus::ColumnSelector,
            Focus::ColumnSelector => Focus::Limit,
            Focus::Limit => Focus::Filters,
            Focus::Filters => Focus::Tabs,
        }
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
        let mut query = self.model.current_type().name().to_ascii_lowercase();

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
            if col.selected {
                match first_col {
                    true => {
                        query.push_str(" show ");
                        first_col = false
                    }
                    false => query.push_str(", "),
                }
                cols.push(*col);
                query.push_str(col.name_db);
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
            if col.selected && col.order_direction != OrderDirection::None {
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

    fn refresh_query(&mut self, db: &Database) {
        match self.refresh_query_impl(db) {
            Ok(()) => {}
            Err(err) => {
                self.message_box = Some(MessageBox::new(MessageBoxType::Error, err.to_string()))
            }
        }

        self.needs_query_refresh = false;
        self.query_resets_selection = false;
    }

    fn refresh_query_impl(&mut self, db: &Database) -> Result<(), FsPulseError> {
        let (query_str, columns) = self.build_query_and_columns()?;

        match QueryProcessor::execute_query(db, &query_str) {
            Ok(rows) => {
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
                    TypeSelection::Items => TypeSelection::Roots,
                    TypeSelection::Changes => TypeSelection::Items,
                    TypeSelection::Scans => TypeSelection::Changes,
                    TypeSelection::Roots => TypeSelection::Scans,
                };
                self.set_current_type(new_type);
            }
            KeyCode::Right => {
                let new_type = match self.model.current_type() {
                    TypeSelection::Items => TypeSelection::Changes,
                    TypeSelection::Changes => TypeSelection::Scans,
                    TypeSelection::Scans => TypeSelection::Roots,
                    TypeSelection::Roots => TypeSelection::Items,
                };
                self.set_current_type(new_type);
            }
            _ => {}
        }

        action
    }

    fn show_limit_input(&mut self) {
        self.input_box = Some(InputBox::new(
            "Choose a new limit".into(),
            Some(self.model.current_limit()),
        ));
    }

    fn show_views(&mut self) {
        self.views_state = Some(ViewsState::new());
    }

    fn set_current_type(&mut self, new_type: TypeSelection) {
        self.model.set_current_type(new_type);
        self.needs_query_refresh = true;
        self.query_resets_selection = false;
    }

    fn apply_view(&mut self, saved_view: &SavedView) {
        self.model.set_current_type(saved_view.type_selection);

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
                .find(| col_info| col_info.name_db == column_spec.col_name) {
                    col_info.selected = column_spec.show_col; 
                    col_info.order_direction = column_spec.order_direction;
            }
        }

        self.needs_query_refresh = true;
        self.query_resets_selection = true;
    }
}
