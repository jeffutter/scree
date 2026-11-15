import { View, div, svg } from "gpui-kit";
import { v_flex, h_flex, Popup, InputState, Input, TextareaState, Textarea } from "gpui-base";
import { spaces, panes, focused, exec, theme, run, props as layoutProps } from "herdr";

// Scree pure helpers: no host calls. scree.test.mjs slices the text between the two markers
// below, so keep this section free of imports and host globals.
// --- pure begin ---
// Panel grouping order over every status scree knows, abandoned included. The board's columns are
// the separate COLUMNS table below; the two must stay consistent on the four they share.
const STATUS_ORDER = ["active", "ready", "draft", "done", "abandoned"];

// The board's columns, left to right, as [status, label]. One table holds the fact "these are the
// board's statuses": BOARD_COLUMNS, OPEN_STATUSES, LABELS, SECTIONS and boardHeadline derive from
// it, so a new column is fetched, rendered, counted and displayed without editing anywhere else.
const COLUMNS = [["draft", "Draft"], ["ready", "Ready"], ["active", "Active"], ["done", "Done"]];
const BOARD_COLUMNS = COLUMNS.map(([s]) => s);
// The statuses that mean "not finished": both open list calls and the all-done fold check read this.
const OPEN_STATUSES = BOARD_COLUMNS.filter((s) => s !== "done");
// Display name per status ("Draft", "Ready", ...), for the board's column headers and the panel's sections.
// Columns shown only on demand (the "Show abandoned" chip). Kept out of COLUMNS so they are neither
// fetched nor counted by default: OPEN_STATUSES and the headline stay the open statuses.
const OPTIONAL_COLUMNS = [["abandoned", "Abandoned"]];
const ABANDONED_STATUSES = OPTIONAL_COLUMNS.map(([s]) => s);
// The board's column statuses in order, plus the optional ones when asked for.
const boardColumns = (showAbandoned) => (showAbandoned ? [...BOARD_COLUMNS, ...ABANDONED_STATUSES] : BOARD_COLUMNS);
const LABELS = Object.fromEntries([...COLUMNS, ...OPTIONAL_COLUMNS]);

// A Status is "finished" when no command settles it: done and abandoned.
const CLOSED_STATUSES = ["done", "abandoned"];
// What a command can still settle. Derived from STATUS_ORDER, so a Status added there already
// offers Done and Abandon in the menu without touching COMMANDS (the transitions mutant proves it).
const SETTLEABLE = STATUS_ORDER.filter((s) => !CLOSED_STATUSES.includes(s)); // draft, ready, active

// Every way the CLI moves a Ticket, and the Statuses each works from (src/ticket.rs `transition`).
// `draft` works from ready only (active must be released first). The order is the menu's: forward,
// up, down, back to draft, finish, quit, restart.
const COMMANDS = [
  { verb: "ready", from: ["draft"], to: "ready" },
  { verb: "start", from: ["ready"], to: "active" },
  { verb: "release", from: ["active"], to: "ready" },
  { verb: "draft", from: ["ready"], to: "draft" },
  { verb: "done", from: SETTLEABLE, to: "done" },
  { verb: "abandon", from: SETTLEABLE, to: "abandoned" },
  { verb: "reopen", from: CLOSED_STATUSES, to: "ready" },
];

// status -> the entries its menu offers. Keyed off STATUS_ORDER so every Status gets a row even
// when no command leaves it (an empty list renders "Nothing reachable").
const TRANSITIONS = Object.fromEntries(STATUS_ORDER.map((s) => [s,
  COMMANDS.filter((c) => c.from.includes(s)).map((c) => ({ status: c.to, verb: c.verb }))]));

// The targets reachable from `status` by command, in COMMANDS order.
function transitionsFor(status) { return TRANSITIONS[status] ?? []; }

// Menu label for a Status: the board's column name where there is one, else the word capitalised.
// Abandoned is never a column, so it needs the fallback.
function statusLabel(status) {
  const s = String(status);
  return LABELS[s] ?? s.charAt(0).toUpperCase() + s.slice(1);
}

// Share of criteria checked, 0..100. Accepts the list-shape summary {checked, total}
// or a full criteria array [{checked}]. No criteria reads as 0.
function criteriaPercent(c) {
  const checked = Array.isArray(c) ? c.filter((x) => x.checked).length : c?.checked ?? 0;
  const total = Array.isArray(c) ? c.length : c?.total ?? 0;
  return total > 0 ? Math.round((checked / total) * 100) : 0;
}

// [{status, tickets}] in STATUS_ORDER; empty groups are dropped, unknown statuses go last.
function groupByStatus(tickets) {
  const by = new Map();
  for (const t of tickets ?? []) {
    if (!by.has(t.status)) by.set(t.status, []);
    by.get(t.status).push(t);
  }
  const known = STATUS_ORDER.filter((s) => by.has(s));
  const extra = [...by.keys()].filter((s) => !STATUS_ORDER.includes(s));
  return [...known, ...extra].map((status) => ({ status, tickets: by.get(status) }));
}

// "no-binary" | "no-project" | "error". Observed: `scree --json list` outside any repo exits 0
// with {"tickets":[]} (it lists the unscoped project), so "no-project" cannot come from the
// exit code alone; the view also treats a missing cwd as no-project. A failed scree whose
// message says it is outside a project/repo is classified here.
function classifyError(text, code) {
  const s = String(text ?? "");
  if (code === 127 || (code !== 0 && /not found|no such file|enoent|cannot find|failed to (spawn|execute)/i.test(s) && !/\bproject\b/i.test(s))) return "no-binary";
  if (/not in a (scree )?(project|repo|git)|no project|outside (of )?a (project|repo)|not a git/i.test(s)) return "no-project";
  return "error";
}
// Last n done tickets, newest done_at first (list orders by position, not by completion).
function doneRecent(tickets, n = 5) {
  return [...(tickets ?? [])].sort((a, b) => String(b.done_at ?? "").localeCompare(String(a.done_at ?? ""))).slice(0, n);
}

// Short label for who holds a ticket: the session if there is one, else the author.
function shortClaimer(c) {
  const s = c?.session ?? c?.author ?? (typeof c === "string" ? c : null);
  if (!s) return null;
  const str = String(s);
  return str.length > 12 ? str.slice(0, 8) : str;
}

// Chips for a row's second line, in fixed order: blocked, human, subtasks, claimer.
function chips(t) {
  const out = [];
  if (t.blocked) out.push({ kind: "blocked", text: "blocked" });
  if (t.for === "human") out.push({ kind: "human", text: "human" });
  const n = t.subtasks?.length ?? 0;
  if (n > 0) out.push({ kind: "subtasks", text: `${n} subtask${n === 1 ? "" : "s"}` });
  const who = shortClaimer(t.claimed_by);
  if (who) out.push({ kind: "claimer", text: who });
  return out;
}

// "3 active, 5 ready, 2 draft" from groupByStatus output; finished and empty groups are skipped.
function countsLine(groups) {
  return (groups ?? []).filter((g) => OPEN_STATUSES.includes(g.status) && g.tickets.length)
    .map((g) => `${g.tickets.length} ${g.status}`).join(", ");
}

// Provenance rather than a denylist: scree() runs an argv only when one of the builders below handed
// it back. The reads register too, which is why listArgs/boardListArgs/showArgs keep working
// unchanged apart from the build() around their returns. An array built anywhere else fails the gate
// on identity alone, whatever verbs it contains.
const BUILT = new WeakSet();
const build = (argv) => {
  BUILT.add(argv);
  return argv;
};
// The gate, named so scree.test.mjs can interrogate it: the test loads this region as text and
// cannot reach BUILT directly.
function isBuilt(argv) { return BUILT.has(argv); }

// Writes name the caller. A GUI spawn has no tty and no AI_AGENT marker, so scree would resolve Role
// agent (src/identity.rs) and refuse a human's claim of a `for human` Ticket. Same choice the TUI
// makes in code (src/lib.rs). Reads leave Role to inference.
const WRITE_FLAGS = ["--json", "--role", "human"];
// Mirrors the TUI's fixed reason (docs/tui.md); the CLI refuses a blank one (src/ticket.rs).
const ABANDON_REASON = "abandoned from the scree mod";

// Done tickets are fetched with --done-since so the server's position order cannot drop recent ones.
const DONE_DAYS = 7;

