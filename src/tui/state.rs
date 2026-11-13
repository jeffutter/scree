//! The TUI's model and its pure transition function. Nothing here touches the
//! terminal or the database, so key handling is tested with scripted events.

use std::collections::HashMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::theme::Theme;
use super::widgets::{Prompt, PromptOutcome};
use crate::model::{Ticket, TicketDetail};
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

/// The open detail view: the full ticket and how far it is scrolled. `scroll`
/// may overshoot (G sets it to usize::MAX); the view clamps it when it knows
/// the wrapped height.
#[derive(Debug, Clone)]
pub struct Detail {
    pub ticket: TicketDetail,
    pub scroll: usize,
    /// The criterion or comment the criteria keys act on.
    pub sel: Option<DetailSel>,
    /// Set when `sel` changed; the view scrolls it into range and clears it.
    pub reveal: bool,
}

/// A selectable item of the detail view; indexes into the ticket's
/// criteria or comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailSel {
    Criterion(usize),
    Comment(usize),
}

impl Detail {
    fn new(ticket: TicketDetail) -> Self {
        Detail {
            ticket,
            scroll: 0,
            sel: None,
            reveal: false,
        }
    }

    /// Position of `sel` over criteria then comments.
    fn linear(&self) -> Option<usize> {
        match self.sel? {
            DetailSel::Criterion(i) => Some(i),
            DetailSel::Comment(i) => Some(self.ticket.criteria.len() + i),
        }
    }

    /// Move the selection one item along criteria then comments, without
    /// wrapping. From no selection, forward picks the first item and
    /// backward the last.
    fn step(&mut self, forward: bool) {
        let nc = self.ticket.criteria.len();
        let total = nc + self.ticket.comments.len();
        if total == 0 {
            return;
        }
        let next = match (self.linear(), forward) {
            (None, true) => 0,
            (None, false) => total - 1,
            (Some(i), true) => (i + 1).min(total - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.sel = Some(if next < nc {
            DetailSel::Criterion(next)
        } else {
            DetailSel::Comment(next - nc)
        });
        self.reveal = true;
    }

    /// Keep `sel` inside the lists after a reload shrank them.
    fn clamp_sel(&mut self) {
        let (nc, nm) = (self.ticket.criteria.len(), self.ticket.comments.len());
        self.sel = match self.sel {
            Some(DetailSel::Criterion(i)) if nc > 0 => Some(DetailSel::Criterion(i.min(nc - 1))),
            Some(DetailSel::Comment(i)) if nm > 0 => Some(DetailSel::Comment(i.min(nm - 1))),
            _ => None,
        };
    }
}

/// A lifecycle change a key asks for on the selected ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Ready,
    Start,
    Release,
    Done {
        force: bool,
    },
    Abandon,
    Reopen,
    /// `archived` is the state to move the ticket to.
    Archive {
        archived: bool,
    },
}

/// One action offered for a ticket: key, footer label, what it does.
pub type Offer = (&'static str, &'static str, Lifecycle);

/// The single source of truth for which lifecycle keys apply to a ticket:
/// the footer lists them and the key handler ignores the rest.
pub fn allowed(status: &str, archived: bool) -> Vec<Offer> {
    let open = matches!(status, "draft" | "ready" | "active");
    let mut v = vec![];
    match status {
        "draft" => v.push(("r", "ready", Lifecycle::Ready)),
        "ready" => v.push(("S", "start", Lifecycle::Start)),
        "active" => v.push(("R", "release", Lifecycle::Release)),
        _ => {}
    }
    if open {
        v.push(("d", "done", Lifecycle::Done { force: false }));
        v.push(("D", "force done", Lifecycle::Done { force: true }));
        v.push(("x", "abandon", Lifecycle::Abandon));
    } else {
        v.push(("o", "reopen", Lifecycle::Reopen));
    }
    v.push((
        "A",
        if archived { "unarchive" } else { "archive" },
        Lifecycle::Archive {
            archived: !archived,
        },
    ));
    v
}

/// A pending y/n question before a destructive action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub id: String,
    pub action: ConfirmAction,
    pub prompt: String,
}

/// What a confirm popup runs when answered yes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    Lifecycle(Lifecycle),
    RemoveCriterion { number: i64 },
    RemoveComment { comment_id: i64 },
}

/// A ticket text field the prompt or $EDITOR edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    Description,
    Plan,
    /// A new comment: the editor opens on an empty file.
    Comment,
}

/// What an open prompt will edit when submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptField {
    Title,
    /// Optional evidence for checking criterion `number`.
    Evidence {
        number: i64,
    },
    /// Text of a new criterion.
    Criterion,
    /// Body of a new comment.
    Comment,
}

/// An open one-line prompt and the ticket it edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptState {
    pub id: String,
    pub field: PromptField,
    /// The text the prompt was seeded with, to detect "unchanged".
    pub seed: String,
    pub prompt: Prompt,
}

/// Whether the dependency picker adds or removes a blocker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    Add,
    Remove,
}

/// An open filterable ticket list choosing a blocker for `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub id: String,
    pub mode: PickerMode,
    /// Case-insensitive substring filter over "id title".
    pub filter: String,
    /// Every candidate as (id, title), before filtering.
    pub candidates: Vec<(String, String)>,
    /// Index into `visible()`.
    pub sel: usize,
}

impl Picker {
    /// The candidates matching the filter, in order.
    pub fn visible(&self) -> Vec<&(String, String)> {
        let f = self.filter.to_lowercase();
        self.candidates
            .iter()
            .filter(|(id, title)| format!("{id} {title}").to_lowercase().contains(&f))
            .collect()
    }
}

/// Which tickets the blocked/available filter keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Avail {
    Blocked,
    Available,
}

/// Client-side narrowing of the loaded tickets. Every field that is set must
/// match (AND).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Case-insensitive substring over id, title and description.
    pub query: String,
    /// Index into STATUSES.
    pub status: Option<usize>,
    /// "agent" or "human".
    pub for_who: Option<String>,
    pub avail: Option<Avail>,
}

impl Filter {
    pub fn is_active(&self) -> bool {
        *self != Filter::default()
    }
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
    /// While Some, the detail view replaces the board or list.
    pub detail: Option<Detail>,
    /// The last action's success message, shown in the status line.
    pub notice: Option<String>,
    /// While Some, a modal popup owns the keyboard.
    pub confirm: Option<Confirm>,
    /// While Some, the footer prompt owns the keyboard.
    pub prompt: Option<PromptState>,
    /// While Some, the dependency picker owns the keyboard.
    pub picker: Option<Picker>,
    pub filter: Filter,
    /// Ticket id to description, for searching; filled on demand.
    pub descriptions: HashMap<String, String>,
    /// While Some, the search input owns the keyboard.
    pub search: Option<Prompt>,
    /// While true, the filter popup owns the keyboard.
    pub filter_menu: bool,
    /// The key help popup is open; it swallows every key but its closers.
    pub help: bool,
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
            detail: None,
            notice: None,
            confirm: None,
            prompt: None,
            picker: None,
            filter: Filter::default(),
            descriptions: HashMap::new(),
            search: None,
            filter_menu: false,
            help: false,
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

    /// The ticket lifecycle keys act on: the open detail's, otherwise the
    /// board or list selection. Returns (id, status, archived).
    pub fn acted_ticket(&self) -> Option<(String, String, bool)> {
        if let Some(d) = &self.detail {
            let t = &d.ticket.ticket;
            return Some((t.id.clone(), t.status.clone(), t.archived_at.is_some()));
        }
        let id = self.selected_id.as_deref()?;
        let t = self.tickets.iter().find(|t| t.id == id)?;
        Some((t.id.clone(), t.status.clone(), t.archived_at.is_some()))
    }

