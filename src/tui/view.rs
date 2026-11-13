//! Drawing: header, one bordered column per status, error line, footer hints.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph};

use super::state::{Detail, ListEntry, STATUSES, State, View, allowed};
use super::theme::Theme;
use super::widgets;
use crate::model::{Claimer, Ticket};

const HINTS: [(&str, &str); 10] = [
    ("Enter", "open"),
    ("Tab", "board/list"),
    ("h/l", "column"),
    ("j/k", "row"),
    ("PgUp/PgDn", "page"),
    ("g/G", "first/last"),
    ("c", "hide closed"),
    ("a", "archived"),
    ("s", "scope"),
    ("q", "quit"),
];

const HINTS_DETAIL: [(&str, &str); 4] = [
    ("Esc/q", "back"),
    ("j/k", "scroll"),
    ("PgUp/PgDn", "page"),
    ("g/G", "top/bottom"),
];

/// Greedy word wrap of `text` to `width` cells; every output line starts with
/// `indent`. Newlines in `text` start new lines; words wider than the room are
/// split. Always returns at least one line.
fn wrap(text: &str, indent: &str, width: usize) -> Vec<String> {
    let cell = |c: char| Span::raw(c.to_string()).width();
    let room = width
        .saturating_sub(indent.chars().map(cell).sum::<usize>())
        .max(1);
    let mut out = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for word in para.split_whitespace() {
            let w: usize = word.chars().map(cell).sum();
            if used > 0 && used + 1 + w <= room {
                line.push(' ');
                line.push_str(word);
                used += 1 + w;
                continue;
            }
            if used > 0 {
                out.push(format!("{indent}{line}"));
                line.clear();
                used = 0;
            }
            for c in word.chars() {
                if used + cell(c) > room {
                    out.push(format!("{indent}{line}"));
                    line.clear();
                    used = 0;
                }
                line.push(c);
                used += cell(c);
            }
        }
        out.push(format!("{indent}{line}"));
    }
    out
}

fn who(c: &Claimer) -> String {
    match &c.session {
        Some(s) => format!("{} ({s})", c.author),
        None => c.author.clone(),
    }
}

/// Every field of the ticket as plain wrapped lines, in reading order.
fn detail_lines(d: &Detail, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let t = &d.ticket.ticket;
    let mut out: Vec<Line<'static>> = vec![];
    let plain = |out: &mut Vec<Line<'static>>, text: &str, indent: &str| {
        out.extend(wrap(text, indent, width).into_iter().map(Line::from));
    };
    let heading = |out: &mut Vec<Line<'static>>, text: &str| {
        out.push(Line::from(""));
        out.push(Line::from(Span::styled(
            text.to_string(),
            theme.key_style(),
        )));
    };
    let item = |out: &mut Vec<Line<'static>>, status: &str, id: &str, title: &str| {
        let (icon, style) = theme.status_style(status, false, false);
        let prefix = format!("{icon} {id} [{status}] ");
        let mut it = wrap(title, "    ", width.saturating_sub(prefix.len())).into_iter();
        let first = it.next().unwrap_or_default();
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(prefix, style),
            Span::raw(first.trim_start().to_string()),
        ]));
        out.extend(it.map(Line::from));
    };

    plain(&mut out, &t.title, "");
    let (icon, style) = theme.status_style(&t.status, t.blocked, t.available);
    out.push(Line::from(vec![
        Span::styled(format!("{icon} {}", t.id), style),
        Span::raw(format!("  {}", t.status)),
    ]));
    let mut field = |name: &str, value: String| {
        plain(&mut out, &format!("{name}: {value}"), "");
    };
    field("for", t.for_who.clone());
    field(
        "claimed by",
        t.claimed_by.as_ref().map_or("none".into(), who),
    );
    field(
        "worktree",
        t.worktree.clone().unwrap_or_else(|| "none".into()),
    );
    field("parent", t.parent.clone().unwrap_or_else(|| "none".into()));
    field("created", t.created_at.clone());
    field("updated", t.updated_at.clone());
    field("done", t.done_at.clone().unwrap_or_else(|| "none".into()));
    if let Some(a) = &t.archived_at {
        field("archived", a.clone());
    }

    heading(&mut out, "Blocked by");
    if t.blocked_by.is_empty() {
        plain(&mut out, "none", "  ");
    }
    for b in &t.blocked_by {
        item(&mut out, &b.status, &b.id, &b.title);
    }
    heading(&mut out, "Subtasks");
    if t.subtasks.is_empty() {
        plain(&mut out, "none", "  ");
    }
    for s in &t.subtasks {
        item(&mut out, &s.status, &s.id, &s.title);
    }
    for (name, body) in [
        ("Description", &d.ticket.description),
        ("Plan", &d.ticket.plan),
    ] {
        heading(&mut out, name);
        plain(
            &mut out,
            if body.trim().is_empty() { "none" } else { body },
            "  ",
        );
    }
    heading(&mut out, "Criteria");
    if d.ticket.criteria.is_empty() {
        plain(&mut out, "none", "  ");
    }
    for c in &d.ticket.criteria {
        let mark = if c.checked { "[x]" } else { "[ ]" };
        plain(&mut out, &format!("{mark} {}. {}", c.number, c.text), "  ");
        if let Some(e) = &c.evidence {
            plain(&mut out, &format!("evidence: {e}"), "      ");
        }
        if let (Some(by), Some(at)) = (&c.checked_by, &c.checked_at) {
            plain(
                &mut out,
                &format!("checked by {} at {at}", who(by)),
                "      ",
            );
        }
    }
    heading(&mut out, "Comments");
    if d.ticket.comments.is_empty() {
        plain(&mut out, "none", "  ");
    }
    for c in &d.ticket.comments {
        let role = c
            .role
            .as_deref()
            .map(|r| format!("{r}/"))
            .unwrap_or_default();
        let sess = c
            .session
            .as_deref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        plain(
            &mut out,
            &format!("{role}{}{sess}  {}", c.author, c.created_at),
            "  ",
        );
        plain(&mut out, &c.body, "    ");
    }
    out
}

