// Run: node scree.test.mjs. Loads the pure helpers out of main.js (which imports host-only
// modules, so it cannot be imported directly).
import { readFileSync, existsSync } from "node:fs";
import assert from "node:assert/strict";

const src = readFileSync(new URL("./main.js", import.meta.url), "utf8");
const from = src.indexOf("// --- pure begin ---"), to = src.indexOf("// --- pure end ---");
assert(from > 0 && to > from, "markers not found in main.js");
const PURE_NAMES = ["STATUS_ORDER", "criteriaPercent", "groupByStatus", "classifyError", "doneRecent", "chips", "countsLine", "listArgs", "shortClaimer", "applyFilters", "showArgs", "detailModel", "tuiArgv", "COLUMNS", "BOARD_COLUMNS", "OPEN_STATUSES", "LABELS", "boardListArgs", "boardModel", "boardCounts", "projectFolded", "nextBoardFold", "machineLabel", "worktreeLabel", "cellVisible", "CELL_LIMIT", "CLOSED_STATUSES", "SETTLEABLE", "COMMANDS", "TRANSITIONS", "transitionsFor", "statusLabel", "isBuilt", "WRITE_FLAGS", "ABANDON_REASON", "statusArgs", "editArgs", "editErrors", "addArgs", "criteriaLines", "CREATE_STATUSES", "textFlag", "parseScreeError", "SECTIONS", "boardHeadline", "boardColumns", "OPTIONAL_COLUMNS"];
const { STATUS_ORDER, criteriaPercent, groupByStatus, classifyError, doneRecent, chips, countsLine, listArgs, shortClaimer, applyFilters, showArgs, detailModel, tuiArgv, COLUMNS, BOARD_COLUMNS, OPEN_STATUSES, LABELS, boardListArgs, boardModel, boardCounts, projectFolded, nextBoardFold, machineLabel, worktreeLabel, cellVisible, CELL_LIMIT, CLOSED_STATUSES, SETTLEABLE, COMMANDS, TRANSITIONS, transitionsFor, statusLabel, isBuilt, WRITE_FLAGS, ABANDON_REASON, statusArgs, editArgs, editErrors, addArgs, criteriaLines, CREATE_STATUSES, textFlag, parseScreeError, SECTIONS, boardHeadline, boardColumns, OPTIONAL_COLUMNS } = new Function(
  `${src.slice(from, to)}; return { ${PURE_NAMES.join(", ")} };`)();

// Compile the pure region again with these edits applied, for the mutant runs below. Throws if an
// edit matches nothing, so a mutant cannot silently stop mutating.
function pureWith(...edits) {
  let pure = src.slice(from, to);
  for (const [find, replace] of edits) {
    assert(pure.includes(find), `mutant did not match: ${find}`);
    pure = pure.replace(find, replace);
  }
  return new Function(`${pure}; return { ${PURE_NAMES.join(", ")} };`)();
}

// The statuses a *list* argv asks for, in the order they appear. Named apart from statusArgs below,
// which builds the argv for one status change.
const statusFlagsIn = (argv) => argv.filter((_, i) => i > 0 && argv[i - 1] === "--status");

assert.deepEqual(STATUS_ORDER, ["active", "ready", "draft", "done", "abandoned"]);

assert.equal(criteriaPercent({ checked: 0, total: 0 }), 0);
assert.equal(criteriaPercent(undefined), 0);
assert.equal(criteriaPercent({ checked: 1, total: 3 }), 33);
assert.equal(criteriaPercent({ checked: 3, total: 3 }), 100);
assert.equal(criteriaPercent([]), 0);
assert.equal(criteriaPercent([{ checked: true }, { checked: false }]), 50);

const t = (id, status) => ({ id, status });
let g = groupByStatus([t("a", "draft"), t("b", "active"), t("c", "draft"), t("d", "done")]);
assert.deepEqual(g.map((x) => x.status), ["active", "draft", "done"]);
assert.deepEqual(g[1].tickets.map((x) => x.id), ["a", "c"]);
assert.deepEqual(groupByStatus([]), []);
assert.deepEqual(groupByStatus([t("x", "weird"), t("y", "ready")]).map((x) => x.status), ["ready", "weird"]);

assert.equal(classifyError("", 127), "no-binary");
assert.equal(classifyError("sh: scree: command not found", 1), "no-binary");
assert.equal(classifyError("not in a scree project", 2), "no-project");
assert.equal(classifyError("no Project at /x", 3), "no-project");
assert.equal(classifyError("database is locked", 1), "error");
const d = (id, at) => ({ id, done_at: at });
assert.deepEqual(doneRecent([d("a", "2026-01-01"), d("b", "2026-03-01"), d("c", "2026-02-01")], 2).map((x) => x.id), ["b", "c"]);
assert.equal(doneRecent([1, 2, 3, 4, 5, 6, 7].map((i) => d(`t${i}`, `2026-01-0${i}`))).length, 5);
assert.deepEqual(doneRecent(undefined), []);

assert.deepEqual(chips({}), []);
assert.deepEqual(chips({ blocked: true, for: "human", subtasks: [1, 2], claimed_by: { session: "abc" } }).map((c) => c.kind), ["blocked", "human", "subtasks", "claimer"]);
assert.deepEqual(chips({ subtasks: [] , for: "agent", claimed_by: null }), []);
assert.equal(chips({ subtasks: [1] })[0].text, "1 subtask");
assert.equal(chips({ claimed_by: { author: "bob" } })[0].text, "bob");
assert.equal(shortClaimer(null), null);
assert.equal(shortClaimer({ session: "0123456789abcdef" }), "01234567");

assert.equal(countsLine([{ status: "active", tickets: [1] }, { status: "ready", tickets: [] }, { status: "draft", tickets: [1, 2] }, { status: "done", tickets: [1] }]), "1 active, 2 draft");
assert.equal(countsLine([]), "");

