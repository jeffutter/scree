//! Drawing: header, one bordered column per status, error line, footer hints.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph};

use super::markdown::markdown_lines;
use super::state::{
    Avail, Detail, DetailCache, DetailSel, Filter, ListEntry, Picker, PickerMode, STATUSES, State,
    View, allowed, decision_key,
};
use super::text::{cell, clean};
use super::theme::Theme;
use super::widgets;
use crate::model::{Claimer, Ticket};

const HINTS: [(&str, &str); 15] = [
    ("Enter", "open"),
    ("Tab", "board/list"),
    ("W", "decisions"),
    ("h/l", "column"),
    ("j/k", "row"),
    ("PgUp/PgDn", "page"),
    ("g/G", "first/last"),
    ("/", "search"),
    ("f", "filter"),
    ("c", "hide closed"),
    ("a", "archived"),
    ("s", "scope"),
    ("S-Up/Down", "reorder"),
    ("?", "help"),
    ("q", "quit"),
];

/// Every bound key, by section. The help popup draws this table and
/// docs/tui.md repeats it, so a new binding is added here first.
const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Board and list",
        &[
            (
                "h/l, Left/Right",
                "previous/next column (board; up/down when stacked)",
            ),
            ("j/k, Down/Up", "next/previous row"),
            ("PgUp/PgDn", "page up/down"),
            ("Ctrl-u/Ctrl-d", "half page up/down"),
            ("g/G, Home/End", "first/last row"),
            ("Enter", "open ticket detail"),
            ("Tab", "switch board/list"),
            ("W", "open decisions view"),
            ("c", "hide/show closed columns"),
            ("a", "show/hide archived"),
            ("s", "cycle scope worktree/project/all"),
            ("/", "search id, title, description"),
            ("f", "filter popup"),
            ("Esc", "clear search and filters"),
            ("Shift-Up/Down", "move ticket before/after neighbour"),
            ("r S R", "ready / start / release"),
            ("d D", "done / done --force (confirm)"),
            ("x o A", "abandon (confirm) / reopen / archive"),
            ("e", "edit title"),
            ("E P", "edit description / plan in editor"),
            ("?", "this help"),
            ("q, Ctrl-c", "quit"),
        ],
    ),
    (
        "Detail",
        &[
            ("j/k, PgUp/PgDn", "scroll"),
            ("Ctrl-u/Ctrl-d", "half page"),
            ("g/G", "top/bottom"),
            ("Wheel, swipe", "scroll 3 lines"),
            ("[ ]", "select previous/next criterion or comment"),
            ("Space, v", "check (evidence prompt) / uncheck"),
            ("n", "add criterion"),
            ("m M", "add comment in prompt / editor"),
            ("X", "remove criterion or comment (confirm)"),
            ("b B", "add / remove blocker"),
            ("r S R d D x o A e E P", "as on the board"),
            ("Esc, q, h, Left", "back to board/list"),
            ("?", "this help"),
            ("Ctrl-c", "quit"),
        ],
    ),
    (
        "Decisions",
        &[
            ("j/k, PgUp/PgDn", "next/previous decision, page"),
            ("Ctrl-u/Ctrl-d", "half page up/down"),
            ("g/G, Home/End", "first/last decision"),
            ("y", "accept the recommendation (confirm)"),
            ("1-9", "answer with that numbered option (confirm)"),
            ("a", "answer with typed text (prompt)"),
            ("t", "open the linked ticket (picker when several)"),
            ("s", "cycle scope worktree/project/all"),
            ("Esc, q, h, Left, Tab, W", "back to board/list"),
            ("?", "this help"),
            ("Ctrl-c", "quit"),
        ],
    ),
    (
        "Popups and prompts",
        &[
            ("Filter: s f b", "cycle status / for / blocked"),
            ("Filter: x", "clear all filters"),
            ("Filter: Esc, Enter", "close"),
            ("Picker: type", "filter tickets"),
            ("Picker: Up/Down, Ctrl-p/n", "move"),
            ("Picker: Enter, Esc", "apply, cancel"),
            ("Prompt: Enter, Esc", "save, cancel"),
            ("Confirm: y, Enter", "confirm; any other key cancels"),
            ("Help: Esc, ?, q", "close"),
        ],
    ),
];

const HINTS_EDIT: [(&str, &str); 1] = [("e/E/P", "title/desc/plan")];

const HINTS_DETAIL: [(&str, &str); 11] = [
    ("Esc/q", "back"),
    ("j/k", "scroll"),
    ("PgUp/PgDn", "page"),
    ("g/G", "top/bottom"),
    ("[/]", "select"),
    ("Space", "check"),
    ("n", "criterion"),
    ("m/M", "comment"),
    ("X", "remove"),
    ("b/B", "add/rm blocker"),
    ("?", "help"),
];

const HINTS_DECISIONS: [(&str, &str); 9] = [
    ("Esc/q", "back"),
    ("j/k", "row"),
    ("g/G", "first/last"),
    ("y", "accept"),
    ("1-9", "option"),
    ("a", "answer"),
    ("t", "ticket"),
    ("s", "scope"),
    ("?", "help"),
];

/// Greedy word wrap of `text` to `width` cells; every output line starts with
/// `indent`. Newlines in `text` start new lines; words wider than the room are
/// split. Always returns at least one line.
fn wrap(text: &str, indent: &str, width: usize) -> Vec<String> {
    let room = width
        .saturating_sub(indent.chars().map(cell).sum::<usize>())
        .max(1);
    let mut out = vec![];
    for para in text.split('\n') {
        let para = clean(para);
        // Leading indentation is content: keep it on the first line.
        let lead_n = para.chars().take_while(|&c| c == ' ').count().min(room - 1);
        let mut line = " ".repeat(lead_n);
        let mut used = lead_n;
        let mut started = false;
        // Splitting on single spaces keeps runs of spaces as empty tokens.
        for word in para.trim_start_matches(' ').split(' ') {
            let w: usize = word.chars().map(cell).sum();
            let sep = usize::from(started);
            if used + sep + w <= room {
                if started {
                    line.push(' ');
                }
                line.push_str(word);
                used += sep + w;
                started = true;
                continue;
            }
            if w == 0 {
                continue;
            }
            // Indentation alone is not content: never flush an indent-only line.
            if has_content(&line) {
                out.push(format!("{indent}{}", line.trim_end_matches(' ')));
                line.clear();
                used = 0;
            }
            started = true;
            for c in word.chars() {
                if has_content(&line) && used + cell(c) > room {
                    out.push(format!("{indent}{}", line.trim_end_matches(' ')));
                    line.clear();
                    used = 0;
                }
                line.push(c);
                used += cell(c);
            }
        }
        out.push(format!("{indent}{}", line.trim_end_matches(' ')));
    }
    out
}