// The list calls of one poll: open tickets, and done tickets (for the Done count and tail), plus an
// abandoned call only when showAbandoned is on (the key is absent otherwise).
function listArgs(allWorktrees, showAbandoned = false) {
  const scope = allWorktrees ? ["--all-worktrees"] : [];
  return {
    ...(showAbandoned ? { abandoned: build(["--json", "list", ...scope, "--status", "abandoned", "--limit", "200"]) } : {}),
    open: build(["--json", "list", ...scope, ...OPEN_STATUSES.flatMap((s) => ["--status", s]), "--limit", "200"]),
    done: build(["--json", "list", ...scope, "--status", "done", "--done-since", `${DONE_DAYS}d`, "--limit", "500"]),
  };
}

// Filters shared by the panel and the board: forRole "all" | "agent" | "human", blockedOnly bool.
function applyFilters(tickets, { forRole = "all", blockedOnly = false } = {}) {
  return (tickets ?? []).filter((t) => (forRole === "all" || t.for === forRole) && (!blockedOnly || t.blocked));
}

// The detail call: read-only `scree --json show ID`.
function showArgs(id) { return build(["--json", "show", String(id)]); }

// The "Open in scree tui" argv.
function tuiArgv(bin) { return [bin || "scree", "tui"]; }

// One menu pick on ticket `id`: the command that reaches `target`, an entry of TRANSITIONS. It takes
// the entry and not a bare status because `ready` is reached by three different verbs depending on
// where the Ticket stands now.
function statusArgs(id, target) {
  const argv = [...WRITE_FLAGS, target.verb, String(id)];
  return build(target.verb === "abandon" ? [...argv, ABANDON_REASON] : argv);
}

// The required text fields of a form and the message scree itself gives for each (src/ticket.rs:796
// for title). One table so the create form can reuse it. Returns one {field, message} per missing
// field, so a form can name every problem before any argv is built. A lone "-" counts as missing: scree
// reads it as stdin, which the mod never feeds.
const REQUIRED_FIELDS = [["title", "title must not be empty"]];
function editErrors(fields = {}) {
  return REQUIRED_FIELDS.filter(([k]) => ["", "-"].includes(String(fields[k] ?? "").trim())).map(([field, message]) => ({ field, message }));
}

// Change title, description, plan or assignee: only the flags whose value differs from the loaded
// `ticket`, in that order, so an untouched box cannot be blanked and a cleared one is sent as "".
// Returns null when nothing differs, because `scree edit` with no flag exits 2 (src/ticket.rs:786);
// the caller then just leaves edit mode. There is no --status: `edit` has none (src/cli.rs
// EditArgs), which is why a status change is a transition. A lone "-" would make scree read stdin
// (src/lib.rs text_arg), so that field is left out instead.
function editArgs(ticket, fields = {}) {
  const t = ticket ?? {};
  const argv = [...WRITE_FLAGS, "edit", String(t.id)];
  const text = (name, key) => {
    if (fields[key] === undefined || fields[key] === (t[key] ?? "")) return;
    const s = String(fields[key]);
    if (s !== "-") argv.push(name, s);
  };
  text("--title", "title"); text("--description", "description"); text("--plan", "plan");
  if (fields.for !== undefined && fields.for !== t.for) argv.push("--for", String(fields.for));
  return argv.length === WRITE_FLAGS.length + 2 ? null : build(argv);
}

// One criterion per non-empty line of the Acceptance Criteria box. A "-" line is dropped:
// scree reads a lone "-" criterion as stdin (src/lib.rs text_arg), which the mod never feeds.
function criteriaLines(text) {
  return String(text ?? "").split(/\r?\n/).map((l) => l.trim()).filter((l) => l && l !== "-");
}

// The only Statuses `add` accepts (src/cli.rs NewStatus); `--status active` is a clap error.
const CREATE_STATUSES = ["draft", "ready"];

// File a new Ticket. `form.criteria` is the box's text or an array of lines. The Scope is never in
// the argv: scope::for_write ignores --project inside any repo, so the caller picks the exec cwd.
// `scope.parent` files it as a subtask (no UI uses it).
function addArgs(form = {}, scope = {}) {
  const argv = [...WRITE_FLAGS, "add", String(form.title ?? "")];
  argv.push(...textFlag("--description", form.description), ...textFlag("--plan", form.plan));
  const lines = Array.isArray(form.criteria) ? form.criteria.map(String).filter((l) => l !== "-") : criteriaLines(form.criteria);
  for (const c of lines) argv.push("--criterion", c);
  if (form.for !== undefined) argv.push("--for", String(form.for));
  if (form.status !== undefined) argv.push("--status", String(form.status));
  if (scope.parent !== undefined && scope.parent !== null) argv.push("--parent", String(scope.parent));
  return build(argv);
}

// A flag carrying free text, or nothing when the value is absent. A lone "-" tells scree to read
// stdin (src/lib.rs text_arg); an empty read exits 2 with a usage error. The mod must never block
// on stdin, so a lone dash is omitted and the field is left alone.
function textFlag(name, v) {
  if (v === undefined || v === null || v === "") return [];
  const s = String(v);
  return s === "-" ? [] : [name, s];
}

// scree reports a failure on stderr as one JSON object {"error":{"code","message"}} (src/error.rs);
// clap's own argv mistakes are plain text with exit 2, so fall back to the raw text. `kind` stays
// classifyError's verdict, so no-binary and no-project read exactly as they did before.
function parseScreeError(text, code) {
  const raw = String(text ?? "");
  for (const line of raw.split("\n")) {
    let parsed;
    try { parsed = JSON.parse(line); } catch { continue; }
    const e = parsed?.error;
    if (e && typeof e.message === "string")
      return { kind: classifyError(e.message, code), message: e.message, screeCode: e.code ?? null };
  }
  return { kind: classifyError(raw, code), message: raw, screeCode: null };
}

// Normalises a `scree --json show` payload for rendering; tolerant of missing fields.
function detailModel(show) {
  const s = show ?? {};
  const arr = (x) => (Array.isArray(x) ? x : []);
  const ref = (b) => (typeof b === "string" ? { id: b, status: "", title: "" } : { id: b.id, status: b.status ?? "", title: b.title ?? "" });
  return {
    id: s.id ?? "", title: s.title ?? "", status: s.status ?? "", for: s.for ?? null, blocked: Boolean(s.blocked),
    claimedBy: shortClaimer(s.claimed_by),
    description: s.description ?? "", plan: s.plan ?? "",
    criteria: arr(s.criteria).map((c) => ({ number: c.number, text: c.text ?? "", checked: Boolean(c.checked), evidence: c.evidence ?? null })),
    comments: arr(s.comments).map((c) => ({ author: c.author ?? "", role: c.role ?? null, body: c.body ?? "", created_at: c.created_at ?? "" })),
    blockedBy: arr(s.blocked_by).map(ref),
    subtasks: arr(s.subtasks).map(ref),
    // Where the Ticket lives, so a write from the detail view runs in its own Project's root.
    project: s.project ?? null, worktree: s.worktree ?? null,
  };
}

// The board's two calls: every Project and Worktree (`--all-projects` already means both; it
// conflicts with `--all-worktrees`). The default limit is 50, so pass a large one. Open and done
// are separate so accumulated done tickets cannot crowd open ones out of the limit. Done is
// ordered by position, so boardModel cuts it to the last days client-side.
function boardListArgs(showAbandoned = false) {
  return {
    ...(showAbandoned ? { abandoned: build(["--json", "list", "--all-projects", "--status", "abandoned", "--limit", "1000"]) } : {}),
    open: build(["--json", "list", "--all-projects", ...OPEN_STATUSES.flatMap((s) => ["--status", s]), "--limit", "1000"]),
    done: build(["--json", "list", "--all-projects", "--status", "done", "--done-since", `${DONE_DAYS}d`, "--limit", "1000"]),
  };
}

function worktreeLabel(path) {
  if (!path) return "root";
  const parts = String(path).split("/").filter(Boolean);
  return parts[parts.length - 1] ?? String(path);
}

// "this computer" for the local machine, else the machine name.
function machineLabel(on) { return on ? String(on) : "this computer"; }

