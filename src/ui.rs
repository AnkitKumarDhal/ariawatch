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
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use std::{io, sync::mpsc::Receiver, time::Duration};

// public function to draw the TUI
pub fn draw(frame: &mut Frame<'_>, state: &AppState, status_message: &str) {
    // 3 sections for the TUI
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
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
    let table_header = Row::new(["Name", "Status", "Progress", "Speed", "Remaining"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows = state.downloads().iter().map(|download| {
        let progress = match download.percentage_downloaded() {
            Some(percent) => format!("{percent:.1}%"),
            None => String::from("N/A"),
        };

        let speed = format!("{} B/s", download.speed);
        let remaining = format!(
            "{:.2} GiB",
            download.remaining_bytes() as f64 / 1024.0 / 1024.0 / 1024.0
        );

        Row::new(vec![
            Cell::from(download.name.as_str()),
            Cell::from(download.status.label()),
            Cell::from(progress),
            Cell::from(speed),
            Cell::from(remaining),
        ])
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

    // Footer content
    let footer = Paragraph::new(format!(
        " {status_message} · {} active · q: quit",
        state.active_download_count()
    ))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, sections[2]);
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
