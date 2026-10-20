// Renders the actual UI and terminal input
use crate::{app_state::AppState, logging, monitor::MonitorUpdate};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{
        Constraint::{self},
        Direction, Layout,
    },
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table},
};
use std::{io, sync::mpsc::Receiver, time::Duration};

// just formats the size by like B, KiB, MiB, and such
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

// now this one does the same but for time rmeaining (i wonder how i can reuse this and the one above)
fn format_duration(seconds: Option<u64>) -> String {
    let Some(seconds) = seconds else {
        return String::from("N/A");
    };

    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remaining_seconds = seconds % 60;

    if days > 0 {
        format!("{days}D {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remaining_seconds}s")
    } else {
        format!("{remaining_seconds}s")
    }
}

// public function to draw the TUI
pub fn draw(frame: &mut Frame<'_>, state: &AppState, status_message: &str) {
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

    // Header content
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" AriaWatch ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("- aria2 download monitor"),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, sections[0]);

    // Download table content
    let table_header = Row::new(["  Name", "Status", "Progress", "Speed", "Remaining"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let selected_download = state.selected_download();
    let selected_gid = selected_download.map(|download| download.gid.as_str());

    let rows = state.downloads().iter().map(|download| {
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

    frame.render_widget(table, sections[1]);

    // now comes the "more info" box on the download
    let details_title = selected_download
        .map(|download| format!(" Selected download: {} ", download.name))
        .unwrap_or_else(|| String::from(" Selected download "));

    let details_block = Block::default().borders(Borders::ALL).title(details_title);

    let details_inner = details_block.inner(sections[2]);
    frame.render_widget(details_block, sections[2]);

    let details_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(details_inner);

    if let Some(download) = selected_download {
        let mut details = vec![
            Line::from(format!(
                "Status: {}  |  ETA: {}",
                download.status.label(),
                format_duration(download.estimated_remaining_seconds())
            )),
            Line::from(format!(
                "Downloaded: {} / {}  |  Remaining: {}",
                format_bytes(download.downloaded),
                format_bytes(download.size),
                format_bytes(download.remaining_bytes())
            )),
            Line::from(format!("Speed: {}/s", format_bytes(download.speed))),
            Line::from(format!(
                "Destination: {}",
                download.directory.as_deref().unwrap_or("N/A")
            )),
            Line::from(format!(
                "Source URI: {}",
                download.source_url.as_deref().unwrap_or("N/A")
            )),
        ];

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

    // Footer content
    let footer = Paragraph::new(format!(
        " {status_message} · {} active · q: quit",
        state.active_download_count()
    ))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, sections[3]);
}

// Function to ope nthe terminal dashboard nad handle keyboard input
pub fn run(mut state: AppState, updates: Receiver<MonitorUpdate>) -> Result<(), String> {
    enable_raw_mode().map_err(|error| format!("Could not enable raw mode: {error}"))?;

    let mut stdout = io::stdout();

    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(format!("Could not enter Alternate Screen: {error}"));
    }

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);

            return Err(format!("Could not initialize terminal: {error}"));
        }
    };

    let app_result = event_loop(&mut terminal, &mut state, &updates);

    let cursor_result = terminal
        .show_cursor()
        .map_err(|error| format!("Could not restore cursor: {error}"));

    let raw_mode_result =
        disable_raw_mode().map_err(|error| format!("Could not disable raw mode: {error}"));

    let screen_result = execute!(io::stdout(), LeaveAlternateScreen)
        .map_err(|error| format!("Could not leave alternate screen: {error}"));

    app_result?;
    cursor_result?;
    raw_mode_result?;
    screen_result?;

    Ok(())
}

// Handles keyboard input
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    updates: &Receiver<MonitorUpdate>,
) -> Result<(), String> {
    let mut status_message = "Starting live monitor".to_owned();
    let mut redraw_needed = true;
    loop {
        for update in updates.try_iter() {
            match update {
                MonitorUpdate::Snapshot(downloads) => match state.replace_downloads(downloads) {
                    Ok(()) => {
                        status_message = "Snapshot Synchronized".to_owned();
                    }
                    Err(error) => {
                        logging::error(&format!("Snapshot rejected: {error}"));
                        status_message = "Snapshot rejected; see log".to_owned();
                    }
                },

                MonitorUpdate::Progress(downloads) => {
                    for download in downloads {
                        state.upsert_download(download);
                    }
                }

                MonitorUpdate::Status(message) => {
                    status_message = message;
                }

                MonitorUpdate::Heartbeat => continue,
            }

            redraw_needed = true;
        }

        if redraw_needed {
            terminal
                .draw(|frame| draw(frame, state, &status_message))
                .map_err(|error| format!("Could not draw dashboard: {error}"))?;

            redraw_needed = false;
        }

        if event::poll(Duration::from_millis(100))
            .map_err(|error| format!("Could not poll terminal events: {error}"))?
        {
            let event =
                event::read().map_err(|error| format!("Could not read terminal event: {error}"))?;

            match event {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press
                        && matches!(
                            key.code,
                            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                        )
                    {
                        return Ok(());
                    }
                }

                Event::Resize(_, _) => {
                    redraw_needed = true;
                }

                _ => {}
            }
        }
    }
}
