use super::super::state::LoopState;
use crate::{history::HistoryStore, logging};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(key: KeyEvent, ui: &mut LoopState, history: &mut HistoryStore) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('h') => {
            ui.history_view = None;
            ui.redraw_needed = true;
        }

        KeyCode::Up | KeyCode::Char('k') => {
            ui.history_selection = ui.history_selection.saturating_sub(1);
            ui.redraw_needed = true;
        }

        KeyCode::Down | KeyCode::Char('j') => {
            let count = ui
                .history_view
                .as_ref()
                .map(|entries| entries.len())
                .unwrap_or(0);

            if count > 0 {
                ui.history_selection = (ui.history_selection + 1).min(count - 1);
            }

            ui.redraw_needed = true;
        }

        KeyCode::Char('r') => {
            match history.list_recent(100) {
                Ok(entries) => {
                    ui.history_selection =
                        ui.history_selection.min(entries.len().saturating_sub(1));
                    ui.history_view = Some(entries);
                    ui.status_message = String::from("History refreshed");
                }
                Err(error) => {
                    logging::error(&format!("Could not refresh history: {error}"));
                    ui.status_message = String::from("Could not refresh history; see logs");
                }
            }

            ui.redraw_needed = true;
        }

        _ => {}
    }
}
