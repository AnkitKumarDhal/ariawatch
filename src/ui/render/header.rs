use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect) {
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" AriaWatch ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("- aria2 download monitor"),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, area);
}
