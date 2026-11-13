//! Drawing. Views for tickets arrive later; for now a header, a footer and an empty body.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::state::State;

pub fn draw(frame: &mut Frame, state: &State) {
    let [header, _body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let title = format!("scree  {}  {} tickets", state.who, state.tickets.len());
    frame.render_widget(
        Paragraph::new(Line::from(title)).style(Style::new().add_modifier(Modifier::BOLD)),
        header,
    );
    let hint = match &state.error {
        Some(e) => format!("error: {e}  |  q quit"),
        None => "q quit".to_string(),
    };
    frame.render_widget(Paragraph::new(Line::from(hint)), footer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(state: &State) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(40, 5)).unwrap();
        t.draw(|f| draw(f, state)).unwrap();
        let buf = t.backend().buffer();
        (0..5)
            .map(|y| (0..40).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    #[test]
    fn header_and_footer_are_drawn() {
        let s = State {
            who: "human/human".into(),
            ..State::default()
        };
        let rows = render(&s);
        assert!(rows[0].starts_with("scree  human/human  0 tickets"));
        assert!(rows[4].starts_with("q quit"));
        assert!(rows[1..4].iter().all(|r| r.trim().is_empty()));
    }
}
