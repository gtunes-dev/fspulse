use ratatui::{
    buffer::Buffer, crossterm::event::{KeyCode, KeyEvent}, layout::{Alignment, Rect}, style::{Color, Style, Stylize}, text::Line, widgets::{Paragraph, Widget}
};

use super::{
    domain_model::DomainModel, explorer::ExplorerAction, filter_window::FilterWindow, utils::Utils
};

pub struct ColumnFrame {
    cursor_position: usize,
    scroll_offset: usize,
    area: Rect,
}

impl ColumnFrame {
    pub fn new() -> Self {
        Self {
            cursor_position: 0,
            scroll_offset: 0,
            area: Rect::default(),
        }
    }

    pub fn handle_key(&mut self, model: &mut DomainModel, key: KeyEvent) -> Option<ExplorerAction> {
        let mut explorer_action = None;

        match key.code {
            KeyCode::Down => {
                    self.move_down(model);
            }
            KeyCode::Up => {
                
                    self.move_up();
            }
            KeyCode::Char('+') => {
                if self.cursor_position >= 1 {
                    let idx = self.cursor_position - 1;
                    if idx > 0 {
                        let current_cols = model.current_columns_mut();
                        let item = current_cols.remove(idx);
                        current_cols.insert(idx - 1, item);

                        // maintain the current selection
                        self.cursor_position -= 1;

                        explorer_action = Some(ExplorerAction::RefreshQuery)
                    }
                }
            }
            KeyCode::Char('-') => {
                if self.cursor_position >= 1 {
                    let idx = self.cursor_position - 1;
                    let current_cols = model.current_columns_mut();
                    if idx < current_cols.len() - 1 {
                        let item = current_cols.remove(idx);
                        current_cols.insert(idx + 1, item);

                        // maintain the current selection
                        self.cursor_position += 1;

                        explorer_action = Some(ExplorerAction::RefreshQuery)
                    }
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                   let idx = self.cursor_position - 1;
                    if let Some(col) = model.current_columns_mut().get_mut(idx) {
                        col.selected = !col.selected;

                        explorer_action = Some(ExplorerAction::RefreshQuery)
                    }
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                if let Some(col_option) = model
                    .current_columns()
                    .get(self.cursor_position.saturating_sub(1))
                {
                    explorer_action = Some(ExplorerAction::ShowAddFilter(
                        FilterWindow::new_add_filter_window(col_option.name, col_option.col_info.col_type.info()),
                    ));
                };
            }
            _ => {}
        }

        explorer_action
    }

    pub fn move_up(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;

            if self.cursor_position >= 1 {
                let column_idx = self.cursor_position - 1;
                if column_idx < self.scroll_offset {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
            }
        }
    }

    pub fn move_down(&mut self, model: &DomainModel) {
        if self.cursor_position < model.current_columns().len() {
            self.cursor_position += 1;

            if self.cursor_position >= 1 {
                let column_idx = self.cursor_position - 1;
                let visible_rows = self.visible_rows();

                if column_idx >= self.scroll_offset + visible_rows {
                    self.scroll_offset += 1;
                }
            }
        }
    }

    fn visible_rows(&self) -> usize {
        self.area.height.saturating_sub(2) as usize
    }

    pub fn set_area(&mut self, new_area: Rect) {
        if self.area.height == new_area.height {
            self.area = new_area;
        } else {
            let old_area = self.area;

            self.area = new_area;
            let new_visible_rows = self.visible_rows();

            if old_area.height != new_area.height {
                self.correct_scroll_for_resize(new_visible_rows);
            }
        }
    }

    fn correct_scroll_for_resize(&mut self, new_visible_rows: usize) {
        if self.cursor_position >= 1 {
            let column_idx = self.cursor_position - 1;

            if column_idx < self.scroll_offset {
                self.scroll_offset = column_idx;
            } else if column_idx >= self.scroll_offset + new_visible_rows {
                self.scroll_offset = column_idx.saturating_sub(new_visible_rows.saturating_sub(1));
            } else {
                let max_scroll_offset =
                    column_idx.saturating_sub(new_visible_rows.saturating_sub(1));
                if self.scroll_offset > max_scroll_offset {
                    self.scroll_offset = max_scroll_offset;
                }
            }
        }
    }
}

pub struct ColumnFrameView<'a> {
    frame: &'a mut ColumnFrame,
    model: &'a DomainModel,
    has_focus: bool,
}

impl <'a> ColumnFrameView<'a> {
    pub fn new(frame: &'a mut ColumnFrame, model: &'a DomainModel, has_focus: bool) -> Self {
        Self { frame, model, has_focus }
    }
}

impl Widget for ColumnFrameView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.frame.set_area(area);
        let mut lines = Vec::new();

        let visible_rows = self.frame.visible_rows();
        for (i, col) in self.model
            .current_columns()
            .iter()
            .enumerate()
            .skip(self.frame.scroll_offset)
            .take(visible_rows)
        {
            let checked = if col.selected { "[x]" } else { "[ ]" };

            let text = format!("{checked} {:<20}", col.name);

            let mut line = Line::from(text);

            if self.frame.cursor_position == i + 1 && self.has_focus {
                line = line.style(Style::default().fg(Color::Yellow).bold());
            }

            lines.push(line);
        }

        let block = Utils::new_frame_block_with_title(self.has_focus, "Columns");

        Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left)
            .render(area, buf);

        //f.render_widget(paragraph, area);
    }
}