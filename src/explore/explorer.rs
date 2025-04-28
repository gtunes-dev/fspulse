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

use super::column_frame::ColumnFrame;

enum Focus {
    Filters,
    ColumnSelector,
    DataGrid,
}

pub struct Explorer {
    focus: Focus,
    column_frame: ColumnFrame,

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
        }
    }

    pub fn explore(&mut self, _db: &Database) -> Result<(), FsPulseError> {
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
                let data_block =
                    Self::focused_block("Data Grid", matches!(self.focus, Focus::DataGrid));
                f.render_widget(data_block, right_chunk);

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
            Focus::Filters => "Tab: Next Section  |  q: Quit  |  Focus: Filters",
            Focus::ColumnSelector => "Tab: Next Section  |  Space/Enter: Select/Toggle  |  +/-: Move Column  |  q: Quit  |  Focus: Type & Columns",
            Focus::DataGrid => "Tab: Next Section  |  q: Quit  |  Focus: Data Grid",
        }
    }
}
