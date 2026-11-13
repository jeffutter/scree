//! The TUI's model and its pure transition function. Nothing here touches the
//! terminal or the database, so key handling is tested with scripted events.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::theme::Theme;
use crate::model::Ticket;
use crate::scope::Scope;

/// Board columns, left to right.
pub const STATUSES: [&str; 5] = ["draft", "ready", "active", "done", "abandoned"];
/// Index of the first closed column (done); it and later ones hide together.
const FIRST_CLOSED: usize = 3;

/// How wide the board reads: this Worktree, its whole Project, or every Project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeMode {
    Worktree,
    Project,
    AllProjects,
}

impl ScopeMode {
    /// The mode a caller starts in: as narrow as its cwd allows.
    pub fn initial(scope: &Scope) -> Self {
        match scope {
            Scope::Worktree { .. } => ScopeMode::Worktree,
            Scope::Checkout { .. } => ScopeMode::Project,
            Scope::Unscoped => ScopeMode::AllProjects,
        }
    }

    /// Modes the cwd can satisfy, in cycling order. Worktree needs a linked
    /// worktree and Project needs a repository.
    fn available(scope: &Scope) -> Vec<ScopeMode> {
        let mut v = vec![];
        if matches!(scope, Scope::Worktree { .. }) {
            v.push(ScopeMode::Worktree);
        }
        if !matches!(scope, Scope::Unscoped) {
            v.push(ScopeMode::Project);
        }
        v.push(ScopeMode::AllProjects);
        v
    }
}

/// What a reload asks the ticket library for; State owns it so the runner
/// does not guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSpec {
    pub mode: ScopeMode,
    pub show_archived: bool,
}

/// Which presentation of the tickets is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Board,
    List,
}

/// One line of the list view, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListEntry {
    /// A status group heading (index into STATUSES).
    Header(usize),
    /// A ticket (index into `State::tickets`), indented `depth` levels.
    Ticket { idx: usize, depth: usize },
}

#[derive(Debug)]
pub struct State {
    pub should_quit: bool,
    /// "role/author" shown in the header.
    pub who: String,
    /// Where the process was started; the scope modes are derived from it.
    pub scope: Scope,
    pub mode: ScopeMode,
    pub theme: Theme,
    pub tickets: Vec<Ticket>,
    /// The last load error, shown in the footer.
    pub error: Option<String>,
    /// Selected column (index into STATUSES) and the selected row of each.
    pub col: usize,
    pub rows: [usize; 5],
    /// Survives reloads: the selection is found again by id.
    pub selected_id: Option<String>,
    pub hide_closed: bool,
    pub show_archived: bool,
    pub view: View,
    /// Rows a PgUp/PgDn moves; set from the terminal height.
    pub page: usize,
}

impl Default for State {
    fn default() -> Self {
        State {
            should_quit: false,
            who: String::new(),
            scope: Scope::Unscoped,
            mode: ScopeMode::AllProjects,
            theme: Theme::default(),
            tickets: vec![],
            error: None,
            col: 0,
            rows: [0; 5],
            selected_id: None,
            hide_closed: false,
            show_archived: false,
            view: View::Board,
            page: 10,
        }
    }
}

impl State {
    /// A state started in `scope`, in the narrowest mode it supports.
    pub fn starting_in(scope: Scope, who: String, theme: Theme) -> Self {
        State {
            mode: ScopeMode::initial(&scope),
            scope,
            who,
            theme,
            ..State::default()
        }
    }

    pub fn load_spec(&self) -> LoadSpec {
        LoadSpec {
            mode: self.mode,
            show_archived: self.show_archived,
        }
    }

    /// The scope the header names: the started scope narrowed or widened to `mode`.
    pub fn display_scope(&self) -> Scope {
        match (self.mode, &self.scope) {
            (ScopeMode::AllProjects, _) | (_, Scope::Unscoped) => Scope::Unscoped,
            (ScopeMode::Project, Scope::Checkout { root } | Scope::Worktree { root, .. }) => {
                Scope::Checkout { root: root.clone() }
            }
            (ScopeMode::Worktree, s) => s.clone(),
        }
    }

