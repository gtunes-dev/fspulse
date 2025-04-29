use crate::query::QueryProcessor;
use crate::{database::Database, error::FsPulseError};

use std::io;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

use super::{column_frame::ColumnFrame, grid_frame::GridFrame};

enum Focus {
    Filters,
    ColumnSelector,
    DataGrid,
}

pub struct Explorer {
    focus: Focus,
    column_frame: ColumnFrame,
    grid_frame: GridFrame,
    error_message: Option<String>,
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let vertical_middle = popup_layout[1];

    let horizontal_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical_middle);

    horizontal_layout[1]
}

impl Explorer {
    pub fn new() -> Self {
        Self {
            focus: Focus::Filters,
            column_frame: ColumnFrame::new(),
            grid_frame: GridFrame::new(),
            error_message: None,
        }
    }

    pub fn explore(&mut self, db: &Database) -> Result<(), FsPulseError> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            terminal.draw(|f| {
                let full_area = f.area(); // updated here

                // Split vertically: Filters / Main / Help
                let vertical_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // Filters
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

                // Draw top (filters)
                let filter_block =
                    Self::focused_block("Filters", matches!(self.focus, Focus::Filters));
                f.render_widget(filter_block, top_chunk);

                // Draw left (Type selector + Column list)
                self.column_frame
                    .draw(f, left_chunk, matches!(self.focus, Focus::ColumnSelector));

                // Draw right (data grid)
                // Draw right (data grid)
                self.grid_frame
                    .draw(f, right_chunk, matches!(self.focus, Focus::DataGrid));

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
                if self.column_frame.dropdown_open {
                    let popup_area = centered_rect(20, 30, f.area());
                    self.column_frame.draw_dropdown(f, popup_area);
                }
            })?;

            // Handle input
            if event::poll(std::time::Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => {
                            break;
                        }
                        KeyCode::Char('r') => {
                            match self.refresh_query(db) {
                                Ok(()) => {}
                                Err(err) => self.error_message = Some(err.to_string()),
                            };
                        }
                        KeyCode::Tab => {
                            self.focus = self.next_focus();
                        }
                        KeyCode::BackTab => {
                            self.focus = self.prev_focus();
                        }
                        _ => match self.focus {
                            Focus::ColumnSelector => {
                                self.column_frame.handle_key(key);
                            }
                            _ => {}
                        },
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

    fn focused_block(title: &str, is_focused: bool) -> Block {
        if is_focused {
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title(title.bold())
        } else {
            Block::default().borders(Borders::ALL).title(title)
        }
    }

    /// Returns help text depending on which frame is focused
    fn help_text(&self) -> &'static str {
        match self.focus {
            Focus::Filters => "Tab: Next Section  |  r: Refresh  |  q: Quit  |  Focus: Filters",
            Focus::ColumnSelector => "Tab: Next Section  |  Space/Enter: Select/Toggle  |  +/-: Move Column  |  r: Refresh  |  q: Quit  |  Focus: Type & Columns",
            Focus::DataGrid => "Tab: Next Section  |  r: Refresh  |  q: Quit  |  Focus: Data Grid",
        }
    }

    fn build_query_and_columns(&mut self) -> Result<(String, Vec<String>), FsPulseError> {
        let mut cols = Vec::new();

        // Build the new query
        let mut query = self.column_frame.selected_type.name().to_ascii_lowercase();

        // TODO: Build the "where" clause after we've built the UI for filters

        // Build the "show" clause and cols vector
        query.push_str(" show ");
        let mut first_col = true;
        for col in self.column_frame.current_columns() {
            if col.selected {
                match first_col {
                    true => first_col = false,
                    false => query.push_str(", "),
                }
                cols.push(col.name.to_owned());
                query.push_str(col.name);
            }
        }

        if first_col {
            return Err(FsPulseError::Error("No columns selected".into()));
        }

        // TODO: Build the "order by" clause once we have UI for that

        // TODO: Build the limit clause once we figure out what the UI is

        Ok((query, cols))
    }

    fn refresh_query(&mut self, db: &Database) -> Result<(), FsPulseError> {
        let (query_str, columns) = self.build_query_and_columns()?;

        match QueryProcessor::execute_query(db, &query_str) {
            Ok(rows) => {
                self.grid_frame.set_data(columns, rows);
                self.error_message = None;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}
