use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use super::utils::Utils;

pub struct LimitWidget {
    limit: String,
    has_focus: bool,
}

impl LimitWidget {
    pub fn new(limit: String, has_focus: bool) -> Self {
        Self { limit, has_focus }
    }

    fn limit_style(&self) -> Style {
        if self.has_focus {
            Style::new().bg(Color::Blue).fg(Color::White) // .add_modifier(Modifier::BOLD)
        } else {
            // normal look
            Style::default()
        }
    }
}

impl Widget for LimitWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let styled_limit = Span::styled(self.limit.to_string(), self.limit_style());
        let line = Line::from(vec![Span::raw("Row Limit: "), styled_limit]);

        let block = Utils::new_frame_block(self.has_focus);
        Paragraph::new(line)
        .block(block)
        .render(area, buf);

    }
}