assert(!listArgs(false).open.includes("--all-worktrees"));
assert(listArgs(true).open.includes("--all-worktrees") && listArgs(true).done.includes("--all-worktrees"));
assert(listArgs(false).done.includes("done"));
const f = (id, forRole, blocked) => ({ id, for: forRole, blocked });
const fs = [f("a", "agent", false), f("b", "human", false), f("c", "agent", true), f("d", "human", true)];
const ids = (x) => x.map((y) => y.id).join("");
assert.equal(ids(applyFilters(fs, {})), "abcd");
assert.equal(ids(applyFilters(fs, { forRole: "all", blockedOnly: false })), "abcd");
assert.equal(ids(applyFilters(fs, { forRole: "agent" })), "ac");
assert.equal(ids(applyFilters(fs, { forRole: "human" })), "bd");
assert.equal(ids(applyFilters(fs, { blockedOnly: true })), "cd");
assert.equal(ids(applyFilters(fs, { forRole: "human", blockedOnly: true })), "d");
assert.deepEqual(applyFilters(undefined, { forRole: "agent" }), []);

assert.deepEqual(showArgs("SCR-6"), ["--json", "show", "SCR-6"]);
const dm = detailModel({
  id: "SCR-6", title: "T", status: "active", for: "agent", blocked: true, claimed_by: { author: "agent", session: "0123456789abcdef" },
  description: "desc", plan: "line1\nline2",
  criteria: [{ number: 1, text: "c1", checked: true, evidence: "ran x" }, { number: 2, text: "c2", checked: false, evidence: null }],
  comments: [{ author: "agent", role: "agent", body: "hi", created_at: "2026-01-01" }],
  blocked_by: [{ id: "SCR-5", status: "done", title: "P", worktree: null }], subtasks: [{ id: "SCR-9", status: "ready", title: "S" }],
  project: { key: "COV", root: "/cov" }, worktree: "/cov/wt",
});
assert.equal(dm.description, "desc"); assert.equal(dm.plan, "line1\nline2");
assert.deepEqual(dm.criteria[0], { number: 1, text: "c1", checked: true, evidence: "ran x" });
assert.equal(dm.criteria[1].evidence, null);
assert.equal(dm.comments[0].body, "hi"); assert.equal(dm.comments[0].role, "agent");
assert.deepEqual(dm.blockedBy, [{ id: "SCR-5", status: "done", title: "P" }]);
assert.deepEqual(dm.subtasks, [{ id: "SCR-9", status: "ready", title: "S" }]);
assert.equal(dm.claimedBy, "01234567");
assert.deepEqual(dm.project, { key: "COV", root: "/cov" }, "the detail keeps its Project so a write runs in the right root");
assert.equal(dm.worktree, "/cov/wt");
const empty = detailModel({});
assert.deepEqual([empty.criteria, empty.comments, empty.blockedBy, empty.subtasks], [[], [], [], []]);
assert.deepEqual([empty.project, empty.worktree], [null, null]);
assert.equal(detailModel(null).description, "");

// The write gate is provenance, not a verb list: scree() runs an argv only if a builder handed it
// back. Reads register too, so they keep working; anything assembled elsewhere is refused whatever
// it contains.
for (const a of [showArgs("X"), listArgs(false).open, listArgs(true).done, boardListArgs().open, boardListArgs().done])
  assert(isBuilt(a), `a read builder's output is built: ${a.join(" ")}`);
for (const a of [
  statusArgs("SCR-1", { status: "active", verb: "start" }),
  editArgs({ id: "SCR-1", title: "S" }, { title: "T" }),
  addArgs({ title: "New" }, {}),
]) assert(isBuilt(a), `a write builder's output is built: ${a.join(" ")}`);
for (const a of [["edit", "SCR-1", "--status", "x"], ["--json", "comment", "SCR-1", "hi"], ["--json", "--role", "human", "done", "SCR-1"], []])
  assert(!isBuilt(a), `hand-written argv is refused: ${a.join(" ")}`);
assert(!isBuilt(tuiArgv("scree")), "the tui argv goes to terminal.open, never through scree()");
assert(!isBuilt(statusArgs("SCR-1", { status: "active", verb: "start" }).slice()), "a copy of a built argv is not built");

// Every transition TRANSITIONS offers resolves to one exact command, written out here rather than
// derived, so a change to COMMANDS that alters an argv fails first.
const EXPECTED = {
  ready: ["--json", "--role", "human", "ready", "SCR-1"],
  start: ["--json", "--role", "human", "start", "SCR-1"],
  release: ["--json", "--role", "human", "release", "SCR-1"],
  draft: ["--json", "--role", "human", "draft", "SCR-1"],
  done: ["--json", "--role", "human", "done", "SCR-1"],
  abandon: ["--json", "--role", "human", "abandon", "SCR-1", "abandoned from the scree mod"],
  reopen: ["--json", "--role", "human", "reopen", "SCR-1"],
};
assert.equal(ABANDON_REASON, "abandoned from the scree mod", "the reason the CLI records for a mod abandon");
assert.deepEqual(EXPECTED.abandon, statusArgs("SCR-1", { status: "abandoned", verb: "abandon" }));
assert.deepEqual(TRANSITIONS, {
  active: [{ status: "ready", verb: "release" }, { status: "done", verb: "done" }, { status: "abandoned", verb: "abandon" }],
  ready: [{ status: "active", verb: "start" }, { status: "draft", verb: "draft" }, { status: "done", verb: "done" }, { status: "abandoned", verb: "abandon" }],
  draft: [{ status: "ready", verb: "ready" }, { status: "done", verb: "done" }, { status: "abandoned", verb: "abandon" }],
  done: [{ status: "ready", verb: "reopen" }],
  abandoned: [{ status: "ready", verb: "reopen" }],
}, "one menu row per Status, in COMMANDS order");
assert.deepEqual(Object.keys(TRANSITIONS), STATUS_ORDER, "TRANSITIONS has a row for every Status");
for (const [from, targets] of Object.entries(TRANSITIONS))
  for (const target of targets) {
    assert.deepEqual(statusArgs("SCR-1", target), EXPECTED[target.verb], `${target.verb} from ${from}`);
    assert.notEqual(target.status, from, `${target.verb} does not offer ${from} its own status`);
    if (target.status === "draft") assert.equal(from, "ready", "only ready can go back to draft");
  }
assert.deepEqual(transitionsFor("nonexistent"), [], "an unknown status offers nothing");
assert.equal(transitionsFor("done").length, 1);
assert.equal(transitionsFor("abandoned")[0].verb, "reopen");
assert.equal(statusLabel("abandoned"), "Abandoned", "a status with no column still gets a label");
assert.equal(statusLabel("ready"), "Ready", "a column's label wins over capitalising");

