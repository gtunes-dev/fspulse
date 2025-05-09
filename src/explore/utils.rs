use ratatui::{
    buffer::Buffer,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    widgets::{Block, Borders, Paragraph, TableState, Widget},
};

pub struct Utils;

pub enum StylePalette {
    TableHeader,
    TableRowHighlight,
    Tab,
    TabHighlight,
    TextFocus,
    PopUp,
}

impl StylePalette {
    pub fn style(&self) -> Style {
        match self {
            StylePalette::TableHeader => Style::default().bg(Color::DarkGray).bold(),
            StylePalette::TableRowHighlight => Style::default().fg(Color::Black).bg(Color::Cyan),
            StylePalette::Tab => Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
            StylePalette::TabHighlight => Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            StylePalette::TextFocus => Style::default().fg(Color::Black).bg(Color::Cyan),
            StylePalette::PopUp => Style::default().bg(Color::White).fg(Color::Black),
        }
    }
}

impl Utils {
    pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

    pub fn _center_horizontal(area: Rect, width: u16) -> Rect {
        let [area] = Layout::horizontal([Constraint::Length(width)])
            .flex(Flex::Center)
            .areas(area);
        area
    }

    pub fn _center_vertical(area: Rect, height: u16) -> Rect {
        let [area] = Layout::vertical([Constraint::Length(height)])
            .flex(Flex::Center)
            .areas(area);
        area
    }

    pub fn center(area: Rect, horizontal: Constraint, vertical: Constraint) -> Rect {
        let [area] = Layout::horizontal([horizontal])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([vertical]).flex(Flex::Center).areas(area);
        area
    }

    pub fn handle_table_state_keys(
        table_state: &mut TableState,
        total_rows: usize,
        visible_rows: usize,
        key: KeyEvent,
    ) -> bool {
        let mut handled = true;

        match key.code {
            KeyCode::Home => {
                match total_rows > 0 {
                    true => table_state.select(Some(0)),
                    false => table_state.select(None),
                };
            }
            KeyCode::End => {
                match total_rows > 0 {
                    true => table_state.select(Some(total_rows - 1)),
                    false => table_state.select(None),
                };
            }
            KeyCode::Up => {
                if let Some(selected) = table_state.selected() {
                    table_state.select(Some(selected.saturating_sub(1)));
                }
            }
            KeyCode::Down => {
                if let Some(selected) = table_state.selected() {
                    if selected + 1 < total_rows {
                        table_state.select(Some(selected + 1));
                    }
                }
            }
            KeyCode::PageUp => {
                if let Some(selected) = table_state.selected() {
                    let new_selected = selected.saturating_sub(visible_rows);
                    table_state.select(Some(new_selected));
                }
            }
            KeyCode::PageDown => {
                if let Some(selected) = table_state.selected() {
                    let new_selected = selected + visible_rows.min(total_rows.saturating_sub(1));
                    table_state.select(Some(new_selected));
                }
            }
            _ => handled = false,
        }

        handled
    }

    pub fn render_popup_help(help_text: &str, area: Rect, buf: &mut Buffer) {
        // only render if height is exactly 2 - this enforces correctness and avoids panics
        if area.height == 2 {
            let [divider_area, help_text_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

            let style = StylePalette::PopUp.style();
            
            Block::default()
                .borders(Borders::TOP)
                .title("Help")
                .title_alignment(Alignment::Center)
                .style(style)
                .render(divider_area, buf);

            Paragraph::new(help_text).style(style).render(help_text_area, buf);
        }
    }
}
