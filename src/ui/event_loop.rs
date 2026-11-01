use super::{
    ExitChoice, PendingRemoval,
    render::{DrawContext, draw},
};
use crate::{
    app_state::AppState,
    backend::DownloadBackend,
    domain::status::Status,
    history::{HistoryEntry, HistoryStore},
    logging,
    monitor::MonitorUpdate,
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, path::Path, sync::mpsc::Receiver, time::Duration};

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

// Handles keyboard input
pub(super) fn run(
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
                        DrawContext {
                            state,
                            status_message: &status_message,
                            managed_mode,
                            show_exit_dialog,
                            selected_exit_option,
                            add_input: add_input.as_deref(),
                            pending_removal: pending_removal.as_ref(),
                            history_entries: history_view.as_deref(),
                            history_selection,
                        },
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
