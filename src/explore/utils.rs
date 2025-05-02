use ratatui::{
    crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Direction, Layout, Rect}, style::Color, widgets::{Block, BorderType, Borders, TableState}
};

pub struct Utils;

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
            _ => handled = false
        }

        handled
    }

    pub fn new_frame_block(is_focused: bool, title_str: &'static str) -> Block<'static> {
        let block = if is_focused {
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Color::LightCyan)
                .title_style(Color::White)
                .title(title_str)
        } else {
            Block::default().borders(Borders::ALL).title(title_str)
        };

        return block;
    }
}
