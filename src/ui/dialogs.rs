use std::path::Path;

use crate::{
    domain::download::Download,
    history::HistoryEntry,
    ui::{
        TorrentPreparation, TorrentPreparationSource, TorrentSelection,
        formatting::{format_bytes, format_duration},
    },
};
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
        Line::from("URL, magnet URI, local .torrent path, or plain-text link file"),
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
        "Downloaded files in the managed directory will be permanently deleted."
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

// draws the torrent file-selection dialog
pub(super) fn draw_torrent_selection_dialog(frame: &mut Frame<'_>, selection: &TorrentSelection) {
    let area = centered_rect(88, 80, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Select Torrent Files ({}/{})",
            selection.selected_count(),
            selection.files().len()
        ))
        .title_alignment(Alignment::Center);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

    let summary = Paragraph::new(format!("Torrent GID: {}", selection.gid()));
    frame.render_widget(summary, sections[0]);

    let visible_rows = sections[1].height.saturating_sub(1).max(1) as usize;
    let first_visible_index = selection
        .cursor()
        .saturating_add(1)
        .saturating_sub(visible_rows);

    let rows = selection
        .files()
        .iter()
        .enumerate()
        .skip(first_visible_index)
        .take(visible_rows)
        .map(|(index, file)| {
            let checkbox = if file.selected { "[x]" } else { "[ ]" };

            let row = Row::new(vec![
                Cell::from(format!("{checkbox} {}", file.path)),
                Cell::from(format_bytes(file.length)),
                Cell::from(format_bytes(file.completed_length)),
            ]);

            if index == selection.cursor() {
                row.style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
            } else {
                row
            }
        });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(66),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
        ],
    )
    .header(
        Row::new(["    File", "Size", "Completed"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .column_spacing(1);

    frame.render_widget(table, sections[1]);

    let footer = match selection.action() {
        Some(super::TorrentSelectionAction::Applying) => {
            "Applying file selection and starting download..."
        }
        Some(super::TorrentSelectionAction::Cancelling) => "Cancelling torrent...",
        None => {
            "↑/↓ or j/k: move  ·  Space: toggle  ·  a: select all  ·  n: deselect all  ·  Enter: start  ·  Esc: cancel"
        }
    };

    frame.render_widget(
        Paragraph::new(footer).alignment(Alignment::Center),
        sections[2],
    );
}

// displays the progress of asyn torrent prep operation
pub(super) fn draw_torrent_prep_dialog(frame: &mut Frame<'_>, preparation: &TorrentPreparation) {
    let area = centered_rect(62, 36, frame.area());
    frame.render_widget(Clear, area);

    let spinner_frames = ["|", "/", "-", "\\"];
    let elapsed = preparation.elapsed();
    let spinner_index = ((elapsed.as_millis() / 150) % 4) as usize;
    let spinner = spinner_frames[spinner_index];

    let (title, description) = match preparation.source() {
        TorrentPreparationSource::Magnet => (
            " Preparing Magnet Torrent ",
            "Waiting for torrent metadata from the swarm...",
        ),
        TorrentPreparationSource::LocalFile => {
            (" Preparing Torrent File ", "Reading torrent metadata...")
        }
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{spinner} {description}"),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "Elapsed: {}",
            format_duration(Some(elapsed.as_secs()))
        )),
    ];

    if let Some(gid) = preparation.metadata_gid() {
        lines.push(Line::from(format!("Metadata GID: {gid}")));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("AriaWatch remains responsive while this runs."));

    let dialog = Paragraph::new(lines).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_alignment(Alignment::Center),
    );

    frame.render_widget(dialog, area);
}

// now this one is an interesting one though,,, like you see, a torrent has multiple files, but
// showing them all directly inside the downloads list is tedious and would make it confusing when
// there are other entries in the list too,,, so the torrent shows up as just one entry with a
// little arrow to its left, and pressing [f] on it would open a dialog that shows all the files.
pub(super) fn draw_download_files_dialog(
    frame: &mut Frame<'_>,
    download: &Download,
    selected_index: usize,
) {
    let area = centered_rect(88, 80, frame.area());
    frame.render_widget(Clear, area);

    let title = format!(
        " Download Files: {} ({}) ",
        download.name,
        download.files.len()
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Center);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    if download.files.is_empty() {
        frame.render_widget(
            Paragraph::new("No file entries were reported by aria2"),
            sections[0],
        );
        frame.render_widget(
            Paragraph::new("f / Esc: clone").alignment(Alignment::Center),
            sections[1],
        );
        return;
    }

    let selected_index = selected_index.min(download.files.len() - 1);
    let visible_rows = sections[0].height.saturating_sub(1).max(1) as usize;
    let first_visible_index = selected_index
        .saturating_add(1)
        .saturating_sub(visible_rows);

    let rows = download
        .files
        .iter()
        .enumerate()
        .skip(first_visible_index)
        .take(visible_rows)
        .map(|(index, file)| {
            let path = Path::new(&file.path);
            let displayed_path = download
                .directory
                .as_deref()
                .and_then(|directory| path.strip_prefix(directory).ok())
                .unwrap_or(path);

            let row = Row::new(vec![
                Cell::from(displayed_path.display().to_string()),
                Cell::from(format_bytes(file.size)),
                Cell::from(format_bytes(file.downloaded)),
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
            Constraint::Percentage(66),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
        ],
    )
    .header(
        Row::new(["    File", "Size", "Downloaded"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .column_spacing(1);

    frame.render_widget(table, sections[0]);

    frame.render_widget(
        Paragraph::new("↑/↓ or j/k: navigate  ·  f/Esc: close").alignment(Alignment::Center),
        sections[1],
    );
}