    /// Title of the ticket the keys act on (same lookup as `acted_ticket`).
    fn acted_title(&self) -> Option<String> {
        if let Some(d) = &self.detail {
            return Some(d.ticket.ticket.title.clone());
        }
        let id = self.selected_id.as_deref()?;
        self.tickets
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.title.clone())
    }

    /// Whether `t` passes the filter.
    pub fn matches(&self, t: &Ticket) -> bool {
        let f = &self.filter;
        if f.status.is_some_and(|c| t.status != STATUSES[c]) {
            return false;
        }
        if f.for_who.as_ref().is_some_and(|w| t.for_who != *w) {
            return false;
        }
        match f.avail {
            Some(Avail::Blocked) if !t.blocked => return false,
            Some(Avail::Available) if !t.available => return false,
            _ => {}
        }
        if f.query.is_empty() {
            return true;
        }
        let q = f.query.to_lowercase();
        t.id.to_lowercase().contains(&q)
            || t.title.to_lowercase().contains(&q)
            || self
                .descriptions
                .get(&t.id)
                .is_some_and(|d| d.to_lowercase().contains(&q))
    }

    /// Tickets of one column that pass the filter, in list order.
    pub fn column(&self, col: usize) -> Vec<&Ticket> {
        self.tickets
            .iter()
            .filter(|t| t.status == STATUSES[col] && self.matches(t))
            .collect()
    }

    /// After the filter changed: keep the selection on a visible ticket.
    fn refilter(&mut self) {
        if self.view == View::List {
            let ids = self.nav_ids();
            if !self.selected_id.as_ref().is_some_and(|s| ids.contains(s)) {
                self.selected_id = ids.first().cloned();
            }
        }
        self.reselect();
    }

    /// Whether descriptions are needed to evaluate the filter.
    fn wants_descriptions(&self) -> bool {
        !self.filter.query.is_empty()
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
                .filter(|&i| {
                    self.tickets[i].status == STATUSES[c] && self.matches(&self.tickets[i])
                })
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

    /// Ids in navigation order: every list row, or the selected board column.
    fn nav_ids(&self) -> Vec<String> {
        if self.view == View::List {
            self.list_order()
                .into_iter()
                .filter_map(|e| match e {
                    ListEntry::Ticket { idx, .. } => Some(self.tickets[idx].id.clone()),
                    ListEntry::Header(_) => None,
                })
                .collect()
        } else {
            self.column(self.col).iter().map(|t| t.id.clone()).collect()
        }
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
    DetailLoaded(Result<Box<TicketDetail>, String>),
    DescriptionsLoaded(Result<HashMap<String, String>, String>),
    /// A lifecycle action finished: Ok is a success message, Err the library's
    /// refusal text.
    ActionDone(Result<String, String>),
    /// Another connection committed to the database: reload.
    DbChanged,
}

/// Work `update` asks the runner to do; the runner feeds the result back as an Event.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Reload(LoadSpec),
    /// Fetch the full ticket with this id.
    LoadDetail(String),
    /// Fetch every ticket's description, for search.
    LoadDescriptions,
    /// Apply a lifecycle action to the ticket with this id.
    Transition {
        id: String,
        action: Lifecycle,
    },
    /// Set a ticket's title.
    EditTitle {
        id: String,
        title: String,
    },
    /// Edit a text field in $EDITOR. The runner owns the terminal, so it
    /// handles this itself and reads the current text fresh.
    EditExternal {
        id: String,
        field: TextField,
    },
    /// Check criterion `number`, with optional evidence.
    CheckCriterion {
        id: String,
        number: i64,
        evidence: Option<String>,
    },
    UncheckCriterion {
        id: String,
        number: i64,
    },
    AddCriterion {
        id: String,
        text: String,
    },
    RemoveCriterion {
        id: String,
        number: i64,
    },
    AddComment {
        id: String,
        body: String,
    },
    RemoveComment {
        id: String,
        comment_id: i64,
    },
    /// Move `id` just before (`before`) or after `anchor` among its siblings.
    Reorder {
        id: String,
        anchor: String,
        before: bool,
    },
    AddDep {
        id: String,
        blocker: String,
    },
    RemoveDep {
        id: String,
        blocker: String,
    },
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
            // If the selected ticket vanished, move to its neighbour in the
            // order the user was navigating: the next survivor, else the previous.
            let old_ids = state.nav_ids();
            state.tickets = tickets;
            if let Some(sel) = state.selected_id.clone()
                && !state.tickets.iter().any(|t| t.id == sel)
                && let Some(pos) = old_ids.iter().position(|i| *i == sel)
            {
                let alive = |i: &&String| state.tickets.iter().any(|t| t.id == **i);
                state.selected_id = old_ids[pos + 1..]
                    .iter()
                    .find(alive)
                    .or_else(|| old_ids[..pos].iter().rev().find(alive))
                    .cloned();
            }
            state.error = None;
            state.reselect();
            vec![]
        }
        Event::Loaded(Err(e)) => {
            state.error = Some(e);
            vec![]
        }
        Event::DetailLoaded(Ok(ticket)) => {
            state.error = None;
            // A reload of the open ticket (after an action) keeps scroll and
            // selection; another ticket starts fresh.
            let mut d = Detail::new(*ticket);
            if let Some(old) = &state.detail
                && old.ticket.ticket.id == d.ticket.ticket.id
            {
                d.scroll = old.scroll;
                d.sel = old.sel;
                d.reveal = old.sel.is_some();
                d.clamp_sel();
            }
            state.detail = Some(d);
            vec![]
        }
        Event::DetailLoaded(Err(e)) => {
            state.error = Some(e);
            vec![]
        }
        Event::DescriptionsLoaded(Ok(d)) => {
            state.descriptions = d;
            state.refilter();
            vec![]
        }
        Event::DescriptionsLoaded(Err(e)) => {
            state.error = Some(e);
            vec![]
        }
        Event::ActionDone(Ok(msg)) => {
            state.notice = Some(msg);
            state.error = None;
            let mut fx = vec![Effect::Reload(state.load_spec())];
            if state.wants_descriptions() {
                fx.push(Effect::LoadDescriptions);
            }
            if let Some(d) = &state.detail {
                fx.push(Effect::LoadDetail(d.ticket.ticket.id.clone()));
            }
            fx
        }
        Event::DbChanged => {
            let mut fx = vec![Effect::Reload(state.load_spec())];
            if state.wants_descriptions() {
                fx.push(Effect::LoadDescriptions);
            }
            if let Some(d) = &state.detail {
                fx.push(Effect::LoadDetail(d.ticket.ticket.id.clone()));
            }
            fx
        }
        Event::ActionDone(Err(e)) => {
            state.notice = None;
            state.error = Some(e);
            vec![]
        }
    }
}

/// Lifecycle keys, shared by board, list and detail. Returns None when the
/// key is not a lifecycle key; Some(effects) (possibly empty) when it is, so
/// a key that does not apply to the ticket's status is inert.
fn lifecycle_key(state: &mut State, c: char) -> Option<Vec<Effect>> {
    const KEYS: [char; 7] = ['r', 'S', 'R', 'd', 'D', 'x', 'o'];
    if !KEYS.contains(&c) && c != 'A' {
        return None;
    }
    let Some((id, status, archived)) = state.acted_ticket() else {
        return Some(vec![]);
    };
    let Some((_, label, action)) = allowed(&status, archived)
        .into_iter()
        .find(|(k, _, _)| k.starts_with(c))
    else {
        return Some(vec![]);
    };
    match action {
        Lifecycle::Done { force: true } | Lifecycle::Abandon => {
            state.confirm = Some(Confirm {
                prompt: format!("{label} {id}?"),
                id,
                action: ConfirmAction::Lifecycle(action),
            });
            Some(vec![])
        }
        _ => Some(vec![Effect::Transition { id, action }]),
    }
}

/// Edit keys, shared by board, list and detail: `e` title prompt, `E`
/// description and `P` plan in $EDITOR. Inert without a selected ticket.
fn edit_key(state: &mut State, c: char) -> Option<Vec<Effect>> {
    if !matches!(c, 'e' | 'E' | 'P') {
        return None;
    }
    let (Some((id, _, _)), Some(title)) = (state.acted_ticket(), state.acted_title()) else {
        return Some(vec![]);
    };
    Some(match c {
        'e' => {
            state.prompt = Some(PromptState {
                id,
                field: PromptField::Title,
                prompt: Prompt::new("title", &title),
                seed: title,
            });
            vec![]
        }
        'E' => vec![Effect::EditExternal {
            id,
            field: TextField::Description,
        }],
        _ => vec![Effect::EditExternal {
            id,
            field: TextField::Plan,
        }],
    })
}