// Writes carry the caller's role: a GUI spawn has no tty and no agent marker, so without --role
// human scree would resolve Role agent and refuse a human's click (src/identity.rs).
for (const argv of [
  statusArgs("SCR-1", { status: "active", verb: "start" }),
  statusArgs("SCR-1", { status: "abandoned", verb: "abandon" }),
  editArgs({ id: "SCR-1", title: "S", for: "agent" }, { title: "T", description: "D", plan: "P", for: "human" }),
  addArgs({ title: "N", criteria: ["a", "b"], status: "ready", for: "agent" }, { parent: "SCR-2" }),
]) assert.deepEqual(argv.slice(0, 3), ["--json", "--role", "human"], argv.join(" "));
assert.deepEqual(WRITE_FLAGS, ["--json", "--role", "human"]);

// Builders pass only what the form holds, and never a bare "-" (which means stdin to scree).
const T = { id: "SCR-1", title: "Old", description: "D", plan: "", for: "agent" };
const EDIT = ["--json", "--role", "human", "edit", "SCR-1"];
assert.equal(editArgs(T, {}), null, "nothing given: nothing to run");
assert.equal(editArgs(T, { title: "Old", description: "D", plan: "", for: "agent" }), null, "an unchanged form runs nothing");
assert.deepEqual(editArgs(T, { title: "New" }), [...EDIT, "--title", "New"]);
assert.deepEqual(editArgs(T, { title: "A", description: "E", plan: "P", for: "human" }),
  [...EDIT, "--title", "A", "--description", "E", "--plan", "P", "--for", "human"], "changed flags only, in order");
assert.deepEqual(editArgs(T, { title: "Old", description: "" }), [...EDIT, "--description", ""], "a cleared description is sent as empty");
assert.deepEqual(editArgs({ ...T, description: undefined }, { description: "" }), null, "a missing description equals empty");
assert.equal(editArgs(T, { plan: "-" }), null, "a lone dash is omitted: it would mean stdin to scree");
assert(isBuilt(editArgs(T, { title: "N" })), "editArgs output is built");
assert(!editArgs(T, { title: "A" }).includes("--status"), "edit has no --status: a change is a transition");
assert.deepEqual(editErrors({ title: "  ", description: "x" }), [{ field: "title", message: "title must not be empty" }], "names the missing field");
assert.deepEqual(editErrors({ title: "ok" }), []);
assert.deepEqual(editErrors({}), [{ field: "title", message: "title must not be empty" }]);
assert.deepEqual(addArgs({ title: "N" }, {}), ["--json", "--role", "human", "add", "N"]);
assert.deepEqual(addArgs({ title: "T", description: "d", criteria: "a\n\n b \n", for: "human", status: "ready" }),
  ["--json", "--role", "human", "add", "T", "--description", "d", "--criterion", "a", "--criterion", "b", "--for", "human", "--status", "ready"],
  "one --criterion per non-empty line");
assert.deepEqual(addArgs({ title: "T", description: "", plan: "", criteria: "  \n" }), ["--json", "--role", "human", "add", "T"], "empty optional fields omitted");
assert(!addArgs({ title: "T", for: "agent", status: "draft" }).includes("--project"), "scope comes from cwd, not --project");
assert.deepEqual(criteriaLines("a\r\n\r\n-\n  b  "), ["a", "b"], "CRLF and blanks dropped, and a lone dash (stdin to scree)");
assert.deepEqual(criteriaLines(""), []);
assert.deepEqual(CREATE_STATUSES, ["draft", "ready"], "only draft and ready are legal at create");
assert.deepEqual(editErrors({ title: " " }).map((e) => e.field), ["title"], "create reuses the title check");
assert.deepEqual(addArgs({ title: "N", plan: "-" }, { parent: "SCR-2" }),
  ["--json", "--role", "human", "add", "N", "--parent", "SCR-2"], "a lone dash is omitted, not a stdin read");
assert.deepEqual(editErrors({ title: "-" }).map((e) => e.field), ["title"], "a lone dash title is refused in the form");
assert.deepEqual(textFlag("--description", null), []);
assert.deepEqual(textFlag("--description", "x"), ["--description", "x"]);
assert.deepEqual(tuiArgv("/bin/scree"), ["/bin/scree", "tui"]);
assert.deepEqual(tuiArgv(null), ["scree", "tui"]);
// Board.
assert.deepEqual(boardListArgs().open, ["--json", "list", "--all-projects", "--status", "draft", "--status", "ready", "--status", "active", "--limit", "1000"]);
assert.deepEqual(boardListArgs().done, ["--json", "list", "--all-projects", "--status", "done", "--done-since", "7d", "--limit", "1000"]);
assert(listArgs(false).done.join(" ").includes("--done-since 7d --limit"), "panel done call passes the since bound");
assert(!boardListArgs().open.includes("done"), "open board call must not include --status done");
assert.equal(machineLabel(null), "this computer");
assert.equal(machineLabel("box"), "box");
assert.equal(worktreeLabel(null), "root");
assert.equal(worktreeLabel("/a/b/feat-x"), "feat-x");
const NOW = Date.parse("2026-11-13T12:00:00Z");
const bt = (id, status, key, wt, extra = {}) => ({ id, status, for: "agent", blocked: false, archived_at: null, done_at: null, worktree: wt, project: { key, root: key === "LOC" ? null : `/r/${key}` }, ...extra });
const tickets = [
  bt("Z-1", "ready", "ZED", null), bt("A-1", "active", "AAA", "/r/AAA-wt/b"), bt("A-2", "draft", "AAA", null),
  bt("A-3", "ready", "AAA", "/r/AAA-wt/a"), bt("A-4", "ready", "AAA", null), bt("L-1", "draft", "LOC", null),
  bt("A-5", "done", "AAA", null, { done_at: "2026-11-10T00:00:00Z" }),
  bt("A-6", "done", "AAA", null, { done_at: "2026-11-01T00:00:00Z" }),
  bt("A-7", "abandoned", "AAA", null), bt("A-8", "ready", "AAA", null, { archived_at: "2026-11-01T00:00:00Z" }),
  bt("Z-2", "ready", "ZED", "/r/ZED-wt/empty", { for: "human", blocked: true }),
];
let bm = boardModel(tickets, { now: NOW });
assert.deepEqual(bm.map((p) => p.key), ["AAA", "ZED", "LOC"], "sorted by key, LOCAL last");
assert.equal(bm[2].root, null);
assert.deepEqual(bm[0].lanes.map((l) => l.label), ["root", "a", "b"], "root first, then worktrees by path");
assert.deepEqual(bm[0].lanes[0].cells.ready.map((x) => x.id), ["A-4"], "archived dropped, list order kept");
assert.deepEqual(bm[0].lanes[0].cells.draft.map((x) => x.id), ["A-2"]);
assert.deepEqual(bm[0].lanes[0].cells.done.map((x) => x.id), ["A-5"], "7-day done cutoff, abandoned dropped");
assert.equal(bm[0].count, 5);
assert.deepEqual(boardCounts(bm), { draft: 2, ready: 4, active: 1, done: 1 });
// Empty lanes and projects are hidden.
bm = boardModel(tickets, { now: NOW, forRole: "agent" });
assert.deepEqual(bm.find((p) => p.key === "ZED").lanes.map((l) => l.label), ["root"], "lane with only a filtered-out ticket is hidden");
bm = boardModel(tickets, { now: NOW, forRole: "human" });
assert.deepEqual(bm.map((p) => p.key), ["ZED"], "projects with no matching tickets are hidden");
assert.deepEqual(bm[0].lanes.map((l) => l.label), ["empty"]);
assert.deepEqual(boardModel(tickets, { now: NOW, blockedOnly: true })[0].lanes[0].cells.ready.map((x) => x.id), ["Z-2"]);
assert.deepEqual(boardModel(undefined), []);
assert.equal(boardModel(tickets, { now: NOW, doneDays: 30 })[0].lanes[0].cells.done.length, 2);

