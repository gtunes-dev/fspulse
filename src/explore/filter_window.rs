use ratatui::{
    crossterm::event::{Event, KeyCode, KeyEvent},
    layout::{Alignment, Constraint, Layout, Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use tui_input::{backend::crossterm::EventHandler, Input};

use crate::query::{columns::ColTypeInfo, QueryProcessor};

use super::{
    domain_model::Filter,
    explorer::ExplorerAction,
    message_box::{MessageBox, MessageBoxType}, utils::StylePalette,
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
        col_type_info: ColTypeInfo,
    ) -> Self {
        FilterWindow {
            filter_window_type,
            col_name,
            filter_index,
            col_type_info,
            input: Input::default(),
        }
    }

    pub fn new_add_filter_window(col_name: &'static str, col_type_info: ColTypeInfo) -> Self {
        Self::new(FilterWindowType::Add, col_name, None, col_type_info)
    }

    pub fn new_edit_filter_window(
        col_name: &'static str,
        filter_index: usize,
        col_type_info: ColTypeInfo,
        filter_text: String,
    ) -> Self {
        FilterWindow {
            filter_window_type: FilterWindowType::Edit,
            col_name,
            filter_index: Some(filter_index),
            col_type_info,
            input: Input::default().with_value(filter_text),
        }
    }

    pub fn draw(&mut self, f: &mut Frame, is_top_window: bool) {
        let screen = f.area();

        let popup_width = screen.width.min(80);
        let popup_height = 12;

        let popup_x = screen.x + (screen.width.saturating_sub(popup_width)) / 2;
        let popup_y = screen.y + (screen.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };

        f.render_widget(Clear, popup_area);

        let popup_style = StylePalette::PopUp.style();

        let outer_block = Block::default().borders(Borders::ALL).style(popup_style);
        f.render_widget(&outer_block, popup_area);

        let [label, _, input, tip, _, divider, help] =
            Layout::vertical([
                Constraint::Length(1),  // label
                Constraint::Length(1),  // spacer
                Constraint::Length(3),  // input
                Constraint::Length(2), // tip
                Constraint::Length(1), // spacer
                Constraint::Length(1), // divider
                Constraint::Length(1), // help
            ]).areas(outer_block.inner(popup_area));

        // Label
        let label_text = format!("Add {} filter:", self.col_name);
        let label_paragraph = Paragraph::new(label_text).alignment(Alignment::Left).style(popup_style);
        f.render_widget(label_paragraph, label);

        // Input
        let scroll = self.input.visual_scroll((input.width - 2) as usize);
        let input_block = Block::default().title("Filter").borders(Borders::ALL);
        let input_paragraph = Paragraph::new(self.input.value())
            .block(input_block)
            .scroll((0, scroll as u16))
            .style(popup_style);
        f.render_widget(input_paragraph, input);

        // Cursor positioning
        if is_top_window {
            let x = self.input.visual_cursor().saturating_sub(scroll) as u16;
            f.set_cursor_position(Position::new(input.x + 1 + x, input.y + 1));
        }

        // Tip
        let tip_paragraph = Paragraph::new(self.col_type_info.tip)
            .style(popup_style)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        f.render_widget(tip_paragraph, tip);

        // Divider
        let divider_block = Block::default()
            .borders(Borders::TOP)
            .title("Help")
            .title_alignment(Alignment::Center)
            .style(popup_style);
        f.render_widget(divider_block, divider);

        // Help text
        let help_text = "Esc: Cancel  |  Enter: Save";
        let help_paragraph =
            Paragraph::new(help_text).style(popup_style);
        f.render_widget(help_paragraph, help);
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

                        match self.filter_window_type {
                            FilterWindowType::Add => {
                                let filter = Filter::new(
                                    self.col_name,
                                    self.col_type_info.type_name,
                                    self.col_type_info,
                                    input_val.to_owned(),
                                );
                                return Some(ExplorerAction::AddFilter(filter));
                            }
                            FilterWindowType::Edit => {
                                if let Some(filter_index) = self.filter_index {
                                    return Some(ExplorerAction::UpdateFilter(
                                        filter_index,
                                        input_val.to_owned(),
                                    ));
                                }
                            }
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
