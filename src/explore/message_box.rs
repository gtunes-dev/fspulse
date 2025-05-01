use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::Alignment, style::{Color, Style, Stylize}, text::{Line, Text}, widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap}, Frame};
use super::utils::Utils;

pub enum MessageBoxType {
    Info,
    Error
}

impl MessageBoxType {
    fn as_title(&self) -> &'static str {
        match self {
            MessageBoxType::Info => "Info:",
            MessageBoxType::Error => "Error:"
        }
    }
}

pub struct MessageBox {
    message_box_type: MessageBoxType,
    message: String,
}

impl MessageBox {
    pub fn new(message_box_type: MessageBoxType, message: String) -> Self {
        MessageBox { message_box_type, message }
    }

    pub fn is_dismiss_event(&self, key: KeyEvent) -> bool {
        key.code == KeyCode::Esc || key.code == KeyCode::Enter
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

        lines.push(Line::styled(
            self.message.to_owned(),
            Style::default().fg(Color::White).bg(Color::Red).bold(),
        ));

        // Calculate how many blank lines we need
        let used_lines = 1 /* error message */ + 1 /* instruction */;
        let available_space = popup_height.saturating_sub(used_lines + 2); // 2 for top/bottom padding
        for _ in 0..available_space {
            lines.push(Line::raw(""));
        }

        lines.push(Line::styled(
            "(press Esc or Enter to dismiss)",
            Style::default().fg(Color::Gray).bg(Color::Red),
        ));

        let paragraph = Paragraph::new(Text::from(lines))
            .style(Style::default().bg(Color::Red))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(block);

        // Clear the popup area to avoid bleed-through
        f.render_widget(Clear, popup_area);
        f.render_widget(paragraph, popup_area);
    }
}