// Long board cells collapse to CELL_LIMIT cards until expanded.
const many = Array.from({ length: 12 }, (_, i) => ({ id: `C-${i}` }));
assert.equal(cellVisible(many, false).shown.length, CELL_LIMIT);
assert.equal(cellVisible(many, false).hidden, 12 - CELL_LIMIT);
assert.equal(cellVisible(many, true).shown.length, 12);
assert.equal(cellVisible(many.slice(0, CELL_LIMIT), false).hidden, 0);
// Done cells list newest first.
const dn = [1, 3, 2].map((d) => ({ id: `D-${d}`, status: "done", done_at: new Date(NOW - d * 3600000).toISOString(), project: { key: "Q", root: "/q" } }));
assert.deepEqual(boardModel(dn, { now: NOW })[0].lanes[0].cells.done.map((x) => x.id), ["D-1", "D-2", "D-3"]);

// A project whose visible tickets are all done folds by default; an explicit click wins.
assert.deepEqual(OPEN_STATUSES, ["draft", "ready", "active"]);
assert.equal(boardModel(tickets, { now: NOW }).find((p) => p.key === "AAA").onlyDone, false, "mixed project is not onlyDone");
assert.equal(boardModel(dn, { now: NOW })[0].onlyDone, true, "recent done tickets only");
const mix = [bt("M-1", "ready", "MIX", null, { for: "human" }), bt("M-2", "done", "MIX", null, { done_at: "2026-11-10T00:00:00Z" })];
assert.equal(boardModel(mix, { now: NOW })[0].onlyDone, false, "open ticket visible: not onlyDone");
assert.equal(boardModel(mix, { now: NOW, forRole: "agent" })[0].onlyDone, true, "filters drop the open ticket: onlyDone");
assert.equal(projectFolded({}, { key: "A", onlyDone: true }), true, "no stored choice follows an all-done default");
assert.equal(projectFolded({}, { key: "A", onlyDone: false }), false, "no stored choice follows a mixed default");
assert.equal(projectFolded({ A: false }, { key: "A", onlyDone: true }), false, "stored expand beats the all-done default");
assert.equal(projectFolded({ A: true }, { key: "A", onlyDone: false }), true, "stored fold beats the expanded default");
assert.equal(projectFolded(undefined, { key: "A", onlyDone: true }), true, "missing store reads as no choice");

// nextBoardFold: the value stored by toggleBoardFold must invert the resolved state, not just the
// raw stored key. A mutation to `!folded[key]` would produce `true` for a missing key (undefined),
// keeping the project folded - this test catches that.
assert.equal(nextBoardFold({}, { key: "A", onlyDone: true }), false, "first click on all-done default expands");
assert.equal(nextBoardFold({}, { key: "A", onlyDone: false }), true, "first click on mixed default folds");
assert.equal(nextBoardFold({ A: true }, { key: "A", onlyDone: true }), false, "click on explicitly-folded expands");
assert.equal(nextBoardFold({ A: false }, { key: "A", onlyDone: false }), true, "click on explicitly-expanded folds");
assert.equal(nextBoardFold(undefined, { key: "A", onlyDone: true }), false, "missing store treated as empty");

// scree's own failures arrive as one JSON object on stderr; clap's argv mistakes are plain text.
assert.deepEqual(parseScreeError('{"error":{"code":"refused","message":"SCR-1 is for a human"}}', 4),
  { kind: "error", message: "SCR-1 is for a human", screeCode: "refused" });
assert.equal(parseScreeError('{"error":{"code":"conflict","message":"SCR-1 is already active"}}', 5).kind, "error");
assert.equal(parseScreeError('prefix noise\n{"error":{"code":"usage","message":"an abandon reason is required"}}', 2).message,
  "an abandon reason is required", "the JSON line is found after other output");
assert.deepEqual(parseScreeError("error: unrecognized subcommand 'draft'", 2),
  { kind: "error", message: "error: unrecognized subcommand 'draft'", screeCode: null }, "clap text falls back unchanged");
// A refusal whose wording happens to name a missing repo keeps classifyError's verdict, which here
// is "error": the pattern list matches "not in a git repo", not "is not inside a git repository".
// Left alone on purpose - the notice strip prints scree's own message either way, and widening
// classifyError would trade a precise sentence for "Not in a scree project".
assert.equal(parseScreeError('{"error":{"code":"refused","message":"/x is not inside a git repository"}}', 4).kind,
  "error");