// Projects as swimlanes: [{key, root, name, count, onlyDone, lanes: [{worktree, label, cells: one key per BOARD_COLUMNS}]}].
// Filters apply first; archived and done older than doneDays are dropped. Projects sort
// by key with LOCAL (no root) last; lanes list the root checkout first, then worktrees by path.
// Tickets keep list order (position) inside a cell. Empty lanes and projects are omitted.
// onlyDone is true when every visible ticket in the project is done; the board folds such a
// project by default (see projectFolded). It reflects the filters, so it can flip per view.
function boardModel(tickets, { forRole = "all", blockedOnly = false, now = Date.now(), doneDays = DONE_DAYS, showAbandoned = false } = {}) {
  const columns = boardColumns(showAbandoned);
  const cutoff = now - doneDays * 86400000;
  const projects = new Map();
  for (const t of applyFilters(tickets, { forRole, blockedOnly })) {
    if (!columns.includes(t.status) || t.archived_at) continue;
    if (t.status === "done") {
      const at = Date.parse(t.done_at ?? "");
      if (!(at >= cutoff)) continue;
    }
    const key = t.project?.key ?? "LOCAL", root = t.project?.root ?? null;
    if (!projects.has(key)) projects.set(key, { key, root, name: key, count: 0, lanes: new Map() });
    const p = projects.get(key);
    const wt = t.worktree ?? null;
    if (!p.lanes.has(wt))
      p.lanes.set(wt, { worktree: wt, label: worktreeLabel(wt), cells: Object.fromEntries(columns.map((s) => [s, []])) });
    p.lanes.get(wt).cells[t.status].push(t);
    // Abandoned cards are shown on demand and never counted, like the headline.
    if (!ABANDONED_STATUSES.includes(t.status)) p.count++;
  }
  // Done shows newest first so the collapsed view keeps the most recent cards.
  for (const p of projects.values()) for (const l of p.lanes.values()) {
    l.cells.done.sort((a, b) => Date.parse(b.done_at ?? "") - Date.parse(a.done_at ?? ""));
    if (l.cells.abandoned) l.cells.abandoned.sort((a, b) => Date.parse(b.updated_at ?? "") - Date.parse(a.updated_at ?? ""));
  }
  const byPath = (a, b) => String(a.worktree).localeCompare(String(b.worktree));
  return [...projects.values()]
    .map((p) => {
      const lanes = [...p.lanes.values()].sort((a, b) => (a.worktree === null ? -1 : b.worktree === null ? 1 : byPath(a, b)));
      return { ...p, lanes, onlyDone: lanes.every((l) => OPEN_STATUSES.every((c) => !l.cells[c].length)) };
    })
    .sort((a, b) => (a.root === null) - (b.root === null) || a.key.localeCompare(b.key));
}

// A board cell shows at most `limit` cards until expanded, so a long column (Done with 50 cards)
// does not push the next project far down the page. Returns {shown, hidden}.
const CELL_LIMIT = 5;
function cellVisible(cards, expanded, limit = CELL_LIMIT) {
  if (expanded || cards.length <= limit) return { shown: cards, hidden: 0 };
  return { shown: cards.slice(0, limit), hidden: cards.length - limit };
}

// Per-column totals across all projects in a boardModel result. An abandoned key appears only when
// the model was built with showAbandoned; boardHeadline never prints it.
function boardCounts(model) {
  const cols = model.some((p) => p.lanes.some((l) => l.cells.abandoned)) ? boardColumns(true) : BOARD_COLUMNS;
  const out = Object.fromEntries(cols.map((s) => [s, 0]));
  for (const p of model) for (const l of p.lanes) for (const c of cols) out[c] += l.cells[c].length;
  return out;
}

// Whether a project renders folded on the board. A stored boolean is an explicit user choice and
// always wins; a missing key means follow the data: folded when every visible ticket is done.
function projectFolded(folded, project) {
  const v = folded?.[project.key];
  return v === undefined ? Boolean(project.onlyDone) : Boolean(v);
}

// The value stored by toggleBoardFold: the opposite of the resolved fold state, so the first click
// on a project folded by its all-done default expands it rather than re-recording "folded".
function nextBoardFold(folded, project) {
  return !projectFolded(folded, project);
}

// Panel sections, Active first (the board's COLUMNS are Draft first); derived from BOARD_COLUMNS
// ordered by STATUS_ORDER so a new column renders without editing this list.
const SECTIONS = [
  ...STATUS_ORDER.filter((s) => BOARD_COLUMNS.includes(s)),
  ...BOARD_COLUMNS.filter((s) => !STATUS_ORDER.includes(s)),
];

// The board headline: one entry per open status in Active-first order, printing zero counts too
// (unlike countsLine which skips empty groups).
function boardHeadline(counts) {
  const ordered = [
    ...STATUS_ORDER.filter((s) => OPEN_STATUSES.includes(s)),
    ...OPEN_STATUSES.filter((s) => !STATUS_ORDER.includes(s)),
  ];
  return ordered.map((s) => `${counts[s]} ${s}`).join(", ");
}
// --- pure end ---

