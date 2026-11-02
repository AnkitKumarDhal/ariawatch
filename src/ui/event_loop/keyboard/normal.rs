use super::{super::state::LoopState, operation_feedback};
use crate::{
    app_state::AppState,
    backend::DownloadBackend,
    domain::status::Status,
    history::HistoryStore,
    logging,
    ui::{ExitChoice, PendingRemoval},
};
use crossterm::event::{KeyCode, KeyEvent};
use std::sync::Arc;

pub(super) fn handle(
    key: KeyEvent,
    ui: &mut LoopState,
    state: &mut AppState,
    managed_mode: bool,
    backend: &Arc<dyn DownloadBackend>,
    history: &mut HistoryStore,
) -> Option<ExitChoice> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
            if managed_mode {
                ui.show_exit_dialog = true;
                ui.redraw_needed = true;
            } else {
                return Some(ExitChoice::LeaveRunning);
            }
        }

        KeyCode::Char('a') => {
            ui.add_input = Some(String::new());
            ui.redraw_needed = true;
        }

        KeyCode::Char('p') => {
            let result = match state.selected_download() {
                Some(download)
                    if download.status.is_active()
                        || matches!(&download.status, Status::Waiting) =>
                {
                    backend.pause_download(&download.gid)
                }

                Some(_) => Err(String::from("Selected download is not active or waiting")),

                None => Err(String::from("No download selected")),
            };

            ui.status_message = operation_feedback(
                result,
                "Paused selected download",
                "Pause selected download",
            );

            ui.redraw_needed = true;
        }

        KeyCode::Char('r') => {
            let result = match state.selected_download() {
                Some(download) if matches!(&download.status, Status::Paused) => {
                    backend.resume_download(&download.gid)
                }

                Some(_) => Err(String::from("Selected download is not paused")),

                None => Err(String::from("No download selected")),
            };

            ui.status_message = operation_feedback(
                result,
                "Resumed selected download",
                "Resume selected download",
            );

            ui.redraw_needed = true;
        }

        KeyCode::Char('P') => {
            ui.status_message = operation_feedback(
                backend.pause_all(),
                "Paused all downloads",
                "Pause all downloads",
            );

            ui.redraw_needed = true;
        }

        KeyCode::Char('R') => {
            ui.status_message = operation_feedback(
                backend.resume_all(),
                "Resumed all downloads",
                "Resume all downloads",
            );

            ui.redraw_needed = true;
        }

        KeyCode::Char('x') | KeyCode::Char('X') => {
            let delete_files = key.code == KeyCode::Char('X');

            if delete_files && !managed_mode {
                ui.status_message =
                    String::from("Deleting files is only available in managed mode");
            } else if let Some(download) = state.selected_download() {
                ui.pending_removal = Some(PendingRemoval {
                    gid: download.gid.clone(),
                    name: download.name.clone(),
                    status: download.status,
                    delete_files,
                });
            } else {
                ui.status_message = String::from("No download selected");
            }

            ui.redraw_needed = true;
        }

        KeyCode::Char('h') => {
            match history.list_recent(100) {
                Ok(entries) => {
                    ui.history_view = Some(entries);
                    ui.history_selection = 0;
                }
                Err(error) => {
                    logging::error(&format!("Could not load history: {error}"));
                    ui.status_message = String::from("Could not load history; see logs");
                }
            }

            ui.redraw_needed = true;
        }

        KeyCode::Down | KeyCode::Char('j') => {
            state.select_next();
            ui.redraw_needed = true;
        }

        KeyCode::Up | KeyCode::Char('k') => {
            state.select_previous();
            ui.redraw_needed = true;
        }

        _ => {}
    }

    None
}
