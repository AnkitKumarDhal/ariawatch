use super::super::state::LoopState;
use crate::{backend::DownloadBackend, logging};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;

fn input_is_uri(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();

    [
        "http://", "https://", "ftp://", "ftps://", "sftp://", "magnet:?",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

pub(super) fn handle(key: KeyEvent, ui: &mut LoopState, backend: &dyn DownloadBackend) {
    match key.code {
        KeyCode::Enter => {
            let value = ui
                .add_input
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_owned();

            if value.is_empty() {
                ui.status_message = String::from("Enter a URL, magnet URI or .torrent path");
            } else {
                ui.add_input = None;

                let result = if input_is_uri(&value) {
                    backend.add_uri(&value)
                } else {
                    backend.add_torrent_file(Path::new(&value))
                };

                ui.status_message = match result {
                    Ok(gid) => format!("Added download: {gid}"),
                    Err(error) => {
                        logging::error(&format!("Add download failed: {error}"));
                        String::from("Add download failed; see logs")
                    }
                };
            }

            ui.redraw_needed = true;
        }

        KeyCode::Esc => {
            ui.add_input = None;
            ui.redraw_needed = true;
        }

        KeyCode::Backspace => {
            if let Some(input) = ui.add_input.as_mut() {
                input.pop();
            }

            ui.redraw_needed = true;
        }

        KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(input) = ui.add_input.as_mut() {
                input.push(character);
            }

            ui.redraw_needed = true;
        }

        _ => {}
    }
}