assert.equal(parseScreeError('{"error":{"code":"refused","message":"not in a scree project"}}', 4).kind, "no-project",
  "the poll path still recognises a space with no project");

// The board's statuses are declared once, as COLUMNS. Everything else - BOARD_COLUMNS, OPEN_STATUSES,
// LABELS, a lane's cells, boardCounts and both open list calls - derives from that table, so every
// check below compares a derived shape with COLUMNS itself, not with another hand-written list.
function checkColumnCoverage(m) {
  const cols = m.COLUMNS.map(([s]) => s), unfinished = cols.filter((s) => s !== "done");
  const tick = (id, s) => ({ id, status: s, for: "agent", blocked: false, archived_at: null, done_at: s === "done" ? "2026-11-12T00:00:00Z" : null, worktree: null, project: { key: "COV", root: "/cov" } });
  assert.deepEqual(m.BOARD_COLUMNS, cols, "BOARD_COLUMNS is the table's statuses");
  assert.deepEqual(m.OPEN_STATUSES, unfinished, "OPEN_STATUSES is every column but Done");
  // A lane carries one cell per column. Probed with a ticket in the FIRST column, so a cells map that
  // fell behind shows up as a wrong key list here rather than as a TypeError further down.
  assert.deepEqual(Object.keys(m.boardModel([tick("C-1", cols[0])], { now: NOW })[0].lanes[0].cells), cols, "a lane has exactly one cell per column");
  // One ticket in every column survives boardModel and lands in its own cell.
  const full = m.boardModel(cols.map((s) => tick(`C-${s}`, s)), { now: NOW });
  assert.deepEqual(cols.map((s) => full[0].lanes[0].cells[s].map((x) => x.id)), cols.map((s) => [`C-${s}`]), "one ticket in every column survives boardModel, each in its own cell");
  assert.deepEqual(Object.keys(m.boardCounts(full)), cols, "boardCounts covers every column");
  assert.deepEqual(Object.keys(m.LABELS), [...cols, ...m.OPTIONAL_COLUMNS.map(([s]) => s)], "LABELS names every column, optional ones last");
  for (const [s, label] of m.COLUMNS) assert.equal(String(m.LABELS[s]), label, `${s} renders as "${label}", not undefined`);
  // Both open list calls ask for exactly the unfinished columns, one --status each, in that order.
  for (const [what, argv] of [["panel", m.listArgs(false).open], ["panel all-worktrees", m.listArgs(true).open], ["board", m.boardListArgs().open]])
    assert.deepEqual(statusFlagsIn(argv), unfinished, `${what} open call fetches exactly the unfinished columns`);
}

// The same trick over the status menu: TRANSITIONS must be derived from STATUS_ORDER, so a Status
// added to the one place gains a row instead of silently dropping out of the menu. `extra` below is
// hand-written on purpose - it states what the CLI accepts per Status (src/ticket.rs) independently
// of how main.js derives it, which is what gives these asserts their teeth.
function checkTransitionCoverage(m) {
  const statuses = m.STATUS_ORDER;
  const closed = statuses.filter((s) => !m.SETTLEABLE.includes(s));
  assert.deepEqual(Object.keys(m.TRANSITIONS), statuses, "TRANSITIONS has a row for every Status");
  assert.deepEqual(m.SETTLEABLE, statuses.filter((s) => !["done", "abandoned"].includes(s)), "SETTLEABLE is every Status a command can still settle");
  assert.deepEqual(closed, statuses.filter((s) => ["done", "abandoned"].includes(s)));
  for (const s of statuses) {
    const extra = s === "draft" ? ["ready"] : s === "ready" ? ["start", "draft"] : s === "active" ? ["release"] : [];
    const expected = (closed.includes(s) ? ["reopen"] : ["abandon", "done", ...extra]).sort();
    assert.deepEqual(m.transitionsFor(s).map((t) => t.verb).sort(), expected, `${s} offers exactly the commands that work from it`);
    for (const t of m.transitionsFor(s)) {
      assert(statuses.includes(t.status), `${t.verb} reaches a Status the board knows`);
      assert.notEqual(t.status, s, `${t.verb} does not offer ${s} its own status`);
      if (t.status === "draft") assert.equal(s, "ready", "only ready reaches draft");
    }
  }
}

// Render-region coverage: SECTIONS and boardHeadline must derive from COLUMNS/STATUS_ORDER so a
// new column renders without editing anything else.
function checkRenderCoverage(m) {
  const cols = m.COLUMNS.map(([s]) => s);
  // SECTIONS equals BOARD_COLUMNS ordered by STATUS_ORDER (known first, then any extras).
  const expectedSections = [
    ...m.STATUS_ORDER.filter((s) => cols.includes(s)),
    ...cols.filter((s) => !m.STATUS_ORDER.includes(s)),
  ];
  assert.deepEqual(m.SECTIONS, expectedSections, "SECTIONS covers every column in STATUS_ORDER display order");
  // boardHeadline includes every open status (Active-first), even at zero count.
  const counts = Object.fromEntries(cols.map((s) => [s, 0]));
  counts.active = 1; // one nonzero to anchor order
  const headline = m.boardHeadline(counts);
  const unfinished = m.OPEN_STATUSES;
  const expectedOrder = [
    ...m.STATUS_ORDER.filter((s) => unfinished.includes(s)),
    ...unfinished.filter((s) => !m.STATUS_ORDER.includes(s)),
  ];
  for (const s of expectedOrder)
    assert(headline.includes(`${counts[s]} ${s}`), `boardHeadline includes ${s} (even at zero)`);
  // No extra statuses in the headline beyond the open ones.
  const allInHeadline = headline.match(/\d+ (\w+)/g).map((x) => x.split(" ")[1]);
  assert.deepEqual(allInHeadline, expectedOrder, "boardHeadline names exactly the open statuses, in STATUS_ORDER");
}

