// Renders the actual UI and terminal input
mod dialogs;
mod event_loop;
mod render;
use crate::{
    app_state::AppState, backend::DownloadBackend, domain::status::Status, history::HistoryStore,
    monitor::MonitorUpdate,
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

// Restores terminal state even if the UI exits unexpectedly
struct TerminalGuard {
    active: bool,
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
