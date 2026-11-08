use super::super::state::LoopState;
use crate::{
    backend::DownloadBackend,
    logging,
    ui::{
        TorrentPreparation, TorrentPreparationSource,
        event_loop::{LinkFileWorker, TorrentWorker, torrent_worker::TorrentSource},
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::{Path, PathBuf};

// checks if input is a URI or not
fn input_is_uri(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();

    [
        "http://", "https://", "ftp://", "ftps://", "sftp://", "magnet:?",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

// Magnets need the special metadata-retrieval workflow
fn input_is_magnet(input: &str) -> bool {
    input.to_ascii_lowercase().starts_with("magnet:?")
}

// removes invisible characters that can occasionaly appear when copying URLs from websites or formatted text
fn normalize_input(input: &str) -> &str {
    input.trim().trim_start_matches('\u{feff}').trim()
}

// identifies local .torrent files
fn input_is_local_torrent(input: &str) -> bool {
    !input_is_uri(input)
        && Path::new(input)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent"))
}

// identifies plain-text files containing donwload links
fn input_is_link_file(input: &str) -> bool {
    Path::new(input)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("txt")
                || extension.eq_ignore_ascii_case("links")
                || extension.eq_ignore_ascii_case("list")
        })
}

// queues torrent prep without making the keyboard handler wait for it
fn queue_torrent_preparation(
    ui: &mut LoopState,
    worker: &TorrentWorker,
    source: TorrentSource,
    preparation_source: TorrentPreparationSource,
    queued_message: &str,
) -> String {
    match worker.prepare(source) {
        Ok(()) => {
            ui.pending_torrent_preparation = Some(TorrentPreparation::new(preparation_source));
            queued_message.to_owned()
        }

        Err(error) => {
            logging::warn(&format!("Could not queue torrent preparartion: {error}"));
            error
        }
    }
}

pub(super) fn handle(
    key: KeyEvent,
    ui: &mut LoopState,
    backend: &dyn DownloadBackend,
    torrent_worker: &TorrentWorker,
    link_file_worker: &LinkFileWorker,
) {
    match key.code {
        KeyCode::Enter => {
            let value = normalize_input(ui.add_input.as_deref().unwrap_or_default()).to_owned();

            if value.is_empty() {
                ui.status_message =
                    String::from("Enter a URL, magnet URI, .torrent path, or a link-file path");
            } else {
                ui.add_input = None;

                if input_is_magnet(&value) {
                    let message = queue_torrent_preparation(
                        ui,
                        torrent_worker,
                        TorrentSource::Magnet(value),
                        TorrentPreparationSource::Magnet,
                        "Retrieving magnet metadata...",
                    );
                    ui.status_message = message;
                } else if input_is_local_torrent(&value) {
                    let message = queue_torrent_preparation(
                        ui,
                        torrent_worker,
                        TorrentSource::LocalFile(PathBuf::from(value)),
                        TorrentPreparationSource::LocalFile,
                        "Preparing local torrent...",
                    );
                    ui.status_message = message;
                } else if input_is_link_file(&value) {
                    match link_file_worker.import_file(PathBuf::from(value)) {
                        Ok(()) => {
                            ui.status_message =
                                String::from("Importing downloads from link file...")
                        }
                        Err(error) => {
                            logging::warn(&format!("Could not queue link-file import: {error}"));
                            ui.status_message = error;
                        }
                    }
                } else {
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

#[cfg(test)]
mod tests {
    use crate::ui::event_loop::keyboard::add_input::input_is_link_file;

    use super::{input_is_magnet, input_is_uri, normalize_input};

    #[test]
    fn recognizes_standard_magnet_links() {
        let input = "magnet:?xt=urn:btih:0123456789abcdef";

        assert!(input_is_magnet(input));
        assert!(input_is_uri(input));
    }

    #[test]
    fn recognizes_magnet_links_case_insensitively() {
        let input = "MAGNET:?xt=urn:btih:0123456789abcdef";

        assert!(input_is_magnet(input));
        assert!(input_is_uri(input));
    }

    #[test]
    fn removes_leading_bom_from_pasted_magnet_links() {
        let input = "\u{feff}magnet:?xt=urn:btih:0123456789abcdef";
        let normalized = normalize_input(input);

        assert_eq!(normalized, "magnet:?xt=urn:btih:0123456789abcdef");
        assert!(input_is_magnet(normalized));
        assert!(input_is_uri(normalized));
    }

    #[test]
    fn recognizes_supported_link_file_extensions() {
        assert!(input_is_link_file("downloads.txt"));
        assert!(input_is_link_file("downloads.links"));
        assert!(input_is_link_file("downloads.list"));
        assert!(input_is_link_file("downloads.TXT"));
    }

    #[test]
    fn does_not_recognize_unrelated_file_extensions() {
        assert!(!input_is_link_file("archive.zip"));
        assert!(!input_is_link_file("torrent.torrent"));
        assert!(!input_is_link_file("downloads.md"));
    }
}
