use crate::{history::HistoryEntry, ui::PendingRemoval};

pub(super) struct LoopState {
    pub(super) status_message: String,
    pub(super) redraw_needed: bool,
    pub(super) show_exit_dialog: bool,
    pub(super) selected_exit_option: usize,
    pub(super) add_input: Option<String>,
    pub(super) pending_removal: Option<PendingRemoval>,
    pub(super) history_view: Option<Vec<HistoryEntry>>,
    pub(super) history_selection: usize,
}

impl LoopState {
    pub(super) fn new() -> Self {
        Self {
            status_message: String::from("Starting live monitor"),
            redraw_needed: true,
            show_exit_dialog: false,
            selected_exit_option: 0,
            add_input: None,
            pending_removal: None,
            history_view: None,
            history_selection: 0,
        }
    }
}
