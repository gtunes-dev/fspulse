use crate::query::columns::ColAlign;

/// Format query result data as a markdown table.
/// Returns the table string, or a message if no rows were returned.
pub fn format_table(
    headers: &[String],
    rows: &[Vec<String>],
    alignments: &[ColAlign],
) -> String {
    if headers.is_empty() {
        return "No columns to display.".to_string();
    }

    if rows.is_empty() {
        return "No results found.".to_string();
    }

    // Calculate column widths (min: header length)
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let mut out = String::new();

    // Header row
    out.push('|');
    for (i, header) in headers.iter().enumerate() {
        out.push_str(&format!(" {:width$} |", header, width = widths[i]));
    }
    out.push('\n');

    // Separator row with alignment
    out.push('|');
    for (i, width) in widths.iter().enumerate() {
        let align = alignments.get(i).copied().unwrap_or(ColAlign::Left);
        let sep = match align {
            ColAlign::Left => format!(" {:-<width$} |", "", width = *width),
            ColAlign::Right => format!(" {:-<width$}:|", "", width = width.saturating_sub(1).max(1)),
            ColAlign::Center => format!(":{:-<width$}:|", "", width = width.saturating_sub(1).max(1)),
        };
        out.push_str(&sep);
    }
    out.push('\n');

    // Data rows
    for row in rows {
        out.push('|');
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                out.push_str(&format!(" {:width$} |", cell, width = widths[i]));
            }
        }
        out.push('\n');
    }

    out
}