fn has_content(line: &str) -> bool {
    line.chars().any(|c| c != ' ')
}

fn who(c: &Claimer) -> String {
    match &c.session {
        Some(s) => format!("{} ({s})", c.author),
        None => c.author.clone(),
    }
}

/// Mark `lines` as the selected item: a ">" in the indent of the first line
/// and the theme's selected style on all of them.
fn mark_selected(lines: &mut [Line<'static>], theme: &Theme) {
    for (i, line) in lines.iter_mut().enumerate() {
        if i == 0
            && let Some(first) = line.spans.first_mut()
            && first.content.starts_with("  ")
        {
            first.content = format!("> {}", &first.content[2..]).into();
        }
        line.style = theme.selected_style();
    }
}

/// One list item in the detail view: "  " + `prefix` + title, with title
/// continuation lines aligned under the first title character.
fn item_lines(prefix: String, style: Style, title: &str, width: usize) -> Vec<Line<'static>> {
    let indent = " ".repeat(2 + Span::raw(prefix.clone()).width());
    let mut it = wrap(title, &indent, width).into_iter();
    let first = it.next().unwrap_or_default();
    let mut lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(prefix, style),
        Span::raw(first[indent.len()..].to_string()),
    ])];
    lines.extend(it.map(Line::from));
    lines
}

/// Every field of the ticket as wrapped lines (Markdown for bodies), in reading order, and
/// the line range of the selected item.
fn detail_lines(
    d: &Detail,
    width: usize,
    theme: &Theme,
) -> (Vec<Line<'static>>, Option<(usize, usize)>) {
    let mut sel_line = None;
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
        out.extend(item_lines(prefix, style, title, width));
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
    if t.blocked_by.is_empty() && t.blocking_decisions.is_empty() {
        plain(&mut out, "none", "  ");
    }
    for b in &t.blocked_by {
        item(&mut out, &b.status, &b.id, &b.title);
    }
    for b in &t.blocking_decisions {
        item(&mut out, "open", &b.id, &b.question);
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
        ("Handoff", &d.ticket.handoff),
    ] {
        // Handoff is rare: show the section only when a note exists.
        if name == "Handoff" && body.trim().is_empty() {
            continue;
        }
        heading(&mut out, name);
        if body.trim().is_empty() {
            plain(&mut out, "none", "  ");
        } else {
            out.extend(markdown_lines(body, "  ", width, theme));
        }
    }
    heading(&mut out, "Criteria");
    if d.ticket.criteria.is_empty() {
        plain(&mut out, "none", "  ");
    }
    for (i, c) in d.ticket.criteria.iter().enumerate() {
        let start = out.len();
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
        if d.sel == Some(DetailSel::Criterion(i)) {
            sel_line = Some((start, out.len()));
            mark_selected(&mut out[start..], theme);
        }
    }
    heading(&mut out, "Comments");
    if d.ticket.comments.is_empty() {
        plain(&mut out, "none", "  ");
    }
    for (i, c) in d.ticket.comments.iter().enumerate() {
        let start = out.len();
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
        out.extend(markdown_lines(&c.body, "    ", width, theme));
        if d.sel == Some(DetailSel::Comment(i)) {
            sel_line = Some((start, out.len()));
            mark_selected(&mut out[start..], theme);
        }
    }
    (out, sel_line)
}