// Both derived shapes plus render coverage, over the live code and over each mutant.
const MODULE = { COLUMNS, BOARD_COLUMNS, OPEN_STATUSES, LABELS, listArgs, boardListArgs, boardModel, boardCounts, STATUS_ORDER, SETTLEABLE, TRANSITIONS, transitionsFor, SECTIONS, boardHeadline, OPTIONAL_COLUMNS, isBuilt, statusArgs, boardColumns };
// SCR-26: abandoned tickets are hidden by default and reachable on demand, so the reopen row in the
// status menu has a card to hang off. Checked on whichever module (real or mutant) is passed in.
function checkAbandonedCoverage(m) {
  const flags = (argv) => argv.filter((_, i) => i > 0 && argv[i - 1] === "--status");
  // Default calls ask for no abandoned and carry no abandoned key.
  for (const a of [m.listArgs(false), m.listArgs(true), m.boardListArgs()]) {
    assert(!("abandoned" in a), "no abandoned call by default");
    for (const argv of Object.values(a)) assert(!flags(argv).includes("abandoned"), "no --status abandoned by default");
  }
  // On: exactly one extra call, and the open and done calls are unchanged.
  for (const [off, on] of [[m.listArgs(false), m.listArgs(false, true)], [m.listArgs(true), m.listArgs(true, true)], [m.boardListArgs(), m.boardListArgs(true)]]) {
    assert.deepEqual(Object.keys(on).sort(), ["abandoned", "done", "open"], "exactly one extra call");
    assert.deepEqual(flags(on.abandoned), ["abandoned"], "the extra call asks only for abandoned");
    assert.deepEqual(on.open, off.open); assert.deepEqual(on.done, off.done);
    assert(!on.abandoned.includes("--done-since"), "abandoned has no done_at to bound");
    assert(m.isBuilt(on.abandoned), "the abandoned call passes the builder gate");
  }
  const ab = (id, at, extra = {}) => ({ id, status: "abandoned", for: "agent", blocked: false, archived_at: null, done_at: null, updated_at: at, worktree: null, project: { key: "ABN", root: "/abn" }, ...extra });
  const ts = [ab("B-1", "2026-11-01T00:00:00Z"), ab("B-2", "2026-11-05T00:00:00Z"), ab("B-3", "2026-11-09T00:00:00Z", { archived_at: "2026-11-10T00:00:00Z" }),
    { ...ab("B-4", null), status: "ready" }];
  const hidden = m.boardModel(ts, { now: NOW });
  assert.deepEqual(Object.keys(hidden[0].lanes[0].cells), m.BOARD_COLUMNS, "no abandoned cell by default");
  assert.deepEqual(hidden[0].lanes[0].cells.ready.map((x) => x.id), ["B-4"]);
  assert(!JSON.stringify(hidden).includes("B-1"), "abandoned tickets are hidden by default");
  const shown = m.boardModel(ts, { now: NOW, showAbandoned: true });
  assert.deepEqual(shown[0].lanes[0].cells.abandoned.map((x) => x.id), ["B-2", "B-1"], "newest first, archived (swept) dropped");
  // Not counted in the headline or the project count, and an abandoned-only project reads as onlyDone.
  assert.equal(shown[0].count, 1);
  assert.equal(m.boardCounts(shown).abandoned, 2);
  assert.equal(m.boardHeadline(m.boardCounts(shown)), m.boardHeadline(m.boardCounts(hidden)), "abandoned stays out of the headline");
  assert.equal(m.boardModel([ts[0]], { now: NOW, showAbandoned: true })[0].onlyDone, true);
  // The reopen row is reachable: one row, running reopen as a human.
  assert.deepEqual(m.transitionsFor("abandoned"), [{ status: "ready", verb: "reopen" }]);
  assert.deepEqual(m.statusArgs("SCR-1", m.transitionsFor("abandoned")[0]), ["--json", "--role", "human", "reopen", "SCR-1"]);
  assert.equal(m.LABELS.abandoned, "Abandoned");
}
function checkCoverage(m) { checkColumnCoverage(m); checkTransitionCoverage(m); checkRenderCoverage(m); checkAbandonedCoverage(m); }
checkCoverage(MODULE);

// Mutants: each one adds a fifth column to the table and, except for the control, hands one of the
// derived shapes back to a hand-written list. Running the same coverage checks over them proves the
// asserts have teeth (SCR-20). SCREE_MUTANT=<name> node scree.test.mjs runs one after the suite:
// exit 0 means it behaved as expected.
const ADD_PARKED = ['["done", "Done"]]', '["done", "Done"], ["parked", "Parked"]]'];
// A sixth Status in the one table that lists them, for the transitions checks: parked is unsettled,
// so Done and Abandon must reach it without COMMANDS being edited.
const ADD_PARKED_STATUS = [
  'const STATUS_ORDER = ["active", "ready", "draft", "done", "abandoned"];',
  'const STATUS_ORDER = ["active", "ready", "draft", "done", "abandoned", "parked"];'];
