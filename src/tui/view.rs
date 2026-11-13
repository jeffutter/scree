//! Drawing: header, one bordered column per status, error line, footer hints.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::state::{STATUSES, State};
use super::theme::Theme;
use super::widgets;
use crate::model::Ticket;

const HINTS: [(&str, &str); 8] = [
    ("h/l", "column"),
    ("j/k", "row"),
    ("PgUp/PgDn", "page"),
    ("g/G", "first/last"),
    ("c", "hide closed"),
    ("a", "archived"),
    ("s", "scope"),
    ("q", "quit"),
];

/// Trim `title` to at most `max` terminal cells, ending in an ellipsis.
fn truncate(title: &str, max: usize, theme: &Theme) -> String {
    if Span::raw(title).width() <= max {
        return title.to_string();
    }
    let ell = if theme.unicode { "\u{2026}" } else { "~" };
    let mut out = String::new();
    for c in title.chars() {
        let next = format!("{out}{c}{ell}");
        if Span::raw(next).width() > max {
            break;
        }
        out.push(c);
    }
    out.push_str(ell);
    out
}

/// "icon id title", with the title trimmed first so markers are never clipped.
fn row_line(t: &Ticket, width: usize, theme: &Theme) -> Line<'static> {
    let (icon, style) = theme.status_style(&t.status, t.blocked, t.available);
    let mut marks = String::new();
    if t.for_who == "human" {
        marks.push_str(" [human]");
    }
    if t.blocked {
        marks.push_str(" [blocked]");
    }
    if t.archived_at.is_some() {
        marks.push_str(" [archived]");
    }
    let prefix = format!("{icon} {} ", t.id);
    let room =
        width.saturating_sub(Span::raw(prefix.clone()).width() + Span::raw(marks.clone()).width());
    let title = truncate(&t.title, room, theme);
    Line::from(vec![
        Span::styled(prefix, style),
        Span::raw(title),
        Span::raw(marks),
    ])
}

fn draw_board(frame: &mut Frame, area: Rect, state: &State) {
    let cols = state.visible_columns();
    let areas =
        Layout::horizontal(vec![Constraint::Ratio(1, cols.len() as u32); cols.len()]).split(area);
    for (&c, &pane) in cols.iter().zip(areas.iter()) {
        let tickets = state.column(c);
        let active = c == state.col;
        let block = widgets::bordered_pane(
            &format!("{} ({})", STATUSES[c], tickets.len()),
            active,
            &state.theme,
        );
        let width = usize::from(pane.width.saturating_sub(2));
        let items: Vec<ListItem> = tickets
            .iter()
            .map(|t| ListItem::new(row_line(t, width, &state.theme)))
            .collect();
        let mut list = List::new(items).block(block);
        if active {
            list = list.highlight_style(state.theme.selected_style());
        } else {
            list = list.highlight_style(Style::new());
        }
        let mut ls = ListState::default();
        if !tickets.is_empty() {
            ls.select(Some(state.rows[c]));
        }
        frame.render_stateful_widget(list, pane, &mut ls);
    }
}

pub fn draw(frame: &mut Frame, state: &State) {
    let area = frame.area();
    let footer_h =
        widgets::footer_height(&HINTS, area.width, &state.theme) + u16::from(state.error.is_some());
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(footer_h),
    ])
    .areas(area);
    widgets::render_header(
        frame.buffer_mut(),
        header,
        &state.display_scope(),
        &state.who,
        state.visible_count(),
        &state.theme,
    );
    draw_board(frame, body, state);
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
    use crate::tui::state::Event;
    use crate::tui::theme::Theme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(state: &State, w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, state)).unwrap();
        let buf = t.backend().buffer();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn ticket(id: &str, status: &str, title: &str, who: &str, blocked: bool) -> Ticket {
        serde_json::from_value(serde_json::json!({
            "archived_at": null, "available": false, "blocked": blocked, "blocked_by": [],
            "claimed_by": null, "created_at": "", "done_at": null, "for": who,
            "id": id, "parent": null, "status": status, "subtasks": [],
            "title": title, "updated_at": "", "worktree": null
        }))
        .unwrap()
    }

    fn sample_state() -> State {
        let mut s = State {
            who: "agent/jeff".into(),
            theme: Theme::new(false, false),
            ..State::default()
        };
        let tickets = vec![
            ticket("SCR-9", "draft", "Sketch the detail view", "agent", false),
            ticket(
                "SCR-8",
                "ready",
                "Review the release workflow wording",
                "human",
                false,
            ),
            ticket("SCR-7", "ready", "Wire the search box", "agent", true),
            ticket("SCR-6", "ready", "Add keyboard help", "agent", false),
            ticket("SCR-5", "active", "Board view", "agent", false),
            ticket("SCR-3", "done", "Theme and header", "agent", false),
            ticket("SCR-2", "done", "TUI scaffold", "agent", false),
            ticket("SCR-1", "abandoned", "Typed tickets", "agent", false),
        ];
        crate::tui::state::update(&mut s, Event::Loaded(Ok(tickets)));
        s
    }

    #[test]
    fn board_sample() {
        let mut s = sample_state();
        crate::tui::state::update(
            &mut s,
            Event::Key(ratatui::crossterm::event::KeyEvent::from(
                ratatui::crossterm::event::KeyCode::Right,
            )),
        );
        insta::assert_snapshot!(render(&s, 110, 14).join("\n"));
    }

    #[test]
    fn board_hide_closed() {
        let mut s = sample_state();
        s.hide_closed = true;
        insta::assert_snapshot!(render(&s, 90, 12).join("\n"));
    }

    #[test]
    fn empty_board_has_five_columns() {
        let s = State {
            who: "human/human".into(),
            ..State::default()
        };
        let rows = render(&s, 200, 8);
        assert!(rows[0].contains("all Projects"));
        assert!(rows[0].contains("human/human"));
        assert!(rows[0].contains("0 tickets"));
        for st in STATUSES {
            assert!(rows[1].contains(&format!("{st} (0)")), "{st}");
        }
        assert!(rows[7].starts_with("[h/l] column"));
    }

    #[test]
    fn error_shows_above_hints() {
        let s = State {
            error: Some("boom".into()),
            ..State::default()
        };
        let rows = render(&s, 200, 8);
        assert!(rows[6].starts_with("error: boom"));
        assert!(rows[7].starts_with("[h/l] column"));
    }
}