/// Draw the open detail view, clamping its scroll to the wrapped height.
fn draw_detail(frame: &mut Frame, area: Rect, state: &mut State) {
    let theme = state.theme;
    let Some(d) = state.detail.as_mut() else {
        return;
    };
    let block = widgets::bordered_pane(&d.ticket.ticket.id, true, &theme);
    let inner = block.inner(area);
    let width = usize::from(inner.width);
    let fresh = d
        .cache
        .as_ref()
        .is_some_and(|c| c.width == width && c.theme == theme && c.sel == d.sel);
    if !fresh {
        let (lines, sel_line) = detail_lines(d, width, &theme);
        d.cache = Some(DetailCache {
            width,
            theme,
            sel: d.sel,
            lines,
            sel_line,
        });
    }
    let Some(cache) = d.cache.as_ref() else {
        return;
    };
    let (lines, sel_line) = (&cache.lines, cache.sel_line);
    let height = usize::from(inner.height);
    let max = lines.len().saturating_sub(height);
    d.scroll = d.scroll.min(max);
    // Bring the whole selected item into view; one taller than the pane
    // shows its first line.
    if let (true, Some((start, end))) = (d.reveal, sel_line) {
        if end > d.scroll + height {
            d.scroll = (end.saturating_sub(height)).min(max);
        }
        if start < d.scroll {
            d.scroll = start;
        }
    }
    d.reveal = false;
    let visible: Vec<Line> = lines.iter().skip(d.scroll).take(height).cloned().collect();
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

/// Width of a collapsed (empty) board column: 2 border cells + the longest
/// status name ("abandoned", 9 chars), so every strip shows its full title and
/// keeps one width whichever column has focus.
const COLLAPSED_W: u16 = 11;

/// Least inner width a non-collapsed column must keep to stay readable.
const MIN_FILL_W: u16 = 20;

/// Height of a collapsed (empty) column in the stacked layout: 2 border rows
/// plus one blank row.
const COLLAPSED_H: u16 = 3;

/// Tallest an unfocused populated column grows in the stacked layout.
const STACKED_OTHER_MAX_H: u16 = 3;

/// Rows the focused column keeps in the stacked layout before the others
/// grow past their border rows.
const STACKED_FOCUS_MIN_H: u16 = 5;

/// Whether the board must stack its columns vertically: the columns do not
/// fit side by side at their readable width (2 border cells + `MIN_FILL_W`
/// per expanded column, `COLLAPSED_W` per collapsed one).
fn stack_columns(width: u16, n_fill: usize, n_collapsed: usize) -> bool {
    let need = n_fill * usize::from(MIN_FILL_W + 2) + n_collapsed * usize::from(COLLAPSED_W);
    usize::from(width) < need
}

fn draw_board(frame: &mut Frame, area: Rect, state: &mut State) {
    let cols = state.visible_columns();
    // Empty columns collapse to a narrow strip (derived from the data at
    // draw time, so a column expands as soon as it gains a ticket). When
    // every column is empty none collapse, so full titles stay readable.
    // The focused column never collapses, so its full title stays visible.
    // The layout is decided from the area width on every draw: when the
    // columns would be too thin side by side they are stacked top to bottom.
    let any_populated = cols.iter().any(|&c| !state.column(c).is_empty());
    let collapsed: Vec<bool> = cols
        .iter()
        .map(|&c| any_populated && c != state.col && state.column(c).is_empty())
        .collect();
    let n_collapsed = collapsed.iter().filter(|&&x| x).count();
    let n_fill = cols.len() - n_collapsed;
    let stacked = stack_columns(area.width, n_fill, n_collapsed);
    // Stacked heights. The focused column is a Fill pane, so it gets whatever
    // the fixed panes leave; to keep STACKED_FOCUS_MIN_H of it, the fixed panes
    // draw from a budget of `area.height - STACKED_FOCUS_MIN_H`, granted in
    // priority order: populated floors (2 border rows), collapsed floors,
    // then growth of populated panes to their wanted height (up to
    // STACKED_OTHER_MAX_H), then of collapsed ones to COLLAPSED_H. On a
    // terminal too short for all of it the later panes shrink, never the
    // focused one.
    let mut fixed_h = vec![0u16; cols.len()];
    if stacked {
        let focus_min = STACKED_FOCUS_MIN_H.min(area.height);
        let mut budget = area.height - focus_min;
        let wanted = |i: usize| -> u16 {
            if collapsed[i] {
                COLLAPSED_H
            } else {
                let rows = u16::try_from(state.column(cols[i]).len()).unwrap_or(u16::MAX);
                rows.saturating_add(2).clamp(2, STACKED_OTHER_MAX_H)
            }
        };
        let focus = state.col;
        let others = |pop: bool| -> Vec<usize> {
            (0..cols.len())
                .filter(|&i| cols[i] != focus && collapsed[i] != pop)
                .collect()
        };
        for pop in [true, false] {
            for i in others(pop) {
                let g = 2.min(budget);
                fixed_h[i] += g;
                budget -= g;
            }
        }
        for pop in [true, false] {
            for i in others(pop) {
                let g = wanted(i).saturating_sub(fixed_h[i]).min(budget);
                fixed_h[i] += g;
                budget -= g;
            }
        }
    }
    let constraints: Vec<Constraint> = cols
        .iter()
        .zip(&collapsed)
        .enumerate()
        .map(|(i, (&c, &e))| match (e, stacked) {
            (_, true) if c != state.col => Constraint::Length(fixed_h[i]),
            (true, false) => Constraint::Length(COLLAPSED_W),
            (_, true) | (false, false) => Constraint::Fill(1),
        })
        .collect();
    let areas = if stacked {
        Layout::vertical(constraints).split(area)
    } else {
        Layout::horizontal(constraints).split(area)
    };
    let mut offsets = state.offsets;
    for (&c, &pane) in cols.iter().zip(areas.iter()) {
        let tickets = state.column(c);
        let active = c == state.col;
        let title = if tickets.is_empty() {
            // Fits whenever the pane is at least COLLAPSED_W wide; the
            // truncation only matters for a pane squeezed below that.
            let name = STATUSES[c];
            name.chars()
                .take(usize::from(pane.width.saturating_sub(2)))
                .collect()
        } else {
            format!("{} ({})", STATUSES[c], tickets.len())
        };
        let block = widgets::bordered_pane(&title, active, &state.theme);
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
        let mut ls = ListState::default().with_offset(offsets[c]);
        if !tickets.is_empty() {
            ls.select(Some(state.rows[c]));
        }
        frame.render_stateful_widget(list, pane, &mut ls);
        offsets[c] = ls.offset();
    }
    state.offsets = offsets;
}

fn draw_list(frame: &mut Frame, area: Rect, state: &mut State) {
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
    let mut ls = ListState::default().with_offset(state.list_offset);
    ls.select(state.list_selected());
    frame.render_stateful_widget(list, area, &mut ls);
    state.list_offset = ls.offset();
}

/// The decisions view: the queue on top, the selected decision's options,
/// recommendation and linked tickets below when there is room.
fn draw_decisions(frame: &mut Frame, area: Rect, state: &mut State) {
    let theme = &state.theme;
    let sel = state.selected_decision().cloned();
    let (list_area, detail_area) = if area.height >= 14 {
        let [l, d] = Layout::vertical([Constraint::Min(5), Constraint::Length(8)]).areas(area);
        (l, Some(d))
    } else {
        (area, None)
    };
    let block = widgets::bordered_pane(
        &format!("decisions ({})", state.decisions.len()),
        true,
        theme,
    );
    let width = usize::from(list_area.width.saturating_sub(2));
    let items: Vec<ListItem> = if state.decisions.is_empty() {
        vec![ListItem::new("no open decisions")]
    } else {
        state
            .decisions
            .iter()
            .map(|d| {
                let head = format!("{:<9} unblocks {:>2}  ", decision_key(d), d.unblocks);
                let room = width.saturating_sub(head.chars().count());
                ListItem::new(format!(
                    "{head}{}",
                    truncate(&clean(&d.question), room, theme)
                ))
            })
            .collect()
    };
    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_style());
    let mut ls = ListState::default().with_offset(state.decisions_offset);
    ls.select(state.decision_pos());
    frame.render_stateful_widget(list, list_area, &mut ls);
    state.decisions_offset = ls.offset();
    if let (Some(area), Some(d)) = (detail_area, sel) {
        let w = usize::from(area.width.saturating_sub(2));
        let mut lines: Vec<String> = wrap(&clean(&d.question), "", w);
        for (i, o) in d.options.iter().enumerate() {
            lines.extend(wrap(&format!("{}. {}", i + 1, clean(o)), "  ", w));
        }
        if let Some(r) = &d.recommendation {
            lines.extend(wrap(&format!("recommended: {}", clean(r)), "", w));
        }
        if !d.tickets.is_empty() {
            lines.extend(wrap(&format!("tickets: {}", d.tickets.join(", ")), "", w));
        }
        let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
        frame.render_widget(
            Paragraph::new(text).block(widgets::bordered_pane(&decision_key(&d), false, theme)),
            area,
        );
    }
}

