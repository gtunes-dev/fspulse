use ratatui::{
    crossterm::event::{Event, KeyCode, KeyEvent},
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use tui_input::{backend::crossterm::EventHandler, Input};

use crate::query::{columns::ColTypeInfo, QueryProcessor};

use super::{
    domain_model::{ColInfo, Filter},
    explorer::ExplorerAction,
    message_box::{MessageBox, MessageBoxType},
};

enum FilterWindowType {
    Add,
    Edit,
}
pub struct FilterWindow {
    filter_window_type: FilterWindowType,
    col_name: &'static str,
    filter_index: Option<usize>,
    col_type_info: ColTypeInfo,
    input: Input,
}

impl FilterWindow {
    fn new(
        filter_window_type: FilterWindowType,
        col_name: &'static str,
        filter_index: Option<usize>,
        col_info: ColInfo,
    ) -> Self {
        FilterWindow {
            filter_window_type,
            col_name,
            filter_index,
            col_type_info: col_info.col_type.info(),
            input: Input::default(),
        }
    }

    pub fn new_add_filter_window(col_name: &'static str, col_info: ColInfo) -> Self {
        Self::new(FilterWindowType::Add, col_name, None, col_info)
    }

    pub fn _new_edit_filter_window(col_name: &'static str, filter_index: usize, col_info: ColInfo) -> Self {
        Self::new(FilterWindowType::Edit, col_name, Some(filter_index), col_info)
    }

    pub fn draw(&mut self, f: &mut Frame, is_top_window: bool) {
        let screen = f.area();

        let popup_width = screen.width.min(80);
        let popup_height = 11;

        let popup_x = screen.x + (screen.width.saturating_sub(popup_width)) / 2;
        let popup_y = screen.y + (screen.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        f.render_widget(Clear, popup_area);

        let outer_block = Block::default().borders(Borders::ALL);
        f.render_widget(outer_block, popup_area);

        let inner_area = Rect {
            x: popup_area.x + 2,
            y: popup_area.y + 1,
            width: popup_area.width.saturating_sub(4),
            height: popup_area.height.saturating_sub(2),
        };

        // Split top portion (label + spacer + input + spacer)
        let top_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Label
                Constraint::Length(1), // Spacer
                Constraint::Length(3), // Input box
                Constraint::Length(1), // Spacer
            ])
            .split(inner_area);

        // Label
        let label_text = format!("Add {} filter:", self.col_name);
        let label_paragraph = Paragraph::new(label_text).alignment(Alignment::Left);
        f.render_widget(label_paragraph, top_layout[0]);

        // Spacer
        f.render_widget(Paragraph::new(" "), top_layout[1]);

        // Input
        let scroll = self.input.visual_scroll((top_layout[2].width - 2) as usize);
        let input_block = Block::default().title("Filter").borders(Borders::ALL);
        let input_paragraph = Paragraph::new(self.input.value())
            .block(input_block)
            .scroll((0, scroll as u16));
        f.render_widget(input_paragraph, top_layout[2]);

        // Cursor positioning
        if is_top_window {
            let x = self.input.visual_cursor().saturating_sub(scroll) as u16;
            f.set_cursor_position(Position::new(top_layout[2].x + 1 + x, top_layout[2].y + 1));
        }

        // Spacer
        f.render_widget(Paragraph::new(" "), top_layout[3]);

        // Full-width footer layout: Tip, Divider, Help
        let footer_area = Rect {
            x: popup_area.x + 1,
            y: top_layout[3].y + 1,
            width: popup_area.width.saturating_sub(2),
            height: 3,
        };

        let footer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tip
                Constraint::Length(1), // Divider
                Constraint::Length(1), // Help
            ])
            .split(footer_area);

        // Tip
        /*
        let tip_text = match self.col_info.col_type {
            ColType::Id | ColType::Int => "Tip: Enter a list or range like 1, 3..5, 10",
            ColType::Date => "Tip: Dates or ranges: 2023-01-01, 2023-02..2023-03",
            ColType::Enum => "Tip: One or more enum values, like A,M",
            ColType::Bool => "Tip: true or false",
            ColType::Path | ColType::String => "Tip: Case-insensitive substring match",
        };
        */
        let tip_paragraph =
            Paragraph::new(self.col_type_info.tip).style(Style::default().fg(Color::Gray));
        f.render_widget(tip_paragraph, footer_layout[0]);

        // Divider
        let divider = Block::default()
            .borders(Borders::TOP)
            .title("Help")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(Color::Blue).fg(Color::White));
        f.render_widget(divider, footer_layout[1]);

        // Help text
        let help_text = "Esc: Cancel  |  Enter: Save";
        let help_paragraph =
            Paragraph::new(help_text).style(Style::default().bg(Color::Blue).fg(Color::White));
        f.render_widget(help_paragraph, footer_layout[2]);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        match key.code {
            KeyCode::Esc => {
                return Some(ExplorerAction::Dismiss);
            }
            KeyCode::Enter => {
                let mut input_val = self.input.value();
                let err_str = QueryProcessor::validate_filter(self.col_type_info.rule, input_val);
                match err_str {
                    Some(err_str) => {
                        return Some(ExplorerAction::ShowMessage(MessageBox::new(
                            MessageBoxType::Info,
                            err_str,
                        )));
                    }
                    None => {
                        input_val = input_val.trim();
                        let filter = Filter::new(self.col_name, self.col_type_info.type_name, input_val.to_owned());
                        match self.filter_window_type {
                            FilterWindowType::Add => return Some(ExplorerAction::AddFilter(filter)),
                            FilterWindowType::Edit => {},
                        }
                    }
                }
            }
            _ => {
                self.input.handle_event(&Event::Key(key));
            }
        }

        None
    }
}
