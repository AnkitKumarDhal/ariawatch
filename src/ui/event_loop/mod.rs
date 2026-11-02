use super::{
    ExitChoice,
    render::{DrawContext, draw},
};
use crate::{
    app_state::AppState,
    backend::DownloadBackend,
    history::HistoryStore,
    logging,
    monitor::MonitorUpdate,
    ui::event_loop::torrent_worker::{TorrentWorker, TorrentWorkerEvent},
};
use crossterm::event::{self, Event};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    sync::{Arc, mpsc::Receiver},
    time::Duration,
};
mod keyboard;
mod state;
mod torrent_worker;
use state::LoopState;

// Handles keyboard input
pub(super) fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    updates: &Receiver<MonitorUpdate>,
    managed_mode: bool,
    backend: &Arc<dyn DownloadBackend>,
    history: &mut HistoryStore,
) -> Result<ExitChoice, String> {
    let mut ui = LoopState::new();
    let torrent_worker = TorrentWorker::start(Arc::clone(backend))?;

    loop {
        while let Ok(worker_event) = torrent_worker.try_recv() {
            match worker_event {
                TorrentWorkerEvent::MetadataStarted { metadata_gid } => {
                    let _ = state.remove_download(&metadata_gid);
                    ui.status_message = String::from("Retrieving magnet metadata...");
                }

                TorrentWorkerEvent::Prepared { gid, files } => {
                    ui.pending_torrent_selection = Some((gid, files));
                    if let Some((pending_gid, pending_files)) =
                        ui.pending_torrent_selection.as_ref()
                    {
                        ui.status_message = format!(
                            "Torrent metadata ready: {} file(s) ({pending_gid})",
                            pending_files.len()
                        );
                    }
                }

                TorrentWorkerEvent::Failed(error) => {
                    logging::error(&format!("Torrent preparation failed: {error}"));
                    ui.status_message = String::from("Torrent preparation failed; see logs");
                }
            }

            ui.redraw_needed = true;
        }

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
                            ui.status_message = match history_reconcilation {
                                Ok(0) => String::from("Snapshot Synchronized"),
                                Ok(count) => format!(
                                    "Snapshot synched; {count} stale history item(s) marked unavailable"
                                ),
                                Err(_) => {
                                    String::from("Snapshot synched; history reconcilation failed")
                                }
                            }
                        }
                        Err(error) => {
                            logging::error(&format!("Snapshot rejected: {error}"));
                            ui.status_message = String::from("Snapshot rejected; see logs");
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
                        ui.status_message = String::from("History update failed; see logs");
                    }

                    for download in downloads {
                        state.upsert_download(download);
                    }
                }

                MonitorUpdate::Status(message) => {
                    ui.status_message = message;
                }

                MonitorUpdate::Heartbeat => continue,
            }

            ui.redraw_needed = true;
        }

        if ui.redraw_needed {
            terminal
                .draw(|frame| {
                    draw(
                        frame,
                        DrawContext {
                            state,
                            status_message: &ui.status_message,
                            managed_mode,
                            show_exit_dialog: ui.show_exit_dialog,
                            selected_exit_option: ui.selected_exit_option,
                            add_input: ui.add_input.as_deref(),
                            pending_removal: ui.pending_removal.as_ref(),
                            history_entries: ui.history_view.as_deref(),
                            history_selection: ui.history_selection,
                        },
                    )
                })
                .map_err(|error| format!("Could not draw dashboard: {error}"))?;

            ui.redraw_needed = false;
        }

        if event::poll(Duration::from_millis(100))
            .map_err(|error| format!("Could not poll terminal events: {error}"))?
        {
            let event =
                event::read().map_err(|error| format!("COuld not read terminal event: {error}"))?;

            match event {
                Event::Key(key) => {
                    if let Some(exit_choice) = keyboard::handle_key(
                        key,
                        &mut ui,
                        state,
                        managed_mode,
                        backend.as_ref(),
                        history,
                        &torrent_worker,
                    ) {
                        return Ok(exit_choice);
                    }
                }

                Event::Resize(_, _) => {
                    ui.redraw_needed = true;
                }

                Event::Paste(text) => {
                    if let Some(input) = ui.add_input.as_mut() {
                        input.push_str(&text);
                        ui.redraw_needed = true;
                    }
                }

                _ => {}
            }
        }
    }
}
