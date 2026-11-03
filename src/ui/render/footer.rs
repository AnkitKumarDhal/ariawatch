use crate::app_state::AppState;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Paragraph},
};

pub(super) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    status_message: &str,
    managed_mode: bool,
) {
    let exit_hint = if managed_mode {
        "a:add h:history f:files p:pause r:resume P/R:all x:remove X:delete+remove q: exit options"
    } else {
        "a:add h:history f:files p:pause r:resume P/R:all x:remove X:managed only q: quit"
    };

    let footer = Paragraph::new(format!(
        " {status_message} · {} active · {exit_hint}",
        state.active_download_count()
    ))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, area);
}
