//! The dependency graph as one server-rendered SVG, from [`crate::graph::Graph`].
//!
//! Layout is layered: a node's column is its longest-path depth from a node
//! with no blockers, so blockers sit to the left of what they block. Inside a
//! column nodes run by ticket number. There is no crossing minimisation: an
//! edge that spans several columns is drawn straight through them, which is
//! legible for tens of nodes and cluttered near the page's cap.
//!
//! Every length here is an SVG user unit, which the page shows 1:1 (the `svg`
//! has `width` and `height` equal to its `viewBox`); the node size mirrors the
//! `--size-graph-*` tokens at a 16px root. State is shown by a distinct glyph
//! and an outline style as well as by colour, and each node is a link.

use std::collections::{HashMap, HashSet};

use maud::{Markup, html};

use crate::graph::{Graph, Node};

const NODE_W: usize = 168;
const NODE_H: usize = 48;
const GAP_X: usize = 56;
const GAP_Y: usize = 10;
const PAD: usize = 16;
/// Characters of a title shown in a node; the full title is in the tooltip.
const TITLE_CHARS: usize = 22;

/// A node's state, in the order the checks run: the first match wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    External,
    Abandoned,
    Done,
    Blocked,
    Active,
    Available,
    Draft,
    Ready,
}

impl State {
    pub const ALL: [State; 8] = [
        State::Available,
        State::Ready,
        State::Draft,
        State::Active,
        State::Blocked,
        State::Done,
        State::Abandoned,
        State::External,
    ];

    pub fn of(n: &Node) -> State {
        if n.external {
            State::External
        } else {
            match n.status.as_str() {
                "abandoned" => State::Abandoned,
                "done" => State::Done,
                _ if n.blocked => State::Blocked,
                "active" => State::Active,
                _ if n.available => State::Available,
                "draft" => State::Draft,
                _ => State::Ready,
            }
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            State::External => "external",
            State::Abandoned => "abandoned",
            State::Done => "done",
            State::Blocked => "blocked",
            State::Active => "active",
            State::Available => "available",
            State::Draft => "draft",
            State::Ready => "ready",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            State::External => "outside this view",
            State::Abandoned => "abandoned",
            State::Done => "done",
            State::Blocked => "blocked",
            State::Active => "active",
            State::Available => "available",
            State::Draft => "draft",
            State::Ready => "ready",
        }
    }

    /// One glyph per state, so colour is never the only cue.
    pub fn icon(self) -> &'static str {
        match self {
            State::External => "\u{2197}",  // north-east arrow
            State::Abandoned => "\u{2715}", // multiplication x
            State::Done => "\u{2713}",      // check mark
            State::Blocked => "\u{25A0}",   // black square
            State::Active => "\u{25B6}",    // right-pointing triangle
            State::Available => "\u{25CF}", // black circle
            State::Draft => "\u{25CC}",     // dotted circle
            State::Ready => "\u{25CB}",     // white circle
        }
    }
}

/// Where each node sits: `(column, row)` per node index, and the grid size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub cells: Vec<(usize, usize)>,
    pub columns: usize,
    pub rows: usize,
}