/// Draw the open detail view, clamping its scroll to the wrapped height.
fn draw_detail(frame: &mut Frame, area: Rect, state: &mut State) {
    let theme = state.theme;
    let Some(d) = state.detail.as_mut() else {
        return;
    };
    let block = widgets::bordered_pane(&d.ticket.ticket.id, true, &theme);
    let inner = block.inner(area);
    let lines = detail_lines(d, usize::from(inner.width), &theme);
    let max = lines.len().saturating_sub(usize::from(inner.height));
    d.scroll = d.scroll.min(max);
    let visible: Vec<Line> = lines
        .into_iter()
        .skip(d.scroll)
        .take(usize::from(inner.height))
        .collect();
    frame.render_widget(Paragraph::new(visible).block(block), area);
}

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

fn draw_list(frame: &mut Frame, area: Rect, state: &State) {
    let order = state.list_order();
    let block = widgets::bordered_pane(
        &format!("list ({})", state.visible_count()),
        true,
        &state.theme,
    );
    let width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem> = order
        .iter()
        .map(|e| match *e {
            ListEntry::Header(c) => {
                let n = state.column(c).len();
                let (icon, style) = state.theme.status_style(STATUSES[c], false, false);
                ListItem::new(Line::from(Span::styled(
                    format!("{icon} {} ({n})", STATUSES[c]),
                    style,
                )))
            }
            ListEntry::Ticket { idx, depth } => {
                let indent = "  ".repeat(depth);
                let mut line = row_line(
                    &state.tickets[idx],
                    width.saturating_sub(indent.len()),
                    &state.theme,
                );
                line.spans.insert(0, Span::raw(indent));
                ListItem::new(line)
            }
        })
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(state.theme.selected_style());
    let mut ls = ListState::default();
    ls.select(state.list_selected());
    frame.render_stateful_widget(list, area, &mut ls);
}

