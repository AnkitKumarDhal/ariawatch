mod download_table;
mod footer;
mod header;
mod more_info;
use super::{
    PendingRemoval,
    dialogs::{
        draw_add_dialog, draw_exit_dialog, draw_history_dialog, draw_remove_dialog,
        draw_torrent_prep_dialog, draw_torrent_selection_dialog,
    },
};
use crate::{
    app_state::AppState,
    history::HistoryEntry,
    ui::{TorrentPreparation, TorrentSelection, dialogs::draw_download_files_dialog},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

pub(super) struct DrawContext<'a> {
    pub(super) state: &'a AppState,
    pub(super) status_message: &'a str,
    pub(super) managed_mode: bool,
    pub(super) show_exit_dialog: bool,
    pub(super) selected_exit_option: usize,
    pub(super) add_input: Option<&'a str>,
    pub(super) pending_removal: Option<&'a PendingRemoval>,
    pub(super) history_entries: Option<&'a [HistoryEntry]>,
    pub(super) history_selection: usize,
    pub(super) torrent_selection: Option<&'a TorrentSelection>,
    pub(super) torrent_preparation: Option<&'a TorrentPreparation>,
    pub(super) show_file_list: bool,
    pub(super) file_list_selection: usize,
}

// public function to draw the TUI
pub(super) fn draw(frame: &mut Frame<'_>, context: DrawContext<'_>) {
    let DrawContext {
        state,
        status_message,
        managed_mode,
        show_exit_dialog,
        selected_exit_option,
        add_input,
        pending_removal,
        history_entries,
        history_selection,
        torrent_selection,
        torrent_preparation,
        show_file_list,
        file_list_selection,
    } = context;

    // 4 sections for the TUI
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(9),
            Constraint::Length(3),
        ])
        .split(frame.area());

    header::draw(frame, sections[0]); // Header content
    download_table::draw(frame, sections[1], state); // Download table content

    // now comes the "more info" box on the download
    // the reason i call it the "more info" box is because i have no clue what its actually called :3
    more_info::draw(frame, sections[2], state);
    footer::draw(frame, sections[3], state, status_message, managed_mode); // Footer content

    if show_exit_dialog {
        draw_exit_dialog(frame, selected_exit_option);
    }

    if let Some(input) = add_input {
        draw_add_dialog(frame, input);
    }

    if let Some(pending) = pending_removal {
        draw_remove_dialog(frame, Some(&pending.name), pending.delete_files);
    }

    if let Some(entries) = history_entries {
        draw_history_dialog(frame, entries, history_selection);
    }

    if let Some(selection) = torrent_selection {
        draw_torrent_selection_dialog(frame, selection);
    }

    if let Some(preparation) = torrent_preparation {
        draw_torrent_prep_dialog(frame, preparation);
    }

    if show_file_list && let Some(download) = state.selected_download() {
        draw_download_files_dialog(frame, download, file_list_selection);
    }
}
