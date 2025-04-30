use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Row, Table, TableState},
    Frame,
};

#[derive(Debug, Clone)]
pub struct Filter {
    pub column: String,
    pub filter_text: String,
    pub type_name: String,
}

pub struct FilterFrame {
    filters: Vec<Filter>,
    table_state: TableState,
    area: Rect,
}

impl FilterFrame {
    pub fn new() -> Self {
        let mut state = TableState::default();
        state.select(Some(0));

        Self {
            filters: Vec::new(),
            table_state: state,
            area: Rect::default(),
        }
    }

    pub fn set_area(&mut self, area: Rect) {
        self.area = area;
    }

    pub fn set_filters(&mut self, filters: Vec<Filter>) {
        self.filters = filters;
        self.table_state.select(Some(0));
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, is_focused: bool) {
        self.set_area(area);

        let header = Row::new(vec!["Column", "Type", "Filter"])
            .style(Style::default().fg(Color::Black).bg(Color::Gray).bold());

        let rows = self.filters.iter().map(|f| {
            Row::new(vec![
                Span::raw(f.column.clone()),
                Span::raw(f.type_name.clone()),
                Span::raw(f.filter_text.clone()),
            ])
        });

        let block = if is_focused {
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title("Filters")
        } else {
            Block::default().borders(Borders::ALL).title("Filters")
        };

        let constraints = vec![
            Constraint::Length(15),
            Constraint::Length(10),
            Constraint::Min(10),
        ];

        let table = Table::new(rows, constraints)
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().fg(Color::Yellow))
            .highlight_symbol("» ");

        f.render_stateful_widget(table, area, &mut self.table_state);
    }
}