// Scree mod: one mod, two modes via props.view ("panel" | "board"). Data comes only from
// `scree --json list` through exec; the sqlite file is never opened here.
//
// The flake's herdr-mod package replaces @scree@ with the store path of the scree it was built
// with; an unreplaced copy (a plain checkout) fails to exec and falls through to `scree` on PATH.
const BINS = ["@scree@", "scree"];
const POLL_MS = 3000;
const DONE_SHOWN = 5;
const store = {
  get(k, d) { try { const v = localStorage.getItem(k); return v === null ? d : JSON.parse(v); } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
};

function location() {
  const space = spaces().find((s) => s.active), f = focused();
  const pane = f?.pane ? panes().find((p) => p.pane === f.pane) : null;
  const on = f?.pane === null ? (f.terminal?.includes("/") ? f.terminal.split("/")[0] : null) : space?.machine ?? null;
  return { cwd: pane?.cwd || f?.cwd || space?.folder, on };
}

// Shared pieces of the create and edit forms, so the two cannot drift apart. A field is its label and
// control kept close together, with the larger gap between fields, so a label reads as belonging to the
// control under it. `solid` / `solid_foreground` are the theme's primary button; the error colour is the
// theme's red (ansi1) because `blocked` is a neutral grey in some themes and made errors easy to miss.
function formKit(t) {
  const red = t.ansi1 ?? t.blocked;
  return {
    field: (label, control, hint) => v_flex().gap(4)
      .child(h_flex().gap(6).items_baseline()
        .child(div().text_size(12).font_weight(500).text_color(t.muted).child(label))
        .when(Boolean(hint), (el) => el.child(div().text_size(11).text_color(t.dim).child(hint))))
      .child(control),
    box: (child) => child.px(8).py(5).rounded(6).border_1().border_color(t.border).bg(t.input).text_size(12),
    chips: (id, current, values, set) => h_flex().gap(6).children(values.map((v) => div().id(`${id}-${v}`).flex_none().px(10).py(3).rounded(6).text_size(12).border_1()
      .border_color(current === v ? t.accent : t.border).bg(current === v ? t.selected : t.background)
      .text_color(current === v ? t.foreground : t.muted).cursor_pointer().hover((s) => s.bg(current === v ? t.selected : t.hover))
      .on_click((_e, cx) => { cx.stop_propagation(); set(v, cx); }).child(v))),
    // Icon and a lead-in carry the meaning too: a theme may map red to a neutral colour (stylix does).
    error: (id, text) => h_flex().id(id).gap(8).items_start().px(8).py(6).rounded(6).border_1().border_color(red).bg(t.raised)
      .child(svg("mods/scree/mark-blocked.svg").size(14).flex_none().text_color(red))
      .child(v_flex().flex_1().min_w(0).gap(1)
        .child(div().text_size(12).font_weight(600).text_color(t.foreground).child("Couldn't save"))
        .child(div().text_size(12).text_color(t.muted).child(text))),
    // Pinned under the scrolling body so Save never scrolls out of reach on a long form.
    footer: (saveId, saveText, onSave, cancelId, onCancel) => {
      const button = (id, text, on, primary) => div().id(id).flex_none().px(14).py(4).rounded(6).text_size(12).border_1()
        .border_color(primary ? t.solid : t.border).bg(primary ? t.solid : t.background)
        .text_color(primary ? t.solid_foreground : t.muted).font_weight(primary ? 600 : 400).cursor_pointer()
        .hover((s) => s.bg(primary ? t.solid : t.hover)).on_click((_e, cx) => { cx.stop_propagation(); on(cx); }).child(text);
      return h_flex().flex_none().px(12).py(8).gap(8).justify_end().border_t(1).border_color(t.border)
        .child(button(cancelId, "Cancel", onCancel, false)).child(button(saveId, saveText, onSave, true));
    },
  };
}

class ScreeError extends Error {
  constructor(kind, message) { super(message); this.kind = kind; }
}



export default class Scree extends View {
  init(props, cx) {
    // init receives no props for a layout block; the host exposes them through props().
    this.mode = (props ?? layoutProps())?.view === "board" ? "board" : "panel";
    this.open = []; this.done = []; this.board = []; this.error = null; this.loaded = false; this.context = null; this.bin = null;
    this.loading = false; this.parked = new Map();
    this.allWorktrees = store.get("allWorktrees", false) === true;
    this.folded = { abandoned: true, ...store.get("folded", { done: true }) };
    this.boardFolded = store.get("boardFolded", {});
    this.boardExpanded = store.get("boardExpanded", {});
    this.selected = null;
    this.filterFor = ["all", "agent", "human"].includes(store.get("filterFor", "all")) ? store.get("filterFor", "all") : "all";
    this.blockedOnly = store.get("blockedOnly", false) === true;
    this.showAbandoned = store.get("showAbandoned", false) === true;
    this.abandoned = [];
    this.detail = null; // {id, data, error, loading} while a ticket's detail is open
    // One status menu may be open at a time: {id, status}. notice is the last write's failure,
    // which survives polls so it can be read, and goes when the next write works or the view changes.
    this.menu = null;
    this.notice = null;
    this.refreshQueued = false;
    // A handle lives longer than a render, so it is made here and never in render().
    this.menuFocus = cx.focus_handle();
    // Edit form: retained widget state made once here, never in render. this.edit is
    // {base, for, error} while editing (base is the Ticket as loaded at startEdit); drafts maps
    // draftKey(id) to the last typed fields so a cancelled or abandoned edit of one Ticket never
    // shows in another. Polls only replace this.detail and never call set_value.
    this.editHandle = cx.focus_handle();
    this.edit = null; this.drafts = new Map();
    this.titleState = InputState.new({ placeholder: "Title" });
    this.descState = TextareaState.new({ placeholder: "Description" });
    this.planState = TextareaState.new({ placeholder: "Plan" });
    this.descState.set_auto_grow(4, 12); this.planState.set_auto_grow(4, 12);
    for (const st of [this.titleState, this.descState, this.planState]) st.on("change", (_e, cx) => this.saveDraft(cx));
    // Create form: same pattern. this.create is {cwd, on, label, from, for, status, error, saving};
    // createDrafts maps draftKey(cwd) to the typed fields so Back-and-return or a failed save keeps text.
    this.createHandle = cx.focus_handle();
    this.create = null; this.createDrafts = new Map();
    this.createTitle = InputState.new({ placeholder: "Title" });
    this.createDesc = TextareaState.new({ placeholder: "Description" });
    this.createPlan = TextareaState.new({ placeholder: "Plan" });
    this.createCrit = TextareaState.new({ placeholder: "One criterion per line" });
    for (const st of [this.createDesc, this.createPlan, this.createCrit]) st.set_auto_grow(3, 10);
    for (const st of [this.createTitle, this.createDesc, this.createPlan, this.createCrit]) st.on("change", (_e, cx) => this.saveCreateDraft(cx));
    this.refresh(cx);
    cx.timer.every(POLL_MS, (cx) => this.refresh(cx));
  }

  // Parked state is per space/folder and per scope, so toggling the scope re-polls at once.
  key() { return JSON.stringify({ ...location(), all: this.allWorktrees }); }

  // Runs scree in the active space's folder (on its machine) so project scoping follows the space.
  async scree(args, { cwd, on }) {
    if (!isBuilt(args)) throw new ScreeError("error", `refusing argv the mod did not build: ${args.join(" ")}`);
    const bins = this.bin ? [this.bin] : BINS;
    let last;
    for (const bin of bins) {
      let r;
      try { r = await exec(bin, args, { cwd, on, timeout: 15000 }); }
      catch (e) { last = e; continue; }
      if (r.code === 127) { last = new Error(`${bin} exited 127`); continue; }
      this.bin = bin;
      if (r.code !== 0) {
        const text = (r.stderr || r.stdout).trim();
        const err = parseScreeError(text, r.code);
        throw new ScreeError(err.kind, err.message || `scree exited ${r.code}`);
      }
      return r.stdout;
    }
    this.bin = null;
    throw new ScreeError("no-binary", `scree not found on ${on ?? "this machine"}${last ? ` (${last.message ?? last})` : ""}`);
  }

  refresh(cx) {
    // A write asked for a fresh board while a poll was in flight: run one as soon as it lands,
    // rather than waiting out the next 3 s tick.
    if (this.loading) { this.refreshQueued = true; return; }
    const loc = location(), context = this.key();
    if (context !== this.context) {
      // A different space, folder or scope: park this one's state and restore the target's.
      if (this.context !== null) this.parked.set(this.context, { open: this.open, done: this.done, board: this.board, abandoned: this.abandoned, error: this.error, loaded: this.loaded });
      const s = this.parked.get(context) ?? { open: [], done: [], board: [], abandoned: [], error: null, loaded: false };
      this.context = context; this.open = s.open; this.done = s.done; this.board = s.board; this.abandoned = s.abandoned; this.error = s.error; this.loaded = s.loaded;
      this.bin = null;
      this.leaveCreate();
      this.detail = null;
      this.menu = null;
      this.notice = null;
    }
    if (!loc.cwd && this.mode !== "board") { this.error = { kind: "no-project" }; this.loaded = true; cx.notify(); return; }
    this.loading = true;
    const args = listArgs(this.allWorktrees, this.showAbandoned);
    cx.spawn(async (cx) => {
      try {
        if (this.mode === "board") {
          // Two calls on the focused space's machine; all-projects needs no repo, so a missing cwd is fine.
          const bargs = boardListArgs(this.showAbandoned);
          const [open, done, abandoned] = (await Promise.all([this.scree(bargs.open, loc), this.scree(bargs.done, loc), ...(bargs.abandoned ? [this.scree(bargs.abandoned, loc)] : [])])).map((x) => JSON.parse(x));
          if (this.key() === context) { this.board = [...(open.tickets ?? []), ...(done.tickets ?? []), ...(abandoned?.tickets ?? [])]; this.error = null; this.loaded = true; }
        } else {
          const [open, done, abandoned] = (await Promise.all([this.scree(args.open, loc), this.scree(args.done, loc), ...(args.abandoned ? [this.scree(args.abandoned, loc)] : [])])).map((x) => JSON.parse(x));
          if (this.key() === context) { this.open = open.tickets ?? []; this.done = done.tickets ?? []; this.abandoned = abandoned?.tickets ?? []; this.error = null; this.loaded = true; }
        }
      } catch (e) {
        if (this.key() === context) { this.error = { kind: e.kind ?? "error", message: String(e.message ?? e) }; this.loaded = true; }
      } finally {
        this.loading = false;
        if (this.refreshQueued) { this.refreshQueued = false; this.refresh(cx); }
        cx.notify();
      }
      // Keep an open detail current (new evidence and comments) on every poll.
      // No branch above returns early, so this runs in board mode too.
      if (this.detail && this.key() === context) await this.fetchDetail(this.detail.id, loc, context, cx);
    });
  }

  // Loads `scree --json show id` into this.detail. A different space or a different ticket opened
  // meanwhile discards the result.
  async fetchDetail(id, loc, context, cx) {
    try {
      const data = detailModel(JSON.parse(await this.scree(showArgs(id), loc)));
      if (this.key() === context && this.detail?.id === id) this.detail = { id, data, error: null, loading: false };
    } catch (e) {
      if (this.key() === context && this.detail?.id === id) this.detail = { id, data: this.detail.data, error: String(e.message ?? e), loading: false };
    }
    cx.notify();
  }

  // Opens the read-only detail for a ticket; the board calls this on card click too.
  openDetail(id, cx) {
    this.leaveEdit();
    this.leaveCreate(true);
    this.selected = id;
    this.detail = { id, data: null, error: null, loading: true };
    this.menu = null;
    this.notice = null;
    cx.notify();
    const loc = location(), context = this.key();
    cx.spawn((cx) => this.fetchDetail(id, loc, context, cx));
  }

  closeDetail(cx) { this.leaveEdit(); this.detail = null; this.menu = null; this.notice = null; cx.notify(); }

  draftKey(id) { return JSON.stringify([this.context, id]); }
  editFields() {
    return { title: this.titleState.value(), description: this.descState.value(), plan: this.planState.value(), for: this.edit?.for };
  }
  // Files the typed fields under the Ticket's key. this.edit is set before any set_value, so the
  // change event that set_value fires lands under the right key (same ordering as cairn's composer).
  saveDraft(cx) {
    if (!this.edit) return;
    this.drafts.set(this.draftKey(this.edit.base.id), this.editFields());
    cx.notify();
  }

  // Enters edit mode from the loaded Ticket, or from its draft when one was kept.
  startEdit(cx) {
    const base = this.detail?.data;
    if (!base) return;
    const draft = this.drafts.get(this.draftKey(base.id));
    this.edit = { base, for: draft?.for ?? base.for, error: null };
    this.menu = null; this.notice = null;
    this.titleState.set_value(draft?.title ?? base.title);
    this.descState.set_value(draft?.description ?? base.description);
    this.planState.set_value(draft?.plan ?? base.plan);
    this.titleState.focus();
    cx.notify();
  }

  // Leaves edit mode and empties the widgets. The draft goes too when `discard` (cancel, Back, save).
  leaveEdit() {
    if (!this.edit) return;
    const key = this.draftKey(this.edit.base.id);
    this.edit = null;
    this.drafts.delete(key);
    for (const st of [this.titleState, this.descState, this.planState]) st.set_value("");
  }

  cancelEdit(cx) { this.leaveEdit(); cx.notify(); }

  // Validates, then runs only the changed flags. A failure shows in edit.error and touches no
  // field. editArgs gets the Ticket as loaded at startEdit, so a poll cannot change what counts as
  // changed.
  saveEdit(cx) {
    const e = this.edit;
    if (!e || e.saving) return;
    const fields = this.editFields();
    const errors = editErrors(fields);
    if (errors.length) { e.error = errors.map((x) => x.message).join("; "); cx.notify(); return; }
    const args = editArgs(e.base, fields);
    if (!args) { this.leaveEdit(); cx.notify(); return; }
    e.saving = true; e.error = null; cx.notify();
    const loc = location(), context = this.key(), id = e.base.id;
    cx.spawn(async (cx) => {
      try { await this.scree(args, this.writeOpts(e.base)); }
      catch (err) { e.saving = false; e.error = String(err.message ?? err); cx.notify(); return; }
      if (this.edit === e) this.leaveEdit();
      this.refresh(cx);
      if (this.detail?.id === id) await this.fetchDetail(id, loc, context, cx);
      cx.notify();
    });
  }

  createFields() {
    return {
      title: this.createTitle.value(), description: this.createDesc.value(), plan: this.createPlan.value(),
      criteria: this.createCrit.value(), for: this.create?.for, status: this.create?.status,
    };
  }
  createKey(cwd) { return JSON.stringify([this.context, cwd]); }
  // this.create is set before any set_value, so the change event set_value fires files under the right key.
  saveCreateDraft(cx) {
    if (!this.create) return;
    this.createDrafts.set(this.createKey(this.create.cwd), this.createFields());
    cx.notify();
  }

  // Opens the create form for a folder. No folder, no form: creating into an unrelated repo or into
  // LOCAL by accident is worse than not offering the button.
  startCreate(target, from, cx) {
    if (!target?.cwd) return;
    this.leaveEdit();
    this.detail = null; this.menu = null; this.notice = null;
    const draft = this.createDrafts.get(this.createKey(target.cwd));
    this.create = { cwd: target.cwd, on: location().on, label: target.label, from, for: draft?.for ?? "agent", status: draft?.status ?? "draft", error: null, saving: false };
    this.createTitle.set_value(draft?.title ?? ""); this.createDesc.set_value(draft?.description ?? "");
    this.createPlan.set_value(draft?.plan ?? ""); this.createCrit.set_value(draft?.criteria ?? "");
    this.createTitle.focus();
    cx.notify();
  }

  // Leaves the form and empties the widgets; `discard` (cancel, success) also drops the draft.
  leaveCreate(discard = false) {
    if (!this.create) return;
    const key = this.createKey(this.create.cwd);
    this.create = null;
    if (discard) this.createDrafts.delete(key);
    for (const st of [this.createTitle, this.createDesc, this.createPlan, this.createCrit]) st.set_value("");
  }
  // Back and Cancel both discard; a failed save is the only path that keeps text.
  cancelCreate(cx) { this.leaveCreate(true); cx.notify(); }

  saveCreate(cx) {
    const c = this.create;
    if (!c || c.saving) return;
    const fields = this.createFields();
    const errors = editErrors(fields);
    if (errors.length) { c.error = errors.map((x) => x.message).join("; "); cx.notify(); return; }
    c.saving = true; c.error = null; cx.notify();
    cx.spawn(async (cx) => {
      let id;
      try { id = JSON.parse(await this.scree(addArgs(fields), { cwd: c.cwd, on: c.on })).id; }
      catch (err) { c.saving = false; c.error = String(err.message ?? err); cx.notify(); return; }
      if (this.create === c) this.leaveCreate(true);
      this.refresh(cx);
      this.openDetail(id, cx);
    });
  }

  createForm(t) {
    const c = this.create, k = formKit(t);
    return v_flex().id("scree-create-form").px(12).py(10).gap(14)
      .child(h_flex().id("scree-create-target").gap(6).items_baseline().text_size(12)
        .child(div().flex_none().text_color(t.muted).child(`Filing into ${c.label}`))
        .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim).child(c.cwd)))
      .child(k.field("Title", k.box(Input.new(this.createTitle).h(30)), "required"))
      .child(k.field("Description", k.box(Textarea.new(this.createDesc))))
      .child(k.field("Plan", k.box(Textarea.new(this.createPlan))))
      .child(k.field("Acceptance criteria", k.box(Textarea.new(this.createCrit)), "one per line"))
      .child(h_flex().gap(24).flex_wrap()
        .child(k.field("For", k.chips("scree-create-for", c.for, ["agent", "human"], (v, cx) => { c.for = v; this.saveCreateDraft(cx); })))
        .child(k.field("Status", k.chips("scree-create-status", c.status, CREATE_STATUSES, (v, cx) => { c.status = v; this.saveCreateDraft(cx); }))))
      .when(Boolean(c.error), (el) => el.child(k.error("scree-create-error", c.error)));
  }

  renderCreate(t) {
    const back = div().id("scree-create-back").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
      .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.cancelCreate(cx)).child("\u2190 Back");
    const header = h_flex().flex_none().px(10).py(8).gap(8).items_center().border_b(1).border_color(t.border)
      .child(back).child(div().flex_1().min_w(0).truncate().font_weight(600).child("New ticket"));
    return v_flex().id("scree-create").size_full().min_h(0).rounded(10).overflow_hidden().bg(t.background).border_1().border_color(t.border).text_color(t.foreground).text_size(13)
      .track_focus(this.createHandle)
      .on_key_down((e, cx) => { if (e.key === "escape") { cx.stop_propagation(); this.cancelCreate(cx); } })
      .child(header)
      .child(v_flex().id("scree-body-create").flex_1().min_h(0).overflow_y_scroll().child(this.createForm(t)))
      .child(formKit(t).footer("scree-create-save", this.create.saving ? "Saving..." : "Create", (cx) => this.saveCreate(cx), "scree-create-cancel", (cx) => this.cancelCreate(cx)));
  }

  // The "New ticket" button. stop_propagation keeps the project header's fold toggle and the view's
  // dismiss handler from firing underneath it.
  newButton(t, id, text, target, from) {
    return div().id(id).flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
      .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover))
      .on_click((_e, cx) => { cx.stop_propagation(); this.startCreate(target, from, cx); }).child(text);
  }

  // Where a write runs: the Project root the Ticket belongs to, because scope::for_write ignores
  // --project whenever cwd is inside any repo, and add/start/done sweep the cwd's Project
  // (src/scope.rs, src/lib.rs). A LOCAL Ticket has no root, so fall back to the space's folder.
  writeOpts(t) { const loc = location(); return { cwd: t?.project?.root ?? loc.cwd, on: loc.on }; }

  // Runs a builder's argv and turns the outcome into view state: the notice strip on a refusal, and
  // a fresh poll either way so the card moves now instead of on the next tick.
  async runWrite(args, opts, cx) {
    try { await this.scree(args, opts); this.notice = null; }
    catch (e) { this.notice = { kind: e.kind ?? "error", message: String(e.message ?? e) }; }
    this.menu = null;
    this.refresh(cx);
    cx.notify();
  }

  // The status menu for one Ticket. Only one is open: picking a mark toggles its own menu, opening
  // another Ticket's replaces this one.
  toggleStatusMenu(id, status, cx) {
    this.menu = this.menu?.id === id ? null : { id, status };
    if (this.menu) this.menuFocus.focus();
    cx.notify();
  }

  dismissMenu(cx) { if (this.menu) { this.menu = null; cx.notify(); } }

  // A menu row: build the command, run it where the Ticket lives, and let runWrite report.
  chooseStatus(k, target, cx) {
    cx.spawn(async (cx) => { await this.runWrite(statusArgs(k.id, target), this.writeOpts(k), cx); });
  }

  // The mark as a trigger. Deliberately not inside mark(): the panel legend draws marks for
  // decoration and must stay inert. stop_propagation keeps the row's own click (open the detail)
  // and the view's dismiss handler from firing underneath the menu.
  markButton(t, k, status) {
    return div().id(`scree-mark-${k.id}`).flex_none().cursor_pointer()
      .on_click((_e, cx) => { cx.stop_propagation(); this.toggleStatusMenu(k.id, status, cx); })
      .child(this.mark(t, k, status));
  }

  // The anchored surface over a trigger, filled only while that Ticket's menu is the open one. Popup
  // owns no dismissal and no open state, so the body below carries the Escape handler and each view
  // puts dismissMenu on its outermost container for the click-outside case.
  statusMenu(t, k, status, trigger) {
    const open = this.menu?.id === k.id;
    return Popup.new(`scree-status-menu-${k.id}`, trigger).anchor("bottom_left")
      .track_focus(this.menuFocus)
      .on_key_down((e, cx) => { if (e.key === "escape") { cx.stop_propagation(); this.dismissMenu(cx); } })
      .when(open, (el) => el.content(this.menuBody(t, k, status)));
  }

  menuBody(t, k, status) {
    const targets = transitionsFor(status);
    return v_flex().id(`scree-menu-${k.id}`).py(4).rounded(6).border_1().border_color(t.border).bg(t.popover ?? t.background)
      .when(!targets.length, (el) => el.child(this.menuRow(t, k, null)))
      .children(targets.map((target) => this.menuRow(t, k, target)));
  }

  menuRow(t, k, target) {
    if (!target) return div().px(10).py(4).text_size(12).text_color(t.dim).child("Nothing reachable");
    return div().id(`scree-menu-${k.id}-${target.verb}`).px(10).py(4).text_size(12).cursor_pointer()
      .hover((s) => s.bg(t.hover))
      .on_click((_e, cx) => { cx.stop_propagation(); this.chooseStatus(k, target, cx); })
      .child(statusLabel(target.status));
  }

  // The last write's failure, shown until the next write succeeds or the view changes. Polls leave it
  // alone, so a refusal does not blink out before anyone reads it.
  noticeStrip(t) {
    return div().id("scree-notice").flex_none().px(10).py(5).text_size(11).text_color(t.blocked)
      .border_b(1).border_color(t.border).child(this.notice.message ?? this.notice.kind);
  }

  setFilterFor(v, cx) { this.filterFor = v; store.set("filterFor", v); cx.notify(); }
  toggleBlockedOnly(cx) { this.blockedOnly = !this.blockedOnly; store.set("blockedOnly", this.blockedOnly); cx.notify(); }
  // The data set changes (an extra list call), so refetch rather than only re-render.
  toggleShowAbandoned(cx) { this.showAbandoned = !this.showAbandoned; store.set("showAbandoned", this.showAbandoned); this.refresh(cx); cx.notify(); }
  filters() { return { forRole: this.filterFor, blockedOnly: this.blockedOnly }; }

  // `scree tui` in a new pane at the space's folder. Not offered for a space on another machine.
  // The space's folder is "" when it was never set, so fall back to the focused pane's cwd.
  tuiFolder() { return spaces().find((s) => s.active)?.folder || location().cwd || null; }
  canOpenTui() { return !location().on && Boolean(this.tuiFolder()); }
  openTui() {
    run("terminal.open", { argv: tuiArgv(this.bin), folder: this.tuiFolder(), place: "right" });
  }

  // The Project key for the panel's create target: the key its tickets carry, else the folder name.
  panelLabel() {
    const k = [...this.open, ...this.done].find((x) => x.project?.key);
    return k?.project?.key ?? (location().cwd ?? "").split("/").filter(Boolean).pop() ?? "";
  }

  stateLine() {
    const e = this.error;
    if (e?.kind === "no-binary") return e.message ?? `scree not found on ${location().on ?? "this machine"}`;
    if (e?.kind === "no-project") return "Not in a scree project";
    if (e) return e.message;
    if (!this.loaded) return "Loading...";
    if (this.mode === "board" ? !this.board.length : !this.open.length && !this.done.length) return "No tickets";
    return null;
  }

  toggleScope(cx) {
    this.allWorktrees = !this.allWorktrees;
    store.set("allWorktrees", this.allWorktrees);
    this.refresh(cx);
    cx.notify();
  }

  toggleFold(status, cx) {
    this.folded = { ...this.folded, [status]: !this.folded[status] };
    store.set("folded", this.folded);
    cx.notify();
  }

  toggleBoardFold(p, cx) {
    this.boardFolded = { ...this.boardFolded, [p.key]: nextBoardFold(this.boardFolded, p) };
    store.set("boardFolded", this.boardFolded);
    cx.notify();
  }

  toggleCell(key, cx) {
    this.boardExpanded = { ...this.boardExpanded, [key]: !this.boardExpanded[key] };
    store.set("boardExpanded", this.boardExpanded);
    cx.notify();
  }

  mark(t, ticket, status) {
    // Status marks, keyed by the board's columns plus "blocked". A new column needs an entry here,
    // a mark-<kind>.svg, and nothing else: COLUMNS covers fetching, labelling and counting it.
    const kind = ticket.blocked ? "blocked" : status;
    const color = { blocked: t.blocked, active: t.working, ready: t.idle, draft: t.dim, done: t.done, abandoned: t.dim }[kind] ?? t.dim;
    return svg(`mods/scree/mark-${kind}.svg`).size(14).flex_none().text_color(color);
  }

  // Criteria bar plus chips: the second line of a panel row and of a board card.
  cardMeta(t, k, indent) {
    const pct = criteriaPercent(k.criteria), total = k.criteria?.total ?? 0;
    const chipColor = { blocked: t.blocked, human: t.working, subtasks: t.muted, claimer: t.muted };
    return h_flex().gap(8).items_center().flex_wrap().pl(indent).text_size(11).text_color(t.dim)
      .child(h_flex().gap(5).items_center().flex_none()
        .child(h_flex().w(48).h(4).rounded(2).overflow_hidden().bg(t.border)
          .child(div().h(4).w(Math.round(48 * pct / 100)).bg(pct === 100 ? t.done : t.working)))
        .child(`${k.criteria?.checked ?? 0}/${total}`))
      .children(chips(k).map((c) => h_flex().gap(3).items_center().flex_none().text_color(chipColor[c.kind] ?? t.muted)
        .when(c.kind === "blocked", (el) => el.child(svg("mods/scree/mark-blocked.svg").size(10).flex_none()))
        .child(c.text)));
  }

  row(t, k, status, cx) {
    const sel = this.selected === k.id;
    const second = this.cardMeta(t, k, 22);
    return v_flex().id(`scree-row-${k.id}`).pl(26).pr(10).py(5).gap(2).cursor_pointer()
      .bg(sel ? t.selected : "#00000000").hover((s) => s.bg(sel ? t.selected : t.hover))
      .on_click((_e, cx) => this.openDetail(k.id, cx))
      .child(h_flex().gap(8).items_center().min_w(0)
        .child(this.statusMenu(t, k, status, this.markButton(t, k, status)))
        .child(div().flex_none().font_family("monospace").text_size(11).text_color(t.dim).child(k.id))
        .child(div().flex_1().min_w(0).truncate().text_size(13).child(k.title)))
      .child(second);
  }

  // A board card: the panel row narrowed. Click opens the detail view.
  card(t, k, status) {
    return v_flex().id(`scree-card-${k.id}`).px(8).py(6).gap(3).rounded(6).border_1().border_color(t.border).cursor_pointer()
      .hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.openDetail(k.id, cx))
      .child(h_flex().gap(6).items_center().min_w(0)
        .child(this.statusMenu(t, k, status, this.markButton(t, k, status)))
        .child(div().flex_none().font_family("monospace").text_size(11).text_color(t.dim).child(k.id)))
      .child(div().min_w(0).text_size(12).child(k.title))
      .child(this.cardMeta(t, k, 0));
  }

  // Builds the board model, mapping a throw from the pure builders onto the error shape a failed
  // poll uses so renderBoard draws a state line instead of the pane dying. Render must not write
  // this.error (the next poll would clear it anyway), so the error is returned, not stored.
  boardView() {
    try {
      const model = boardModel(this.board, { ...this.filters(), now: Date.now(), showAbandoned: this.showAbandoned });
      return { model, counts: boardCounts(model), error: null };
    } catch (e) {
      return { model: [], counts: boardCounts([]), error: { kind: "error", message: `Board failed to render: ${String(e?.message ?? e)}` } };
    }
  }

  renderBoard(t, state) {
    const { model, counts, error: viewError } = this.boardView();
    state = state ?? viewError?.message ?? null;
    const COL_W = 220;
    const columns = boardColumns(this.showAbandoned);
    const fatal = this.error && ["no-binary"].includes(this.error.kind);
    const header = h_flex().flex_none().px(10).py(8).gap(8).items_center().border_b(1).border_color(t.border)
      .child(svg("mods/scree/scree.svg").size(14).flex_none().text_color(t.muted))
      .child(div().font_weight(600).child("Scree board"))
      .child(div().id("scree-board-machine").flex_none().px(6).py(1).rounded(4).text_size(11).border_1().border_color(t.border).text_color(t.muted)
        .child(machineLabel(location().on)))
      .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim)
        .child(fatal || viewError ? "" : boardHeadline(counts)));
    const colHeader = h_flex().flex_none().gap(8).px(10).py(5).border_b(1).border_color(t.border).text_size(12).text_color(t.muted)
      .child(div().w(90).flex_none())
      .children(columns.map((c) => div().w(COL_W).flex_none().child(`${LABELS[c]} (${counts[c]})`)));
    const lanes = model.flatMap((p) => {
      const folded = projectFolded(this.boardFolded, p);
      const head = h_flex().id(`scree-proj-${p.key}`).px(10).py(5).gap(6).items_center().cursor_pointer().bg(t.hover).text_size(12)
        .on_click((_e, cx) => this.toggleBoardFold(p, cx))
        .child(div().w(10).flex_none().child(folded ? "\u25b8" : "\u25be"))
        .child(div().font_weight(600).child(p.key))
        .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim).child(`${p.root ?? "local"} (${p.count})`))
        .when(Boolean(p.root), (el) => el.child(this.newButton(t, `scree-new-${p.key}`, "New ticket", { cwd: p.root, label: p.key }, "board")));
      const rows = folded ? [] : p.lanes.map((l) =>
        h_flex().gap(8).px(10).py(6).items_start().border_b(1).border_color(t.border)
          .child(div().w(90).flex_none().truncate().text_size(11).text_color(t.dim).child(l.label))
          .children(columns.map((c) => {
            const ck = `${p.key}|${l.worktree ?? ""}|${c}`, open = Boolean(this.boardExpanded[ck]);
            const { shown, hidden } = cellVisible(l.cells[c], open);
            const toggle = (text) => div().id(`scree-cell-${ck}`).px(8).py(3).rounded(6).text_size(11).text_color(t.muted).cursor_pointer()
              .hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.toggleCell(ck, cx)).child(text);
            return v_flex().w(COL_W).flex_none().gap(6)
              .children(shown.map((k) => this.card(t, k, c)))
              .when(hidden > 0, (el) => el.child(toggle(`+${hidden} more`)))
              .when(open && l.cells[c].length > CELL_LIMIT, (el) => el.child(toggle("Show less")));
          })));
      return [head, ...rows];
    });
    let body;
    if (state) body = [div().p(10).text_color(this.error || viewError ? t.blocked : t.dim).child(state)];
    else if (!model.length) body = [div().p(10).text_color(t.dim).child("No tickets match the filters")];
    else body = [colHeader, ...lanes];
    // One container scrolls both axes; the inner column keeps a minimum width so narrow windows
    // scroll sideways instead of squeezing or clipping cards.
    const minW = 90 + columns.length * (COL_W + 8) + 28;
    return v_flex().id("scree-board").size_full().min_h(0).rounded(10).overflow_hidden().bg(t.background).border_1().border_color(t.border).text_color(t.foreground).text_size(13)
      .on_click((_e, cx) => this.dismissMenu(cx))
      .child(header)
      .when(this.notice, (el) => el.child(this.noticeStrip(t)))
      .child(this.filterBar(t))
      .child(v_flex().id("scree-board-body").flex_1().min_h(0).overflow_scroll()
        .child(v_flex().min_w(minW).children(body)));
  }

  section(t, g, total, cx) {
    const folded = Boolean(this.folded[g.status]);
    const label = `${LABELS[g.status] ?? g.status} (${total})`;
    return v_flex().child(
      h_flex().id(`scree-sec-${g.status}`).px(10).py(5).gap(6).items_center().cursor_pointer().text_color(t.muted).text_size(12)
        .hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.toggleFold(g.status, cx))
        .child(div().w(10).flex_none().child(folded ? "\u25b8" : "\u25be")).child(label))
      .when(!folded, (el) => el.children(g.tickets.map((k) => this.row(t, k, g.status, cx))));
  }

  chip(t, id, label, active, onClick) {
    return div().id(id).flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
      .bg(active ? t.selected : "#00000000").text_color(active ? t.foreground : t.muted).cursor_pointer()
      .hover((s) => s.bg(active ? t.selected : t.hover)).on_click((_e, cx) => onClick(cx)).child(label);
  }

  filterBar(t) {
    return h_flex().flex_none().px(10).py(5).gap(6).items_center().flex_wrap().border_b(1).border_color(t.border)
      .children([["all", "All"], ["agent", "Agent"], ["human", "Human"]].map(([v, label]) =>
        this.chip(t, `scree-for-${v}`, label, this.filterFor === v, (cx) => this.setFilterFor(v, cx))))
      .child(this.chip(t, "scree-blocked-only", "Blocked only", this.blockedOnly, (cx) => this.toggleBlockedOnly(cx)))
      .child(this.chip(t, "scree-show-abandoned", "Show abandoned", this.showAbandoned, (cx) => this.toggleShowAbandoned(cx)));
  }

  detailSection(t, title, items) {
    return v_flex().px(12).py(6).gap(4)
      .child(div().text_size(11).font_weight(600).text_color(t.muted).child(title))
      .children(items.length ? items : [div().text_size(12).text_color(t.dim).child("None")]);
  }

  refRow(t, b) {
    return h_flex().id(`scree-ref-${b.id}`).gap(8).items_center().px(4).py(2).rounded(4).cursor_pointer().hover((s) => s.bg(t.hover))
      .on_click((_e, cx) => this.openDetail(b.id, cx))
      .child(div().flex_none().font_family("monospace").text_size(11).text_color(t.dim).child(b.id))
      .child(div().flex_none().text_size(11).text_color(t.muted).child(b.status))
      .child(div().flex_1().min_w(0).text_size(12).child(b.title));
  }

  // The edit form: Title, Description, Plan, For. Criteria, comments and blockers stay read-only
  // below it. The red line sits above the buttons and never touches a widget value.
  editForm(t) {
    const e = this.edit, k = formKit(t);
    const releases = e.base.status === "active" && e.for !== e.base.for;
    return v_flex().id("scree-edit-form").px(12).py(10).gap(14)
      .child(k.field("Title", k.box(Input.new(this.titleState).h(30)), "required"))
      .child(k.field("Description", k.box(Textarea.new(this.descState))))
      .child(k.field("Plan", k.box(Textarea.new(this.planState))))
      .child(k.field("For", k.chips("scree-edit-for", e.for, ["agent", "human"], (v, cx) => { e.for = v; this.saveDraft(cx); })))
      .when(releases, (el) => el.child(div().text_size(11).text_color(t.dim).child("Changing For releases the claim to ready")))
      .when(Boolean(e.error), (el) => el.child(k.error("scree-edit-error", e.error)));
  }

  renderDetail(t) {
    const d = this.detail, m = d.data;
    const back = div().id("scree-back").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
      .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => (this.edit ? this.cancelEdit(cx) : this.closeDetail(cx))).child("← Back");
    const header = h_flex().flex_none().px(10).py(8).gap(8).items_center().border_b(1).border_color(t.border)
      .child(back)
      .child(div().flex_none().font_family("monospace").text_size(11).text_color(t.dim).child(d.id))
      .child(div().flex_1().min_w(0).truncate().font_weight(600).child(m?.title ?? ""))
      .when(Boolean(m) && !this.edit, (el) => el.child(div().id("scree-edit").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
        .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => { cx.stop_propagation(); this.startEdit(cx); }).child("Edit")))
      .when(this.canOpenTui(), (el) => el.child(div().id("scree-open-tui").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
        .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, _cx) => this.openTui()).child("Open in scree tui")));
    let body;
    if (!m) body = [div().p(10).text_color(d.error ? t.blocked : t.dim).child(d.error ?? "Loading...")];
    else {
      // The status chip doubles as the menu trigger; the rest stay decoration.
      const statusChip = this.statusMenu(t, m, m.status, div().id(`scree-mark-${m.id}`).flex_none().px(6).py(1).rounded(4)
        .text_size(11).border_1().border_color(t.border).cursor_pointer().text_color(t.muted)
        .on_click((_e, cx) => { cx.stop_propagation(); this.toggleStatusMenu(m.id, m.status, cx); })
        .child(m.status));
      const chipEls = [statusChip].concat([m.for ? `for ${m.for}` : null, m.blocked ? "blocked" : null, m.claimedBy ? `claimed ${m.claimedBy}` : null]
        .filter(Boolean)
        .map((x) => div().flex_none().px(6).py(1).rounded(4).text_size(11).border_1().border_color(t.border).text_color(x === "blocked" ? t.blocked : t.muted).child(x)));
      body = [
        d.error ? div().px(12).py(4).text_size(11).text_color(t.blocked).child(d.error) : null,
        h_flex().px(12).py(6).gap(6).flex_wrap().children(chipEls),
        ...(this.edit ? [this.editForm(t)] : [
          this.detailSection(t, "Description", m.description ? [div().text_size(12).child(m.description)] : []),
          this.detailSection(t, "Plan", m.plan ? [div().font_family("monospace").text_size(11).child(m.plan)] : []),
        ]),
        this.detailSection(t, `Criteria (${m.criteria.filter((c) => c.checked).length}/${m.criteria.length})`, m.criteria.map((c) =>
          v_flex().gap(1).py(2)
            .child(h_flex().gap(6).items_start()
              .child(svg(`mods/scree/mark-${c.checked ? "done" : "ready"}.svg`).size(14).flex_none().text_color(c.checked ? t.done : t.idle))
              .child(div().flex_1().min_w(0).text_size(12).child(`${c.number}. ${c.text}`)))
            .when(Boolean(c.evidence), (el) => el.child(div().pl(20).text_size(11).text_color(t.dim).child(`Evidence: ${c.evidence}`))))),
        this.detailSection(t, `Comments (${m.comments.length})`, m.comments.map((c) =>
          v_flex().gap(1).py(3)
            .child(div().text_size(11).text_color(t.dim).child(`${c.author}${c.role ? ` (${c.role})` : ""} ${c.created_at}`))
            .child(div().text_size(12).child(c.body)))),
        this.detailSection(t, "Blocked by", m.blockedBy.map((b) => this.refRow(t, b))),
        this.detailSection(t, "Subtasks", m.subtasks.map((b) => this.refRow(t, b))),
      ].filter(Boolean);
    }
    return v_flex().id("scree-detail").size_full().min_h(0).rounded(10).overflow_hidden().bg(t.background).border_1().border_color(t.border).text_color(t.foreground).text_size(13)
      .track_focus(this.editHandle)
      .on_key_down((e, cx) => { if (this.edit && e.key === "escape") { cx.stop_propagation(); this.cancelEdit(cx); } })
      .on_click((_e, cx) => this.dismissMenu(cx))
      .child(header)
      .when(this.notice, (el) => el.child(this.noticeStrip(t)))
      .child(v_flex().id("scree-body-detail").flex_1().min_h(0).overflow_y_scroll().children(body))
      .when(Boolean(this.edit), (el) => el.child(formKit(t).footer("scree-edit-save", this.edit.saving ? "Saving..." : "Save", (cx) => this.saveEdit(cx), "scree-edit-cancel", (cx) => this.cancelEdit(cx))));
  }

  render(cx) {
    if (this.key() !== this.context) this.refresh(cx);
    const t = theme();
    if (this.create) return this.renderCreate(t);
    if (this.detail) return this.renderDetail(t);
    const filtered = applyFilters(this.open, this.filters()), filteredDone = applyFilters(this.done, this.filters());
    let state = this.stateLine();
    if (!state && !filtered.length && !filteredDone.length && !(this.showAbandoned && this.abandoned.length)) state = "No tickets match the filters";
    if (this.mode === "board") return this.renderBoard(t, this.stateLine());
    const fatal = this.error && ["no-binary", "no-project"].includes(this.error.kind);
    const groups = new Map(groupByStatus(filtered).map((g) => [g.status, g]));
    const sections = SECTIONS.map((s) => {
      if (s === "done") {
        const tickets = doneRecent(filteredDone, DONE_SHOWN);
        return tickets.length ? { g: { status: "done", tickets }, total: filteredDone.length } : null;
      }
      const g = groups.get(s);
      return g ? { g, total: g.tickets.length } : null;
    }).filter(Boolean);
    // Abandoned comes after Done, only when the chip is on, and never enters the header counts.
    const filteredAbandoned = this.showAbandoned ? applyFilters(this.abandoned, this.filters()) : [];
    if (filteredAbandoned.length) sections.push({ g: { status: "abandoned", tickets: filteredAbandoned }, total: filteredAbandoned.length });
    const counts = countsLine([...groups.values()]);
    const header = h_flex().flex_none().px(10).py(8).gap(8).items_center().border_b(1).border_color(t.border)
      .child(svg("mods/scree/scree.svg").size(14).flex_none().text_color(t.muted))
      .child(div().font_weight(600).child("Scree"))
      .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim).child(fatal ? "" : counts))
      .when(!fatal, (el) => el.child(div().id("scree-scope").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
        .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.toggleScope(cx))
        .child(this.allWorktrees ? "All worktrees" : "This worktree")))
      .when(!fatal && Boolean(location().cwd), (el) => el.child(this.newButton(t, "scree-new", "New", { cwd: location().cwd, label: this.panelLabel() }, "panel")))
      .when(!fatal && this.canOpenTui(), (el) => el.child(div().id("scree-open-tui").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
        .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, _cx) => this.openTui()).child("tui")));
    const body = state
      ? [div().p(10).text_color(this.error ? t.blocked : t.dim).child(state)]
      : sections.map((x) => this.section(t, x.g, x.total, cx));
    // The legend is panel-only and lists the marks Active-first; its statuses are COLUMNS' plus "blocked".
    const legend = h_flex().flex_none().px(10).py(6).gap(12).gap_y(4).items_center().flex_wrap().border_t(1).border_color(t.border).text_size(11).text_color(t.dim)
      .children([...SECTIONS.map((s) => [s, LABELS[s]]), ["blocked", "Blocked"]].map(([kind, label]) =>
        h_flex().gap(4).items_center().flex_none()
          .child(this.mark(t, { blocked: kind === "blocked" }, kind))
          .child(label)));
    return v_flex().id("scree-panel").size_full().min_h(0).rounded(10).overflow_hidden().bg(t.background).border_1().border_color(t.border).text_color(t.foreground).text_size(13)
      .on_click((_e, cx) => this.dismissMenu(cx))
      .child(header)
      .when(this.notice, (el) => el.child(this.noticeStrip(t)))
      .when(!fatal, (el) => el.child(this.filterBar(t)))
      .child(v_flex().id("scree-body-panel").flex_1().min_h(0).overflow_y_scroll().children(body))
      .when(!state, (el) => el.child(legend));
  }
}
