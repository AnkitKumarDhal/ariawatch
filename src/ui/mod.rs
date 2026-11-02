// Renders the actual UI and terminal input
mod dialogs;
mod event_loop;
mod render;
use crate::{
    app_state::AppState, aria2::TorrentFile, backend::DownloadBackend, domain::status::Status,
    history::HistoryStore, monitor::MonitorUpdate,
};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    sync::{Arc, mpsc::Receiver},
};

// this is for the choice of how to exit the applciation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitChoice {
    LeaveRunning,
    StopController,
}

// this is the struct that stores the details of the download that was requested to be removed
#[derive(Debug, Clone)]
struct PendingRemoval {
    gid: String,
    name: String,
    status: Status,
    delete_files: bool,
}

// this one describes waht the worker is currently doing with the selected torrent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TorrentSelectionAction {
    Applying,
    Cancelling,
}

// stores the torrent file list and the user's current selection
pub(super) struct TorrentSelection {
    gid: String,
    files: Vec<TorrentFile>,
    cursor: usize,
    action: Option<TorrentSelectionAction>,
}

// Restores terminal state even if the UI exits unexpectedly
struct TerminalGuard {
    active: bool,
}

impl TorrentSelection {
    pub(super) fn new(gid: String, files: Vec<TorrentFile>) -> Self {
        Self {
            gid,
            files,
            cursor: 0,
            action: None,
        }
    }

    pub(super) fn gid(&self) -> &str {
        &self.gid
    }

    pub(super) fn files(&self) -> &[TorrentFile] {
        &self.files
    }

    pub(super) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(super) fn action(&self) -> Option<TorrentSelectionAction> {
        self.action
    }

    pub(super) fn set_action(&mut self, action: Option<TorrentSelectionAction>) {
        self.action = action;
    }

    pub(super) fn selected_count(&self) -> usize {
        self.files.iter().filter(|file| file.selected).count()
    }

    // returns aria2's file indices, not rust's 0-based vector positions
    pub(super) fn selected_indices(&self) -> Vec<u32> {
        self.files
            .iter()
            .filter(|file| file.selected)
            .map(|file| file.index)
            .collect()
    }

    // moves the cursor up, wrapping at the first file
    pub(super) fn move_previous(&mut self) {
        if self.files.is_empty() {
            return;
        }

        self.cursor = if self.cursor == 0 {
            self.files.len() - 1
        } else {
            self.cursor - 1
        };
    }

    // moves the cursor down, wrapping at the last file
    pub(super) fn move_next(&mut self) {
        if !self.files.is_empty() {
            self.cursor = (self.cursor + 1) % self.files.len()
        }
    }

    // toggles the seelction of the file under the cursor
    pub(super) fn toggle_current(&mut self) {
        if let Some(file) = self.files.get_mut(self.cursor) {
            file.selected = !file.selected;
        }
    }

    // selects or deselects every file
    pub(super) fn set_all(&mut self, selected: bool) {
        for file in &mut self.files {
            file.selected = selected;
        }
    }
}

impl TerminalGuard {
    fn new() -> Self {
        Self { active: true }
    }

    // this one reports cleanup errors
    fn restore(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();

        if let Err(error) = execute!(io::stdout(), crossterm::cursor::Show) {
            errors.push(format!("Coult not restore cursor: {error}"));
        }

        if let Err(error) = execute!(io::stdout(), LeaveAlternateScreen) {
            errors.push(format!("Couldd not leave alternate screen: {error}"));
        }

        if let Err(error) = disable_raw_mode() {
            errors.push(format!("Could not disable raw mode: {error}"));
        }

        if errors.is_empty() {
            self.active = false;
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

impl Drop for TerminalGuard {
    // this one makes the best-effort attmept if the normal cleanup path didn't finish successfully
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        let _ = execute!(io::stdout(), crossterm::cursor::Show);
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

// Function to ope nthe terminal dashboard nad handle keyboard input
pub fn run(
    mut state: AppState,
    updates: Receiver<MonitorUpdate>,
    managed_mode: bool,
    back: &Arc<dyn DownloadBackend>,
    history: &mut HistoryStore,
) -> Result<ExitChoice, String> {
    enable_raw_mode().map_err(|error| format!("Could not enable raw mode: {error}"))?;

    let mut stdout = io::stdout();

    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(format!("Could not enter Alternate Screen: {error}"));
    }

    let mut terminal_guard = TerminalGuard::new();

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            return Err(format!("Could not initialize terminal: {error}"));
        }
    };

    let app_result = event_loop::run(
        &mut terminal,
        &mut state,
        &updates,
        managed_mode,
        back,
        history,
    );

    let cursor_result = terminal
        .show_cursor()
        .map_err(|error| format!("Could not restore cursor: {error}"));

    let cleanup_result = terminal_guard.restore();

    let exit_choice = app_result?;
    cursor_result?;
    cleanup_result?;

    Ok(exit_choice)
}

#[cfg(test)]
mod torrent_selection_tests {
    use super::{TorrentFile, TorrentSelection};

    fn sample_files() -> Vec<TorrentFile> {
        vec![
            TorrentFile {
                index: 1,
                path: String::from("folder/one.txt"),
                length: 1024,
                completed_length: 0,
                selected: true,
            },
            TorrentFile {
                index: 2,
                path: String::from("folder/two.txt"),
                length: 2048,
                completed_length: 0,
                selected: true,
            },
            TorrentFile {
                index: 3,
                path: String::from("folder/three.txt"),
                length: 4096,
                completed_length: 0,
                selected: false,
            },
        ]
    }

    #[test]
    fn cursor_moves_and_wraps() {
        let mut selection = TorrentSelection::new(String::from("0123456789abcdef"), sample_files());

        assert_eq!(selection.cursor(), 0);

        selection.move_previous();
        assert_eq!(selection.cursor(), 2);

        selection.move_next();
        assert_eq!(selection.cursor(), 0);
    }

    #[test]
    fn toggles_current_file() {
        let mut selection = TorrentSelection::new(String::from("0123456789abcdef"), sample_files());

        selection.toggle_current();

        assert_eq!(selection.selected_count(), 1);
        assert_eq!(selection.selected_indices(), vec![2]);
    }

    #[test]
    fn selects_and_deselects_all_files() {
        let mut selection = TorrentSelection::new(String::from("0123456789abcdef"), sample_files());

        selection.set_all(false);
        assert_eq!(selection.selected_count(), 0);

        selection.set_all(true);
        assert_eq!(selection.selected_count(), 3);
    }

    #[test]
    fn returns_actual_aria2_file_indices() {
        let selection = TorrentSelection::new(String::from("0123456789abcdef"), sample_files());

        assert_eq!(selection.selected_indices(), vec![1, 2]);
    }
}