    /// Tickets of one column, in list order.
    pub fn column(&self, col: usize) -> Vec<&Ticket> {
        self.tickets
            .iter()
            .filter(|t| t.status == STATUSES[col])
            .collect()
    }

    /// Column indexes on screen.
    pub fn visible_columns(&self) -> Vec<usize> {
        let n = if self.hide_closed {
            FIRST_CLOSED
        } else {
            STATUSES.len()
        };
        (0..n).collect()
    }

    pub fn visible_count(&self) -> usize {
        self.visible_columns()
            .into_iter()
            .map(|c| self.column(c).len())
            .sum()
    }

    /// The list view's lines: per visible status a header, then its tickets
    /// with subtasks indented under a parent in the same group. A subtask
    /// whose parent has another status (or is not loaded) stands at depth 0
    /// in its own group. Draw and navigation both read this.
    pub fn list_order(&self) -> Vec<ListEntry> {
        fn push(
            out: &mut Vec<ListEntry>,
            members: &[usize],
            tickets: &[Ticket],
            idx: usize,
            depth: usize,
        ) {
            out.push(ListEntry::Ticket { idx, depth });
            for &m in members {
                if tickets[m].parent.as_deref() == Some(tickets[idx].id.as_str()) {
                    push(out, members, tickets, m, depth + 1);
                }
            }
        }
        let mut out = vec![];
        for c in self.visible_columns() {
            let members: Vec<usize> = (0..self.tickets.len())
                .filter(|&i| self.tickets[i].status == STATUSES[c])
                .collect();
            out.push(ListEntry::Header(c));
            for &i in &members {
                let nested = self.tickets[i]
                    .parent
                    .as_deref()
                    .is_some_and(|p| members.iter().any(|&m| self.tickets[m].id == p && m != i));
                if !nested {
                    push(&mut out, &members, &self.tickets, i, 0);
                }
            }
        }
        out
    }

    /// Position of the selected ticket within `list_order()`.
    pub fn list_selected(&self) -> Option<usize> {
        let id = self.selected_id.as_deref()?;
        self.list_order()
            .iter()
            .position(|e| matches!(e, ListEntry::Ticket { idx, .. } if self.tickets[*idx].id == id))
    }

    /// Move over the list's selectable rows (tickets, not headers).
    fn move_list(&mut self, f: impl FnOnce(usize, usize) -> usize) {
        let ids: Vec<String> = self
            .list_order()
            .into_iter()
            .filter_map(|e| match e {
                ListEntry::Ticket { idx, .. } => Some(self.tickets[idx].id.clone()),
                ListEntry::Header(_) => None,
            })
            .collect();
        if !ids.is_empty() {
            let cur = self
                .selected_id
                .as_ref()
                .and_then(|id| ids.iter().position(|i| i == id))
                .unwrap_or(0);
            self.selected_id = Some(ids[f(cur, ids.len()).min(ids.len() - 1)].clone());
        }
        self.reselect();
    }

    /// Clamp every column's row to its length, then record the selected id.
    fn settle(&mut self) {
        for c in 0..STATUSES.len() {
            let len = self.column(c).len();
            self.rows[c] = self.rows[c].min(len.saturating_sub(1));
        }
        self.selected_id = self
            .column(self.col)
            .get(self.rows[self.col])
            .map(|t| t.id.clone());
    }

    fn move_col(&mut self, delta: isize) {
        if self.view == View::List {
            return;
        }
        let vis = self.visible_columns();
        let pos = vis.iter().position(|&c| c == self.col).unwrap_or(0) as isize;
        self.col = vis[(pos + delta).clamp(0, vis.len() as isize - 1) as usize];
        self.settle();
    }

    fn move_row(&mut self, f: impl FnOnce(usize, usize) -> usize) {
        if self.view == View::List {
            return self.move_list(f);
        }
        let len = self.column(self.col).len();
        if len > 0 {
            self.rows[self.col] = f(self.rows[self.col], len).min(len - 1);
        }
        self.settle();
    }

