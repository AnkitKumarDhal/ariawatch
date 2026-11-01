use super::{
    PendingRemoval,
    dialogs::{draw_add_dialog, draw_exit_dialog, draw_history_dialog, draw_remove_dialog},
};
use crate::{app_state::AppState, history::HistoryEntry};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table},
};

// just formats the size by like B, KiB, MiB, and such
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

// now this one does the same but for time rmeaining (i wonder how i can reuse this and the one above)
fn format_duration(seconds: Option<u64>) -> String {
    let Some(seconds) = seconds else {
        return String::from("N/A");
    };

    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remaining_seconds = seconds % 60;

    if days > 0 {
        format!("{days}D {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remaining_seconds}s")
    } else {
        format!("{remaining_seconds}s")
    }
}

pub(super) struct DrawContext<'a> {
    pub(super) state: &'a AppState,
    pub(super) status_message: &'a str,
    pub(super) managed_mode: bool,
    pub(super) show_exit_dialog: bool,
    pub(super) selected_exit_option: usize,
    pub(super) add_input: Option<&'a str>,
    pub(super) pending_removal: Option<&'a PendingRemoval>,
    pub(super) history_entries: Option<&'a [HistoryEntry]>,
    pub(super) history_selection: usize,
}

// public function to draw the TUI
pub(super) fn draw(frame: &mut Frame<'_>, context: DrawContext<'_>) {
    let DrawContext {
        state,
        status_message,
        managed_mode,
        show_exit_dialog,
        selected_exit_option,
        add_input,
        pending_removal,
        history_entries,
        history_selection,
    } = context;

    // 4 sections for the TUI
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(9),
            Constraint::Length(3),
        ])
        .split(frame.area());

    // Header content
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" AriaWatch ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("- aria2 download monitor"),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, sections[0]);

    // Download table content
    let table_header = Row::new(["  Name", "Status", "Progress", "Speed", "Remaining"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let selected_download = state.selected_download();
    let selected_gid = selected_download.map(|download| download.gid.as_str());

    let selected_index = selected_gid.and_then(|gid| {
        state
            .downloads()
            .iter()
            .position(|download| download.gid.as_str() == gid)
    });

    let visible_rows = sections[1].height.saturating_sub(3) as usize;

    let first_visible_index = selected_index
        .map(|index| index.saturating_add(1).saturating_sub(visible_rows))
        .unwrap_or(0);

    let rows = state
        .downloads()
        .iter()
        .skip(first_visible_index)
        .take(visible_rows)
        .map(|download| {
            let progress = match download.percentage_downloaded() {
                Some(percent) => format!("{percent:.1}%"),
                None => String::from("N/A"),
            };

            let speed = format!("{}/s", format_bytes(download.speed));
            let remaining = format_bytes(download.remaining_bytes());

            let is_selected = Some(download.gid.as_str()) == selected_gid;

            // this is the cursor hehe
            let name = if is_selected {
                format!("▶ {}", download.name)
            } else {
                format!("  {}", download.name)
            };

            let row = Row::new(vec![
                Cell::from(name),
                Cell::from(download.status.label()),
                Cell::from(progress),
                Cell::from(speed),
                Cell::from(remaining),
            ]);

            if is_selected {
                row.style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
            } else {
                row
            }
        });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(16),
            Constraint::Percentage(12),
            Constraint::Percentage(16),
            Constraint::Percentage(16),
        ],
    )
    .header(table_header)
    .column_spacing(1)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Downloads ({}) ", state.download_count())),
    );

    frame.render_widget(table, sections[1]);

    // now comes the "more info" box on the download
    // the reason i call it the "more info" box is because i have no clue what its actually called :3
    let details_title = selected_download
        .map(|download| format!(" Selected download: {} ", download.name))
        .unwrap_or_else(|| String::from(" Selected download "));

    let details_block = Block::default().borders(Borders::ALL).title(details_title);

    let details_inner = details_block.inner(sections[2]);
    frame.render_widget(details_block, sections[2]);

    let details_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(details_inner);

    if let Some(download) = selected_download {
        let mut details = vec![
            Line::from(format!(
                "Status: {}  |  ETA: {}",
                download.status.label(),
                format_duration(download.estimated_remaining_seconds())
            )),
            Line::from(format!(
                "Downloaded: {} / {}  |  Remaining: {}",
                format_bytes(download.downloaded),
                format_bytes(download.size),
                format_bytes(download.remaining_bytes())
            )),
            Line::from(format!("Speed: {}/s", format_bytes(download.speed))),
            Line::from(format!(
                "Destination: {}",
                download.directory.as_deref().unwrap_or("N/A")
            )),
            Line::from(format!(
                "Source URI: {}",
                download.source_url.as_deref().unwrap_or("N/A")
            )),
        ];

        if let Some(error) = download
            .error_message
            .as_deref()
            .filter(|message| !message.trim().is_empty())
        {
            details.push(Line::from(format!("Error: {error}")));
        }

        frame.render_widget(Paragraph::new(details), details_sections[0]);

        let progress = download.percentage_downloaded();
        let ratio = progress.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0;
        let label = progress
            .map(|percent| format!("{percent:.1}%"))
            .unwrap_or_else(|| String::from("N/A"));

        let gauge = Gauge::default().ratio(ratio).label(label);
        frame.render_widget(gauge, details_sections[1]);
    } else {
        frame.render_widget(
            Paragraph::new("No downloads to inspect yet."),
            details_sections[0],
        );
    }

    // Footer content
    let exit_hint = if managed_mode {
        "a:add h:history p:pause r:resume P/R:all x:remove X:delete+remove q: exit options"
    } else {
        "a:add h:history p:pause r:resume P/R:all x:remove X:managed only q: quit"
    };

    let footer = Paragraph::new(format!(
        " {status_message} · {} active · {exit_hint}",
        state.active_download_count()
    ))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, sections[3]);

    if show_exit_dialog {
        draw_exit_dialog(frame, selected_exit_option);
    }

    if let Some(input) = add_input {
        draw_add_dialog(frame, input);
    }

    if let Some(pending) = pending_removal {
        draw_remove_dialog(frame, Some(&pending.name), pending.delete_files);
    }

    if let Some(entries) = history_entries {
        draw_history_dialog(frame, entries, history_selection);
    }
}

// Tests for duration formatting
#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn formats_unknown_duration() {
        assert_eq!(format_duration(None), "N/A");
    }

    #[test]
    fn formats_seconds() {
        assert_eq!(format_duration(Some(59)), "59s");
    }

    #[test]
    fn formats_minutes_and_seconds() {
        assert_eq!(format_duration(Some(125)), "2m 5s");
    }

    #[test]
    fn formats_hours_and_minutes() {
        assert_eq!(format_duration(Some(3661)), "1h 1m");
    }

    #[test]
    fn formats_days_and_hours() {
        assert_eq!(format_duration(Some(90_061)), "1D 1h");
    }
}
