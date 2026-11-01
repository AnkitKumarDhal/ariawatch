use super::super::state::LoopState;
use crate::{app_state::AppState, backend::DownloadBackend, history::HistoryStore, logging};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(
    key: KeyEvent,
    ui: &mut LoopState,
    state: &mut AppState,
    backend: &dyn DownloadBackend,
    history: &mut HistoryStore,
) {
    match key.code {
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
            let Some(pending) = ui.pending_removal.clone() else {
                return;
            };

            let removal_result: Result<(String, usize, usize), String> = if pending.delete_files {
                backend
                    .remove_download_and_delete_files(&pending.gid, &pending.status)
                    .map(|report| {
                        let message = if report.failed_files == 0 {
                            format!("Removed download; deleted {} file(s)", report.deleted_files)
                        } else {
                            format!(
                                "Removed download; {} file deletion(s) failed (see logs)",
                                report.failed_files
                            )
                        };

                        (message, report.deleted_files, report.failed_files)
                    })
            } else {
                backend
                    .remove_download(&pending.gid, &pending.status)
                    .map(|()| (String::from("Removed download; files kept"), 0, 0))
            };

            match removal_result {
                Ok((message, deleted_files, failed_file_deletions)) => {
                    if let Err(error) = state.remove_download(&pending.gid) {
                        logging::error(&format!(
                            "aria2 removal succeeded but local state update failed: {error}"
                        ));
                    }

                    match history.mark_removed(&pending.gid, deleted_files, failed_file_deletions) {
                        Ok(()) => ui.status_message = message,
                        Err(error) => {
                            logging::error(&format!(
                                "Download was removed but history update failed: {error}"
                            ));
                            ui.status_message =
                                String::from("Removed from aria2; history update failed");
                        }
                    }
                }

                Err(error) => {
                    logging::error(&format!("Download removal failed: {error}"));
                    ui.status_message = String::from("Removal failed; see logs");
                }
            }

            ui.pending_removal = None;
            ui.redraw_needed = true;
        }

        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            ui.pending_removal = None;
            ui.status_message = String::from("Removal cancelled");
            ui.redraw_needed = true;
        }

        _ => {}
    }
}
