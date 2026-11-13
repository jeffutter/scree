import { View, div, svg } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { spaces, panes, focused, exec, theme, run, props as layoutProps } from "herdr";

// Scree pure helpers: no host calls. scree.test.mjs slices the text between the two markers
// below, so keep this section free of imports and host globals.
// --- pure begin ---
const STATUS_ORDER = ["active", "ready", "draft", "done", "abandoned"];

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

// "3 active, 5 ready, 2 draft" from groupByStatus output; done and empty groups are skipped.
function countsLine(groups) {
  return (groups ?? []).filter((g) => ["active", "ready", "draft"].includes(g.status) && g.tickets.length)
    .map((g) => `${g.tickets.length} ${g.status}`).join(", ");
}

// The two list calls of one poll: open tickets, and done tickets (for the Done count and tail).
function listArgs(allWorktrees) {
  const scope = allWorktrees ? ["--all-worktrees"] : [];
  return {
    open: ["--json", "list", ...scope, "--status", "draft", "--status", "ready", "--status", "active", "--limit", "200"],
    done: ["--json", "list", ...scope, "--status", "done", "--limit", "500"],
  };
}

// Filters shared by the panel and the board: forRole "all" | "agent" | "human", blockedOnly bool.
function applyFilters(tickets, { forRole = "all", blockedOnly = false } = {}) {
  return (tickets ?? []).filter((t) => (forRole === "all" || t.for === forRole) && (!blockedOnly || t.blocked));
}

// The detail call: read-only `scree --json show ID`.
function showArgs(id) { return ["--json", "show", String(id)]; }

// The "Open in scree tui" argv.
function tuiArgv(bin) { return [bin || "scree", "tui"]; }

// Subcommands that change the database. scree() refuses these; the mod only lists, shows and runs tui.
const WRITE_COMMANDS = ["add", "edit", "check", "uncheck", "start", "done", "release", "abandon", "comment", "dep", "reorder", "criteria", "ready", "draft", "move", "reopen", "init", "sweep"];
// First non-flag argument is the subcommand (the flags used here take no value).
function isWriteCommand(args) {
  const sub = (args ?? []).find((a) => !String(a).startsWith("-"));
  return sub !== undefined && WRITE_COMMANDS.includes(sub);
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
  };
}

// Board columns, left to right.
const BOARD_COLUMNS = ["draft", "ready", "active", "done"];

// The board's single call: every Project and Worktree (`--all-projects` already means both; it
// conflicts with `--all-worktrees`). The default limit is 50, so pass a large one. Done is
// ordered by position, so boardModel cuts it to the last days client-side.
function boardListArgs() {
  return ["--json", "list", "--all-projects", ...BOARD_COLUMNS.flatMap((s) => ["--status", s]), "--limit", "1000"];
}

function worktreeLabel(path) {
  if (!path) return "root";
  const parts = String(path).split("/").filter(Boolean);
  return parts[parts.length - 1] ?? String(path);
}

// "this computer" for the local machine, else the machine name.
function machineLabel(on) { return on ? String(on) : "this computer"; }

