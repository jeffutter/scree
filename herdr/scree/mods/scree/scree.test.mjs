// Run: node scree.test.mjs. Loads the pure helpers out of main.js (which imports host-only
// modules, so it cannot be imported directly).
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const src = readFileSync(new URL("./main.js", import.meta.url), "utf8");
const from = src.indexOf("// --- pure begin ---"), to = src.indexOf("// --- pure end ---");
assert(from > 0 && to > from, "markers not found in main.js");
const { STATUS_ORDER, criteriaPercent, groupByStatus, classifyError, doneRecent, chips, countsLine, listArgs, shortClaimer, applyFilters, showArgs, detailModel, isWriteCommand, tuiArgv, WRITE_COMMANDS, boardListArgs, boardModel, boardCounts, machineLabel, worktreeLabel, cellVisible, CELL_LIMIT } = new Function(
  `${src.slice(from, to)}; return { STATUS_ORDER, criteriaPercent, groupByStatus, classifyError, doneRecent, chips, countsLine, listArgs, shortClaimer, applyFilters, showArgs, detailModel, isWriteCommand, tuiArgv, WRITE_COMMANDS, boardListArgs, boardModel, boardCounts, machineLabel, worktreeLabel, cellVisible, CELL_LIMIT };`)();

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
assert.deepEqual(boardListArgs().done, ["--json", "list", "--all-projects", "--status", "done", "--limit", "1000"]);
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

// Regression: refresh() must reach the post-finally fetchDetail in board mode (no early return
// inside the try), so an open detail updates on poll.
{
  const body = src.slice(src.indexOf("  refresh(cx) {"), src.indexOf("  async fetchDetail("));
  const tryBody = body.slice(body.indexOf("try {"), body.indexOf("} finally"));
  assert(body.includes("await this.fetchDetail(this.detail.id"), "refresh fetches the open detail");
  assert(!/\breturn\b/.test(tryBody), "no return inside refresh()'s try block");
}

console.log("ok");