    /// Point col/row at `selected_id` if it is still loaded; otherwise keep
    /// the old position, clamped.
    fn reselect(&mut self) {
        if let Some(id) = self.selected_id.clone() {
            for c in self.visible_columns() {
                if let Some(r) = self.column(c).iter().position(|t| t.id == id) {
                    self.col = c;
                    self.rows[c] = r;
                    break;
                }
            }
        }
        self.settle();
    }

    fn cycle_scope(&mut self) -> bool {
        let modes = ScopeMode::available(&self.scope);
        let pos = modes.iter().position(|&m| m == self.mode);
        let next = modes[pos.map_or(0, |p| (p + 1) % modes.len())];
        let changed = next != self.mode;
        self.mode = next;
        changed
    }
}

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    /// The terminal's new height in rows.
    Resize(u16),
    Loaded(Result<Vec<Ticket>, String>),
}

/// Work `update` asks the runner to do; the runner feeds the result back as an Event.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Reload(LoadSpec),
}

pub fn update(state: &mut State, event: Event) -> Vec<Effect> {
    match event {
        Event::Key(k) if k.kind != KeyEventKind::Release => on_key(state, k),
        Event::Key(_) => vec![],
        Event::Resize(h) => {
            // Header, footer and the pane borders take the rest.
            state.page = usize::from(h).saturating_sub(5).max(1);
            vec![]
        }
        Event::Loaded(Ok(tickets)) => {
            state.tickets = tickets;
            state.error = None;
            state.reselect();
            vec![]
        }
        Event::Loaded(Err(e)) => {
            state.error = Some(e);
            vec![]
        }
    }
}

