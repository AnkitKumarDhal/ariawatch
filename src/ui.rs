use std::io;

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

use crate::app_state::AppState;

// public function to draw the TUI
pub fn draw(frame: &mut Frame<'_>, state: &AppState) {
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
            .title(format!(" Downloads ({})", state.download_count())),
    );

    frame.render_widget(table, sections[1]);

    // Footer content
    let footer =
        Paragraph::new(" Download Overview ").block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, sections[2]);
}

// Function to ope nthe terminal dashboard nad handle keyboard input
pub fn run(state: &AppState) -> Result<(), String> {
    let _ = enable_raw_mode().map_err(|error| format!("Could not enable raw mode: {error}"));

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

    let app_result = event_loop(&mut terminal, state);

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

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &AppState,
) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| draw(frame, state))
            .map_err(|error| format!("Could not draw dashboard: {error}"))?;
        let event =
            event::read().map_err(|error| format!("Could not read keyboard event: {error}"))?;

        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press
                && matches!(
                    key.code,
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
                )
            {
                return Ok(());
            }
        }
    }
}
