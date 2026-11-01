use crate::history::HistoryEntry;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
};

// layout of the central box that shows the quit options
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical_padding = (100 - percent_y) / 2;
    let horizontal_padding = (100 - percent_x) / 2;

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(vertical_padding),
            Constraint::Percentage(percent_y),
            Constraint::Percentage(100 - vertical_padding - percent_y),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(horizontal_padding),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(100 - horizontal_padding - percent_x),
        ])
        .split(vertical[1]);

    horizontal[1]
}

// this just draws the thing above
pub(super) fn draw_exit_dialog(frame: &mut Frame<'_>, selected: usize) {
    let area = centered_rect(40, 20, frame.area());
    frame.render_widget(Clear, area);

    let option = |label: &str, index: usize| {
        let text = if selected == index {
            format!("› {label}")
        } else {
            format!("  {label}")
        };

        let style = if selected == index {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
        };

        Span::styled(text, style)
    };

    let dialog = Paragraph::new(vec![
        Line::from(""),
        Line::from("What should happen to your downloads?"),
        Line::from(""),
        Line::from(vec![
            option("[L] Leave", 0),
            Span::raw("   "),
            option("[S] Stop aria2", 1),
            Span::raw("   "),
            option("[C / Esc] Cancel", 2),
        ]),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Exit AriaWatch ")
            .title_alignment(Alignment::Center),
    );

    frame.render_widget(dialog, area);
}

// this draws the add download dialog
pub(super) fn draw_add_dialog(frame: &mut Frame<'_>, input: &str) {
    let area = centered_rect(65, 25, frame.area());
    frame.render_widget(Clear, area);

    let dialog = Paragraph::new(vec![
        Line::from("URL, magnet URI, or local .torrent path"),
        Line::from(""),
        Line::from(format!("> {input}_")),
        Line::from(""),
        Line::from("Enter: add    Esc: cancel"),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Add Download ")
            .title_alignment(Alignment::Center),
    );

    frame.render_widget(dialog, area);
}

// draws the remove download dialog
pub(super) fn draw_remove_dialog(
    frame: &mut Frame<'_>,
    download_name: Option<&str>,
    delete_files: bool,
) {
    let area = centered_rect(50, 28, frame.area());
    frame.render_widget(Clear, area);

    let title = if delete_files {
        " Remove Download and Delete Files "
    } else {
        " Remove Download "
    };

    let warning = if delete_files {
        "Downloaded files under ~/Downloads will be permanently deleted."
    } else {
        "The aria2 entry will be removed, but downloaded files will remain."
    };

    let dialog = Paragraph::new(vec![
        Line::from(format!(
            "Selected: {}",
            download_name.unwrap_or("No download selected")
        )),
        Line::from(""),
        Line::from(warning),
        Line::from(""),
        Line::from("Enter / Y: confirm    Esc / N: cancel"),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_alignment(Alignment::Center),
    );

    frame.render_widget(dialog, area);
}

// draws the history dialogbox
pub(super) fn draw_history_dialog(
    frame: &mut Frame<'_>,
    entries: &[HistoryEntry],
    selected_index: usize,
) {
    let area = centered_rect(86, 76, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Download History ({})", entries.len()))
        .title_alignment(Alignment::Center);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new("No download history yet.").alignment(Alignment::Center),
            sections[0],
        );
    } else {
        let visible_rows = (sections[0].height.saturating_sub(1).max(1)) as usize;
        let start = selected_index
            .saturating_add(1)
            .saturating_sub(visible_rows);

        let rows = entries
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_rows)
            .map(|(index, entry)| {
                let file_summary = if entry.failed_file_deletions > 0 {
                    format!(
                        "{} deleted / {} failed",
                        entry.deleted_files, entry.failed_file_deletions
                    )
                } else if entry.deleted_files > 0 {
                    format!("{} deleted", entry.deleted_files)
                } else {
                    String::from("-")
                };

                let row = Row::new(vec![
                    Cell::from(entry.id.to_string()),
                    Cell::from(entry.name.as_str()),
                    Cell::from(entry.status.as_str()),
                    Cell::from(entry.last_seen_at.as_str()),
                    Cell::from(file_summary),
                ]);

                if index == selected_index {
                    row.style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
                } else {
                    row
                }
            });

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(7),
                Constraint::Percentage(33),
                Constraint::Percentage(15),
                Constraint::Percentage(25),
                Constraint::Percentage(20),
            ],
        )
        .header(
            Row::new(["ID", "Name", "Status", "Last Seen", "File Deletion"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        );

        frame.render_widget(table, sections[0]);
    }

    frame.render_widget(
        Paragraph::new("↑/↓ or j/k: navigate  ·  r: refresh  ·  h/Esc: close")
            .alignment(Alignment::Center),
        sections[1],
    );
}
