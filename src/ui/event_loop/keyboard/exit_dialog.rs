use super::super::state::LoopState;
use crate::ui::ExitChoice;
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(key: KeyEvent, ui: &mut LoopState) -> Option<ExitChoice> {
    match key.code {
        KeyCode::Left | KeyCode::Up => {
            ui.selected_exit_option = (ui.selected_exit_option + 2) % 3;
            ui.redraw_needed = true;
        }

        KeyCode::Right | KeyCode::Down => {
            ui.selected_exit_option = (ui.selected_exit_option + 1) % 3;
            ui.redraw_needed = true;
        }

        KeyCode::Enter => match ui.selected_exit_option {
            0 => return Some(ExitChoice::LeaveRunning),
            1 => return Some(ExitChoice::StopController),
            _ => {
                ui.show_exit_dialog = false;
                ui.redraw_needed = true;
            }
        },

        KeyCode::Char('l') | KeyCode::Char('L') => {
            return Some(ExitChoice::LeaveRunning);
        }

        KeyCode::Char('s') | KeyCode::Char('S') => {
            return Some(ExitChoice::StopController);
        }

        KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
            ui.show_exit_dialog = false;
            ui.redraw_needed = true;
        }

        _ => {}
    }

    None
}
