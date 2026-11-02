use crate::{app_state::AppState, ui::formatting::format_bytes};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let table_header = Row::new(["  Name", "Status", "Progress", "Speed", "Remaining"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let selected_download = state.selected_download();
    let selected_gid = selected_download.map(|download| download.gid.as_str());

    let selected_index = selected_gid.and_then(|gid| {
        state
            .downloads()
            .iter()
            .position(|download| download.gid.as_str() == gid)
    });

    let visible_rows = area.height.saturating_sub(3) as usize;

    let first_visible_index = selected_index
        .map(|index| index.saturating_add(1).saturating_sub(visible_rows))
        .unwrap_or(0);

    let rows = state
        .downloads()
        .iter()
        .skip(first_visible_index)
        .take(visible_rows)
        .map(|download| {
            let progress = match download.percentage_downloaded() {
                Some(percent) => format!("{percent:.1}%"),
                None => String::from("N/A"),
            };

            let speed = format!("{}/s", format_bytes(download.speed));
            let remaining = format_bytes(download.remaining_bytes());

            let is_selected = Some(download.gid.as_str()) == selected_gid;

            // this is the cursor hehe
            let name = if is_selected {
                format!("▶ {}", download.name)
            } else {
                format!("  {}", download.name)
            };

            let row = Row::new(vec![
                Cell::from(name),
                Cell::from(download.status.label()),
                Cell::from(progress),
                Cell::from(speed),
                Cell::from(remaining),
            ]);

            if is_selected {
                row.style(Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD))
            } else {
                row
            }
        });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(16),
            Constraint::Percentage(12),
            Constraint::Percentage(16),
            Constraint::Percentage(16),
        ],
    )
    .header(table_header)
    .column_spacing(1)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Downloads ({}) ", state.download_count())),
    );

    frame.render_widget(table, area);
}
