use crate::{
    app_state::AppState,
    domain::status::Status,
    ui::formatting::{format_bytes, format_duration},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Gauge, Paragraph},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let selected_download = state.selected_download();

    let details_title = selected_download
        .map(|download| format!(" Selected download: {} ", download.name))
        .unwrap_or_else(|| String::from(" Selected download "));

    let details_block = Block::default().borders(Borders::ALL).title(details_title);

    let details_inner = details_block.inner(area);
    frame.render_widget(details_block, area);

    let details_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(details_inner);

    if let Some(download) = selected_download {
        let eta = if download.status == Status::Seeding {
            String::from("-")
        } else {
            format_duration(download.estimated_remaining_seconds())
        };

        let mut details = vec![
            Line::from(format!(
                "Status: {}  |  ETA: {}",
                download.status.label(),
                eta
            )),
            Line::from(format!(
                "Downloaded: {} / {}  |  Remaining: {}",
                format_bytes(download.downloaded),
                format_bytes(download.size),
                format_bytes(download.remaining_bytes())
            )),
        ];

        if let Some(torrent) = &download.torrent {
            let upload_speed = torrent
                .upload_speed
                .map(|speed| format!("{}/s", format_bytes(speed)))
                .unwrap_or_else(|| String::from("N/A"));
            let uploaded = torrent
                .uploaded
                .map(format_bytes)
                .unwrap_or_else(|| String::from("N/A"));
            let seeders = torrent
                .seeders
                .map(|count| count.to_string())
                .unwrap_or_else(|| String::from("N/A"));
            let peers = torrent
                .peers
                .map(|count| count.to_string())
                .unwrap_or_else(|| String::from("N/A"));

            details.extend([
                Line::from(format!(
                    "Download: {}/s  |  Upload: {}",
                    format_bytes(download.speed),
                    upload_speed
                )),
                Line::from(format!("Uploaded: {}", uploaded)),
                Line::from(format!("Seeders: {}  |  Peers: {}", seeders, peers)),
            ]);
        } else {
            details.push(Line::from(format!(
                "Speed: {}/s",
                format_bytes(download.speed)
            )));
        }

        details.extend([
            Line::from(format!(
                "Destination: {}",
                download.directory.as_deref().unwrap_or("N/A")
            )),
            Line::from(format!(
                "Source URI: {}",
                download.source_url.as_deref().unwrap_or("N/A")
            )),
        ]);

        if let Some(error) = download
            .error_message
            .as_deref()
            .filter(|message| !message.trim().is_empty())
        {
            details.push(Line::from(format!("Error: {error}")));
        }

        frame.render_widget(Paragraph::new(details), details_sections[0]);

        let progress = download.percentage_downloaded();
        let ratio = progress.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0;
        let label = progress
            .map(|percent| format!("{percent:.1}%"))
            .unwrap_or_else(|| String::from("N/A"));

        let gauge = Gauge::default().ratio(ratio).label(label);
        frame.render_widget(gauge, details_sections[1]);
    } else {
        frame.render_widget(
            Paragraph::new("No downloads to inspect yet."),
            details_sections[0],
        );
    }
}