// Projects as swimlanes: [{key, root, name, count, lanes: [{worktree, label, cells: {draft, ready, active, done}}]}].
// Filters apply first; abandoned, archived and done older than doneDays are dropped. Projects sort
// by key with LOCAL (no root) last; lanes list the root checkout first, then worktrees by path.
// Tickets keep list order (position) inside a cell. Empty lanes and projects are omitted.
function boardModel(tickets, { forRole = "all", blockedOnly = false, now = Date.now(), doneDays = 7 } = {}) {
  const cutoff = now - doneDays * 86400000;
  const projects = new Map();
  for (const t of applyFilters(tickets, { forRole, blockedOnly })) {
    if (!BOARD_COLUMNS.includes(t.status) || t.archived_at) continue;
    if (t.status === "done") {
      const at = Date.parse(t.done_at ?? "");
      if (!(at >= cutoff)) continue;
    }
    const key = t.project?.key ?? "LOCAL", root = t.project?.root ?? null;
    if (!projects.has(key)) projects.set(key, { key, root, name: key, count: 0, lanes: new Map() });
    const p = projects.get(key);
    const wt = t.worktree ?? null;
    if (!p.lanes.has(wt)) p.lanes.set(wt, { worktree: wt, label: worktreeLabel(wt), cells: { draft: [], ready: [], active: [], done: [] } });
    p.lanes.get(wt).cells[t.status].push(t);
    p.count++;
  }
  // Done shows newest first so the collapsed view keeps the most recent cards.
  for (const p of projects.values()) for (const l of p.lanes.values())
    l.cells.done.sort((a, b) => Date.parse(b.done_at ?? "") - Date.parse(a.done_at ?? ""));
  const byPath = (a, b) => String(a.worktree).localeCompare(String(b.worktree));
  return [...projects.values()]
    .map((p) => ({ ...p, lanes: [...p.lanes.values()].sort((a, b) => (a.worktree === null ? -1 : b.worktree === null ? 1 : byPath(a, b))) }))
    .sort((a, b) => (a.root === null) - (b.root === null) || a.key.localeCompare(b.key));
}

// A board cell shows at most `limit` cards until expanded, so a long column (Done with 50 cards)
// does not push the next project far down the page. Returns {shown, hidden}.
const CELL_LIMIT = 5;
function cellVisible(cards, expanded, limit = CELL_LIMIT) {
  if (expanded || cards.length <= limit) return { shown: cards, hidden: 0 };
  return { shown: cards.slice(0, limit), hidden: cards.length - limit };
}

