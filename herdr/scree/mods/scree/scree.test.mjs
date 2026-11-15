// Run: node scree.test.mjs. Loads the pure helpers out of main.js (which imports host-only
// modules, so it cannot be imported directly).
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const src = readFileSync(new URL("./main.js", import.meta.url), "utf8");
const from = src.indexOf("// --- pure begin ---"), to = src.indexOf("// --- pure end ---");
assert(from > 0 && to > from, "markers not found in main.js");
const PURE_NAMES = ["STATUS_ORDER", "criteriaPercent", "groupByStatus", "classifyError", "doneRecent", "chips", "countsLine", "listArgs", "shortClaimer", "applyFilters", "showArgs", "detailModel", "isWriteCommand", "tuiArgv", "WRITE_COMMANDS", "COLUMNS", "BOARD_COLUMNS", "OPEN_STATUSES", "LABELS", "boardListArgs", "boardModel", "boardCounts", "projectFolded", "nextBoardFold", "machineLabel", "worktreeLabel", "cellVisible", "CELL_LIMIT"];
const { STATUS_ORDER, criteriaPercent, groupByStatus, classifyError, doneRecent, chips, countsLine, listArgs, shortClaimer, applyFilters, showArgs, detailModel, isWriteCommand, tuiArgv, WRITE_COMMANDS, COLUMNS, BOARD_COLUMNS, OPEN_STATUSES, LABELS, boardListArgs, boardModel, boardCounts, projectFolded, nextBoardFold, machineLabel, worktreeLabel, cellVisible, CELL_LIMIT } = new Function(
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

// The statuses a list argv asks for, in the order they appear.
const statusArgs = (argv) => argv.filter((_, i) => i > 0 && argv[i - 1] === "--status");

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
});
assert.equal(dm.description, "desc"); assert.equal(dm.plan, "line1\nline2");
assert.deepEqual(dm.criteria[0], { number: 1, text: "c1", checked: true, evidence: "ran x" });
assert.equal(dm.criteria[1].evidence, null);
assert.equal(dm.comments[0].body, "hi"); assert.equal(dm.comments[0].role, "agent");
assert.deepEqual(dm.blockedBy, [{ id: "SCR-5", status: "done", title: "P" }]);
assert.deepEqual(dm.subtasks, [{ id: "SCR-9", status: "ready", title: "S" }]);
assert.equal(dm.claimedBy, "01234567");
const empty = detailModel({});
assert.deepEqual([empty.criteria, empty.comments, empty.blockedBy, empty.subtasks], [[], [], [], []]);
assert.equal(detailModel(null).description, "");

// Command audit: only list/show/tui are ever built; every write command is refused.
for (const a of [showArgs("X"), listArgs(false).open, listArgs(true).done]) assert(!isWriteCommand(a), a.join(" "));
assert.deepEqual(tuiArgv("/bin/scree"), ["/bin/scree", "tui"]);
assert.deepEqual(tuiArgv(null), ["scree", "tui"]);
assert(!isWriteCommand(tuiArgv("scree").slice(1)));
for (const w of ["add", "edit", "check", "start", "done", "release", "abandon", "comment", "dep", "reorder", "criteria"]) {
  assert(isWriteCommand(["--json", w, "SCR-1"]), w);
  assert(WRITE_COMMANDS.includes(w), w);
}
assert(!isWriteCommand([]));
// Board.
assert.deepEqual(boardListArgs().open, ["--json", "list", "--all-projects", "--status", "draft", "--status", "ready", "--status", "active", "--limit", "1000"]);
assert.deepEqual(boardListArgs().done, ["--json", "list", "--all-projects", "--status", "done", "--done-since", "7d", "--limit", "1000"]);
assert(listArgs(false).done.join(" ").includes("--done-since 7d --limit"), "panel done call passes the since bound");
assert(!boardListArgs().open.includes("done"), "open board call must not include --status done");
assert(!isWriteCommand(boardListArgs().open) && !isWriteCommand(boardListArgs().done));
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
  assert.deepEqual(Object.keys(m.LABELS), cols, "LABELS names every column");
  for (const [s, label] of m.COLUMNS) assert.equal(String(m.LABELS[s]), label, `${s} renders as "${label}", not undefined`);
  // Both open list calls ask for exactly the unfinished columns, one --status each, in that order.
  for (const [what, argv] of [["panel", m.listArgs(false).open], ["panel all-worktrees", m.listArgs(true).open], ["board", m.boardListArgs().open]])
    assert.deepEqual(statusArgs(argv), unfinished, `${what} open call fetches exactly the unfinished columns`);
}
checkColumnCoverage({ COLUMNS, BOARD_COLUMNS, OPEN_STATUSES, LABELS, listArgs, boardListArgs, boardModel, boardCounts });

// Mutants: each one adds a fifth column to the table and, except for the control, hands one of the
// derived shapes back to a hand-written list. Running the same coverage checks over them proves the
// asserts have teeth (SCR-20). SCREE_MUTANT=<name> node scree.test.mjs runs one after the suite:
// exit 0 means it behaved as expected.
const ADD_PARKED = ['["done", "Done"]]', '["done", "Done"], ["parked", "Parked"]]'];
const MUTANTS = {
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
};

// Regression: refresh() must reach the post-finally fetchDetail in board mode (no early return
// inside the try), so an open detail updates on poll.
{
  const body = src.slice(src.indexOf("  refresh(cx) {"), src.indexOf("  async fetchDetail("));
  const tryBody = body.slice(body.indexOf("try {"), body.indexOf("} finally"));
  assert(body.includes("await this.fetchDetail(this.detail.id"), "refresh fetches the open detail");
  assert(!/\breturn\b/.test(tryBody), "no return inside refresh()'s try block");
}

console.log("ok");

// Run one mutant over the same coverage checks. An unexpected crash is reported as such, because a
// mutant must fail a named assert instead.
if (process.env.SCREE_MUTANT) {
  const name = process.env.SCREE_MUTANT, spec = MUTANTS[name];
  assert(spec, `unknown mutant ${name}; known: ${Object.keys(MUTANTS).join(", ")}`);
  let outcome, detail = "";
  try {
    checkColumnCoverage(pureWith(...spec.edits));
    outcome = "passed";
  } catch (e) {
    if (!(e instanceof assert.AssertionError)) {
      console.log(`${name}: CRASHED instead of failing an assert (${e.constructor.name}: ${e.message})`);
      process.exit(1);
    }
    outcome = "failed a named assert";
    detail = `\n${String(e.message).split("\n").slice(0, 14).join("\n")}`;
  }
  const expected = spec.expect === "pass" ? "passed" : "failed a named assert";
  console.log(`${name}: ${outcome}${detail} ${outcome === expected ? "(expected)" : `(EXPECTED ${spec.expect.toUpperCase()})`}`);
  process.exit(outcome === expected ? 0 : 1);
}
