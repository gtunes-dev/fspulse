use crate::explore::{explorer::ExplorerAction};

use super::utils::Utils;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    layout::Alignment,
    style::{Color, Style, Stylize},
    text::{Line, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub enum MessageBoxType {
    Info,
    Error,
}

impl MessageBoxType {
    fn as_title(&self) -> &'static str {
        match self {
            MessageBoxType::Info => "Info:",
            MessageBoxType::Error => "Error:",
        }
    }
}

#[derive(Debug)]
pub struct MessageBoxState {
    message_box_type: MessageBoxType,
    message: String,
}

impl MessageBoxState {
    pub fn new(message_box_type: MessageBoxType, message: String) -> Self {
        MessageBoxState {
            message_box_type,
            message,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ExplorerAction> {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(ExplorerAction::Dismiss),
            _ => None,
        }
    }

    pub fn draw(&self, f: &mut Frame) {
        let popup_area = Utils::centered_rect(60, 20, f.area());
        let popup_height = popup_area.height as usize;

        let block = Block::default()
            .title(self.message_box_type.as_title())
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Double);

        let mut lines = Vec::new();

        for message_line in self.message.split('\n') {
            lines.push(Line::styled(
                message_line.to_owned(),
                Style::default()
                    .fg(self.fg_color())
                    .bg(self.bg_color())
                    .bold(),
            ));
        }

        // Calculate how many blank lines we need
        let used_lines = 1 /* error message */ + 1 /* instruction */;
        let available_space = popup_height.saturating_sub(used_lines + 2); // 2 for top/bottom padding
        for _ in 0..available_space {
            lines.push(Line::raw(""));
        }

        lines.push(Line::styled(
            "(press Esc or Enter to dismiss)",
            Style::default().fg(self.fg_color()).bg(self.bg_color()),
        ));

        let paragraph = Paragraph::new(Text::from(lines))
            .style(Style::default().bg(self.bg_color()))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(block);

        // Clear the popup area to avoid bleed-through
        f.render_widget(Clear, popup_area);
        f.render_widget(paragraph, popup_area);
    }

    fn fg_color(&self) -> Color {
        match self.message_box_type {
            MessageBoxType::Info => Color::Black,
            MessageBoxType::Error => Color::Black,
        }
    }
    fn bg_color(&self) -> Color {
        match self.message_box_type {
            MessageBoxType::Info => Color::Gray,
            MessageBoxType::Error => Color::Red,
        }
    }
}