/// The ticket number of an id (`SCR-10` is 10); ids without one sort last.
fn number(id: &str) -> usize {
    id.rsplit('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(usize::MAX)
}

pub fn layout(g: &Graph) -> Layout {
    let index: HashMap<&str, usize> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let n = g.nodes.len();
    let mut dependents = vec![vec![]; n];
    let mut waiting = vec![0usize; n];
    for e in &g.edges {
        if let (Some(&f), Some(&t)) = (index.get(e.from.as_str()), index.get(e.to.as_str())) {
            dependents[f].push(t);
            waiting[t] += 1;
        }
    }
    // Kahn's pass: a node is placed once every blocker is. A cycle (corrupt
    // data; `graph::build` refuses it) leaves its nodes at depth 0.
    let mut depth = vec![0usize; n];
    let mut ready: Vec<usize> = (0..n).filter(|&i| waiting[i] == 0).collect();
    while let Some(i) = ready.pop() {
        for &d in &dependents[i] {
            depth[d] = depth[d].max(depth[i] + 1);
            waiting[d] -= 1;
            if waiting[d] == 0 {
                ready.push(d);
            }
        }
    }
    let columns = depth.iter().max().map_or(0, |d| d + 1);
    let mut by_column: Vec<Vec<usize>> = vec![vec![]; columns];
    for (i, d) in depth.iter().enumerate() {
        by_column[*d].push(i);
    }
    let mut cells = vec![(0, 0); n];
    let mut rows = 0;
    for (c, col) in by_column.iter_mut().enumerate() {
        col.sort_by(|&a, &b| {
            number(&g.nodes[a].id)
                .cmp(&number(&g.nodes[b].id))
                .then_with(|| g.nodes[a].id.cmp(&g.nodes[b].id))
        });
        for (r, &i) in col.iter().enumerate() {
            cells[i] = (c, r);
        }
        rows = rows.max(col.len());
    }
    Layout {
        cells,
        columns,
        rows,
    }
}

fn truncate(title: &str) -> String {
    if title.chars().count() <= TITLE_CHARS {
        title.to_string()
    } else {
        let head: String = title.chars().take(TITLE_CHARS - 1).collect();
        format!("{}\u{2026}", head.trim_end())
    }
}

/// The SVG: edges first (so nodes draw over them), then one link per node.
pub fn render(g: &Graph, l: &Layout) -> Markup {
    let width = PAD * 2 + l.columns * NODE_W + l.columns.saturating_sub(1) * GAP_X;
    let height = PAD * 2 + l.rows * NODE_H + l.rows.saturating_sub(1) * GAP_Y;
    let pos = |i: usize| {
        let (c, r) = l.cells[i];
        (PAD + c * (NODE_W + GAP_X), PAD + r * (NODE_H + GAP_Y))
    };
    let index: HashMap<&str, usize> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let critical: HashSet<(&str, &str)> = g
        .critical_path
        .windows(2)
        .map(|w| (w[0].as_str(), w[1].as_str()))
        .collect();
    let summary = format!(
        "Dependency graph: {} tickets, {} dependencies, blockers on the left",
        g.nodes.len(),
        g.edges.len()
    );
    html! {
        svg class="graph-svg" role="img" aria-label=(summary)
            viewBox=(format!("0 0 {width} {height}")) width=(width) height=(height) {
            @for e in &g.edges {
                @if let (Some(&f), Some(&t)) = (index.get(e.from.as_str()), index.get(e.to.as_str())) {
                    @let (fx, fy) = pos(f);
                    @let (tx, ty) = pos(t);
                    @let (x1, y1, x2, y2) = (fx + NODE_W, fy + NODE_H / 2, tx, ty + NODE_H / 2);
                    @let mid = (x1 + x2) / 2;
                    @let hot = critical.contains(&(e.from.as_str(), e.to.as_str()));
                    path class=(if hot { "graph-edge graph-edge-critical" } else { "graph-edge" })
                        d=(format!("M{x1} {y1} C{mid} {y1} {mid} {y2} {x2} {y2}")) {}
                }
            }
            @for (i, n) in g.nodes.iter().enumerate() {
                @let (x, y) = pos(i);
                @let s = State::of(n);
                a href=(format!("/ticket/{}", n.id)) {
                    g class=(format!("graph-node graph-{}{}", s.class(),
                        if n.critical { " graph-node-critical" } else { "" }))
                        data-id=(n.id) {
                        title { (n.id) ": " (n.title) " (" (s.label()) ")" }
                        rect x=(x) y=(y) width=(NODE_W) height=(NODE_H) rx="6" {}
                        text class="graph-icon" x=(x + 10) y=(y + 19) { (s.icon()) }
                        text class="graph-id" x=(x + 26) y=(y + 19) { (n.id) }
                        text class="graph-label" x=(x + 10) y=(y + 38) { (truncate(&n.title)) }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Edge;

    fn node(id: &str, status: &str) -> Node {
        Node {
            available: false,
            blocked: false,
            blocks: 0,
            critical: false,
            external: false,
            for_who: Some("agent".into()),
            id: id.into(),
            status: status.into(),
            title: format!("Title of {id}"),
        }
    }

    fn graph(nodes: Vec<Node>, edges: &[(&str, &str)]) -> Graph {
        Graph {
            critical_path: vec![],
            edges: edges
                .iter()
                .map(|(f, t)| Edge {
                    from: (*f).into(),
                    to: (*t).into(),
                })
                .collect(),
            nodes,
            top_blockers: vec![],
        }
    }

    #[test]
    fn a_chain_runs_left_to_right() {
        let g = graph(
            vec![
                node("SCR-3", "ready"),
                node("SCR-1", "ready"),
                node("SCR-2", "ready"),
            ],
            &[("SCR-1", "SCR-2"), ("SCR-2", "SCR-3")],
        );
        let l = layout(&g);
        assert_eq!(l.cells, vec![(2, 0), (0, 0), (1, 0)]);
        assert_eq!((l.columns, l.rows), (3, 1));
    }

    #[test]
    fn a_diamond_puts_the_join_one_past_its_deepest_blocker() {
        let g = graph(
            vec![
                node("SCR-1", "ready"),
                node("SCR-2", "ready"),
                node("SCR-3", "ready"),
                node("SCR-4", "ready"),
            ],
            &[
                ("SCR-1", "SCR-2"),
                ("SCR-1", "SCR-3"),
                ("SCR-2", "SCR-4"),
                ("SCR-3", "SCR-4"),
                ("SCR-1", "SCR-4"),
            ],
        );
        let l = layout(&g);
        assert_eq!(l.cells, vec![(0, 0), (1, 0), (1, 1), (2, 0)]);
        assert_eq!((l.columns, l.rows), (3, 2));
    }

    #[test]
    fn a_column_orders_by_ticket_number_not_text() {
        let g = graph(
            vec![
                node("SCR-10", "ready"),
                node("SCR-2", "ready"),
                node("SCR-9", "ready"),
            ],
            &[],
        );
        let l = layout(&g);
        assert_eq!(l.cells, vec![(0, 2), (0, 0), (0, 1)]);
    }

    #[test]
    fn an_empty_graph_has_no_cells() {
        let l = layout(&graph(vec![], &[]));
        assert_eq!((l.columns, l.rows, l.cells.len()), (0, 0, 0));
    }

    #[test]
    fn every_state_has_its_own_glyph_and_class() {
        let mut seen = HashSet::new();
        for s in State::ALL {
            assert!(seen.insert((s.icon(), s.class())), "{s:?}");
        }
        assert_eq!(seen.len(), 8);
        let n = |status: &str, available: bool, blocked: bool, external: bool| {
            let mut n = node("SCR-1", status);
            n.available = available;
            n.blocked = blocked;
            n.external = external;
            State::of(&n)
        };
        assert_eq!(n("ready", false, false, true), State::External);
        assert_eq!(n("abandoned", false, true, false), State::Abandoned);
        assert_eq!(n("done", false, false, false), State::Done);
        assert_eq!(n("ready", false, true, false), State::Blocked);
        assert_eq!(n("active", false, false, false), State::Active);
        assert_eq!(n("ready", true, false, false), State::Available);
        assert_eq!(n("draft", false, false, false), State::Draft);
        assert_eq!(n("ready", false, false, false), State::Ready);
    }

    #[test]
    fn render_links_nodes_marks_critical_edges_and_escapes() {
        let mut a = node("SCR-1", "ready");
        a.critical = true;
        a.title = "A <b>very long title that is cut</b>".into();
        let mut b = node("SCR-2", "active");
        b.critical = true;
        let c = node("SCR-3", "ready");
        let mut g = graph(vec![a, b, c], &[("SCR-1", "SCR-2"), ("SCR-1", "SCR-3")]);
        g.critical_path = vec!["SCR-1".into(), "SCR-2".into()];
        let h = render(&g, &layout(&g)).into_string();
        assert_eq!(h, render(&g, &layout(&g)).into_string(), "deterministic");
        assert!(h.contains("href=\"/ticket/SCR-1\""), "{h}");
        assert!(
            h.contains("graph-node graph-ready graph-node-critical"),
            "{h}"
        );
        assert!(
            h.contains("graph-node graph-active graph-node-critical"),
            "{h}"
        );
        assert_eq!(h.matches("graph-edge-critical").count(), 1, "{h}");
        assert_eq!(h.matches("class=\"graph-edge").count(), 2, "{h}");
        assert!(h.contains("&lt;b&gt;"), "{h}");
        assert!(!h.contains("<b>"), "{h}");
        assert!(h.contains('\u{2026}'), "long titles are cut: {h}");
        assert!(!h.contains("style="), "{h}");
    }
}