/// Detail-only keys over criteria and comments: `[`/`]` move the selection,
/// Space or `v` checks (via an evidence prompt) or unchecks the selected
/// criterion, `n` adds a criterion, `X` removes the selection after a
/// confirm, `m` adds a comment in the prompt and `M` in $EDITOR. Returns
/// None for other keys; keys that need a selection are inert without one.
fn criteria_key(state: &mut State, c: char) -> Option<Vec<Effect>> {
    if !matches!(c, ' ' | 'v' | 'n' | 'X' | 'm' | 'M' | '[' | ']') {
        return None;
    }
    let d = state.detail.as_mut()?;
    let id = d.ticket.ticket.id.clone();
    let prompt = |label: &str, field| PromptState {
        id: id.clone(),
        field,
        seed: String::new(),
        prompt: Prompt::new(label, ""),
    };
    let criterion = match d.sel {
        Some(DetailSel::Criterion(i)) => d.ticket.criteria.get(i).cloned(),
        _ => None,
    };
    let comment = match d.sel {
        Some(DetailSel::Comment(i)) => d.ticket.comments.get(i).cloned(),
        _ => None,
    };
    Some(match c {
        '[' | ']' => {
            d.step(c == ']');
            vec![]
        }
        ' ' | 'v' => match criterion {
            Some(cr) if cr.checked => vec![Effect::UncheckCriterion {
                id,
                number: cr.number,
            }],
            Some(cr) => {
                let p = prompt(
                    "evidence (optional)",
                    PromptField::Evidence { number: cr.number },
                );
                state.prompt = Some(p);
                vec![]
            }
            None => vec![],
        },
        'n' => {
            let p = prompt("criterion", PromptField::Criterion);
            state.prompt = Some(p);
            vec![]
        }
        'm' => {
            let p = prompt("comment", PromptField::Comment);
            state.prompt = Some(p);
            vec![]
        }
        'M' => vec![Effect::EditExternal {
            id,
            field: TextField::Comment,
        }],
        _ => {
            let (action, prompt) = match (criterion, comment) {
                (Some(cr), _) => (
                    ConfirmAction::RemoveCriterion { number: cr.number },
                    format!("remove criterion {id}#{}?", cr.number),
                ),
                (_, Some(cm)) => (
                    ConfirmAction::RemoveComment { comment_id: cm.id },
                    format!("delete comment {}?", cm.id),
                ),
                _ => return Some(vec![]),
            };
            state.confirm = Some(Confirm { id, action, prompt });
            vec![]
        }
    })
}

/// Keys while the search input is open: each key edits the query live, Enter
/// closes the input and keeps it, Esc clears it and closes.
fn on_search_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let Some(p) = state.search.as_mut() else {
        return vec![];
    };
    match p.handle_key(k) {
        PromptOutcome::Pending => state.filter.query = p.text.clone(),
        PromptOutcome::Submit(_) => state.search = None,
        PromptOutcome::Cancel => {
            state.search = None;
            state.filter.query.clear();
        }
    }
    state.refilter();
    vec![]
}

/// Keys in the filter popup: s/f/b cycle status, for and blocked/available;
/// x clears every filter; Esc or Enter closes.
fn on_filter_menu_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let f = &mut state.filter;
    match k.code {
        KeyCode::Char('s') => {
            f.status = match f.status {
                None => Some(0),
                Some(c) if c + 1 < STATUSES.len() => Some(c + 1),
                Some(_) => None,
            }
        }
        KeyCode::Char('f') => {
            f.for_who = match f.for_who.as_deref() {
                None => Some("agent".into()),
                Some("agent") => Some("human".into()),
                Some(_) => None,
            }
        }
        KeyCode::Esc | KeyCode::Enter => state.filter_menu = false,
        KeyCode::Char('b') => {
            f.avail = match f.avail {
                None => Some(Avail::Blocked),
                Some(Avail::Blocked) => Some(Avail::Available),
                Some(Avail::Available) => None,
            }
        }
        KeyCode::Char('x') => *f = Filter::default(),
        _ => {}
    }
    state.refilter();
    vec![]
}

/// Keys while the prompt is open. Enter submits: an unchanged title makes no
/// effect (no write); an empty one closes the prompt with an error.
fn on_prompt_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let Some(p) = state.prompt.as_mut() else {
        return vec![];
    };
    match p.prompt.handle_key(k) {
        PromptOutcome::Pending => vec![],
        PromptOutcome::Cancel => {
            state.prompt = None;
            vec![]
        }
        PromptOutcome::Submit(text) => {
            let Some(p) = state.prompt.take() else {
                return vec![];
            };
            match p.field {
                PromptField::Evidence { number } => vec![Effect::CheckCriterion {
                    id: p.id,
                    number,
                    evidence: (!text.trim().is_empty()).then_some(text),
                }],
                PromptField::Criterion | PromptField::Comment if text.trim().is_empty() => {
                    state.error = Some(
                        if p.field == PromptField::Criterion {
                            "criterion must not be empty"
                        } else {
                            "comment must not be empty"
                        }
                        .into(),
                    );
                    vec![]
                }
                PromptField::Criterion => vec![Effect::AddCriterion { id: p.id, text }],
                PromptField::Comment => vec![Effect::AddComment {
                    id: p.id,
                    body: text,
                }],
                PromptField::Title if text.trim().is_empty() => {
                    state.error = Some("title must not be empty".into());
                    vec![]
                }
                PromptField::Title if text.trim() == p.seed.trim() => vec![],
                PromptField::Title => vec![Effect::EditTitle {
                    id: p.id,
                    title: text,
                }],
            }
        }
    }
}

/// Shift-Up/Down on the board or list: move the selected ticket before its
/// previous or after its next neighbour. Inert at the edge of a column or
/// group; done and abandoned tickets are refused here with a message.
fn reorder_key(state: &mut State, up: bool) -> Vec<Effect> {
    let Some((id, status, _)) = state.acted_ticket() else {
        return vec![];
    };
    if matches!(status.as_str(), "done" | "abandoned") {
        state.error = Some(format!("cannot reorder a ticket that is {status}"));
        return vec![];
    }
    let ids: Vec<&str> = match state.view {
        View::Board => state
            .column(state.col)
            .into_iter()
            .map(|t| t.id.as_str())
            .collect(),
        View::List => state
            .list_order()
            .into_iter()
            .filter_map(|e| match e {
                ListEntry::Ticket { idx, .. } if state.tickets[idx].status == status => {
                    Some(state.tickets[idx].id.as_str())
                }
                _ => None,
            })
            .collect(),
    };
    let Some(at) = ids.iter().position(|i| *i == id) else {
        return vec![];
    };
    let neighbour = if up { at.checked_sub(1) } else { Some(at + 1) }.and_then(|n| ids.get(n));
    match neighbour {
        Some(n) => vec![Effect::Reorder {
            id,
            anchor: (*n).to_string(),
            before: up,
        }],
        None => vec![],
    }
}