// Per-column totals across all projects in a boardModel result.
function boardCounts(model) {
  const out = { draft: 0, ready: 0, active: 0, done: 0 };
  for (const p of model) for (const l of p.lanes) for (const c of BOARD_COLUMNS) out[c] += l.cells[c].length;
  return out;
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

class ScreeError extends Error {
  constructor(kind, message) { super(message); this.kind = kind; }
}

const SECTIONS = ["active", "ready", "draft", "done"];
const LABELS = { active: "Active", ready: "Ready", draft: "Draft", done: "Done" };

export default class Scree extends View {
  init(props, cx) {
    // init receives no props for a layout block; the host exposes them through props().
    this.mode = (props ?? layoutProps())?.view === "board" ? "board" : "panel";
    this.open = []; this.done = []; this.board = []; this.error = null; this.loaded = false; this.context = null; this.bin = null;
    this.loading = false; this.parked = new Map();
    this.allWorktrees = store.get("allWorktrees", false) === true;
    this.folded = store.get("folded", { done: true });
    this.boardFolded = store.get("boardFolded", {});
    this.boardExpanded = store.get("boardExpanded", {});
    this.selected = null;
    this.filterFor = ["all", "agent", "human"].includes(store.get("filterFor", "all")) ? store.get("filterFor", "all") : "all";
    this.blockedOnly = store.get("blockedOnly", false) === true;
    this.detail = null; // {id, data, error, loading} while a ticket's detail is open
    this.refresh(cx);
    cx.timer.every(POLL_MS, (cx) => this.refresh(cx));
  }

  // Parked state is per space/folder and per scope, so toggling the scope re-polls at once.
  key() { return JSON.stringify({ ...location(), all: this.allWorktrees }); }

  // Runs scree in the active space's folder (on its machine) so project scoping follows the space.
  async scree(args, { cwd, on }) {
    if (isWriteCommand(args)) throw new ScreeError("error", `refusing write command: ${args.join(" ")}`);
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
        throw new ScreeError(classifyError(text, r.code), text || `scree exited ${r.code}`);
      }
      return r.stdout;
    }
    this.bin = null;
    throw new ScreeError("no-binary", `scree not found on ${on ?? "this machine"}${last ? ` (${last.message ?? last})` : ""}`);
  }

  refresh(cx) {
    if (this.loading) return;
    const loc = location(), context = this.key();
    if (context !== this.context) {
      // A different space, folder or scope: park this one's state and restore the target's.
      if (this.context !== null) this.parked.set(this.context, { open: this.open, done: this.done, board: this.board, error: this.error, loaded: this.loaded });
      const s = this.parked.get(context) ?? { open: [], done: [], board: [], error: null, loaded: false };
      this.context = context; this.open = s.open; this.done = s.done; this.board = s.board; this.error = s.error; this.loaded = s.loaded;
      this.bin = null;
      this.detail = null;
    }
    if (!loc.cwd && this.mode !== "board") { this.error = { kind: "no-project" }; this.loaded = true; cx.notify(); return; }
    this.loading = true;
    const args = listArgs(this.allWorktrees);
    cx.spawn(async (cx) => {
      try {
        if (this.mode === "board") {
          // One call on the focused space's machine; all-projects needs no repo, so a missing cwd is fine.
          const all = JSON.parse(await this.scree(boardListArgs(), loc));
          if (this.key() !== context) return;
          this.board = all.tickets ?? []; this.error = null; this.loaded = true;
          return;
        }
        const open = JSON.parse(await this.scree(args.open, loc));
        const done = JSON.parse(await this.scree(args.done, loc));
        if (this.key() !== context) return;
        this.open = open.tickets ?? []; this.done = done.tickets ?? []; this.error = null; this.loaded = true;
      } catch (e) {
        if (this.key() === context) { this.error = { kind: e.kind ?? "error", message: String(e.message ?? e) }; this.loaded = true; }
      } finally { this.loading = false; cx.notify(); }
      // Keep an open detail current (new evidence and comments) on every poll.
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
    this.selected = id;
    this.detail = { id, data: null, error: null, loading: true };
    cx.notify();
    const loc = location(), context = this.key();
    cx.spawn((cx) => this.fetchDetail(id, loc, context, cx));
  }

  closeDetail(cx) { this.detail = null; cx.notify(); }

  setFilterFor(v, cx) { this.filterFor = v; store.set("filterFor", v); cx.notify(); }
  toggleBlockedOnly(cx) { this.blockedOnly = !this.blockedOnly; store.set("blockedOnly", this.blockedOnly); cx.notify(); }
  filters() { return { forRole: this.filterFor, blockedOnly: this.blockedOnly }; }

  // `scree tui` in a new pane at the space's folder. Not offered for a space on another machine.
  // The space's folder is "" when it was never set, so fall back to the focused pane's cwd.
  tuiFolder() { return spaces().find((s) => s.active)?.folder || location().cwd || null; }
  canOpenTui() { return !location().on && Boolean(this.tuiFolder()); }
  openTui() {
    run("terminal.open", { argv: tuiArgv(this.bin), folder: this.tuiFolder(), place: "right" });
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

  toggleBoardFold(key, cx) {
    this.boardFolded = { ...this.boardFolded, [key]: !this.boardFolded[key] };
    store.set("boardFolded", this.boardFolded);
    cx.notify();
  }

  toggleCell(key, cx) {
    this.boardExpanded = { ...this.boardExpanded, [key]: !this.boardExpanded[key] };
    store.set("boardExpanded", this.boardExpanded);
    cx.notify();
  }

  mark(t, ticket, status) {
    const kind = ticket.blocked ? "blocked" : status;
    const color = { blocked: t.blocked, active: t.working, ready: t.idle, draft: t.dim, done: t.done }[kind] ?? t.dim;
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
        .child(this.mark(t, k, status))
        .child(div().flex_none().font_family("monospace").text_size(11).text_color(t.dim).child(k.id))
        .child(div().flex_1().min_w(0).truncate().text_size(13).child(k.title)))
      .child(second);
  }

  // A board card: the panel row narrowed. Click opens the detail view.
  card(t, k, status) {
    return v_flex().id(`scree-card-${k.id}`).px(8).py(6).gap(3).rounded(6).border_1().border_color(t.border).cursor_pointer()
      .hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.openDetail(k.id, cx))
      .child(h_flex().gap(6).items_center().min_w(0)
        .child(this.mark(t, k, status))
        .child(div().flex_none().font_family("monospace").text_size(11).text_color(t.dim).child(k.id)))
      .child(div().min_w(0).text_size(12).child(k.title))
      .child(this.cardMeta(t, k, 0));
  }

  renderBoard(t, state) {
    const model = boardModel(this.board, { ...this.filters(), now: Date.now() });
    const counts = boardCounts(model);
    const COL_W = 220;
    const fatal = this.error && ["no-binary"].includes(this.error.kind);
    const header = h_flex().flex_none().px(10).py(8).gap(8).items_center().border_b(1).border_color(t.border)
      .child(svg("mods/scree/scree.svg").size(14).flex_none().text_color(t.muted))
      .child(div().font_weight(600).child("Scree board"))
      .child(div().id("scree-board-machine").flex_none().px(6).py(1).rounded(4).text_size(11).border_1().border_color(t.border).text_color(t.muted)
        .child(machineLabel(location().on)))
      .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim)
        .child(fatal ? "" : `${counts.active} active, ${counts.ready} ready, ${counts.draft} draft`));
    const colHeader = h_flex().flex_none().gap(8).px(10).py(5).border_b(1).border_color(t.border).text_size(12).text_color(t.muted)
      .child(div().w(90).flex_none())
      .children(BOARD_COLUMNS.map((c) => div().w(COL_W).flex_none().child(`${LABELS[c]} (${counts[c]})`)));
    const lanes = model.flatMap((p) => {
      const folded = Boolean(this.boardFolded[p.key]);
      const head = h_flex().id(`scree-proj-${p.key}`).px(10).py(5).gap(6).items_center().cursor_pointer().bg(t.hover).text_size(12)
        .on_click((_e, cx) => this.toggleBoardFold(p.key, cx))
        .child(div().w(10).flex_none().child(folded ? "\u25b8" : "\u25be"))
        .child(div().font_weight(600).child(p.key))
        .child(div().min_w(0).truncate().text_size(11).text_color(t.dim).child(`${p.root ?? "local"} (${p.count})`));
      const rows = folded ? [] : p.lanes.map((l) =>
        h_flex().gap(8).px(10).py(6).items_start().border_b(1).border_color(t.border)
          .child(div().w(90).flex_none().truncate().text_size(11).text_color(t.dim).child(l.label))
          .children(BOARD_COLUMNS.map((c) => {
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
    if (state) body = [div().p(10).text_color(this.error ? t.blocked : t.dim).child(state)];
    else if (!model.length) body = [div().p(10).text_color(t.dim).child("No tickets match the filters")];
    else body = [colHeader, ...lanes];
    // One container scrolls both axes; the inner column keeps a minimum width so narrow windows
    // scroll sideways instead of squeezing or clipping cards.
    const minW = 90 + BOARD_COLUMNS.length * (COL_W + 8) + 28;
    return v_flex().id("scree-board").size_full().min_h(0).rounded(10).overflow_hidden().bg(t.background).border_1().border_color(t.border).text_color(t.foreground).text_size(13)
      .child(header)
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
      .child(this.chip(t, "scree-blocked-only", "Blocked only", this.blockedOnly, (cx) => this.toggleBlockedOnly(cx)));
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

  renderDetail(t) {
    const d = this.detail, m = d.data;
    const back = div().id("scree-back").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
      .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.closeDetail(cx)).child("← Back");
    const header = h_flex().flex_none().px(10).py(8).gap(8).items_center().border_b(1).border_color(t.border)
      .child(back)
      .child(div().flex_none().font_family("monospace").text_size(11).text_color(t.dim).child(d.id))
      .child(div().flex_1().min_w(0).truncate().font_weight(600).child(m?.title ?? ""))
      .when(this.canOpenTui(), (el) => el.child(div().id("scree-open-tui").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
        .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, _cx) => this.openTui()).child("Open in scree tui")));
    let body;
    if (!m) body = [div().p(10).text_color(d.error ? t.blocked : t.dim).child(d.error ?? "Loading...")];
    else {
      const chipEls = [m.status, m.for ? `for ${m.for}` : null, m.blocked ? "blocked" : null, m.claimedBy ? `claimed ${m.claimedBy}` : null].filter(Boolean)
        .map((x) => div().flex_none().px(6).py(1).rounded(4).text_size(11).border_1().border_color(t.border).text_color(x === "blocked" ? t.blocked : t.muted).child(x));
      body = [
        d.error ? div().px(12).py(4).text_size(11).text_color(t.blocked).child(d.error) : null,
        h_flex().px(12).py(6).gap(6).flex_wrap().children(chipEls),
        this.detailSection(t, "Description", m.description ? [div().text_size(12).child(m.description)] : []),
        this.detailSection(t, "Plan", m.plan ? [div().font_family("monospace").text_size(11).child(m.plan)] : []),
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
      .child(header)
      .child(v_flex().id("scree-body-detail").flex_1().min_h(0).overflow_y_scroll().children(body));
  }

  render(cx) {
    if (this.key() !== this.context) this.refresh(cx);
    const t = theme();
    if (this.detail) return this.renderDetail(t);
    const filtered = applyFilters(this.open, this.filters()), filteredDone = applyFilters(this.done, this.filters());
    let state = this.stateLine();
    if (!state && !filtered.length && !filteredDone.length) state = "No tickets match the filters";
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
    const counts = countsLine([...groups.values()]);
    const header = h_flex().flex_none().px(10).py(8).gap(8).items_center().border_b(1).border_color(t.border)
      .child(svg("mods/scree/scree.svg").size(14).flex_none().text_color(t.muted))
      .child(div().font_weight(600).child("Scree"))
      .child(div().flex_1().min_w(0).truncate().text_size(11).text_color(t.dim).child(fatal ? "" : counts))
      .when(!fatal, (el) => el.child(div().id("scree-scope").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
        .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.toggleScope(cx))
        .child(this.allWorktrees ? "All worktrees" : "This worktree")))
      .when(!fatal && this.canOpenTui(), (el) => el.child(div().id("scree-open-tui").flex_none().px(8).py(2).rounded(6).text_size(11).border_1().border_color(t.border)
        .text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, _cx) => this.openTui()).child("tui")));
    const body = state
      ? [div().p(10).text_color(this.error ? t.blocked : t.dim).child(state)]
      : sections.map((x) => this.section(t, x.g, x.total, cx));
    const legend = h_flex().flex_none().px(10).py(6).gap(12).gap_y(4).items_center().flex_wrap().border_t(1).border_color(t.border).text_size(11).text_color(t.dim)
      .children([["active", "Active"], ["ready", "Ready"], ["draft", "Draft"], ["done", "Done"], ["blocked", "Blocked"]].map(([kind, label]) =>
        h_flex().gap(4).items_center().flex_none()
          .child(this.mark(t, { blocked: kind === "blocked" }, kind))
          .child(label)));
    return v_flex().id("scree-panel").size_full().min_h(0).rounded(10).overflow_hidden().bg(t.background).border_1().border_color(t.border).text_color(t.foreground).text_size(13)
      .child(header)
      .when(!fatal, (el) => el.child(this.filterBar(t)))
      .child(v_flex().id("scree-body-panel").flex_1().min_h(0).overflow_y_scroll().children(body))
      .when(!state, (el) => el.child(legend));
  }
}
