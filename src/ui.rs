// Renders the actual UI and terminal input
use crate::{
    app_state::AppState,
    backend::DownloadBackend,
    domain::status::Status,
    history::{HistoryEntry, HistoryStore},
    logging,
    monitor::MonitorUpdate,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{
        Alignment,
        Constraint::{self},
        Direction, Layout, Rect,
    },
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Table},
};
use std::{io, path::Path, sync::mpsc::Receiver, time::Duration};

// this is for the choice of how to exit the applciation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitChoice {
    LeaveRunning,
    StopController,
}

// this is the struct that stores the details of the download that was requested to be removed
#[derive(Debug, Clone)]
struct PendingRemoval {
    gid: String,
    name: String,
    status: Status,
    delete_files: bool,
}

// Restores terminal state even if the UI exits unexpectedly
struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn new() -> Self {
        Self { active: true }
    }

    // this one reports cleanup errors
    fn restore(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();

        if let Err(error) = execute!(io::stdout(), crossterm::cursor::Show) {
            errors.push(format!("Coult not restore cursor: {error}"));
        }

        if let Err(error) = execute!(io::stdout(), LeaveAlternateScreen) {
            errors.push(format!("Couldd not leave alternate screen: {error}"));
        }

        if let Err(error) = disable_raw_mode() {
            errors.push(format!("Could not disable raw mode: {error}"));
        }

        if errors.is_empty() {
            self.active = false;
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

impl Drop for TerminalGuard {
    // this one makes the best-effort attmept if the normal cleanup path didn't finish successfully
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        let _ = execute!(io::stdout(), crossterm::cursor::Show);
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

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
fn draw_exit_dialog(frame: &mut Frame<'_>, selected: usize) {
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
fn draw_add_dialog(frame: &mut Frame<'_>, input: &str) {
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
fn draw_remove_dialog(frame: &mut Frame<'_>, download_name: Option<&str>, delete_files: bool) {
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
fn draw_history_dialog(frame: &mut Frame<'_>, entries: &[HistoryEntry], selected_index: usize) {
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

// checks if input is a URI or not
fn input_is_uri(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();

    [
        "http://", "https://", "ftp://", "ftps://", "sftp://", "magnet:?",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

// just a kind of messenger that takes the result of something, and has two states - one is the
// success statement and another is the action statement,,, basically like this happened - this is
// supposed to happen
fn operation_feedback(
    result: Result<(), String>,
    success_message: &str,
    action_name: &str,
) -> String {
    match result {
        Ok(()) => success_message.to_owned(),
        Err(error) => {
            logging::error(&format!("{action_name} failed: {error}"));
            format!("{action_name} failed; see logs")
        }
    }
}

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

// public function to draw the TUI
fn draw(
    frame: &mut Frame<'_>,
    state: &AppState,
    status_message: &str,
    managed_mode: bool,
    show_exit_dialog: bool,
    selected_exit_option: usize,
    add_input: Option<&str>,
    pending_removal: Option<&PendingRemoval>,
    history_entries: Option<&[HistoryEntry]>,
    history_selection: usize,
) {
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

// Function to ope nthe terminal dashboard nad handle keyboard input
pub fn run(
    mut state: AppState,
    updates: Receiver<MonitorUpdate>,
    managed_mode: bool,
    back: &dyn DownloadBackend,
    history: &mut HistoryStore,
) -> Result<ExitChoice, String> {
    enable_raw_mode().map_err(|error| format!("Could not enable raw mode: {error}"))?;

    let mut stdout = io::stdout();

    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(format!("Could not enter Alternate Screen: {error}"));
    }

    let mut terminal_guard = TerminalGuard::new();

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);

            return Err(format!("Could not initialize terminal: {error}"));
        }
    };

    let app_result = event_loop(
        &mut terminal,
        &mut state,
        &updates,
        managed_mode,
        back,
        history,
    );

    let cursor_result = terminal
        .show_cursor()
        .map_err(|error| format!("Could not restore cursor: {error}"));

    let cleanup_result = terminal_guard.restore();

    let exit_choice = app_result?;
    cursor_result?;
    cleanup_result?;

    Ok(exit_choice)
}

// Handles keyboard input
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    updates: &Receiver<MonitorUpdate>,
    managed_mode: bool,
    backend: &dyn DownloadBackend,
    history: &mut HistoryStore,
) -> Result<ExitChoice, String> {
    let mut status_message = "Starting live monitor".to_owned();
    let mut redraw_needed = true;
    let mut show_exit_dialog = false;
    let mut selected_exit_option = 0_usize;
    let mut add_input: Option<String> = None;
    let mut pending_removal: Option<PendingRemoval> = None;
    let mut history_view: Option<Vec<HistoryEntry>> = None;
    let mut history_selection = 0usize;

    loop {
        for update in updates.try_iter() {
            match update {
                MonitorUpdate::Snapshot(downloads) => {
                    let downloads: Vec<_> = downloads
                        .into_iter()
                        .filter(|download| !state.is_removed_gid(&download.gid))
                        .collect();

                    let history_reconcilation = history.reconcile_snapshot(&downloads);

                    if let Err(error) = &history_reconcilation {
                        logging::error(&format!(
                            "Could not reconcile history with snapshot: {error}"
                        ));
                    }

                    match state.replace_downloads(downloads) {
                        Ok(()) => {
                            status_message = match history_reconcilation {
                                Ok(0) => String::from("SNapshot Synchronized"),
                                Ok(count) => format!(
                                    "Snapshot synched; {count} stale history item(s) marked unavailable"
                                ),
                                Err(_) => {
                                    String::from("Snapshot synched; history reconcilation failed")
                                }
                            };
                        }
                        Err(error) => {
                            logging::error(&format!("Snapshot rejected: {error}"));
                            status_message = String::from("Snapshot rejected; see log")
                        }
                    }
                }

                MonitorUpdate::Progress(downloads) => {
                    let downloads: Vec<_> = downloads
                        .into_iter()
                        .filter(|download| !state.is_removed_gid(&download.gid))
                        .collect();

                    if let Err(error) = history.record_downloads(&downloads) {
                        logging::error(&format!("Could nto save progress to history: {error}"));
                        status_message = String::from("History update failed; see logs")
                    }

                    for download in downloads {
                        state.upsert_download(download);
                    }
                }

                MonitorUpdate::Status(message) => {
                    status_message = message;
                }

                MonitorUpdate::Heartbeat => continue,
            }

            redraw_needed = true;
        }

        if redraw_needed {
            terminal
                .draw(|frame| {
                    draw(
                        frame,
                        state,
                        &status_message,
                        managed_mode,
                        show_exit_dialog,
                        selected_exit_option,
                        add_input.as_deref(),
                        pending_removal.as_ref(),
                        history_view.as_deref(),
                        history_selection,
                    )
                })
                .map_err(|error| format!("Could not draw dashboard: {error}"))?;

            redraw_needed = false;
        }

        if event::poll(Duration::from_millis(100))
            .map_err(|error| format!("Could not poll terminal events: {error}"))?
        {
            let event =
                event::read().map_err(|error| format!("Could not read terminal event: {error}"))?;

            match event {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        if let Some(pending) = pending_removal.clone() {
                            match key.code {
                                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let removal_result: Result<(String, usize, usize), String> =
                                        if pending.delete_files {
                                            backend.remove_download_and_delete_files(&pending.gid, &pending.status).map(|report| {
                                            let message = if report.failed_files == 0 {
                                                format!("Removed download; deleted {} file(s)", report.deleted_files)
                                            } else {
                                                format!("Removed download; {} file deletion(s) failed (see logs)", report.failed_files)
                                            };

                                            (message, report.deleted_files, report.failed_files)
                                        })
                                        } else {
                                            backend
                                                .remove_download(&pending.gid, &pending.status)
                                                .map(|()| {
                                                    (
                                                        String::from(
                                                            "Removed download; files kept",
                                                        ),
                                                        0,
                                                        0,
                                                    )
                                                })
                                        };

                                    match removal_result {
                                        Ok((message, deleted_files, failed_file_deletions)) => {
                                            if let Err(error) = state.remove_download(&pending.gid)
                                            {
                                                logging::error(&format!(
                                                    "aria2 removal succeeded but local state update failed: {error}"
                                                ));
                                            }

                                            match history.mark_removed(
                                                &pending.gid,
                                                deleted_files,
                                                failed_file_deletions,
                                            ) {
                                                Ok(()) => status_message = message,
                                                Err(error) => {
                                                    logging::error(&format!(
                                                        "Download was removed but history update failed: {error}"
                                                    ));
                                                    status_message = String::from(
                                                        "Removed from aria2; history update failed",
                                                    );
                                                }
                                            }
                                        }

                                        Err(error) => {
                                            logging::error(&format!(
                                                "Download removal failed: {error}"
                                            ));
                                            status_message =
                                                String::from("Removal failed; see logs");
                                        }
                                    }

                                    pending_removal = None;
                                    redraw_needed = true;
                                }

                                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                                    pending_removal = None;
                                    status_message = String::from("Removal cancelled");
                                    redraw_needed = true;
                                }

                                _ => {}
                            }
                        } else if history_view.is_some() {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('h') => {
                                    history_view = None;
                                    redraw_needed = true;
                                }

                                KeyCode::Up | KeyCode::Char('k') => {
                                    history_selection = history_selection.saturating_sub(1);
                                    redraw_needed = true;
                                }

                                KeyCode::Down | KeyCode::Char('j') => {
                                    let count = history_view
                                        .as_ref()
                                        .map(|entries| entries.len())
                                        .unwrap_or(0);

                                    if count > 0 {
                                        history_selection = (history_selection + 1).min(count - 1);
                                    }

                                    redraw_needed = true;
                                }

                                KeyCode::Char('r') => {
                                    match history.list_recent(100) {
                                        Ok(entries) => {
                                            history_selection = history_selection
                                                .min(entries.len().saturating_sub(1));
                                            history_view = Some(entries);
                                            status_message = String::from("History refreshed");
                                        }
                                        Err(error) => {
                                            logging::error(&format!(
                                                "Could not refresh history: {error}"
                                            ));
                                            status_message =
                                                String::from("Could not refresh history; see logs");
                                        }
                                    }

                                    redraw_needed = true;
                                }

                                _ => {}
                            }
                        } else if add_input.is_some() {
                            match key.code {
                                KeyCode::Enter => {
                                    let value =
                                        add_input.as_deref().unwrap_or_default().trim().to_owned();

                                    if value.is_empty() {
                                        status_message =
                                            "Enter a URL, magnet URI or .torrent path".to_owned();
                                    } else {
                                        add_input = None;

                                        let result = if input_is_uri(&value) {
                                            backend.add_uri(&value)
                                        } else {
                                            backend.add_torrent_file(Path::new(&value))
                                        };

                                        status_message = match result {
                                            Ok(gid) => format!("Added download: {gid}"),
                                            Err(error) => {
                                                logging::error(&format!(
                                                    "Add download failed: {error}"
                                                ));
                                                String::from("Add download failed; see logs")
                                            }
                                        };
                                    }

                                    redraw_needed = true;
                                }

                                KeyCode::Esc => {
                                    add_input = None;
                                    redraw_needed = true;
                                }

                                KeyCode::Backspace => {
                                    if let Some(input) = add_input.as_mut() {
                                        input.pop();
                                    }
                                    redraw_needed = true;
                                }

                                KeyCode::Char(character)
                                    if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    if let Some(input) = add_input.as_mut() {
                                        input.push(character);
                                    }
                                    redraw_needed = true;
                                }

                                _ => {}
                            }
                        } else if show_exit_dialog {
                            match key.code {
                                KeyCode::Left | KeyCode::Up => {
                                    selected_exit_option = (selected_exit_option + 2) % 3;
                                    redraw_needed = true;
                                }

                                KeyCode::Right | KeyCode::Down => {
                                    selected_exit_option = (selected_exit_option + 1) % 3;
                                    redraw_needed = true;
                                }

                                KeyCode::Enter => match selected_exit_option {
                                    0 => return Ok(ExitChoice::LeaveRunning),
                                    1 => return Ok(ExitChoice::StopController),
                                    _ => {
                                        show_exit_dialog = false;
                                        redraw_needed = true;
                                    }
                                },

                                KeyCode::Char('l') | KeyCode::Char('L') => {
                                    return Ok(ExitChoice::LeaveRunning);
                                }

                                KeyCode::Char('s') | KeyCode::Char('S') => {
                                    return Ok(ExitChoice::StopController);
                                }

                                KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                                    show_exit_dialog = false;
                                    redraw_needed = true;
                                }

                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                                    if managed_mode {
                                        show_exit_dialog = true;
                                        redraw_needed = true;
                                    } else {
                                        return Ok(ExitChoice::LeaveRunning);
                                    }
                                }

                                KeyCode::Char('a') => {
                                    add_input = Some(String::new());
                                    redraw_needed = true;
                                }

                                KeyCode::Char('p') => {
                                    let result = match state.selected_download() {
                                        Some(download)
                                            if download.status.is_active()
                                                || matches!(&download.status, Status::Waiting) =>
                                        {
                                            backend.pause_download(&download.gid)
                                        }

                                        Some(_) => Err(String::from(
                                            "Selected download is not active or waiting",
                                        )),

                                        None => Err(String::from("No download selected")),
                                    };

                                    status_message = operation_feedback(
                                        result,
                                        "Paused selected download",
                                        "Pause selected download",
                                    );

                                    redraw_needed = true;
                                }

                                KeyCode::Char('r') => {
                                    let result = match state.selected_download() {
                                        Some(download)
                                            if matches!(&download.status, Status::Paused) =>
                                        {
                                            backend.resume_download(&download.gid)
                                        }

                                        Some(_) => {
                                            Err(String::from("Selected download is not paused"))
                                        }

                                        None => Err(String::from("No download selected")),
                                    };

                                    status_message = operation_feedback(
                                        result,
                                        "Resumed selected download",
                                        "Resume selected download",
                                    );

                                    redraw_needed = true;
                                }

                                KeyCode::Char('P') => {
                                    status_message = operation_feedback(
                                        backend.pause_all(),
                                        "Paused all downloads",
                                        "Pause all downloads",
                                    );
                                    redraw_needed = true;
                                }

                                KeyCode::Char('R') => {
                                    status_message = operation_feedback(
                                        backend.resume_all(),
                                        "Resumed all downloads",
                                        "Resume all downloads",
                                    );
                                    redraw_needed = true;
                                }

                                KeyCode::Char('x') => {
                                    if let Some(download) = state.selected_download() {
                                        pending_removal = Some(PendingRemoval {
                                            gid: download.gid.clone(),
                                            name: download.name.clone(),
                                            status: download.status,
                                            delete_files: false,
                                        });
                                    } else {
                                        status_message = String::from("No download selected")
                                    }

                                    redraw_needed = true;
                                }

                                KeyCode::Char('X') => {
                                    if !managed_mode {
                                        status_message = String::from(
                                            "Deleting files is only available in managed mode",
                                        )
                                    } else if let Some(download) = state.selected_download() {
                                        pending_removal = Some(PendingRemoval {
                                            gid: download.gid.clone(),
                                            name: download.name.clone(),
                                            status: download.status,
                                            delete_files: true,
                                        })
                                    } else {
                                        status_message = String::from("No download selected")
                                    }

                                    redraw_needed = true
                                }

                                KeyCode::Char('h') => {
                                    match history.list_recent(100) {
                                        Ok(entries) => {
                                            history_view = Some(entries);
                                            history_selection = 0;
                                        }
                                        Err(error) => {
                                            logging::error(&format!(
                                                "Could not load history: {error}"
                                            ));
                                            status_message =
                                                String::from("Counld not load history; see logs");
                                        }
                                    }

                                    redraw_needed = true;
                                }

                                KeyCode::Down | KeyCode::Char('j') => {
                                    state.select_next();
                                    redraw_needed = true;
                                }

                                KeyCode::Up | KeyCode::Char('k') => {
                                    state.select_previous();
                                    redraw_needed = true;
                                }

                                _ => {}
                            }
                        }
                    }
                }

                Event::Resize(_, _) => {
                    redraw_needed = true;
                }

                // this is for the paste feature like if the download TUI is just chillling and you
                // just randomly come and paste something into the TUI, which turns out to be a
                // download link, it autoamtically captures it and adds it
                Event::Paste(text) => {
                    if let Some(input) = add_input.as_mut() {
                        input.push_str(&text);
                        redraw_needed = true;
                    }
                }

                _ => {}
            }
        }
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