/// `b` / `B` in the detail view: open the picker to add or remove a blocker.
/// Add lists every loaded ticket except itself and its current blockers (not
/// pre-filtered by Project, so the library's cross-Project refusal shows).
fn picker_key(state: &mut State, c: char) -> Option<Vec<Effect>> {
    let mode = match c {
        'b' => PickerMode::Add,
        'B' => PickerMode::Remove,
        _ => return None,
    };
    let Some(d) = &state.detail else {
        return Some(vec![]);
    };
    let t = &d.ticket.ticket;
    let candidates: Vec<(String, String)> = match mode {
        PickerMode::Add => state
            .tickets
            .iter()
            .filter(|o| o.id != t.id && !t.blocked_by.iter().any(|b| b.id == o.id))
            .map(|o| (o.id.clone(), o.title.clone()))
            .collect(),
        PickerMode::Remove => t
            .blocked_by
            .iter()
            .map(|b| (b.id.clone(), b.title.clone()))
            .collect(),
    };
    if candidates.is_empty() {
        state.error = Some(
            match mode {
                PickerMode::Add => "no tickets to add as a blocker",
                PickerMode::Remove => "this ticket has no blockers to remove",
            }
            .into(),
        );
        return Some(vec![]);
    }
    state.picker = Some(Picker {
        id: t.id.clone(),
        mode,
        filter: String::new(),
        candidates,
        sel: 0,
    });
    Some(vec![])
}

/// Keys while the picker is open: typing filters, Up/Down (Ctrl-n/p) move,
/// Enter applies, Esc cancels.
fn on_picker_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let Some(p) = state.picker.as_mut() else {
        return vec![];
    };
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let n = p.visible().len();
    match (k.code, ctrl) {
        (KeyCode::Esc, _) => state.picker = None,
        (KeyCode::Up, _) | (KeyCode::Char('p'), true) => p.sel = p.sel.saturating_sub(1),
        (KeyCode::Down, _) | (KeyCode::Char('n'), true) => {
            p.sel = (p.sel + 1).min(n.saturating_sub(1));
        }
        (KeyCode::Backspace, _) => {
            p.filter.pop();
            p.sel = 0;
        }
        (KeyCode::Char(c), false) => {
            p.filter.push(c);
            p.sel = 0;
        }
        (KeyCode::Enter, _) => {
            let chosen = p.visible().get(p.sel).map(|(id, _)| id.clone());
            if let Some(blocker) = chosen {
                let p = state.picker.take().expect("picker is open");
                let id = p.id;
                return vec![match p.mode {
                    PickerMode::Add => Effect::AddDep { id, blocker },
                    PickerMode::Remove => Effect::RemoveDep { id, blocker },
                }];
            }
        }
        _ => {}
    }
    vec![]
}

/// Keys while a confirm popup is open: y/Enter confirm, anything else cancels.
fn on_confirm_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let Some(c) = state.confirm.take() else {
        return vec![];
    };
    match k.code {
        KeyCode::Char('y') | KeyCode::Enter => vec![match c.action {
            ConfirmAction::Lifecycle(action) => Effect::Transition { id: c.id, action },
            ConfirmAction::RemoveCriterion { number } => {
                Effect::RemoveCriterion { id: c.id, number }
            }
            ConfirmAction::RemoveComment { comment_id } => Effect::RemoveComment {
                id: c.id,
                comment_id,
            },
        }],
        _ => vec![],
    }
}

/// Keys while the detail view is open. q closes the view, not the app.
fn on_detail_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let page = state.page;
    let half = (page / 2).max(1);
    if let (KeyCode::Char(c), false) = (k.code, ctrl)
        && let Some(fx) = lifecycle_key(state, c)
            .or_else(|| edit_key(state, c))
            .or_else(|| criteria_key(state, c))
            .or_else(|| picker_key(state, c))
    {
        return fx;
    }
    let Some(d) = state.detail.as_mut() else {
        return vec![];
    };
    match (k.code, ctrl) {
        (KeyCode::Char('c'), true) => state.should_quit = true,
        (KeyCode::Esc | KeyCode::Char('q' | 'h') | KeyCode::Left, false) => state.detail = None,
        (KeyCode::Up | KeyCode::Char('k'), false) => d.scroll = d.scroll.saturating_sub(1),
        (KeyCode::Down | KeyCode::Char('j'), false) => d.scroll = d.scroll.saturating_add(1),
        (KeyCode::PageUp, _) => d.scroll = d.scroll.saturating_sub(page),
        (KeyCode::PageDown, _) => d.scroll = d.scroll.saturating_add(page),
        (KeyCode::Char('u'), true) => d.scroll = d.scroll.saturating_sub(half),
        (KeyCode::Char('d'), true) => d.scroll = d.scroll.saturating_add(half),
        (KeyCode::Home | KeyCode::Char('g'), false) => d.scroll = 0,
        (KeyCode::End | KeyCode::Char('G'), false) => d.scroll = usize::MAX,
        _ => {}
    }
    vec![]
}

