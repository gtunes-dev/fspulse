use crate::query::QueryProcessor;
use crate::{database::Database, error::FsPulseError};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::style::Modifier;
use ratatui::symbols;
use ratatui::widgets::{Block, Clear, Tabs};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Borders, Paragraph},
    Terminal,
};
use std::io;

use super::column_frame::ColumnFrameView;
use super::domain_model::{ColInfo, DomainModel, Filter, TypeSelection};
use super::filter_frame::{FilterFrame, FilterFrameView};
use super::filter_window::FilterWindow;
use super::grid_frame::GridFrameView;
use super::input_box::{InputBox, InputBoxState};
use super::limit_widget::LimitWidget;
use super::message_box::{MessageBox, MessageBoxType};
use super::utils::Utils;
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
    ShowAddFilter(FilterWindow),
    ShowMessage(MessageBox),
    AddFilter(Filter),
    ShowEditFilter(usize),
    DeleteFilter(usize),
    UpdateFilter(usize, String),
    ShowLimit,
    SetLimit(String),
}

pub struct Explorer {
    model: DomainModel,
    focus: Focus,
    needs_query_refresh: bool,
    query_resets_selection: bool,
    column_frame: ColumnFrame,
    grid_frame: GridFrame,
    filter_frame: FilterFrame,
    filter_window: Option<FilterWindow>,
    message_box: Option<MessageBox>,
    input_box: Option<InputBox>,
}

impl Explorer {
    pub fn new() -> Self {
        Self {
            model: DomainModel::new(),
            focus: Focus::Filters,
            needs_query_refresh: true,
            query_resets_selection: true,
            column_frame: ColumnFrame::new(),
            grid_frame: GridFrame::new(),
            filter_frame: FilterFrame::new(),
            filter_window: None,
            message_box: None,
            input_box: None,
        }
    }

    fn draw(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), FsPulseError> {
        terminal.draw(|f| {
            let full_area = f.area(); // updated here

            // Split vertically: Filters / Main / Help
            let vertical_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Tabs
                    Constraint::Length(8), // Filters
                    Constraint::Min(0),    // Main content
                    Constraint::Length(2), // Help/status
                ])
                .split(full_area);

            let tab_chunk = vertical_chunks[0];
            let top_chunk = vertical_chunks[1];
            let main_chunk = vertical_chunks[2];
            let help_chunk = vertical_chunks[3];

            let current_type = self.model.current_type();

            let titles = TypeSelection::all_types()
                .iter()
                .map(|t| t.title(current_type));

            let mut tabs_width: u16 = TypeSelection::all_types()
                .iter()
                .map(|l| l.title(current_type).width() as u16) // Line::width() is in ratatui >=0.25
                .sum::<u16>()
                + ((titles.len().saturating_sub(1)) as u16);

            tabs_width += 3 * 3; // " * " in between each tab

            let tabs_rect = Utils::center(
                tab_chunk,
                Constraint::Length(tabs_width),
                Constraint::Length(1),
            );

            let tabs_highlight = match self.focus {
                Focus::Tabs => Style::default().bg(Color::Gray).fg(Color::Black),
                _ => Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED), // .add_modifier(Modifier::UNDERLINED),
            };

            let tabs = Tabs::new(titles)
                .highlight_style(tabs_highlight)
                .divider(symbols::DOT)
                .select(self.model.current_type().index());

            f.render_widget(tabs, tabs_rect);

