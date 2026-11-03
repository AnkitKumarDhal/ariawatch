use crate::{app_state::AppState, ui::event_loop::LoopState};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(key: KeyEvent, ui: &mut LoopState, state: &AppState) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('f') => {
            ui.show_file_list = false;
            ui.redraw_needed = true;
        }

        KeyCode::Up | KeyCode::Char('k') => {
            ui.file_list_selection = ui.file_list_selection.saturating_sub(1);
            ui.redraw_needed = true;
        }

        KeyCode::Down | KeyCode::Char('j') => {
            let file_count = state
                .selected_download()
                .map(|download| download.files.len())
                .unwrap_or(0);

            if file_count > 0 {
                ui.file_list_selection = (ui.file_list_selection + 1).min(file_count - 1);
            }

            ui.redraw_needed = true;
        }

        _ => {}
    }
}
