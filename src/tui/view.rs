//! Drawing. Views for tickets arrive later; for now a header, a footer and an empty body.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::state::State;
use super::widgets;

const HINTS: [(&str, &str); 1] = [("q", "quit")];

pub fn draw(frame: &mut Frame, state: &State) {
    let area = frame.area();
    let footer_h =
        widgets::footer_height(&HINTS, area.width, &state.theme) + u16::from(state.error.is_some());
    let [header, _body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(footer_h),
    ])
    .areas(area);
    widgets::render_header(
        frame.buffer_mut(),
        header,
        &state.scope,
        &state.who,
        state.tickets.len(),
        &state.theme,
    );
    match &state.error {
        Some(e) => {
            let [err, hints] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(footer);
            frame.render_widget(Paragraph::new(Line::from(format!("error: {e}"))), err);
            widgets::render_footer(frame.buffer_mut(), hints, &HINTS, &state.theme);
        }
        None => widgets::render_footer(frame.buffer_mut(), footer, &HINTS, &state.theme),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(state: &State) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(70, 5)).unwrap();
        t.draw(|f| draw(f, state)).unwrap();
        let buf = t.backend().buffer();
        (0..5)
            .map(|y| (0..70).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    #[test]
    fn header_and_footer_are_drawn() {
        let s = State {
            who: "human/human".into(),
            ..State::default()
        };
        let rows = render(&s);
        assert!(rows[0].contains("all Projects"));
        assert!(rows[0].contains("human/human"));
        assert!(rows[0].contains("0 tickets"));
        assert!(rows[4].starts_with("[q] quit"));
        assert!(rows[1..4].iter().all(|r| r.trim().is_empty()));
    }

    #[test]
    fn error_shows_above_hints() {
        let s = State {
            error: Some("boom".into()),
            ..State::default()
        };
        let rows = render(&s);
        assert!(rows[3].starts_with("error: boom"));
        assert!(rows[4].starts_with("[q] quit"));
    }
}
