use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    logging,
    ui::{
        TorrentSelectionAction,
        event_loop::{LoopState, TorrentWorker},
    },
};

pub(super) fn handle(key: KeyEvent, ui: &mut LoopState, worker: &TorrentWorker) {
    let Some(selection) = ui.pending_torrent_selection.as_mut() else {
        return;
    };

    if selection.action().is_some() {
        return;
    }

    let status_message = match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            selection.move_previous();
            None
        }

        KeyCode::Down | KeyCode::Char('j') => {
            selection.move_next();
            None
        }

        KeyCode::Char(' ') => {
            selection.toggle_current();
            None
        }

        KeyCode::Char('a') | KeyCode::Char('A') => {
            selection.set_all(true);
            None
        }

        KeyCode::Char('n') | KeyCode::Char('N') => {
            selection.set_all(false);
            None
        }

        KeyCode::Enter => {
            let indices = selection.selected_indices();

            if indices.is_empty() {
                Some(String::from("Select at least one file"))
            } else {
                match worker.apply_selection(selection.gid().to_owned(), indices) {
                    Ok(()) => {
                        selection.set_action(Some(TorrentSelectionAction::Applying));
                        Some(String::from("Applying selection and starting download..."))
                    }
                    Err(error) => {
                        logging::warn(&format!("Could not apply torrent selection: {error}"));
                        Some(error)
                    }
                }
            }
        }

        KeyCode::Esc => match worker.cancel_selection(selection.gid().to_owned()) {
            Ok(()) => {
                selection.set_action(Some(TorrentSelectionAction::Cancelling));
                Some(String::from("Cancelling torrent..."))
            }
            Err(error) => {
                logging::warn(&format!("Could not queue torrent cancellation: {error}"));
                Some(error)
            }
        },

        _ => None,
    };

    if let Some(message) = status_message {
        ui.status_message = message;
    }

    ui.redraw_needed = true;
}
