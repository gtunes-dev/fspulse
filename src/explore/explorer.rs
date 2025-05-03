use crate::query::QueryProcessor;
use crate::{database::Database, error::FsPulseError};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::widgets::{Block, Clear};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Borders, Paragraph},
    Terminal,
};
use std::io;

use super::column_frame::ColumnFrameView;
use super::domain_model::{ColInfo, DomainModel, Filter};
use super::filter_frame::{FilterFrame, FilterFrameView};
use super::filter_window::FilterWindow;
use super::grid_frame::GridFrameView;
use super::message_box::{MessageBox, MessageBoxType};
use super::utils::Utils;
use super::{column_frame::ColumnFrame, grid_frame::GridFrame};

enum Focus {
    Filters,
    ColumnSelector,
    DataGrid,
}

pub enum ExplorerAction {
    Dismiss,
    ShowAddFilter(FilterWindow),
    ShowMessage(MessageBox),
    AddFilter(Filter),
    ShowEditFilter(usize),
    DeleteFilter(usize),
    UpdateFilter(usize, String),
}

pub struct Explorer {
    model: DomainModel,
    focus: Focus,
    column_frame: ColumnFrame,
    grid_frame: GridFrame,
    filter_frame: FilterFrame,
    filter_window: Option<FilterWindow>,
    message_box: Option<MessageBox>,
}

impl Explorer {
    pub fn new() -> Self {
        Self {
            model: DomainModel::new(),
            focus: Focus::Filters,
            column_frame: ColumnFrame::new(),
            grid_frame: GridFrame::new(),
            filter_frame: FilterFrame::new(),
            filter_window: None,
            message_box: None,
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
                    Constraint::Length(8), // Filters
                    Constraint::Min(0),    // Main content
                    Constraint::Length(2), // Help/status
                ])
                .split(full_area);

            let top_chunk = vertical_chunks[0];
            let main_chunk = vertical_chunks[1];
            let help_chunk = vertical_chunks[2];

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

            let column_frame_view = ColumnFrameView::new(
                &mut self.column_frame,
                &self.model,
                matches!(self.focus, Focus::ColumnSelector),
            );
            f.render_widget(column_frame_view, left_chunk);

            // Draw right (data grid)
            let grid_frame_view = GridFrameView::new(&mut self.grid_frame, &self.model, matches!(self.focus, Focus::DataGrid));
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

            // Draw the type selector if it's open
            if self.column_frame.is_dropdown_open() {
                let popup_area = Utils::centered_rect(20, 30, f.area());
                // Clear the popup area before drawing into it
                f.render_widget(Clear, popup_area);
                self.column_frame.draw_dropdown(f, popup_area);
            }

            if let Some(ref mut filter_window) = self.filter_window {
                let is_top_window = self.message_box.is_none();
                filter_window.draw(f, is_top_window);
            }

            // Draw the message box if needed
            if let Some(ref message_box) = self.message_box {
                message_box.draw(f);
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
            self.draw(&mut terminal)?;

            // Handle input
            if event::poll(std::time::Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    if self.modal_input_handled(key) {
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') => {
                            break;
                        }
                        KeyCode::Char('r') => {
                            match self.refresh_query(db) {
                                Ok(()) => {}
                                Err(err) => {
                                    self.message_box = Some(MessageBox::new(
                                        MessageBoxType::Error,
                                        err.to_string(),
                                    ))
                                }
                            };
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
                    }
                    ExplorerAction::UpdateFilter(filter_index, new_filter_text) => {
                        if let Some(filter) = self.model.current_filters_mut().get_mut(filter_index)
                        {
                            filter.filter_text = new_filter_text;
                        }
                        self.filter_window = None;
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
            Focus::ColumnSelector => self.column_frame.handle_key(&mut self.model, key),
            Focus::DataGrid => self.grid_frame.handle_key(key),
            Focus::Filters => self.filter_frame.handle_key(&self.model, key),
        };

        if let Some(action) = action {
            match action {
                ExplorerAction::ShowMessage(message_box) => self.message_box = Some(message_box),
                ExplorerAction::ShowAddFilter(filter_window) => {
                    self.filter_window = Some(filter_window)
                }
                ExplorerAction::DeleteFilter(filter_index) => {
                    self.model.current_filters_mut().remove(filter_index);
                    if filter_index > self.model.current_filters().len() {
                        self.filter_frame.set_selected(filter_index - 1);
                    }
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
            Focus::Filters => Focus::ColumnSelector,
            Focus::ColumnSelector => Focus::DataGrid,
            Focus::DataGrid => Focus::Filters,
        }
    }

    fn prev_focus(&self) -> Focus {
        match self.focus {
            Focus::Filters => Focus::DataGrid,
            Focus::ColumnSelector => Focus::Filters,
            Focus::DataGrid => Focus::ColumnSelector,
        }
    }

    /// Returns help text depending on which frame is focused
    fn help_text(&self) -> &'static str {
        match self.focus {
            Focus::Filters => "Tab: Next Section  |  r: Refresh  |  q: Quit  |  Focus: Filters",
            Focus::ColumnSelector => "Tab: Next Section  |  Space/Enter: Toggle  |  +/-: Reorder  |  r: Refresh  |  q: Quit  |  Focus: Type & Columns",
            Focus::DataGrid => "↑↓: Scroll  |  PgUp/PgDn: Page  |  Home/End: Top/Bottom  |  Tab: Next Section  |  r: Refresh  |  q: Quit  |  Focus: Data Grid",
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

        // TODO: Build the limit clause once we figure out what the UI is

        Ok((query, cols, col_infos))
    }

    fn refresh_query(&mut self, db: &Database) -> Result<(), FsPulseError> {
        let (query_str, columns, col_types) = self.build_query_and_columns()?;

        match QueryProcessor::execute_query(db, &query_str) {
            Ok(rows) => {
                self.grid_frame.set_data(columns, col_types, rows);
                //self.error_message = None;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}