pub fn draw(frame: &mut Frame, state: &mut State) {
    let area = frame.area();
    let in_decisions = state.detail.is_none() && state.view == View::Decisions;
    let base: &[(&str, &str)] = if state.detail.is_some() {
        &HINTS_DETAIL
    } else if in_decisions {
        &HINTS_DECISIONS
    } else {
        &HINTS
    };
    // Actions valid for the selected ticket's status, as "[k] label" hints.
    let offers = if in_decisions {
        vec![]
    } else {
        state
            .acted_ticket()
            .map(|(_, status, archived)| allowed(&status, archived))
            .unwrap_or_default()
    };
    let mut hints: Vec<(&str, &str)> = base.to_vec();
    // Insert before the trailing quit hint so it stays last on the row.
    let at = if state.detail.is_some() {
        hints.len()
    } else {
        hints.len() - 1
    };
    let n_offers = offers.len();
    for (i, (k, label, _)) in offers.into_iter().enumerate() {
        hints.insert(at + i, (k, label));
    }
    if !in_decisions && state.acted_ticket().is_some() {
        for (i, h) in HINTS_EDIT.into_iter().enumerate() {
            hints.insert(at + n_offers + i, h);
        }
    }
    let hints: &[(&str, &str)] = &hints;
    let status_line = state.error.is_some() || state.notice.is_some();
    let footer_h = if state.prompt.is_some() {
        1
    } else {
        widgets::footer_height(hints, area.width, &state.theme) + u16::from(status_line)
    };
    let show_filter = !in_decisions
        && state.detail.is_none()
        && (state.filter.is_active() || state.search.is_some());
    let show_banner = !in_decisions && !state.decisions.is_empty();
    let [header, banner, filter_row, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(show_banner)),
        Constraint::Length(u16::from(show_filter)),
        Constraint::Min(0),
        Constraint::Length(footer_h),
    ])
    .areas(area);
    // Paging moves by exactly the rows the body pane shows (minus borders),
    // whatever the footer wrapped to or whether a status line is present.
    state.page = usize::from(body.height.saturating_sub(2)).max(1);
    widgets::render_header(
        frame.buffer_mut(),
        header,
        &state.display_scope(),
        &state.who,
        state.visible_count(),
        &state.theme,
    );
    if show_banner {
        widgets::render_banner(
            frame.buffer_mut(),
            banner,
            state.decisions.len(),
            state.decision_ticket_count(),
            &state.theme,
        );
    }
    if show_filter {
        let line = filter_line(&state.filter, state.search.is_some());
        frame.render_widget(Paragraph::new(line), filter_row);
        if let Some(p) = &state.search {
            let col = (p.cursor_col() as u16).min(filter_row.width.saturating_sub(1));
            frame.set_cursor_position((filter_row.x + col, filter_row.y));
        }
    }
    if state.detail.is_some() {
        draw_detail(frame, body, state);
    } else {
        match state.view {
            View::Board => draw_board(frame, body, state),
            View::List => draw_list(frame, body, state),
            View::Decisions => draw_decisions(frame, body, state),
        }
    }
    let message = match (&state.error, &state.notice) {
        (Some(e), _) => Some(format!("error: {e}")),
        (None, Some(n)) => Some(n.clone()),
        (None, None) => None,
    };
    if let Some(p) = &state.prompt {
        let line = p.prompt.line();
        frame.render_widget(Clear, footer);
        frame.render_widget(Paragraph::new(Line::from(line)), footer);
        let col = (p.prompt.cursor_col() as u16).min(footer.width.saturating_sub(1));
        frame.set_cursor_position((footer.x + col, footer.y));
        return;
    }
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
    if let Some(p) = &state.picker {
        draw_picker(frame, area, p, &state.theme);
    }
    if state.filter_menu {
        draw_filter_menu(frame, area, &state.filter, &state.theme);
    }
    if state.help {
        draw_help(frame, area, state);
    }
}

/// A centered popup listing every key by section from `HELP`. It is as wide
/// as the widest line, up to the screen; longer lines end in an ellipsis.
/// Content taller than the popup scrolls by `state.help_scroll`, which is
/// clamped here and written back so keys act on the real position.
fn draw_help(frame: &mut Frame, area: Rect, state: &mut State) {
    let theme = &state.theme;
    let key_w = HELP
        .iter()
        .flat_map(|(_, rows)| rows.iter())
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let mut lines: Vec<String> = vec![];
    for (section, rows) in HELP {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("{section}:"));
        for (k, m) in *rows {
            lines.push(format!("  {k:<key_w$}  {m}"));
        }
    }
    let widest = lines
        .iter()
        .map(|l| Span::raw(l.as_str()).width())
        .max()
        .unwrap_or(0);
    let w = ((widest + 2) as u16).min(area.width);
    let h = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    let inner_h = usize::from(h.saturating_sub(2));
    let max_scroll = lines.len().saturating_sub(inner_h);
    let scroll = usize::from(state.help_scroll).min(max_scroll);
    state.help_scroll = scroll as u16;
    let inner_w = usize::from(w.saturating_sub(2));
    let text: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(inner_h)
        .map(|l| Line::from(truncate(l, inner_w, theme)))
        .collect();
    let title = if max_scroll == 0 {
        "help (Esc closes)".to_string()
    } else if scroll < max_scroll {
        "help (j/k scroll, Esc closes) - more below".to_string()
    } else {
        "help (j/k scroll, Esc closes) - end".to_string()
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(text).block(widgets::bordered_pane(&title, true, theme)),
        rect,
    );
}

/// The filter header: "/ query" then each set filter, and how to clear.
fn filter_line(f: &Filter, searching: bool) -> String {
    let mut parts = vec![format!("/ {}", f.query)];
    if let Some(c) = f.status {
        parts.push(format!("status:{}", STATUSES[c]));
    }
    if let Some(w) = &f.for_who {
        parts.push(format!("for:{w}"));
    }
    match f.avail {
        Some(Avail::Blocked) => parts.push("blocked".into()),
        Some(Avail::Available) => parts.push("available".into()),
        None => {}
    }
    parts.push(if searching {
        "(Enter keeps, Esc clears)".into()
    } else {
        "(Esc clears)".into()
    });
    parts.join("  ")
}

/// A centered popup showing the three cyclable filters and their keys.
fn draw_filter_menu(frame: &mut Frame, area: Rect, f: &Filter, theme: &Theme) {
    let w = 40.min(area.width);
    let h = 7.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, rect);
    let any = |o: Option<&str>| o.unwrap_or("any").to_string();
    let avail = match f.avail {
        Some(Avail::Blocked) => "blocked",
        Some(Avail::Available) => "available",
        None => "any",
    };
    let text = vec![
        Line::from(format!(
            "[s] status:  {}",
            any(f.status.map(|c| STATUSES[c]))
        )),
        Line::from(format!("[f] for:     {}", any(f.for_who.as_deref()))),
        Line::from(format!("[b] blocked: {avail}")),
        Line::from("[x] clear | Esc/Enter close"),
    ];
    frame.render_widget(
        Paragraph::new(text).block(widgets::bordered_pane("filter", true, theme)),
        rect,
    );
}

