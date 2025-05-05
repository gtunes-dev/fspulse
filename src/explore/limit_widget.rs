use ratatui::{
    buffer::Buffer, crossterm::event::{KeyCode, KeyEvent}, layout::Rect, style::Style, text::{Line, Span}, widgets::{Paragraph, Widget}
};

use super::{explorer::ExplorerAction, utils::StylePalette};

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
            StylePalette::TableRowHighlight.style()
        } else {
            // normal look
            Style::default()
        }
    }

    pub fn handle_key(key: KeyEvent) -> Option<ExplorerAction> {
        let mut action = None;

        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter => {
                action = Some(ExplorerAction::ShowLimit);
            }
            _ => {}
        }

        action
    }
}

impl Widget for LimitWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let limit_str = if self.limit.is_empty() {
            " All"
        } else {
            &format!(" {}", self.limit)
        };

        let styled_limit = Span::styled(limit_str, self.limit_style());

        let line = Line::from(styled_limit);

        Paragraph::new(line)
        .render(area, buf);

    }
}
