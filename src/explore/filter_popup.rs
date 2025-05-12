use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Paragraph, StatefulWidget, Widget, Wrap},
};
use tui_textarea::TextArea;

use crate::query::{columns::ColTypeInfo, QueryProcessor};

use super::{
    domain_model::Filter,
    explorer::ExplorerAction,
    message_box::{MessageBox, MessageBoxType},
    utils::{StylePalette, Utils},
};

#[derive(Debug, Copy, Clone)]
enum FilterPopupType {
    Add,
    Edit,
}

impl FilterPopupType {
    fn to_str(self) -> &'static str {
        match self {
            FilterPopupType::Add => "Add",
            FilterPopupType::Edit => "Edit",
        }
    }
}

#[derive(Debug)]
pub struct FilterPopupState {
    filter_popup_type: FilterPopupType,
    col_name: &'static str,
    filter_index: Option<usize>,
    col_type_info: ColTypeInfo,
    text_area: TextArea<'static>,
}

pub struct FilterPopupWidget;

impl StatefulWidget for FilterPopupWidget {
    type State = FilterPopupState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let popup_style = StylePalette::PopUp.style();

        let outer_block = Block::default().borders(Borders::ALL).style(popup_style);

        let [label_area, _, input_area, tip_area, _, help_area] = Layout::vertical([
            Constraint::Length(1), // label
            Constraint::Length(1), // spacer
            Constraint::Length(3), // input
            Constraint::Length(2), // tip
            Constraint::Length(1), // spacer
            Constraint::Length(2), // help
        ])
        .areas(outer_block.inner(area));

        outer_block.render(area, buf);

        // Label
        let label_text = format!(
            "{} Filter ({}):",
            state.filter_popup_type.to_str(),
            state.col_name
        );
        Paragraph::new(label_text)
            .alignment(Alignment::Left)
            .style(popup_style)
            .render(label_area, buf);

        state.text_area.set_style(StylePalette::PopUp.style());
        state.text_area.set_cursor_line_style(Style::default());
        state
            .text_area
            .set_block(Block::default().borders(Borders::ALL));
        state.text_area.render(input_area, buf);

        // Tip
        Paragraph::new(state.col_type_info.tip)
            .style(popup_style)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .render(tip_area, buf);

        Utils::render_popup_help("Esc: Cancel  |  Enter: Save", help_area, buf);
    }
}

impl FilterPopupState {
    fn new(
        filter_popup_type: FilterPopupType,
        col_name: &'static str,
        filter_index: Option<usize>,
        col_type_info: ColTypeInfo,
    ) -> Self {
        FilterPopupState {
            filter_popup_type,
            col_name,
            filter_index,
            col_type_info,
            text_area: TextArea::default(),
        }
    }

    pub fn new_add_filter_popup(col_name: &'static str, col_type_info: ColTypeInfo) -> Self {
        Self::new(FilterPopupType::Add, col_name, None, col_type_info)
    }

    pub fn new_edit_filter_popup(
        col_name: &'static str,
        filter_index: usize,
        col_type_info: ColTypeInfo,
        filter_text: String,
    ) -> Self {
        FilterPopupState {
            filter_popup_type: FilterPopupType::Edit,
            col_name,
            filter_index: Some(filter_index),
            col_type_info,
            text_area: {
                let lines = vec![filter_text];
                TextArea::new(lines)
            },
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        match key.code {
            KeyCode::Esc => {
                return Some(ExplorerAction::Dismiss);
            }
            KeyCode::Enter => {
                let mut input_val = self.text_area.lines()[0].as_str();
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

                        match self.filter_popup_type {
                            FilterPopupType::Add => {
                                let filter = Filter::new(
                                    self.col_name,
                                    self.col_type_info.type_name,
                                    self.col_type_info,
                                    input_val.to_owned(),
                                );
                                return Some(ExplorerAction::AddFilter(filter));
                            }
                            FilterPopupType::Edit => {
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
                self.text_area.input(key);
            }
        }
        None
    }
}
