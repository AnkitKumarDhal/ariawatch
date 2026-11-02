use crossterm::event::{KeyEvent, KeyEventKind};

use crate::{
    app_state::AppState,
    backend::DownloadBackend,
    history::HistoryStore,
    logging,
    ui::{ExitChoice, event_loop::LoopState},
};
use std::sync::Arc;

mod add_input;
mod exit_dialog;
mod history;
mod normal;
mod removal;

pub(super) fn handle_key(
    key: KeyEvent,
    ui: &mut LoopState,
    state: &mut AppState,
    managed_mode: bool,
    backend: &dyn DownloadBackend,
    history: &mut HistoryStore,
) -> Option<ExitChoice> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    if ui.pending_removal.is_some() {
        removal::handle(key, ui, state, backend, history);
    } else if ui.history_view.is_some() {
        history::handle(key, ui, history);
    } else if ui.add_input.is_some() {
        add_input::handle(key, ui, backend);
    } else if ui.show_exit_dialog {
        return exit_dialog::handle(key, ui);
    } else {
        return normal::handle(key, ui, state, managed_mode, backend, history);
    }

    None
}

// just a kind of messenger that takes the result of something, and has two states - one is the
// success statement and another is the action statement,,, basically like this happened - this is
// supposed to happen
pub(super) fn operation_feedback(
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