pub fn draw(frame: &mut Frame, state: &mut State) {
    let area = frame.area();
    let base: &[(&str, &str)] = if state.detail.is_some() {
        &HINTS_DETAIL
    } else {
        &HINTS
    };
    // Actions valid for the selected ticket's status, as "[k] label" hints.
    let offers = state
        .acted_ticket()
        .map(|(_, status, archived)| allowed(&status, archived))
        .unwrap_or_default();
    let mut hints: Vec<(&str, &str)> = base.to_vec();
    // Insert before the trailing quit hint so it stays last on the row.
    let at = if state.detail.is_some() {
        hints.len()
    } else {
        hints.len() - 1
    };
    for (i, (k, label, _)) in offers.into_iter().enumerate() {
        hints.insert(at + i, (k, label));
    }
    let hints: &[(&str, &str)] = &hints;
    let status_line = state.error.is_some() || state.notice.is_some();
    let footer_h = widgets::footer_height(hints, area.width, &state.theme) + u16::from(status_line);
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
    if state.detail.is_some() {
        draw_detail(frame, body, state);
    } else {
        match state.view {
            View::Board => draw_board(frame, body, state),
            View::List => draw_list(frame, body, state),
        }
    }
    let message = match (&state.error, &state.notice) {
        (Some(e), _) => Some(format!("error: {e}")),
        (None, Some(n)) => Some(n.clone()),
        (None, None) => None,
    };
    match message {
        Some(m) => {
            let [line, hints_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(footer);
            frame.render_widget(Paragraph::new(Line::from(m)), line);
            widgets::render_footer(frame.buffer_mut(), hints_area, hints, &state.theme);
        }
        None => widgets::render_footer(frame.buffer_mut(), footer, hints, &state.theme),
    }
    if let Some(c) = &state.confirm {
        draw_confirm(frame, area, &c.prompt, &state.theme);
    }
}

/// A centered modal asking y/n.
fn draw_confirm(frame: &mut Frame, area: Rect, prompt: &str, theme: &Theme) {
    let w = (prompt.chars().count() as u16 + 6).max(30).min(area.width);
    let h = 4.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, rect);
    let text = vec![
        Line::from(prompt.to_string()),
        Line::from("[y] confirm | [n] cancel"),
    ];
    frame.render_widget(
        Paragraph::new(text).block(widgets::bordered_pane("confirm", true, theme)),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::Event;
    use crate::tui::theme::Theme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(state: &mut State, w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, &mut *state)).unwrap();
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
        insta::assert_snapshot!(render(&mut s, 110, 14).join("\n"));
    }

    #[test]
    fn list_sample() {
        let mut s = sample_state();
        let mut t = s.tickets.clone();
        let mut c1 = ticket(
            "SCR-10",
            "ready",
            "Subtask of keyboard help",
            "agent",
            false,
        );
        c1.parent = Some("SCR-6".into());
        let mut c2 = ticket("SCR-11", "ready", "Another subtask", "agent", true);
        c2.parent = Some("SCR-6".into());
        t.extend([c1, c2]);
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        for k in [
            ratatui::crossterm::event::KeyCode::Tab,
            ratatui::crossterm::event::KeyCode::Down,
            ratatui::crossterm::event::KeyCode::Down,
        ] {
            crate::tui::state::update(
                &mut s,
                Event::Key(ratatui::crossterm::event::KeyEvent::from(k)),
            );
        }
        insta::assert_snapshot!(render(&mut s, 90, 20).join("\n"));
    }

    #[test]
    fn board_hide_closed() {
        let mut s = sample_state();
        s.hide_closed = true;
        insta::assert_snapshot!(render(&mut s, 90, 12).join("\n"));
    }

    #[test]
    fn empty_board_has_five_columns() {
        let mut s = State {
            who: "human/human".into(),
            ..State::default()
        };
        let rows = render(&mut s, 200, 8);
        assert!(rows[0].contains("all Projects"));
        assert!(rows[0].contains("human/human"));
        assert!(rows[0].contains("0 tickets"));
        for st in STATUSES {
            assert!(rows[1].contains(&format!("{st} (0)")), "{st}");
        }
        assert!(rows[7].starts_with("[Enter] open"));
    }

    #[test]
    fn error_shows_above_hints() {
        let mut s = State {
            error: Some("boom".into()),
            ..State::default()
        };
        let rows = render(&mut s, 200, 8);
        assert!(rows[6].starts_with("error: boom"));
        assert!(rows[7].starts_with("[Enter] open"));
    }

    fn detail_state() -> State {
        let mut s = sample_state();
        let d: crate::model::TicketDetail = serde_json::from_value(serde_json::json!({
            "archived_at": null, "available": false, "blocked": true,
            "blocked_by": [{"id": "SCR-5", "status": "active", "title": "Board view", "worktree": null}],
            "claimed_by": {"author": "jeff", "session": "s1"},
            "created_at": "2026-11-13T05:00:00Z", "done_at": null, "for": "agent",
            "id": "SCR-6", "parent": null, "status": "active",
            "subtasks": [{"id": "SCR-12", "status": "ready", "title": "Scroll keys"}],
            "title": "TUI detail view", "updated_at": "2026-11-13T06:00:00Z",
            "worktree": "feat-detail",
            "comments": [
                {"author": "jeff", "body": "Started work on the detail pane.", "created_at": "2026-11-13T05:30:00Z", "id": 1, "role": "agent", "session": "s1"},
                {"author": "bob", "body": "Looks good, please keep the long lines wrapping so nothing is clipped at the pane edge.", "created_at": "2026-11-13T05:40:00Z", "id": 2, "role": "human", "session": null}
            ],
            "criteria": [
                {"checked": true, "checked_at": "2026-11-13T05:50:00Z", "checked_by": {"author": "jeff", "session": null}, "evidence": "cargo test passed", "number": 1, "ref": "SCR-6#1", "text": "Renders every field"},
                {"checked": false, "checked_at": null, "checked_by": null, "evidence": null, "number": 2, "ref": "SCR-6#2", "text": "Scrolls"}
            ],
            "description": "Enter opens the detail view.\nEsc closes it.",
            "plan": "Add a full pane screen."
        }))
        .unwrap();
        crate::tui::state::update(&mut s, Event::DetailLoaded(Ok(Box::new(d))));
        s
    }

    #[test]
    fn detail_sample() {
        let mut s = detail_state();
        insta::assert_snapshot!(render(&mut s, 80, 40).join("\n"));
    }

    #[test]
    fn detail_scrolls() {
        let mut s = detail_state();
        let top = render(&mut s, 60, 12);
        s.detail.as_mut().unwrap().scroll = 5;
        let mid = render(&mut s, 60, 12);
        assert_ne!(top[2], mid[2]);
        assert!(top[2].contains("TUI detail view"));
        // G overshoots; draw clamps so the last comment line is visible and
        // a following k moves up one row from the real bottom.
        s.detail.as_mut().unwrap().scroll = usize::MAX;
        let bottom = render(&mut s, 60, 12);
        assert!(bottom.iter().any(|l| l.contains("pane edge")), "{bottom:?}");
        let at = s.detail.as_ref().unwrap().scroll;
        assert!(at < usize::MAX);
        crate::tui::state::update(
            &mut s,
            Event::Key(ratatui::crossterm::event::KeyEvent::from(
                ratatui::crossterm::event::KeyCode::Char('k'),
            )),
        );
        assert_eq!(s.detail.as_ref().unwrap().scroll, at - 1);
    }

    #[test]
    fn wrap_splits_long_words() {
        assert_eq!(wrap("abcdefgh", "", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap("a b\nc", "> ", 5), ["> a b", "> c"]);
    }

    fn key(s: &mut State, c: char) {
        crate::tui::state::update(
            s,
            Event::Key(ratatui::crossterm::event::KeyEvent::from(
                ratatui::crossterm::event::KeyCode::Char(c),
            )),
        );
    }

    #[test]
    fn footer_hints_follow_status() {
        let mut s = sample_state();
        let draft = render(&mut s, 200, 14).join("\n");
        assert!(draft.contains("[r] ready"), "{draft}");
        assert!(!draft.contains("[S] start"));
        key(&mut s, 'l');
        let ready = render(&mut s, 200, 14).join("\n");
        assert!(ready.contains("[S] start") && !ready.contains("[r] ready"));
        assert!(ready.contains("[A] archive"));
    }

    #[test]
    fn notice_and_error_show_in_status_line() {
        let mut s = sample_state();
        crate::tui::state::update(&mut s, Event::ActionDone(Ok("SCR-9 -> ready".into())));
        let rows = render(&mut s, 200, 14);
        assert!(rows.iter().any(|r| r.starts_with("SCR-9 -> ready")));
        crate::tui::state::update(&mut s, Event::ActionDone(Err("SCR-9 is blocked".into())));
        let rows = render(&mut s, 200, 14);
        assert!(
            rows.iter()
                .any(|r| r.starts_with("error: SCR-9 is blocked"))
        );
    }

    #[test]
    fn confirm_popup_sample() {
        let mut s = sample_state();
        key(&mut s, 'l');
        key(&mut s, 'x');
        insta::assert_snapshot!(render(&mut s, 90, 14).join("\n"));
    }
}