fn on_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let page = state.page;
    let half = (page / 2).max(1);
    match (k.code, ctrl) {
        (KeyCode::Char('c'), true) | (KeyCode::Char('q'), false) => state.should_quit = true,
        (KeyCode::Left | KeyCode::Char('h'), false) => state.move_col(-1),
        (KeyCode::Right | KeyCode::Char('l'), false) => state.move_col(1),
        (KeyCode::Up | KeyCode::Char('k'), false) => state.move_row(|r, _| r.saturating_sub(1)),
        (KeyCode::Down | KeyCode::Char('j'), false) => state.move_row(|r, _| r + 1),
        (KeyCode::PageUp, _) => state.move_row(|r, _| r.saturating_sub(page)),
        (KeyCode::PageDown, _) => state.move_row(|r, _| r + page),
        (KeyCode::Char('u'), true) => state.move_row(|r, _| r.saturating_sub(half)),
        (KeyCode::Char('d'), true) => state.move_row(|r, _| r + half),
        (KeyCode::Home | KeyCode::Char('g'), false) => state.move_row(|_, _| 0),
        (KeyCode::End | KeyCode::Char('G'), false) => state.move_row(|_, len| len - 1),
        (KeyCode::Tab | KeyCode::BackTab, _) => {
            state.view = match state.view {
                View::Board => View::List,
                View::List => View::Board,
            };
            state.reselect();
        }
        (KeyCode::Char('c'), false) => {
            state.hide_closed = !state.hide_closed;
            if state.hide_closed && state.col >= FIRST_CLOSED {
                state.col = FIRST_CLOSED - 1;
            }
            state.settle();
        }
        (KeyCode::Char('a'), false) => {
            state.show_archived = !state.show_archived;
            return vec![Effect::Reload(state.load_spec())];
        }
        (KeyCode::Char('s'), false) if state.cycle_scope() => {
            return vec![Effect::Reload(state.load_spec())];
        }
        _ => {}
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventState;

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        })
    }
    fn press(c: char) -> Event {
        key(KeyCode::Char(c), KeyModifiers::NONE, KeyEventKind::Press)
    }

    /// Feed scripted events; return the state and every effect asked for.
    fn drive(events: Vec<Event>) -> (State, Vec<Effect>) {
        let mut s = State::default();
        let mut effects = vec![];
        for e in events {
            effects.extend(update(&mut s, e));
        }
        (s, effects)
    }

    #[test]
    fn q_quits() {
        let (s, fx) = drive(vec![press('q')]);
        assert!(s.should_quit);
        assert!(fx.is_empty());
    }

    #[test]
    fn ctrl_c_quits() {
        let (s, _) = drive(vec![key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        )]);
        assert!(s.should_quit);
    }

    #[test]
    fn other_keys_do_nothing() {
        let (s, fx) = drive(vec![
            press('x'),
            key(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press),
            Event::Resize(30),
        ]);
        assert!(!s.should_quit);
        assert!(fx.is_empty());
    }

    #[test]
    fn release_is_ignored() {
        let (s, _) = drive(vec![key(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )]);
        assert!(!s.should_quit);
    }

    #[test]
    fn scripted_sequence_quits_at_q() {
        let (s, _) = drive(vec![press('j'), press('k'), press('q')]);
        assert!(s.should_quit);
    }

    #[test]
    fn load_error_is_kept_then_cleared() {
        let mut s = State::default();
        update(&mut s, Event::Loaded(Err("boom".into())));
        assert_eq!(s.error.as_deref(), Some("boom"));
        update(&mut s, Event::Loaded(Ok(vec![])));
        assert_eq!(s.error, None);
    }

    use crate::model::Ticket;
    use std::path::PathBuf;

    fn ticket(id: &str, status: &str) -> Ticket {
        serde_json::from_value(serde_json::json!({
            "archived_at": null, "available": false, "blocked": false, "blocked_by": [],
            "claimed_by": null, "created_at": "", "done_at": null, "for": "agent",
            "id": id, "parent": null, "status": status, "subtasks": [],
            "title": format!("title {id}"), "updated_at": "", "worktree": null
        }))
        .unwrap()
    }

    /// draft: D1; ready: R1 R2 R3; active: none; done: X1 X2; abandoned: A1.
    fn sample() -> Vec<Ticket> {
        vec![
            ticket("D1", "draft"),
            ticket("R1", "ready"),
            ticket("R2", "ready"),
            ticket("R3", "ready"),
            ticket("X1", "done"),
            ticket("X2", "done"),
            ticket("A1", "abandoned"),
        ]
    }

    fn loaded(extra: Vec<Event>) -> State {
        let mut s = State::default();
        update(&mut s, Event::Loaded(Ok(sample())));
        for e in extra {
            update(&mut s, e);
        }
        s
    }

    fn code(c: KeyCode) -> Event {
        key(c, KeyModifiers::NONE, KeyEventKind::Press)
    }
    fn rep(c: KeyCode, n: usize) -> Vec<Event> {
        (0..n).map(|_| code(c)).collect()
    }
    fn ctrl(c: char) -> Event {
        key(KeyCode::Char(c), KeyModifiers::CONTROL, KeyEventKind::Press)
    }

    #[test]
    fn hjkl_and_arrows_move_without_wrapping() {
        let s = loaded(vec![press('h'), code(KeyCode::Up)]);
        assert_eq!((s.col, s.rows[0]), (0, 0));
        let s = loaded(vec![press('l'), press('j'), code(KeyCode::Down)]);
        assert_eq!((s.col, s.rows[1]), (1, 2));
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        let s = loaded(vec![
            press('l'),
            press('j'),
            press('j'),
            press('j'),
            press('k'),
        ]);
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        let s = loaded(rep(KeyCode::Right, 9));
        assert_eq!(s.col, 4);
        let s = loaded(vec![code(KeyCode::Right), code(KeyCode::Left)]);
        assert_eq!(s.col, 0);
    }

    #[test]
    fn row_clamps_on_shorter_column() {
        let s = loaded(vec![
            press('l'),
            press('j'),
            press('j'),
            press('l'),
            press('l'),
            press('h'),
        ]);
        // Active column is empty; moving back to ready keeps row 2.
        assert_eq!(s.col, 2);
        assert_eq!(s.selected_id, None);
        let s = loaded(vec![
            press('l'),
            press('j'),
            press('j'),
            press('l'),
            press('l'),
            press('l'),
            press('l'),
        ]);
        assert_eq!(s.col, 4);
        assert_eq!(s.rows[4], 0);
        assert_eq!(s.selected_id.as_deref(), Some("A1"));
    }

    #[test]
    fn paging_home_end() {
        let mut s = loaded(vec![Event::Resize(8), press('l')]);
        assert_eq!(s.page, 3);
        update(&mut s, code(KeyCode::PageDown));
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        update(&mut s, code(KeyCode::PageUp));
        assert_eq!(s.selected_id.as_deref(), Some("R1"));
        update(&mut s, ctrl('d'));
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        update(&mut s, ctrl('u'));
        assert_eq!(s.selected_id.as_deref(), Some("R1"));
        update(&mut s, code(KeyCode::End));
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        update(&mut s, code(KeyCode::Home));
        assert_eq!(s.selected_id.as_deref(), Some("R1"));
        update(&mut s, press('G'));
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        update(&mut s, press('g'));
        assert_eq!(s.selected_id.as_deref(), Some("R1"));
        assert!(!s.should_quit);
    }

    #[test]
    fn hide_closed_skips_columns_and_moves_selection() {
        let mut s = loaded(rep(KeyCode::Right, 3));
        assert_eq!(s.col, 3);
        let fx = update(&mut s, press('c'));
        assert!(fx.is_empty());
        assert!(s.hide_closed);
        assert_eq!(s.col, 2);
        assert_eq!(s.visible_count(), 4);
        update(&mut s, code(KeyCode::Right));
        assert_eq!(s.col, 2);
        update(&mut s, press('c'));
        assert!(!s.hide_closed);
        assert_eq!(s.visible_count(), 7);
    }

    #[test]
    fn archived_toggle_reloads() {
        let mut s = State::default();
        let fx = update(&mut s, press('a'));
        assert!(s.show_archived);
        assert_eq!(
            fx,
            vec![Effect::Reload(LoadSpec {
                mode: ScopeMode::AllProjects,
                show_archived: true
            })]
        );
        let fx = update(&mut s, press('a'));
        assert!(!s.show_archived);
        assert_eq!(fx.len(), 1);
    }

    fn worktree_scope() -> Scope {
        Scope::Worktree {
            root: PathBuf::from("/r/proj"),
            worktree: PathBuf::from("/r/wt"),
        }
    }

    #[test]
    fn scope_cycles_and_header_follows() {
        let mut s = State::starting_in(worktree_scope(), String::new(), Theme::default());
        assert_eq!(s.mode, ScopeMode::Worktree);
        assert_eq!(s.display_scope(), worktree_scope());
        let fx = update(&mut s, press('s'));
        assert_eq!(s.mode, ScopeMode::Project);
        assert_eq!(fx, vec![Effect::Reload(s.load_spec())]);
        assert_eq!(
            s.display_scope(),
            Scope::Checkout {
                root: PathBuf::from("/r/proj")
            }
        );
        update(&mut s, press('s'));
        assert_eq!(s.mode, ScopeMode::AllProjects);
        assert_eq!(s.display_scope(), Scope::Unscoped);
        update(&mut s, press('s'));
        assert_eq!(s.mode, ScopeMode::Worktree);
    }

    #[test]
    fn scope_skips_modes_the_cwd_cannot_satisfy() {
        let checkout = Scope::Checkout {
            root: PathBuf::from("/r/proj"),
        };
        let mut s = State::starting_in(checkout, String::new(), Theme::default());
        assert_eq!(s.mode, ScopeMode::Project);
        update(&mut s, press('s'));
        assert_eq!(s.mode, ScopeMode::AllProjects);
        update(&mut s, press('s'));
        assert_eq!(s.mode, ScopeMode::Project);

        let mut s = State::starting_in(Scope::Unscoped, String::new(), Theme::default());
        assert_eq!(s.mode, ScopeMode::AllProjects);
        assert!(update(&mut s, press('s')).is_empty());
        assert_eq!(s.mode, ScopeMode::AllProjects);
    }

    #[test]
    fn selection_survives_reload_by_id() {
        let mut s = loaded(vec![press('l'), press('j')]);
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        // R2 moves to active and a new ready ticket appears first.
        let t = vec![
            ticket("R0", "ready"),
            ticket("R1", "ready"),
            ticket("R2", "active"),
        ];
        update(&mut s, Event::Loaded(Ok(t)));
        assert_eq!((s.col, s.rows[2]), (2, 0));
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
    }

    #[test]
    fn selection_clamps_when_its_ticket_vanishes() {
        let mut s = loaded(vec![press('l'), press('j'), press('j')]);
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        update(
            &mut s,
            Event::Loaded(Ok(vec![ticket("R1", "ready"), ticket("R2", "ready")])),
        );
        assert_eq!((s.col, s.rows[1]), (1, 1));
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
    }

    fn child(id: &str, status: &str, parent: &str) -> Ticket {
        let mut t = ticket(id, status);
        t.parent = Some(parent.to_string());
        t
    }

    fn list_ids(s: &State) -> Vec<String> {
        s.list_order()
            .into_iter()
            .map(|e| match e {
                ListEntry::Header(c) => format!("#{}", STATUSES[c]),
                ListEntry::Ticket { idx, depth } => {
                    format!("{}{}", "-".repeat(depth), s.tickets[idx].id)
                }
            })
            .collect()
    }

    #[test]
    fn tab_toggles_view_and_keeps_selection_and_mode() {
        let mut s = loaded(vec![press('l'), press('j')]);
        s.mode = ScopeMode::Project;
        update(&mut s, code(KeyCode::Tab));
        assert_eq!(s.view, View::List);
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        assert_eq!(s.mode, ScopeMode::Project);
        update(&mut s, press('j'));
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        let fx = update(&mut s, code(KeyCode::Tab));
        assert!(fx.is_empty());
        assert_eq!(s.view, View::Board);
        assert_eq!((s.col, s.rows[1]), (1, 2));
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
    }

    #[test]
    fn list_selection_survives_reload() {
        let mut s = loaded(vec![code(KeyCode::Tab), press('j'), press('j')]);
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        update(
            &mut s,
            Event::Loaded(Ok(vec![ticket("R0", "ready"), ticket("R2", "ready")])),
        );
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        assert_eq!(s.list_selected(), Some(3));
    }

    #[test]
    fn list_groups_by_status_with_subtasks_indented() {
        let mut s = State::default();
        update(
            &mut s,
            Event::Loaded(Ok(vec![
                ticket("R1", "ready"),
                child("R1a", "ready", "R1"),
                child("R1b", "ready", "R1"),
                ticket("X1", "done"),
                child("X1a", "done", "X1"),
                child("D1a", "draft", "R1"),
            ])),
        );
        assert_eq!(
            list_ids(&s),
            [
                "#draft",
                "D1a",
                "#ready",
                "R1",
                "-R1a",
                "-R1b",
                "#active",
                "#done",
                "X1",
                "-X1a",
                "#abandoned"
            ]
        );
    }

    #[test]
    fn list_navigation_skips_headers() {
        let mut s = loaded(vec![code(KeyCode::Tab)]);
        assert_eq!(s.selected_id.as_deref(), Some("D1"));
        update(&mut s, press('j'));
        assert_eq!(s.selected_id.as_deref(), Some("R1"));
        update(&mut s, press('k'));
        assert_eq!(s.selected_id.as_deref(), Some("D1"));
        update(&mut s, press('G'));
        assert_eq!(s.selected_id.as_deref(), Some("A1"));
        update(&mut s, press('g'));
        assert_eq!(s.selected_id.as_deref(), Some("D1"));
        update(&mut s, press('l'));
        assert_eq!(s.selected_id.as_deref(), Some("D1"));
    }

    #[test]
    fn hide_closed_hides_closed_groups_in_list() {
        let mut s = loaded(vec![code(KeyCode::Tab), press('c')]);
        let ids = list_ids(&s);
        assert!(!ids.contains(&"#done".to_string()));
        assert!(!ids.contains(&"X1".to_string()));
        assert!(ids.contains(&"#active".to_string()));
        update(&mut s, press('c'));
        assert!(list_ids(&s).contains(&"#abandoned".to_string()));
    }
}