/// A centered popup: filter line, matching tickets with the selected one
/// highlighted, and a hints line.
fn draw_picker(frame: &mut Frame, area: Rect, p: &Picker, theme: &Theme) {
    let w = 60.min(area.width);
    let h = 14.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, rect);
    let title = match p.mode {
        PickerMode::Add => format!("{} blocked by", p.id),
        PickerMode::Remove => format!("{} no longer blocked by", p.id),
        PickerMode::Jump => format!("open a ticket of {}", p.id),
    };
    let block = widgets::bordered_pane(&title, true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let [filter, list, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(format!("filter: {}", p.filter)), filter);
    let width = usize::from(list.width);
    let items: Vec<ListItem> = p
        .visible()
        .into_iter()
        .map(|(id, title)| {
            let line = format!("{id} {title}");
            ListItem::new(truncate(&line, width, theme))
        })
        .collect();
    let mut ls = ListState::default().with_selected(Some(p.sel));
    frame.render_stateful_widget(
        List::new(items).highlight_style(Style::default().reversed()),
        list,
        &mut ls,
    );
    frame.render_widget(Paragraph::new("Enter select | Esc cancel"), hint);
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

    /// Non-issue: only the header line (indent "  ") gets the ">" marker; a
    /// body that starts with a quote or rule sits on a later line under a
    /// deeper indent and keeps it.
    #[test]
    fn selection_marker_is_not_confused_with_a_quote_or_rule_body() {
        let theme = Theme::default();
        for body in ["> quoted", "---", ""] {
            let mut lines = vec![Line::from("  me  now")];
            lines.extend(crate::tui::markdown::markdown_lines(
                body, "    ", 40, &theme,
            ));
            mark_selected(&mut lines, &theme);
            let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            assert_eq!(text[0], "> me  now");
            for l in &text[1..] {
                assert!(l.starts_with("    "), "{l:?}");
            }
        }
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
    fn page_size_tracks_wrapped_footer_and_error_line() {
        let mut s = sample_state();
        render(&mut s, 600, 30);
        let wide = s.page;

        let rows = render(&mut s, 90, 30);
        let narrow = s.page;
        assert!(narrow < wide, "narrow {narrow} wide {wide}");
        // Rows between the pane's top and bottom borders in the rendered frame.
        let bottom = rows
            .iter()
            .rposition(|r| r.starts_with("\u{2514}") || r.starts_with('+'))
            .unwrap();
        let top = rows
            .iter()
            .position(|r| r.starts_with("\u{250c}") || r.starts_with('+'))
            .unwrap();
        assert_eq!(narrow, bottom - top - 1);

        s.error = Some("boom".into());
        render(&mut s, 90, 30);
        assert_eq!(s.page, narrow - 1);
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
    fn filter_header_on_board_while_searching() {
        let mut s = sample_state();
        key(&mut s, '/');
        key(&mut s, 'b');
        key(&mut s, 'o');
        let rows = render(&mut s, 100, 12);
        assert!(rows[1].starts_with("/ bo"), "{:?}", rows[1]);
        assert!(rows[1].contains("Enter keeps"));
        insta::assert_snapshot!(rows.join("\n"));
    }

    #[test]
    fn filter_header_on_list_with_filters() {
        let mut s = sample_state();
        crate::tui::state::update(
            &mut s,
            Event::Key(ratatui::crossterm::event::KeyEvent::from(
                ratatui::crossterm::event::KeyCode::Tab,
            )),
        );
        s.filter.status = Some(1);
        s.filter.for_who = Some("agent".into());
        s.filter.avail = Some(Avail::Blocked);
        s.filter.query = "wire".into();
        let rows = render(&mut s, 100, 12);
        assert!(rows[1].contains("/ wire  status:ready  for:agent  blocked  (Esc clears)"));
        insta::assert_snapshot!(rows.join("\n"));
    }

    #[test]
    fn help_popup_sample() {
        let mut s = sample_state();
        s.help = true;
        insta::assert_snapshot!(render(&mut s, 90, 60).join("\n"));
    }

    #[test]
    fn help_popup_small_reaches_last_section_after_scrolling() {
        let mut s = sample_state();
        s.help = true;
        let top = render(&mut s, 80, 24);
        assert!(!top.iter().any(|r| r.contains("Detail")));
        assert!(top.iter().any(|r| r.contains("more below")));
        insta::assert_snapshot!("help_popup_small", top.join("\n"));
        s.help_scroll = u16::MAX;
        let bottom = render(&mut s, 80, 24);
        let (last, rows) = HELP.last().unwrap();
        assert!(bottom.iter().any(|r| r.contains(&format!("{last}:"))));
        let (k, m) = rows.last().unwrap();
        assert!(bottom.iter().any(|r| r.contains(k) && r.contains(m)));
        assert_ne!(s.help_scroll, u16::MAX);
        insta::assert_snapshot!("help_popup_small_scrolled", bottom.join("\n"));
    }

    #[test]
    fn filter_popup_sample() {
        let mut s = sample_state();
        key(&mut s, 'f');
        key(&mut s, 's');
        insta::assert_snapshot!(render(&mut s, 80, 14).join("\n"));
    }

    #[test]
    fn board_hide_closed() {
        let mut s = sample_state();
        s.hide_closed = true;
        insta::assert_snapshot!(render(&mut s, 90, 12).join("\n"));
    }

    fn press(s: &mut State, code: ratatui::crossterm::event::KeyCode) {
        crate::tui::state::update(
            s,
            Event::Key(ratatui::crossterm::event::KeyEvent::from(code)),
        );
    }

    #[test]
    fn board_some_empty_columns() {
        use ratatui::crossterm::event::KeyCode;
        let mut s = sample_state();
        let t: Vec<_> = s
            .tickets
            .iter()
            .filter(|t| t.status == "ready" || t.status == "done")
            .cloned()
            .collect();
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        // Move to an empty column (draft/active/abandoned are collapsed).
        press(&mut s, KeyCode::Left);
        insta::assert_snapshot!(render(&mut s, 110, 12).join("\n"));
    }

    #[test]
    fn focused_empty_column_shows_full_status_name() {
        let mut s = sample_state();
        let t: Vec<_> = s
            .tickets
            .iter()
            .filter(|t| t.status == "ready")
            .cloned()
            .collect();
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        s.col = STATUSES.iter().position(|&x| x == "abandoned").unwrap();
        let full = STATUSES[s.col];
        assert!(s.column(s.col).is_empty());
        assert!(
            render(&mut s, 110, 12)[1].contains(full),
            "focused empty column must show {full}"
        );
    }

    #[test]
    fn stack_threshold_boundary() {
        // 5 expanded columns need 5 * 22 = 110 cells.
        assert!(!stack_columns(110, 5, 0));
        assert!(stack_columns(109, 5, 0));
        // 1 expanded + 4 strips need 22 + 4 * 11 = 66 cells.
        assert!(!stack_columns(66, 1, 4));
        assert!(stack_columns(65, 1, 4));
    }

    fn ready_only_state() -> State {
        let mut s = sample_state();
        let t: Vec<_> = s
            .tickets
            .iter()
            .filter(|t| t.status == "ready")
            .cloned()
            .collect();
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        s.col = STATUSES.iter().position(|&x| x == "ready").unwrap();
        s
    }

    fn key_event(c: ratatui::crossterm::event::KeyCode) -> Event {
        Event::Key(ratatui::crossterm::event::KeyEvent::from(c))
    }

    #[test]
    fn stacked_short_terminal_shows_focused_selected_row() {
        let mut s = sample_state();
        let out = render(&mut s, 90, 14);
        let ticket = &s.column(s.col)[s.rows[s.col]];
        let id = ticket.id.clone();
        assert!(out.iter().any(|l| l.contains(&id)), "{}", out.join("\n"));
    }

    #[test]
    fn stacked_short_terminal_shows_selected_row_after_focus_change() {
        use ratatui::crossterm::event::KeyCode;
        let mut s = sample_state();
        render(&mut s, 90, 14);
        for _ in 0..2 {
            crate::tui::state::update(&mut s, key_event(KeyCode::Right));
            let out = render(&mut s, 90, 14);
            let id = s.column(s.col)[s.rows[s.col]].id.clone();
            assert!(
                out.iter().any(|l| l.contains(&id)),
                "col {}: {}",
                s.col,
                out.join("\n")
            );
        }
    }

    #[test]
    fn stacked_focused_column_keeps_selected_row_on_short_terminals() {
        use ratatui::crossterm::event::KeyCode;
        for h in [8u16, 10, 12] {
            let mut s = sample_state();
            render(&mut s, 90, h);
            for step in 0..STATUSES.len() {
                let out = render(&mut s, 90, h);
                let col = s.column(s.col);
                if !col.is_empty() {
                    let id = col[s.rows[s.col]].id.clone();
                    // Skip the header row; the id must be in the board.
                    assert!(
                        out.iter().skip(1).any(|l| l.contains(&id)),
                        "h {h} step {step} col {}: {}",
                        s.col,
                        out.join("\n")
                    );
                }
                crate::tui::state::update(&mut s, key_event(KeyCode::Right));
            }
        }
    }

    #[test]
    fn stacked_focused_column_keeps_selected_row_with_all_columns_populated() {
        use ratatui::crossterm::event::KeyCode;
        let mut s = sample_state();
        let mut t = s.tickets.clone();
        t.push(ticket("SCR-20", "active", "Another active", "agent", false));
        t.push(ticket(
            "SCR-21",
            "abandoned",
            "Another abandoned",
            "agent",
            false,
        ));
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        for h in [8u16, 10, 12] {
            s.col = 0;
            for step in 0..STATUSES.len() {
                let out = render(&mut s, 90, h);
                let id = s.column(s.col)[s.rows[s.col]].id.clone();
                assert!(
                    out.iter().skip(1).any(|l| l.contains(&id)),
                    "h {h} step {step}: {}",
                    out.join("\n")
                );
                crate::tui::state::update(&mut s, key_event(KeyCode::Right));
            }
        }
    }

    #[test]
    fn narrow_width_stacks_columns_vertically() {
        let mut s = ready_only_state();
        // Width 65 is one under the threshold for 1 expanded + 4 strips.
        let out = render(&mut s, 65, 24);
        // Every column title sits on its own row, full width, left-aligned.
        let titles: Vec<&String> = out
            .iter()
            .filter(|l| l.starts_with('+') && l.chars().any(char::is_alphabetic))
            .collect();
        assert_eq!(titles.len(), cols_count(&s), "{}", out.join("\n"));
        for t in titles {
            assert!(t.ends_with('+') && t.chars().count() >= 65, "{t}");
        }
        // The populated column shows its title and count; empty ones collapse
        // to a short stacked pane and still show the full name.
        let joined = out.join("\n");
        assert!(joined.contains("ready ("), "{joined}");
        assert!(joined.contains("abandoned"), "{joined}");
    }

    #[test]
    fn at_threshold_columns_stay_side_by_side() {
        let mut s = ready_only_state();
        let out = render(&mut s, 66, 12);
        // Side by side: the first border row holds all five panes.
        let corners = out[1].chars().filter(|&c| c == '+').count();
        assert_eq!(corners, 2 * cols_count(&s), "{}", out.join("\n"));
    }

    #[test]
    fn resizing_across_threshold_switches_layout() {
        let mut s = ready_only_state();
        let wide = render(&mut s, 66, 12);
        let narrow = render(&mut s, 65, 12);
        let wide_again = render(&mut s, 66, 12);
        assert_ne!(wide, narrow);
        assert_eq!(wide, wide_again);
    }

    #[test]
    fn collapsed_strips_show_full_names_and_keep_width() {
        let mut s = ready_only_state();
        let out = render(&mut s, 110, 12);
        for name in ["draft", "active", "done", "abandoned"] {
            assert!(out[1].contains(name), "{name}: {}", out.join("\n"));
        }
        // Moving focus onto a collapsed column leaves the other strips alone.
        let before = out[1].clone();
        s.col = STATUSES.iter().position(|&x| x == "draft").unwrap();
        let after = render(&mut s, 110, 12)[1].clone();
        for name in ["active", "done", "abandoned"] {
            let w = |l: &str| {
                let i = l.find(name).unwrap();
                l[i..].find('+').unwrap() + l[..i].len()
            };
            assert_eq!(
                before.len() - w(&before),
                after.len() - w(&after),
                "{before}\n{after}"
            );
        }
    }

    fn cols_count(s: &State) -> usize {
        s.visible_columns().len()
    }

    #[test]
    fn board_all_populated() {
        let mut s = sample_state();
        let mut t = s.tickets.clone();
        t.push(ticket(
            "SCR-20",
            "abandoned",
            "Dropped idea",
            "agent",
            false,
        ));
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        insta::assert_snapshot!(render(&mut s, 110, 12).join("\n"));
    }

    #[test]
    fn board_all_empty() {
        let mut s = State {
            theme: Theme::new(false, false),
            ..State::default()
        };
        insta::assert_snapshot!(render(&mut s, 110, 8).join("\n"));
    }

    #[test]
    fn column_expands_and_collapses_with_tickets() {
        let mut s = sample_state();
        assert!(render(&mut s, 110, 12)[1].contains("abandoned (1)"));
        let mut t = s.tickets.clone();
        t.retain(|t| t.status != "abandoned");
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        let collapsed = render(&mut s, 110, 12)[1].clone();
        assert!(!collapsed.contains("abandoned ("), "{collapsed}");
        let mut t = s.tickets.clone();
        t.push(ticket("SCR-30", "abandoned", "Back again", "agent", false));
        crate::tui::state::update(&mut s, Event::Loaded(Ok(t)));
        assert!(render(&mut s, 110, 12)[1].contains("abandoned (1)"));
    }

    #[test]
    fn empty_board_has_five_columns() {
        let mut s = State {
            who: "human/human".into(),
            ..State::default()
        };
        let rows = render(&mut s, 260, 8);
        assert!(rows[0].contains("all Projects"));
        assert!(rows[0].contains("human/human"));
        assert!(rows[0].contains("0 tickets"));
        for st in STATUSES.iter().copied() {
            assert!(rows[1].contains(st), "{st}");
        }
        assert!(!rows[1].contains("(0)"));
        assert!(rows[7].starts_with("[Enter] open"));
    }

    #[test]
    fn error_shows_above_hints() {
        let mut s = State {
            error: Some("boom".into()),
            ..State::default()
        };
        let rows = render(&mut s, 260, 8);
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
            "description": "Enter opens the detail view.\nEsc closes it.\n\n## Notes\n\n- **bold** item with `code`\n- second item that is long enough to wrap onto another line of the pane\n\n> quoted words\n\n```\nlet x = 1;\n```",
            "plan": "Add a full pane screen."
        }))
        .unwrap();
        crate::tui::state::update(&mut s, Event::DetailLoaded(Ok(Box::new(d))));
        s
    }

    #[test]
    fn detail_sample() {
        let mut s = detail_state();
        insta::assert_snapshot!(render(&mut s, 80, 56).join("\n"));
    }

    #[test]
    fn detail_shows_handoff_only_when_set() {
        let mut s = detail_state();
        let lines = render(&mut s, 60, 80);
        assert!(!lines.iter().any(|l| l.contains("Handoff")), "{lines:?}");
        s.detail.as_mut().unwrap().ticket.handoff = "next: wire the view".into();
        s.detail.as_mut().unwrap().cache = None;
        let lines = render(&mut s, 60, 80);
        assert!(lines.iter().any(|l| l.contains("Handoff")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("wire the view")));
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
    fn detail_cache_reused_until_key_changes() {
        use crate::tui::state::DetailSel;
        let mut s = detail_state();
        assert!(s.detail.as_ref().unwrap().cache.is_none());
        render(&mut s, 60, 12);
        assert!(s.detail.as_ref().unwrap().cache.is_some());

        // Poison the cached lines: a draw that reuses them shows the marker.
        let poison = |s: &mut State| {
            s.detail.as_mut().unwrap().cache.as_mut().unwrap().lines[0] = Line::from("CACHED");
        };
        poison(&mut s);
        assert!(render(&mut s, 60, 12).iter().any(|r| r.contains("CACHED")));
        s.detail.as_mut().unwrap().scroll = 3;
        render(&mut s, 60, 12);
        assert_eq!(
            s.detail.as_ref().unwrap().cache.as_ref().unwrap().lines[0].to_string(),
            "CACHED"
        );

        // A different width rebuilds.
        assert!(!render(&mut s, 70, 12).iter().any(|r| r.contains("CACHED")));
        poison(&mut s);

        // A different selection rebuilds.
        s.detail.as_mut().unwrap().sel = Some(DetailSel::Criterion(0));
        render(&mut s, 70, 12);
        assert_ne!(
            s.detail.as_ref().unwrap().cache.as_ref().unwrap().lines[0].to_string(),
            "CACHED"
        );
        poison(&mut s);

        // A different theme rebuilds.
        s.theme = Theme::new(true, false);
        render(&mut s, 70, 12);
        assert_ne!(
            s.detail.as_ref().unwrap().cache.as_ref().unwrap().lines[0].to_string(),
            "CACHED"
        );

        // A newly loaded ticket starts with no cache.
        let d = s.detail.as_ref().unwrap().ticket.clone();
        crate::tui::state::update(&mut s, Event::DetailLoaded(Ok(Box::new(d))));
        assert!(s.detail.as_ref().unwrap().cache.is_none());
    }

    #[test]
    fn wrap_splits_long_words() {
        assert_eq!(wrap("abcdefgh", "", 3), ["abc", "def", "gh"]);
        assert_eq!(wrap("a b\nc", "> ", 5), ["> a b", "> c"]);
    }

    #[test]
    fn wrap_keeps_indentation_and_inner_spaces() {
        assert_eq!(wrap("    code  x", "", 40), ["    code  x"]);
    }

    #[test]
    fn wrap_drops_tabs_and_cr() {
        let out = wrap("a\tb\r", "", 40);
        assert!(out.iter().all(|l| !l.contains(['\t', '\r'])), "{out:?}");
    }

    #[test]
    fn wrap_long_word_after_indent_has_no_blank_line() {
        let out = wrap("    aaaaaaaaaaaaaaaaaaaa", "", 10);
        assert!(out.iter().all(|l| !l.trim().is_empty()), "{out:?}");
    }

    #[test]
    fn wrap_has_no_trailing_spaces() {
        let out = wrap("aaaa  bbbb", "", 9);
        assert!(out.iter().all(|l| !l.ends_with(' ')), "{out:?}");
        assert_eq!(out, ["aaaa", "bbbb"]);
    }

    #[test]
    fn wrap_wide_char_in_narrow_width_has_no_blank_lines() {
        let out = wrap("\u{4e2d}\u{6587}", "", 1);
        assert!(out.iter().all(|l| !l.is_empty()), "{out:?}");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn item_lines_use_cell_width_of_prefix() {
        // "\u{25cf}" is multibyte (3 bytes) but 1 cell wide.
        let lines = item_lines(
            "\u{25cf} SCR-1 [x] ".to_string(),
            Style::default(),
            "aaaa bbbb",
            22,
        );
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        // prefix is 12 cells, so 2+12 leaves 8 cells: "aaaa bbbb" must wrap.
        assert_eq!(text, ["  \u{25cf} SCR-1 [x] aaaa", "              bbbb"]);
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
    fn board_scroll_offset_survives_frames() {
        let mut s = State {
            theme: Theme::new(false, false),
            ..State::default()
        };
        let tickets = (0..20)
            .map(|i| {
                ticket(
                    &format!("SCR-{i}"),
                    "ready",
                    &format!("item{i:02}"),
                    "agent",
                    false,
                )
            })
            .collect();
        crate::tui::state::update(&mut s, Event::Loaded(Ok(tickets)));
        s.col = 1;
        let first = |rows: &[String]| {
            rows.iter()
                .find_map(|r| r.find("item").map(|i| r[i..i + 6].to_string()))
                .unwrap()
        };
        for _ in 0..12 {
            key(&mut s, 'j');
            render(&mut s, 120, 10);
        }
        let before = first(&render(&mut s, 120, 10));
        key(&mut s, 'k');
        let after = first(&render(&mut s, 120, 10));
        assert_eq!(before, after);
        assert_ne!(before, "item00");
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
    fn prompt_replaces_hints_line() {
        let mut s = sample_state();
        key(&mut s, 'e');
        key(&mut s, '!');
        insta::assert_snapshot!(render(&mut s, 90, 14).join("\n"));
    }

    #[test]
    fn confirm_popup_sample() {
        let mut s = sample_state();
        key(&mut s, 'l');
        key(&mut s, 'x');
        insta::assert_snapshot!(render(&mut s, 90, 14).join("\n"));
    }

    #[test]
    fn selection_is_marked_and_scroll_follows_it() {
        let mut s = detail_state();
        s.detail.as_mut().unwrap().scroll = usize::MAX;
        render(&mut s, 60, 12);
        key(&mut s, ']');
        let rows = render(&mut s, 60, 12);
        assert!(
            rows.iter()
                .any(|r| r.contains("> [x] 1. Renders every field")),
            "{rows:?}"
        );
        key(&mut s, ']');
        key(&mut s, ']');
        key(&mut s, ']');
        let rows = render(&mut s, 60, 12);
        assert!(rows.iter().any(|r| r.contains("bob")), "{rows:?}");
        assert!(rows.iter().any(|r| r.contains("pane edge")), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("> [x]")));
    }

    #[test]
    fn selected_multiline_markdown_comment_spans_its_rendered_lines() {
        let mut s = detail_state();
        {
            let d = s.detail.as_mut().unwrap();
            d.ticket.comments[1].body =
                "- one **two** three four five six seven\n- eight\n\n```\ncode\n```".into();
        }
        for _ in 0..4 {
            key(&mut s, ']');
        }
        let d = s.detail.as_ref().unwrap();
        let (lines, sel) = detail_lines(d, 30, &s.theme);
        let (start, end) = sel.unwrap();
        assert_eq!(end, lines.len(), "comment is last, so it runs to the end");
        let text: Vec<String> = lines[start..end].iter().map(|l| l.to_string()).collect();
        // header, wrapped item, second item, gap, code
        assert_eq!(text.len(), 7, "{text:?}");
        assert!(text[0].starts_with("> "), "{text:?}");
        assert_eq!(text[6].trim(), "code");
    }

    #[test]
    fn picker_popup_sample() {
        let mut s = detail_state();
        key(&mut s, 'b');
        key(&mut s, 'r');
        insta::assert_snapshot!(render(&mut s, 90, 24).join("\n"));
    }

    fn decisions_state() -> State {
        let mut s = sample_state();
        let d = |n: i64, q: &str, rec: Option<&str>, unblocks: i64, tickets: &[&str]| {
            serde_json::from_value(serde_json::json!({
                "created_at": "", "id": format!("D-{n}"), "number": n,
                "options": ["Keep sqlite", "Move to postgres"],
                "project": {"key": "SCR", "root": "/r"},
                "question": q, "recommendation": rec,
                "tickets": tickets, "unblocks": unblocks
            }))
            .unwrap()
        };
        crate::tui::state::update(
            &mut s,
            Event::DecisionsLoaded(Ok(vec![
                d(
                    4,
                    "Which store backs the cache?",
                    Some("Keep sqlite"),
                    3,
                    &["SCR-7", "SCR-8"],
                ),
                d(2, "Should the export be JSON or CSV?", None, 1, &["SCR-6"]),
            ])),
        );
        s.back_view = View::Board;
        s.view = View::Decisions;
        s
    }

    #[test]
    fn decisions_sample() {
        let mut s = decisions_state();
        insta::assert_snapshot!(render(&mut s, 90, 24).join("\n"));
    }

    #[test]
    fn pending_decisions_banner_on_board() {
        let mut s = decisions_state();
        s.view = View::Board;
        let rows = render(&mut s, 90, 24);
        assert!(
            rows[1].contains("2 OPEN DECISIONS NEED A HUMAN ANSWER"),
            "{}",
            rows[1]
        );
        assert!(rows[1].contains("blocking 3 tickets"));
        assert!(rows[1].contains("press W"));
        insta::assert_snapshot!(rows.join("\n"));
    }

    #[test]
    fn banner_is_inverse_bold_so_it_reads_without_color() {
        let mut s = decisions_state();
        s.view = View::Board;
        let mut t = Terminal::new(TestBackend::new(90, 24)).unwrap();
        t.draw(|f| draw(f, &mut s)).unwrap();
        let cell = &t.backend().buffer()[(0, 1)];
        assert!(
            cell.modifier
                .contains(ratatui::style::Modifier::REVERSED | ratatui::style::Modifier::BOLD)
        );
    }

    #[test]
    fn no_banner_without_decisions_or_inside_the_decisions_view() {
        let mut s = decisions_state();
        assert!(
            !render(&mut s, 90, 24)
                .iter()
                .any(|r| r.contains("NEED A HUMAN ANSWER"))
        );
        s.view = View::Board;
        crate::tui::state::update(&mut s, Event::DecisionsLoaded(Ok(vec![])));
        assert!(
            !render(&mut s, 90, 24)
                .iter()
                .any(|r| r.contains("NEED A HUMAN ANSWER"))
        );
    }

    #[test]
    fn decisions_empty() {
        let mut s = decisions_state();
        crate::tui::state::update(&mut s, Event::DecisionsLoaded(Ok(vec![])));
        insta::assert_snapshot!(render(&mut s, 90, 24).join("\n"));
    }
}