const MUTANTS = {
  // Abandoned tickets shown without the toggle: the filter on boardModel is dropped.
  abandoned_shown_without_toggle: { expect: "fail", edits: [["const columns = boardColumns(showAbandoned);", "const columns = boardColumns(true);"]] },
  // The abandoned list call sent on every poll.
  abandoned_call_always_sent: { expect: "fail", edits: [["...(showAbandoned ? { abandoned: build([\"--json\", \"list\", \"--all-projects\"", "...(true ? { abandoned: build([\"--json\", \"list\", \"--all-projects\""]] },
  abandoned_counted_in_project: { expect: "fail", edits: [["if (!ABANDONED_STATUSES.includes(t.status)) p.count++;", "p.count++;"]] },
  // The point of the change: the table is the only edit a new column needs.
  add_column_to_table: { expect: "pass", edits: [ADD_PARKED] },
  // A lane's cells written out by hand again. Before SCR-20 this died in renderBoard with
  // TypeError: Cannot read properties of undefined (reading 'push'); see main.js at 4a23353.
  extra_column_cells_left_behind: { expect: "fail", edits: [ADD_PARKED, ["cells: Object.fromEntries(BOARD_COLUMNS.map((s) => [s, []]))", "cells: { draft: [], ready: [], active: [], done: [] }"]] },
  // OPEN_STATUSES written out by hand again: parked tickets are never fetched, so a Project whose
  // open work is parked reads as onlyDone and starts folded with live work behind a ▸.
  extra_column_open_statuses_hand_written: { expect: "fail", edits: [ADD_PARKED, ['const OPEN_STATUSES = BOARD_COLUMNS.filter((s) => s !== "done");', 'const OPEN_STATUSES = ["draft", "ready", "active"];']] },
  // One open call missing a status while the rest agree it is unfinished - how listArgs read
  // before it started using OPEN_STATUSES.
  status_missing_from_panel_call: { expect: "fail", edits: [['...scope, ...OPEN_STATUSES.flatMap((s) => ["--status", s])', '...scope, ...OPEN_STATUSES.filter((s) => s !== "draft").flatMap((s) => ["--status", s])']] },
  status_missing_from_board_call: { expect: "fail", edits: [['"--all-projects", ...OPEN_STATUSES.flatMap((s) => ["--status", s])', '"--all-projects", ...OPEN_STATUSES.filter((s) => s !== "active").flatMap((s) => ["--status", s])']] },
  // The point of the transitions table: a new Status gets a menu row from one edit. Unlike the column
  // mutants this one needs both tables touched, because STATUS_ORDER and COLUMNS are separate lists
  // that "must stay consistent on the four they share" (main.js).
  new_status_gains_a_row: { expect: "pass", edits: [ADD_PARKED, ADD_PARKED_STATUS] },
  // TRANSITIONS written out by hand again: the new Status has no row, so its card shows a mark whose
  // menu says "Nothing reachable" forever, whatever the CLI would accept.
  transitions_hand_written: { expect: "fail", edits: [ADD_PARKED, ADD_PARKED_STATUS, [
    'const TRANSITIONS = Object.fromEntries(STATUS_ORDER.map((s) => [s,\n  COMMANDS.filter((c) => c.from.includes(s)).map((c) => ({ status: c.to, verb: c.verb }))]));',
    'const TRANSITIONS = { active: [{ status: "ready", verb: "release" }, { status: "done", verb: "done" }, { status: "abandoned", verb: "abandon" }], ready: [{ status: "active", verb: "start" }, { status: "draft", verb: "draft" }, { status: "done", verb: "done" }, { status: "abandoned", verb: "abandon" }], draft: [{ status: "ready", verb: "ready" }, { status: "done", verb: "done" }, { status: "abandoned", verb: "abandon" }], done: [{ status: "ready", verb: "reopen" }], abandoned: [{ status: "ready", verb: "reopen" }] };']] },
  // SETTLEABLE pinned to today's three: a settled Status would offer nothing but Reopen and an
  // unsettled one would quietly lose Done and Abandon.
  settleable_hand_written: { expect: "fail", edits: [ADD_PARKED, ADD_PARKED_STATUS, ['const SETTLEABLE = STATUS_ORDER.filter((s) => !CLOSED_STATUSES.includes(s));', 'const SETTLEABLE = ["draft", "ready", "active"];']] },
  // SECTIONS written out by hand again: parked is fetched and counted but never gets a panel section.
  extra_column_sections_hand_written: { expect: "fail", edits: [ADD_PARKED, ['const SECTIONS = [\n  ...STATUS_ORDER.filter((s) => BOARD_COLUMNS.includes(s)),\n  ...BOARD_COLUMNS.filter((s) => !STATUS_ORDER.includes(s)),\n];', 'const SECTIONS = ["active", "ready", "draft", "done"];']] },
  // boardHeadline reverted to three-property accesses: parked shows in column headers but not the headline.
  board_summary_hand_written: { expect: "fail", edits: [ADD_PARKED, ['function boardHeadline(counts) {\n  const ordered = [\n    ...STATUS_ORDER.filter((s) => OPEN_STATUSES.includes(s)),\n    ...OPEN_STATUSES.filter((s) => !STATUS_ORDER.includes(s)),\n  ];\n  return ordered.map((s) => `${counts[s]} ${s}`).join(", ");\n}', 'function boardHeadline(counts) {\n  return `${counts.active} active, ${counts.ready} ready, ${counts.draft} draft`;\n}']], },
};

// Regression: refresh() must reach the post-finally fetchDetail in board mode (no early return
// inside the try), so an open detail updates on poll.
{
  const body = src.slice(src.indexOf("  refresh(cx) {"), src.indexOf("  async fetchDetail("));
  const tryBody = body.slice(body.indexOf("try {"), body.indexOf("} finally"));
  assert(body.includes("await this.fetchDetail(this.detail.id"), "refresh fetches the open detail");
  assert(!/\breturn\b/.test(tryBody), "no return inside refresh()'s try block");
}

// SCR-22: the class-body toggle methods must call their helpers. A mutation that reverts
// nextBoardFold(this.boardFolded, p) to !this.boardFolded[p.key], or projectFolded(...) to
// Boolean(...), must fail here rather than silently break the click path in production.

// Write path: exercise the actual toggle methods by slicing them into an object literal.
{
  const start = src.indexOf("  toggleFold(status, cx) {");
  const end = src.indexOf("  mark(t, ticket, status) {");
  assert(start > 0 && end > start, "toggle methods not found in main.js class body");
  const store = { set() {} };
  const obj = new Function("store", "nextBoardFold",
    `"use strict"; return new class {${src.slice(start, end)}};`)(store, nextBoardFold);
  const cx = { notify() {} };

  // toggleBoardFold: first click on an all-done default-folded project must store false (expand).
  // A mutation to !this.boardFolded[p.key] would store true (negation of undefined), keeping it folded.
  obj.boardFolded = {};
  obj.toggleBoardFold({ key: "A", onlyDone: true }, cx);
  assert.equal(obj.boardFolded.A, false, "click on all-done default stores expand (not raw inversion)");

  // First click on a mixed project stores true (fold).
  obj.boardFolded = {};
  obj.toggleBoardFold({ key: "B", onlyDone: false }, cx);
  assert.equal(obj.boardFolded.B, true, "click on mixed default stores fold");

  // Click on explicitly-folded project stores expand.
  obj.boardFolded = { C: true };
  obj.toggleBoardFold({ key: "C", onlyDone: true }, cx);
  assert.equal(obj.boardFolded.C, false, "click on explicitly-folded stores expand");

  // Click on explicitly-expanded project stores fold.
  obj.boardFolded = { D: false };
  obj.toggleBoardFold({ key: "D", onlyDone: false }, cx);
  assert.equal(obj.boardFolded.D, true, "click on explicitly-expanded stores fold");

  // toggleFold: simple inversion (panel sections have no data-driven default).
  obj.folded = {};
  obj.toggleFold("done", cx);
  assert.equal(obj.folded.done, true, "toggleFold stores inverted value");
  obj.toggleFold("done", cx);
  assert.equal(obj.folded.done, false, "second toggleFold reverts");

  // toggleCell: simple inversion (cell expansion has no data-driven default).
  obj.boardExpanded = {};
  obj.toggleCell("X|wt|ready", cx);
  assert.equal(obj.boardExpanded["X|wt|ready"], true, "toggleCell stores inverted value");
  obj.toggleCell("X|wt|ready", cx);
  assert.equal(obj.boardExpanded["X|wt|ready"], false, "second toggleCell reverts");
}