fn on_key(state: &mut State, k: KeyEvent) -> Vec<Effect> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && k.code == KeyCode::Char('c') {
        state.should_quit = true;
        return vec![];
    }
    state.notice = None;
    state.error = None;
    if state.prompt.is_some() {
        return on_prompt_key(state, k);
    }
    if state.picker.is_some() {
        return on_picker_key(state, k);
    }
    if state.search.is_some() {
        return on_search_key(state, k);
    }
    if state.filter_menu {
        return on_filter_menu_key(state, k);
    }
    if state.confirm.is_some() {
        return on_confirm_key(state, k);
    }
    if state.help {
        if matches!(k.code, KeyCode::Esc | KeyCode::Char('?' | 'q')) {
            state.help = false;
        }
        return vec![];
    }
    if k.code == KeyCode::Char('?') && !ctrl {
        state.help = true;
        return vec![];
    }
    if state.detail.is_some() {
        return on_detail_key(state, k);
    }
    if k.modifiers.contains(KeyModifiers::SHIFT)
        && let KeyCode::Up | KeyCode::Down = k.code
    {
        return reorder_key(state, k.code == KeyCode::Up);
    }
    if let (KeyCode::Char(c), false) = (k.code, ctrl) {
        match c {
            '/' => {
                state.search = Some(Prompt::new("/", &state.filter.query));
                return vec![Effect::LoadDescriptions];
            }
            'f' => {
                state.filter_menu = true;
                return vec![];
            }
            _ => {}
        }
    }
    if let (KeyCode::Char(c), false) = (k.code, ctrl)
        && let Some(fx) = lifecycle_key(state, c).or_else(|| edit_key(state, c))
    {
        return fx;
    }
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
        (KeyCode::Enter, false) => {
            if let Some(id) = state.selected_id.clone() {
                return vec![Effect::LoadDetail(id)];
            }
        }
        (KeyCode::Esc, _) if state.filter.is_active() => {
            state.filter = Filter::default();
            state.refilter();
        }
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
    fn help_opens_closes_and_swallows_keys() {
        let (s, fx) = drive(vec![press('?')]);
        assert!(s.help && fx.is_empty());
        let (s, _) = drive(vec![press('?'), press('j'), press('s'), press('d')]);
        assert!(s.help && !s.should_quit);
        for close in [
            press('?'),
            press('q'),
            key(KeyCode::Esc, KeyModifiers::NONE, KeyEventKind::Press),
        ] {
            let (s, _) = drive(vec![press('?'), close]);
            assert!(!s.help && !s.should_quit);
        }
    }

    #[test]
    fn help_is_not_opened_over_text_entry_and_ctrl_c_quits_it() {
        let (s, _) = drive(vec![press('/'), press('?')]);
        assert!(!s.help);
        assert_eq!(s.filter.query, "?");
        let (s, _) = drive(vec![
            press('?'),
            key(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
        ]);
        assert!(s.should_quit);
    }

    #[test]
    fn other_keys_do_nothing() {
        let (s, fx) = drive(vec![press('x'), Event::Resize(30)]);
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

    #[test]
    fn list_selection_moves_to_neighbour_when_vanished() {
        let mut s = loaded(vec![code(KeyCode::Tab), press('j'), press('j')]);
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        update(
            &mut s,
            Event::Loaded(Ok(vec![ticket("R1", "ready"), ticket("R3", "ready")])),
        );
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        update(&mut s, Event::Loaded(Ok(vec![ticket("R1", "ready")])));
        assert_eq!(s.selected_id.as_deref(), Some("R1"));
    }

    #[test]
    fn board_selection_moves_to_next_in_column_when_vanished() {
        let mut s = loaded(vec![press('l'), press('j')]);
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        update(
            &mut s,
            Event::Loaded(Ok(vec![ticket("R1", "ready"), ticket("R3", "ready")])),
        );
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        assert_eq!((s.col, s.rows[1]), (1, 1));
    }

    #[test]
    fn db_changed_reloads_and_reloads_open_detail() {
        let mut s = loaded(vec![]);
        let fx = update(&mut s, Event::DbChanged);
        assert_eq!(fx, vec![Effect::Reload(s.load_spec())]);
        let mut s = loaded(vec![Event::DetailLoaded(Ok(Box::new(detail_of("D1"))))]);
        let fx = update(&mut s, Event::DbChanged);
        assert_eq!(
            fx,
            vec![
                Effect::Reload(s.load_spec()),
                Effect::LoadDetail("D1".into())
            ]
        );
    }

    #[test]
    fn db_changed_leaves_prompt_and_confirm_untouched() {
        let mut s = loaded(vec![press('l'), press('j'), press('e')]);
        let before = format!("{:?}{:?}{:?}", s.prompt, s.confirm, s.picker);
        update(&mut s, Event::DbChanged);
        update(&mut s, Event::Loaded(Ok(sample())));
        assert_eq!(
            format!("{:?}{:?}{:?}", s.prompt, s.confirm, s.picker),
            before
        );
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

    fn detail_of(id: &str) -> TicketDetail {
        serde_json::from_value(serde_json::json!({
            "archived_at": null, "available": false, "blocked": false, "blocked_by": [],
            "claimed_by": null, "created_at": "", "done_at": null, "for": "agent",
            "id": id, "parent": null, "status": "ready", "subtasks": [],
            "title": "t", "updated_at": "", "worktree": null,
            "comments": [], "criteria": [], "description": "", "plan": ""
        }))
        .unwrap()
    }

    fn opened() -> State {
        loaded(vec![Event::DetailLoaded(Ok(Box::new(detail_of("D1"))))])
    }

    #[test]
    fn enter_requests_detail_of_selection() {
        let mut s = loaded(vec![press('l'), press('j')]);
        let fx = update(&mut s, code(KeyCode::Enter));
        assert_eq!(fx, vec![Effect::LoadDetail("R2".into())]);
    }

    #[test]
    fn enter_with_no_tickets_does_nothing() {
        let (s, fx) = drive(vec![code(KeyCode::Enter)]);
        assert!(fx.is_empty() && s.detail.is_none());
    }

    #[test]
    fn detail_loaded_opens_and_error_does_not() {
        assert!(opened().detail.is_some());
        let s = loaded(vec![Event::DetailLoaded(Err("gone".into()))]);
        assert!(s.detail.is_none());
        assert_eq!(s.error.as_deref(), Some("gone"));
    }

    #[test]
    fn esc_q_h_left_close_detail_without_quitting() {
        for e in [
            code(KeyCode::Esc),
            press('q'),
            press('h'),
            code(KeyCode::Left),
        ] {
            let mut s = opened();
            update(&mut s, e);
            assert!(s.detail.is_none());
            assert!(!s.should_quit);
        }
    }

    #[test]
    fn ctrl_c_quits_from_detail() {
        let mut s = opened();
        update(&mut s, ctrl('c'));
        assert!(s.should_quit);
    }

    #[test]
    fn detail_scroll_keys() {
        let mut s = opened();
        s.page = 10;
        let scroll = |s: &State| s.detail.as_ref().unwrap().scroll;
        update(&mut s, press('k'));
        assert_eq!(scroll(&s), 0);
        update(&mut s, press('j'));
        update(&mut s, code(KeyCode::Down));
        assert_eq!(scroll(&s), 2);
        update(&mut s, code(KeyCode::PageDown));
        assert_eq!(scroll(&s), 12);
        update(&mut s, code(KeyCode::PageUp));
        update(&mut s, code(KeyCode::Up));
        assert_eq!(scroll(&s), 1);
        update(&mut s, ctrl('d'));
        assert_eq!(scroll(&s), 6);
        update(&mut s, ctrl('u'));
        assert_eq!(scroll(&s), 1);
        update(&mut s, press('G'));
        assert_eq!(scroll(&s), usize::MAX);
        update(&mut s, press('g'));
        assert_eq!(scroll(&s), 0);
    }

    #[test]
    fn reload_keeps_detail_open() {
        let mut s = opened();
        update(&mut s, Event::Loaded(Ok(sample())));
        assert!(s.detail.is_some());
    }

    fn tr(id: &str, action: Lifecycle) -> Effect {
        Effect::Transition {
            id: id.into(),
            action,
        }
    }

    /// Board selection on column `col` (0 = draft, 1 = ready, ...), row 0.
    fn at_col(col: usize) -> State {
        let mut s = loaded(vec![]);
        for _ in 0..col {
            update(&mut s, press('l'));
        }
        s
    }

    #[test]
    fn lifecycle_keys_per_status_from_board() {
        // (column, selected id, key, expected effect)
        let cases = [
            (0, "D1", 'r', tr("D1", Lifecycle::Ready)),
            (1, "R1", 'S', tr("R1", Lifecycle::Start)),
            (0, "D1", 'd', tr("D1", Lifecycle::Done { force: false })),
            (3, "X1", 'o', tr("X1", Lifecycle::Reopen)),
            (4, "A1", 'o', tr("A1", Lifecycle::Reopen)),
            (
                1,
                "R1",
                'A',
                tr("R1", Lifecycle::Archive { archived: true }),
            ),
        ];
        for (col, id, k, want) in cases {
            let mut s = at_col(col);
            assert_eq!(s.selected_id.as_deref(), Some(id));
            assert_eq!(update(&mut s, press(k)), vec![want], "{k} on {id}");
        }
    }

    #[test]
    fn lifecycle_keys_inert_for_wrong_status() {
        let cases = [
            (0, 'S'),
            (0, 'R'),
            (0, 'o'),
            (1, 'r'),
            (1, 'R'),
            (3, 'r'),
            (3, 'S'),
            (3, 'd'),
            (3, 'x'),
            (4, 'D'),
        ];
        for (col, k) in cases {
            let mut s = at_col(col);
            assert!(update(&mut s, press(k)).is_empty(), "{k} col {col}");
            assert!(s.confirm.is_none() && s.error.is_none() && s.notice.is_none());
        }
    }

    #[test]
    fn lifecycle_keys_work_from_list_and_detail() {
        let mut s = loaded(vec![code(KeyCode::Tab)]);
        assert_eq!(s.view, View::List);
        assert_eq!(update(&mut s, press('r')), vec![tr("D1", Lifecycle::Ready)]);
        let mut s = opened();
        // detail_of is a ready ticket
        assert_eq!(update(&mut s, press('S')), vec![tr("D1", Lifecycle::Start)]);
        assert!(update(&mut s, press('r')).is_empty());
        let mut s = opened();
        update(&mut s, press('R'));
        assert!(s.detail.is_some(), "inert key leaves the detail open");
    }

    #[test]
    fn archive_toggles_by_archived_at() {
        let mut s = State::default();
        let mut t = ticket("Z1", "done");
        t.archived_at = Some("2026-01-01T00:00:00Z".into());
        update(&mut s, Event::Loaded(Ok(vec![t])));
        for _ in 0..3 {
            update(&mut s, press('l'));
        }
        assert_eq!(
            update(&mut s, press('A')),
            vec![tr("Z1", Lifecycle::Archive { archived: false })]
        );
    }

    #[test]
    fn force_done_and_abandon_need_confirmation() {
        for (k, action) in [
            ('D', Lifecycle::Done { force: true }),
            ('x', Lifecycle::Abandon),
        ] {
            for confirm_key in [press('y'), code(KeyCode::Enter)] {
                let mut s = at_col(1);
                assert!(update(&mut s, press(k)).is_empty());
                assert!(s.confirm.is_some());
                assert_eq!(update(&mut s, confirm_key), vec![tr("R1", action)]);
                assert!(s.confirm.is_none());
            }
            for cancel in [press('n'), code(KeyCode::Esc), press('j')] {
                let mut s = at_col(1);
                update(&mut s, press(k));
                assert!(update(&mut s, cancel).is_empty());
                assert!(s.confirm.is_none());
                assert_eq!(s.selected_id.as_deref(), Some("R1"));
            }
        }
    }

    #[test]
    fn ctrl_c_quits_through_confirm() {
        let mut s = at_col(1);
        update(&mut s, press('x'));
        update(&mut s, ctrl('c'));
        assert!(s.should_quit);
    }

    #[test]
    fn action_error_sets_status_and_keeps_running() {
        let mut s = at_col(1);
        let fx = update(&mut s, Event::ActionDone(Err("R1 is blocked".into())));
        assert!(fx.is_empty());
        assert_eq!(s.error.as_deref(), Some("R1 is blocked"));
        assert!(!s.should_quit);
        // Next key clears it and still works.
        assert_eq!(update(&mut s, press('S')), vec![tr("R1", Lifecycle::Start)]);
        assert!(s.error.is_none());
    }

    #[test]
    fn action_ok_reloads_and_refreshes_detail() {
        let mut s = at_col(1);
        let fx = update(&mut s, Event::ActionDone(Ok("R1 -> active".into())));
        assert_eq!(fx, vec![Effect::Reload(s.load_spec())]);
        assert_eq!(s.notice.as_deref(), Some("R1 -> active"));
        let mut s = opened();
        let fx = update(&mut s, Event::ActionDone(Ok("ok".into())));
        assert_eq!(
            fx,
            vec![
                Effect::Reload(s.load_spec()),
                Effect::LoadDetail("D1".into())
            ]
        );
    }

    fn type_str(s: &mut State, text: &str) {
        for c in text.chars() {
            update(s, press(c));
        }
    }

    #[test]
    fn e_opens_prompt_seeded_with_title_in_board_and_detail() {
        let mut s = at_col(1);
        assert!(update(&mut s, press('e')).is_empty());
        let p = s.prompt.as_ref().unwrap();
        assert_eq!((p.id.as_str(), p.prompt.text.as_str()), ("R1", "title R1"));
        let mut s = opened();
        update(&mut s, press('e'));
        assert_eq!(s.prompt.as_ref().unwrap().id, "D1");
    }

    #[test]
    fn prompt_captures_keys_and_enter_emits_edit_title() {
        let mut s = at_col(1);
        update(&mut s, press('e'));
        // 'q' and 'S' are text, not quit or start.
        type_str(&mut s, "qS");
        assert!(!s.should_quit);
        let fx = update(&mut s, code(KeyCode::Enter));
        assert_eq!(
            fx,
            vec![Effect::EditTitle {
                id: "R1".into(),
                title: "title R1qS".into()
            }]
        );
        assert!(s.prompt.is_none());
    }

    #[test]
    fn prompt_esc_cancels_without_effect() {
        let mut s = at_col(1);
        update(&mut s, press('e'));
        type_str(&mut s, "zz");
        assert!(update(&mut s, code(KeyCode::Esc)).is_empty());
        assert!(s.prompt.is_none() && s.error.is_none());
    }

    #[test]
    fn unchanged_title_makes_no_effect_and_empty_title_errors() {
        let mut s = at_col(1);
        update(&mut s, press('e'));
        assert!(update(&mut s, code(KeyCode::Enter)).is_empty());
        assert!(s.prompt.is_none() && s.error.is_none());
        update(&mut s, press('e'));
        for _ in 0..20 {
            update(&mut s, code(KeyCode::Backspace));
        }
        assert!(update(&mut s, code(KeyCode::Enter)).is_empty());
        assert!(s.prompt.is_none());
        assert_eq!(s.error.as_deref(), Some("title must not be empty"));
    }

    #[test]
    fn ctrl_c_quits_through_prompt() {
        let mut s = at_col(1);
        update(&mut s, press('e'));
        update(&mut s, ctrl('c'));
        assert!(s.should_quit);
    }

    #[test]
    fn capital_e_and_p_ask_for_external_edit() {
        let mut s = at_col(1);
        assert_eq!(
            update(&mut s, press('E')),
            vec![Effect::EditExternal {
                id: "R1".into(),
                field: TextField::Description
            }]
        );
        let mut s = opened();
        assert_eq!(
            update(&mut s, press('P')),
            vec![Effect::EditExternal {
                id: "D1".into(),
                field: TextField::Plan
            }]
        );
    }

    #[test]
    fn edit_keys_are_inert_without_a_ticket() {
        let (s, fx) = drive(vec![press('e'), press('E'), press('P')]);
        assert!(fx.is_empty() && s.prompt.is_none());
    }

    fn rich_detail() -> TicketDetail {
        let mut d = detail_of("D1");
        d.criteria = serde_json::from_value(serde_json::json!([
            {"checked": false, "checked_at": null, "checked_by": null, "evidence": null, "number": 1, "ref": "D1#1", "text": "a"},
            {"checked": true, "checked_at": "t", "checked_by": {"author": "x", "session": null}, "evidence": "e", "number": 2, "ref": "D1#2", "text": "b"}
        ]))
        .unwrap();
        d.comments = serde_json::from_value(serde_json::json!([
            {"author": "x", "body": "hi", "created_at": "t", "id": 7, "role": null, "session": null}
        ]))
        .unwrap();
        d
    }

    fn rich() -> State {
        loaded(vec![Event::DetailLoaded(Ok(Box::new(rich_detail())))])
    }

    fn sel(s: &State) -> Option<DetailSel> {
        s.detail.as_ref().unwrap().sel
    }

    #[test]
    fn brackets_walk_criteria_then_comments_without_wrapping() {
        let mut s = rich();
        assert_eq!(sel(&s), None);
        let want = [
            DetailSel::Criterion(0),
            DetailSel::Criterion(1),
            DetailSel::Comment(0),
            DetailSel::Comment(0),
        ];
        for w in want {
            assert!(update(&mut s, press(']')).is_empty());
            assert_eq!(sel(&s), Some(w));
        }
        update(&mut s, press('['));
        assert_eq!(sel(&s), Some(DetailSel::Criterion(1)));
        update(&mut s, press('['));
        update(&mut s, press('['));
        assert_eq!(sel(&s), Some(DetailSel::Criterion(0)));
        // Backward from nothing starts at the last item.
        let mut s = rich();
        update(&mut s, press('['));
        assert_eq!(sel(&s), Some(DetailSel::Comment(0)));
        // Nothing to select: inert.
        let mut s = opened();
        update(&mut s, press(']'));
        assert_eq!(sel(&s), None);
    }

    #[test]
    fn space_on_unchecked_prompts_for_evidence_then_checks() {
        for (typed, want) in [("proof", Some("proof")), ("", None)] {
            let mut s = rich();
            update(&mut s, press(']'));
            assert!(update(&mut s, press(' ')).is_empty());
            assert_eq!(
                s.prompt.as_ref().unwrap().prompt.label,
                "evidence (optional)"
            );
            type_str(&mut s, typed);
            assert_eq!(
                update(&mut s, code(KeyCode::Enter)),
                vec![Effect::CheckCriterion {
                    id: "D1".into(),
                    number: 1,
                    evidence: want.map(String::from),
                }]
            );
        }
        let mut s = rich();
        update(&mut s, press('v'));
        assert!(s.prompt.is_none(), "inert without a selection");
        update(&mut s, press(']'));
        update(&mut s, press('v'));
        update(&mut s, code(KeyCode::Esc));
        assert!(s.prompt.is_none());
    }

    #[test]
    fn space_on_checked_unchecks_and_is_inert_on_a_comment() {
        let mut s = rich();
        update(&mut s, press(']'));
        update(&mut s, press(']'));
        assert_eq!(
            update(&mut s, press(' ')),
            vec![Effect::UncheckCriterion {
                id: "D1".into(),
                number: 2
            }]
        );
        update(&mut s, press(']'));
        assert!(update(&mut s, press(' ')).is_empty());
        assert!(s.prompt.is_none());
    }

    #[test]
    fn n_adds_a_criterion_and_refuses_empty() {
        let mut s = rich();
        update(&mut s, press('n'));
        type_str(&mut s, "new one");
        assert_eq!(
            update(&mut s, code(KeyCode::Enter)),
            vec![Effect::AddCriterion {
                id: "D1".into(),
                text: "new one".into()
            }]
        );
        update(&mut s, press('n'));
        assert!(update(&mut s, code(KeyCode::Enter)).is_empty());
        assert_eq!(s.error.as_deref(), Some("criterion must not be empty"));
    }

    #[test]
    fn x_removes_criterion_or_comment_after_confirm() {
        let mut s = rich();
        update(&mut s, press('X'));
        assert!(s.confirm.is_none(), "inert without a selection");
        update(&mut s, press(']'));
        update(&mut s, press(']'));
        update(&mut s, press('X'));
        assert_eq!(s.confirm.as_ref().unwrap().prompt, "remove criterion D1#2?");
        assert_eq!(
            update(&mut s, press('y')),
            vec![Effect::RemoveCriterion {
                id: "D1".into(),
                number: 2
            }]
        );
        update(&mut s, press(']'));
        update(&mut s, press('X'));
        assert_eq!(s.confirm.as_ref().unwrap().prompt, "delete comment 7?");
        assert_eq!(
            update(&mut s, code(KeyCode::Enter)),
            vec![Effect::RemoveComment {
                id: "D1".into(),
                comment_id: 7
            }]
        );
        update(&mut s, press('X'));
        assert!(update(&mut s, press('n')).is_empty());
        assert!(s.confirm.is_none());
    }

    #[test]
    fn m_prompts_for_a_comment_and_capital_m_uses_the_editor() {
        let mut s = rich();
        update(&mut s, press('m'));
        assert_eq!(s.prompt.as_ref().unwrap().prompt.label, "comment");
        assert!(update(&mut s, code(KeyCode::Enter)).is_empty());
        assert_eq!(s.error.as_deref(), Some("comment must not be empty"));
        update(&mut s, press('m'));
        type_str(&mut s, "note");
        assert_eq!(
            update(&mut s, code(KeyCode::Enter)),
            vec![Effect::AddComment {
                id: "D1".into(),
                body: "note".into()
            }]
        );
        assert_eq!(
            update(&mut s, press('M')),
            vec![Effect::EditExternal {
                id: "D1".into(),
                field: TextField::Comment
            }]
        );
    }

    #[test]
    fn criteria_keys_do_nothing_on_the_board() {
        let (s, fx) = drive(vec![press('n'), press('m'), press('X'), press(']')]);
        assert!(fx.is_empty() && s.prompt.is_none() && s.confirm.is_none());
    }

    #[test]
    fn reload_keeps_and_clamps_selection_for_same_ticket() {
        let mut s = rich();
        for _ in 0..3 {
            update(&mut s, press(']'));
        }
        s.detail.as_mut().unwrap().scroll = 4;
        let mut shrunk = rich_detail();
        shrunk.comments.clear();
        update(&mut s, Event::DetailLoaded(Ok(Box::new(shrunk))));
        assert_eq!(sel(&s), None);
        assert_eq!(s.detail.as_ref().unwrap().scroll, 4);
        update(&mut s, Event::DetailLoaded(Ok(Box::new(rich_detail()))));
        update(&mut s, press(']'));
        update(&mut s, press(']'));
        let mut fewer = rich_detail();
        fewer.criteria.truncate(1);
        update(&mut s, Event::DetailLoaded(Ok(Box::new(fewer))));
        assert_eq!(sel(&s), Some(DetailSel::Criterion(0)));
        // A different ticket resets.
        update(&mut s, Event::DetailLoaded(Ok(Box::new(detail_of("D2")))));
        assert_eq!(sel(&s), None);
        assert_eq!(s.detail.as_ref().unwrap().scroll, 0);
    }

    fn shift(c: KeyCode) -> Event {
        key(c, KeyModifiers::SHIFT, KeyEventKind::Press)
    }

    fn reorder(id: &str, anchor: &str, before: bool) -> Effect {
        Effect::Reorder {
            id: id.into(),
            anchor: anchor.into(),
            before,
        }
    }

    #[test]
    fn shift_up_down_reorder_in_board() {
        let mut s = at_col(1);
        update(&mut s, press('j'));
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        assert_eq!(
            update(&mut s, shift(KeyCode::Up)),
            vec![reorder("R2", "R1", true)]
        );
        assert_eq!(
            update(&mut s, shift(KeyCode::Down)),
            vec![reorder("R2", "R3", false)]
        );
    }

    #[test]
    fn shift_up_down_reorder_in_list() {
        let mut s = at_col(1);
        update(&mut s, code(KeyCode::Tab));
        update(&mut s, press('j'));
        assert_eq!(s.selected_id.as_deref(), Some("R2"));
        assert_eq!(
            update(&mut s, shift(KeyCode::Up)),
            vec![reorder("R2", "R1", true)]
        );
        assert_eq!(
            update(&mut s, shift(KeyCode::Down)),
            vec![reorder("R2", "R3", false)]
        );
    }

    #[test]
    fn shift_reorder_is_inert_at_edges() {
        let mut s = at_col(1);
        assert!(update(&mut s, shift(KeyCode::Up)).is_empty());
        for _ in 0..2 {
            update(&mut s, press('j'));
        }
        assert!(update(&mut s, shift(KeyCode::Down)).is_empty());
        assert!(s.error.is_none());
        // the list does not step across a status header
        update(&mut s, code(KeyCode::Tab));
        assert!(update(&mut s, shift(KeyCode::Down)).is_empty());
    }

    #[test]
    fn shift_reorder_refuses_closed_tickets_with_a_message() {
        for (col, status) in [(3, "done"), (4, "abandoned")] {
            let mut s = at_col(col);
            assert!(update(&mut s, shift(KeyCode::Down)).is_empty());
            assert_eq!(
                s.error.as_deref(),
                Some(format!("cannot reorder a ticket that is {status}").as_str())
            );
        }
    }

    fn blocker_detail() -> State {
        let mut d = rich_detail();
        d.ticket.blocked_by = serde_json::from_value(serde_json::json!([
            {"id": "R1", "status": "ready", "title": "t R1", "worktree": null}
        ]))
        .unwrap();
        loaded(vec![Event::DetailLoaded(Ok(Box::new(d)))])
    }

    #[test]
    fn b_opens_add_picker_without_self_or_current_blockers() {
        let mut s = blocker_detail();
        update(&mut s, press('b'));
        let p = s.picker.as_ref().expect("picker open");
        assert_eq!(p.mode, PickerMode::Add);
        let ids: Vec<&str> = p.candidates.iter().map(|(i, _)| i.as_str()).collect();
        assert_eq!(ids, ["R2", "R3", "X1", "X2", "A1"]);
    }

    #[test]
    fn capital_b_lists_only_blockers_and_enter_removes() {
        let mut s = blocker_detail();
        update(&mut s, press('B'));
        let p = s.picker.as_ref().expect("picker open");
        assert_eq!(p.mode, PickerMode::Remove);
        assert_eq!(p.candidates.len(), 1);
        assert_eq!(
            update(&mut s, code(KeyCode::Enter)),
            vec![Effect::RemoveDep {
                id: "D1".into(),
                blocker: "R1".into()
            }]
        );
        assert!(s.picker.is_none());
    }

    #[test]
    fn capital_b_without_blockers_says_so() {
        let mut s = rich();
        update(&mut s, press('B'));
        assert!(s.picker.is_none());
        assert!(s.error.is_some());
    }

    #[test]
    fn picker_filters_moves_selects_and_cancels() {
        let mut s = blocker_detail();
        update(&mut s, press('b'));
        type_str(&mut s, "r3");
        assert_eq!(s.picker.as_ref().unwrap().visible().len(), 1);
        assert_eq!(
            update(&mut s, code(KeyCode::Enter)),
            vec![Effect::AddDep {
                id: "D1".into(),
                blocker: "R3".into()
            }]
        );
        update(&mut s, press('b'));
        update(&mut s, code(KeyCode::Down));
        update(&mut s, code(KeyCode::Down));
        assert_eq!(s.picker.as_ref().unwrap().sel, 2);
        update(&mut s, code(KeyCode::Up));
        assert_eq!(s.picker.as_ref().unwrap().sel, 1);
        // a filter matching nothing makes Enter inert
        type_str(&mut s, "zzz");
        assert!(update(&mut s, code(KeyCode::Enter)).is_empty());
        assert!(s.picker.is_some());
        update(&mut s, code(KeyCode::Esc));
        assert!(s.picker.is_none());
        assert!(s.detail.is_some());
    }

    #[test]
    fn ctrl_c_quits_through_picker() {
        let mut s = blocker_detail();
        update(&mut s, press('b'));
        update(&mut s, ctrl('c'));
        assert!(s.should_quit);
    }

    /// sample() with R2 human-for, R3 blocked, X1 available, and
    /// descriptions on R1 and D1.
    fn filterable() -> State {
        let mut t = sample();
        t[2].for_who = "human".into();
        t[3].blocked = true;
        t[4].available = true;
        let mut s = State::default();
        update(&mut s, Event::Loaded(Ok(t)));
        let d: HashMap<String, String> = [
            ("R1".to_string(), "Mentions Needle here".to_string()),
            ("D1".to_string(), "other".to_string()),
        ]
        .into();
        update(&mut s, Event::DescriptionsLoaded(Ok(d)));
        s
    }

    fn ids(s: &State) -> Vec<String> {
        s.nav_ids()
    }

    fn all_ids(s: &State) -> Vec<String> {
        s.visible_columns()
            .into_iter()
            .flat_map(|c| s.column(c).into_iter().map(|t| t.id.clone()))
            .collect()
    }

    #[test]
    fn slash_opens_search_and_typing_narrows_live_on_board_and_list() {
        let mut s = filterable();
        let fx = update(&mut s, press('/'));
        assert_eq!(fx, vec![Effect::LoadDescriptions]);
        type_str(&mut s, "r2");
        assert_eq!(all_ids(&s), ["R2"]);
        assert_eq!(s.visible_count(), 1);
        update(&mut s, code(KeyCode::Tab));
        let rows: Vec<String> = list_ids(&s)
            .into_iter()
            .filter(|i| !i.starts_with('#'))
            .collect();
        assert_eq!(rows, ["R2"]);
        type_str(&mut s, "x");
        assert!(all_ids(&s).is_empty());
        update(&mut s, code(KeyCode::Backspace));
        assert_eq!(all_ids(&s), ["R2"]);
    }

    #[test]
    fn search_covers_id_title_and_description_case_insensitively() {
        let mut s = filterable();
        update(&mut s, press('/'));
        type_str(&mut s, "NEEDLE");
        assert_eq!(all_ids(&s), ["R1"]);
        s.filter.query = "TITLE d1".into();
        assert_eq!(all_ids(&s), ["D1"]);
        s.filter.query = "r3".into();
        assert_eq!(all_ids(&s), ["R3"]);
    }

    #[test]
    fn esc_clears_search_enter_keeps_it() {
        let mut s = filterable();
        update(&mut s, press('/'));
        type_str(&mut s, "r1");
        update(&mut s, code(KeyCode::Enter));
        assert!(s.search.is_none());
        assert_eq!(all_ids(&s), ["R1"]);
        // j is a normal key again, not typed into the query.
        update(&mut s, press('j'));
        assert_eq!(s.filter.query, "r1");
        update(&mut s, press('/'));
        update(&mut s, code(KeyCode::Esc));
        assert!(s.search.is_none());
        assert!(s.filter.query.is_empty());
        assert_eq!(all_ids(&s).len(), 7);
    }

    #[test]
    fn esc_in_normal_mode_clears_every_filter() {
        let mut s = filterable();
        s.filter.status = Some(1);
        s.filter.for_who = Some("agent".into());
        s.filter.query = "r".into();
        update(&mut s, code(KeyCode::Esc));
        assert!(!s.filter.is_active());
        assert_eq!(all_ids(&s).len(), 7);
        assert!(!s.should_quit);
    }

    #[test]
    fn filter_menu_cycles_and_clears() {
        let mut s = filterable();
        update(&mut s, press('f'));
        assert!(s.filter_menu);
        update(&mut s, press('s'));
        update(&mut s, press('s'));
        assert_eq!(s.filter.status, Some(1));
        assert_eq!(all_ids(&s), ["R1", "R2", "R3"]);
        update(&mut s, press('f'));
        assert_eq!(s.filter.for_who.as_deref(), Some("agent"));
        assert_eq!(all_ids(&s), ["R1", "R3"]);
        update(&mut s, press('f'));
        assert_eq!(all_ids(&s), ["R2"]);
        update(&mut s, press('f'));
        assert_eq!(s.filter.for_who, None);
        update(&mut s, press('b'));
        assert_eq!(s.filter.avail, Some(Avail::Blocked));
        assert_eq!(all_ids(&s), ["R3"]);
        update(&mut s, press('b'));
        assert_eq!(s.filter.avail, Some(Avail::Available));
        assert!(all_ids(&s).is_empty());
        update(&mut s, code(KeyCode::Esc));
        assert!(!s.filter_menu);
        assert!(s.filter.is_active());
        update(&mut s, press('f'));
        update(&mut s, press('x'));
        assert!(!s.filter.is_active());
        for _ in 0..6 {
            update(&mut s, press('s'));
        }
        assert_eq!(s.filter.status, None);
    }

    #[test]
    fn filters_compose_with_search_and_widen_when_removed() {
        let mut s = filterable();
        s.filter.query = "r".into();
        // R1 R2 R3 plus "title ..." matches every ticket's title, so narrow by id.
        s.filter.query = "title r".into();
        assert_eq!(all_ids(&s), ["R1", "R2", "R3"]);
        s.filter.for_who = Some("agent".into());
        assert_eq!(all_ids(&s), ["R1", "R3"]);
        s.filter.avail = Some(Avail::Blocked);
        assert_eq!(all_ids(&s), ["R3"]);
        s.filter.status = Some(0);
        assert!(all_ids(&s).is_empty());
        s.filter.status = Some(1);
        assert_eq!(all_ids(&s), ["R3"]);
        s.filter.avail = None;
        assert_eq!(all_ids(&s), ["R1", "R3"]);
        s.filter.for_who = None;
        assert_eq!(all_ids(&s), ["R1", "R2", "R3"]);
        s.filter.query.clear();
        s.filter.status = None;
        assert_eq!(all_ids(&s).len(), 7);
        s.filter.avail = Some(Avail::Available);
        assert_eq!(all_ids(&s), ["X1"]);
    }

    #[test]
    fn filtering_keeps_selection_on_a_visible_ticket() {
        let mut s = filterable();
        update(&mut s, press('j'));
        update(&mut s, code(KeyCode::Right));
        update(&mut s, press('/'));
        type_str(&mut s, "r3");
        assert_eq!(s.selected_id.as_deref(), Some("R3"));
        // List view: selection that vanishes moves to the first visible row.
        let mut s = filterable();
        update(&mut s, code(KeyCode::Tab));
        update(&mut s, press('/'));
        type_str(&mut s, "x2");
        assert_eq!(s.selected_id.as_deref(), Some("X2"));
        assert_eq!(ids(&s), ["X2"]);
        s.filter.query = "zzz".into();
        s.refilter();
        assert_eq!(s.selected_id, None);
    }

    #[test]
    fn reload_with_a_query_keeps_it_and_asks_for_descriptions() {
        let mut s = filterable();
        s.filter.query = "r2".into();
        assert_eq!(
            update(&mut s, Event::DbChanged),
            vec![Effect::Reload(s.load_spec()), Effect::LoadDescriptions]
        );
        update(&mut s, Event::Loaded(Ok(sample())));
        assert_eq!(all_ids(&s), ["R2"]);
    }
}