            // Inside the main content, split horizontally
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(30), // Left (Type + Columns)
                    Constraint::Min(0),     // Right (Data Grid)
                ])
                .split(main_chunk);

            let left_chunk = main_chunks[0];
            let right_chunk = main_chunks[1];

            let filter_frame_view = FilterFrameView::new(
                &mut self.filter_frame,
                &self.model,
                matches!(self.focus, Focus::Filters),
            );
            f.render_widget(filter_frame_view, top_chunk);

            // render the left chunk
            let left_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Fill(1)])
                .split(left_chunk);

            let limit_widget = LimitWidget::new(
                self.model.current_limit(),
                matches!(self.focus, Focus::Limit),
            );
            f.render_widget(limit_widget, left_layout[0]);

            let column_frame_view = ColumnFrameView::new(
                &mut self.column_frame,
                &self.model,
                matches!(self.focus, Focus::ColumnSelector),
            );
            f.render_widget(column_frame_view, left_layout[1]);

            // Draw right (data grid)
            let grid_frame_view = GridFrameView::new(
                &mut self.grid_frame,
                &self.model,
                matches!(self.focus, Focus::DataGrid),
            );
            f.render_widget(grid_frame_view, right_chunk);

            // Draw bottom (help/status)
            let help_block = Block::default()
                .borders(Borders::TOP)
                .title("Help")
                .title_alignment(Alignment::Center);
            let help_paragraph = Paragraph::new(self.help_text())
                .style(Style::default().bg(Color::Blue).fg(Color::White))
                .block(help_block);
            f.render_widget(help_paragraph, help_chunk);

            if let Some(ref mut filter_window) = self.filter_window {
                let is_top_window = self.message_box.is_none();
                filter_window.draw(f, is_top_window);
            }

            // Draw the message box if needed
            if let Some(ref message_box) = self.message_box {
                message_box.draw(f);
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

        if let Some(ref mut filter_window) = self.filter_window {
            let action = filter_window.handle_key(key);
            if let Some(action) = action {
                match action {
                    ExplorerAction::Dismiss => self.filter_window = None,
                    ExplorerAction::ShowMessage(message_box) => {
                        self.message_box = Some(message_box)
                    }
                    ExplorerAction::AddFilter(filter) => {
                        self.model.current_filters_mut().push(filter);
                        self.filter_window = None;
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

                        self.filter_window = None;
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
                ExplorerAction::ShowAddFilter(filter_window) => {
                    self.filter_window = Some(filter_window)
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
                        let edit_filter_window = FilterWindow::new_edit_filter_window(
                            filter.col_name,
                            filter_index,
                            filter.col_type_info,
                            filter.filter_text.to_owned(),
                        );
                        self.filter_window = Some(edit_filter_window);
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    fn next_focus(&self) -> Focus {
        match self.focus {
            Focus::Tabs => Focus::Filters,
            Focus::Filters => Focus::DataGrid,
            Focus::DataGrid => Focus::ColumnSelector,
            Focus::ColumnSelector => Focus::Limit,
            Focus::Limit => Focus::Tabs,
        }
    }

    fn prev_focus(&self) -> Focus {
        match self.focus {
            Focus::Tabs => Focus::Limit,
            Focus::Limit => Focus::ColumnSelector,
            Focus::ColumnSelector => Focus::DataGrid,
            Focus::DataGrid => Focus::Filters,
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

    fn build_query_and_columns(
        &mut self,
    ) -> Result<(String, Vec<String>, Vec<ColInfo>), FsPulseError> {
        let mut cols = Vec::new();
        let mut col_infos = Vec::new();

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
        query.push_str(" show ");
        let mut first_col = true;
        for col in self.model.current_columns() {
            if col.selected {
                match first_col {
                    true => first_col = false,
                    false => query.push_str(", "),
                }
                cols.push(col.name.to_owned());
                col_infos.push(col.col_info);
                query.push_str(col.name);
            }
        }

        if first_col {
            return Err(FsPulseError::Error("No columns selected".into()));
        }

        // TODO: Build the "order by" clause once we have UI for that

        // Build the Limit clause
        let limit = self.model.current_limit();
        if !limit.is_empty() {
            query.push_str(" limit ");
            query.push_str(&limit);
        }

        Ok((query, cols, col_infos))
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
        let (query_str, columns, col_types) = self.build_query_and_columns()?;

        match QueryProcessor::execute_query(db, &query_str) {
            Ok(rows) => {
                self.grid_frame.set_data(self.query_resets_selection, columns, col_types, rows);
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

    fn set_current_type(&mut self, new_type: TypeSelection) {
        self.model.set_current_type(new_type);
        self.needs_query_refresh = true;
        self.query_resets_selection = false;
    }
}