// Read sites: the render methods must read through projectFolded so the all-done default applies.
// A mutation to Boolean(this.boardFolded[p.key]) means an all-done project with no stored choice
// reads as expanded instead of folded.
{
  const renderSrc = src.slice(
    src.indexOf("  renderBoard(t, state) {"),
    src.indexOf("  section(t, g, total, cx) {"));
  assert(renderSrc.includes("projectFolded(this.boardFolded, p)"),
    "renderBoard reads board fold through projectFolded, not Boolean()");
  assert(renderSrc.includes("Boolean(this.boardExpanded[ck])"),
    "renderBoard reads cell expansion via Boolean (correct: no helper needed)");
  const sectionSrc = src.slice(src.indexOf("  section(t, g, total, cx) {"));
  assert(sectionSrc.includes("Boolean(this.folded[g.status])"),
    "section reads panel fold via Boolean (correct: no data-driven default)");
}

// SCR-23: a throw from the board's model build becomes the error shape renderBoard draws, not a
// dead pane. Claim covered: boardView() absorbs the throw and renderBoard routes through it; the
// gpui builders in renderBoard itself are not executed here.
{
  const start = src.indexOf("  boardView() {");
  const end = src.indexOf("  renderBoard(t, state) {");
  assert(start > 0 && end > start, "boardView not found in main.js class body");
  const make = (bm) => {
    const o = new Function("boardModel", "boardCounts", "BOARD_COLUMNS",
      `"use strict"; return new class {${src.slice(start, end)}};`)(bm, boardCounts, BOARD_COLUMNS);
    o.board = []; o.showAbandoned = false; o.filters = () => ({});
    return o;
  };
  const trace = "Cannot read properties of undefined (reading 'push')";
  const bad = make(() => { throw new TypeError(trace); });
  let v;
  assert.doesNotThrow(() => { v = bad.boardView(); }, "boardView absorbs a throwing boardModel");
  assert.equal(v.error.kind, "error", "boardView maps a model throw onto kind error");
  assert(v.error.message.includes(trace), "boardView error message carries the thrown text");
  assert.deepEqual(v.model, [], "boardView model is empty on error");
  for (const c of BOARD_COLUMNS) assert.equal(v.counts[c], 0, `boardView zeroes count for ${c} on error`);

  const tk = { id: "SCR-1", title: "t", status: "ready", project: { key: "SCR", root: "/r" }, worktree: null };
  const good = make(boardModel);
  good.board = [tk];
  const g = good.boardView();
  assert.equal(g.error, null, "boardView has no error for a valid board");
  assert.deepEqual(g.counts, boardCounts(boardModel([tk], { now: Date.now() })), "boardView counts match boardCounts(boardModel(...))");

  const rb = src.slice(end, src.indexOf("  section(t, g, total, cx) {"));
  assert(rb.includes("this.boardView()"), "renderBoard builds its model through this.boardView()");
  assert(!/\bboardModel\(/.test(rb), "renderBoard does not call boardModel directly");
}

// Static checks on the render region that cannot be exercised through pureWith:
// mark-<status>.svg files exist for every column, and the colour table in mark() names each status.
{
  const cols = COLUMNS.map(([s]) => s);
  cols.push(...OPTIONAL_COLUMNS.map(([s]) => s));
  for (const s of cols)
    assert(existsSync(new URL(`./mark-${s}.svg`, import.meta.url)), `mark-${s}.svg exists beside main.js`);
  assert(existsSync(new URL("./mark-blocked.svg", import.meta.url)), "mark-blocked.svg exists beside main.js");
  // The colour table in mark() must name every column (plus blocked).
  const colorMatch = src.match(/const color = \{([^}]+)\}\[kind\]/);
  assert(colorMatch, "colour table found in mark()");
  for (const s of cols)
    assert(new RegExp(`\\b${s}\\b`).test(colorMatch[1]), `colour table names ${s}`);
  assert(/\bblocked\b/.test(colorMatch[1]), "colour table names blocked");
}

console.log("ok");

// Run mutants: if SCREE_MUTANT is set, run just that one (for debugging). Otherwise run all
// automatically so nix flake check exercises them and their teeth cannot rot.
function runMutant(name) {
  const spec = MUTANTS[name];
  assert(spec, `unknown mutant ${name}; known: ${Object.keys(MUTANTS).join(", ")}`);
  let outcome;
  try {
    checkCoverage(pureWith(...spec.edits));
    outcome = "passed";
  } catch (e) {
    if (!(e instanceof assert.AssertionError)) {
      return { name, status: "crash", msg: `${name}: CRASHED instead of failing an assert (${e.constructor.name}: ${e.message})` };
    }
    outcome = "failed a named assert";
  }
  const expected = spec.expect === "pass" ? "passed" : "failed a named assert";
  return { name, status: outcome === expected ? "ok" : "unexpected", msg: `${name}: ${outcome} ${outcome === expected ? "(expected)" : `(EXPECTED ${spec.expect.toUpperCase()})`}` };
}

if (process.env.SCREE_MUTANT) {
  const r = runMutant(process.env.SCREE_MUTANT);
  console.log(r.msg);
  process.exit(r.status === "ok" ? 0 : 1);
} else {
  let failures = 0;
  for (const name of Object.keys(MUTANTS)) {
    const r = runMutant(name);
    if (r.status !== "ok") { console.log(r.msg); failures++; }
  }
  if (failures) { console.log(`${failures} mutant(s) misbehaved`); process.exit(1); }
}
