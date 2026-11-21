//! `scree graph`: the dependency graph of a list result, with the critical
//! path and the tickets that block the most others, rendered as Unicode or
//! ASCII text, a Mermaid flowchart, or JSON (the `Graph` value).
//!
//! An edge runs from a blocker to the ticket it blocks. The text tree puts
//! each ticket above its blockers, so reading down means "this needs that".

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::{Error, Result};
use crate::identity::Env;
use crate::model::ListResult;

/// Fields are declared alphabetically: that is the JSON key order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub available: bool,
    pub blocked: bool,
    /// Distinct tickets that transitively wait on this one.
    pub blocks: usize,
    /// On the critical path.
    pub critical: bool,
    /// A blocker that is not in the listed set (other status filter or scope).
    pub external: bool,
    /// `agent` or `human`; null for an external node, which is known only as a blocker.
    #[serde(rename = "for")]
    pub for_who: Option<String>,
    pub id: String,
    pub status: String,
    pub title: String,
}

/// `from` blocks `to`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopBlocker {
    pub blocks: usize,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    /// The longest chain of unfinished tickets, first blocker first; empty
    /// when no unfinished ticket waits on another.
    pub critical_path: Vec<String>,
    pub edges: Vec<Edge>,
    pub nodes: Vec<Node>,
    /// Up to five tickets that block the most others, most first.
    pub top_blockers: Vec<TopBlocker>,
}

fn unfinished(status: &str) -> bool {
    matches!(status, "draft" | "ready" | "active")
}

pub fn build(list: &ListResult) -> Result<Graph> {
    let mut nodes: Vec<Node> = list
        .tickets
        .iter()
        .map(|t| Node {
            available: t.available,
            blocked: t.blocked,
            blocks: 0,
            critical: false,
            external: false,
            for_who: Some(t.for_who.clone()),
            id: t.id.clone(),
            status: t.status.clone(),
            title: t.title.clone(),
        })
        .collect();
    let mut index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect();
    let mut edges = BTreeSet::new();
    let mut stubs: BTreeMap<String, Node> = BTreeMap::new();
    for t in &list.tickets {
        for b in &t.blocked_by {
            if !index.contains_key(&b.id) {
                stubs.entry(b.id.clone()).or_insert_with(|| Node {
                    available: false,
                    blocked: false,
                    blocks: 0,
                    critical: false,
                    external: true,
                    for_who: None,
                    id: b.id.clone(),
                    status: b.status.clone(),
                    title: b.title.clone(),
                });
            }
            edges.insert(Edge {
                from: b.id.clone(),
                to: t.id.clone(),
            });
        }
    }
    for (_, n) in stubs {
        index.insert(n.id.clone(), nodes.len());
        nodes.push(n);
    }
    let edges: Vec<Edge> = edges.into_iter().collect();

    // blockers_of[i] / dependents_of[i], as node indices.
    let mut blockers_of = vec![vec![]; nodes.len()];
    let mut dependents_of = vec![vec![]; nodes.len()];
    for e in &edges {
        let (f, t) = (index[&e.from], index[&e.to]);
        blockers_of[t].push(f);
        dependents_of[f].push(t);
    }

    // Longest chain of unfinished tickets ending at each node, walking
    // blockers. The database rejects cycles; a cycle here is corrupt data.
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Open,
        Done,
    }
    let mut marks = vec![Mark::New; nodes.len()];
    let mut chain = vec![0usize; nodes.len()];
    for start in 0..nodes.len() {
        if marks[start] != Mark::New {
            continue;
        }
        // Iterative post-order DFS: (node, next blocker position).
        let mut stack = vec![(start, 0usize)];
        marks[start] = Mark::Open;
        while let Some(&(n, pos)) = stack.last() {
            if pos < blockers_of[n].len() {
                stack.last_mut().expect("stack is non-empty").1 += 1;
                let b = blockers_of[n][pos];
                match marks[b] {
                    Mark::Open => {
                        return Err(Error::Other(format!(
                            "dependency cycle through {} and {}",
                            nodes[n].id, nodes[b].id
                        )));
                    }
                    Mark::New => {
                        marks[b] = Mark::Open;
                        stack.push((b, 0));
                    }
                    Mark::Done => {}
                }
            } else {
                chain[n] = if unfinished(&nodes[n].status) {
                    1 + blockers_of[n]
                        .iter()
                        .filter(|&&b| unfinished(&nodes[b].status))
                        .map(|&b| chain[b])
                        .max()
                        .unwrap_or(0)
                } else {
                    0
                };
                marks[n] = Mark::Done;
                stack.pop();
            }
        }
    }

    // The critical path ends at the longest chain (ties: smallest id) and
    // follows the longest blocker at each step (ties: smallest id).
    let mut critical_path = vec![];
    let best = (0..nodes.len())
        .filter(|&i| chain[i] >= 2)
        .max_by(|&a, &b| chain[a].cmp(&chain[b]).then(nodes[b].id.cmp(&nodes[a].id)));
    if let Some(mut cur) = best {
        loop {
            critical_path.push(cur);
            let next = blockers_of[cur]
                .iter()
                .copied()
                .filter(|&b| unfinished(&nodes[b].status) && chain[b] + 1 == chain[cur])
                .min_by(|&a, &b| nodes[a].id.cmp(&nodes[b].id));
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        critical_path.reverse();
    }
    for &i in &critical_path {
        nodes[i].critical = true;
    }

    // Distinct transitive dependents.
    for i in 0..nodes.len() {
        let mut seen = BTreeSet::new();
        let mut todo = dependents_of[i].clone();
        while let Some(n) = todo.pop() {
            if seen.insert(n) {
                todo.extend(&dependents_of[n]);
            }
        }
        nodes[i].blocks = seen.len();
    }
    let mut top_blockers: Vec<TopBlocker> = nodes
        .iter()
        .filter(|n| n.blocks >= 1 && unfinished(&n.status))
        .map(|n| TopBlocker {
            blocks: n.blocks,
            id: n.id.clone(),
        })
        .collect();
    top_blockers.sort_by(|a, b| b.blocks.cmp(&a.blocks).then_with(|| a.id.cmp(&b.id)));
    top_blockers.truncate(5);

    Ok(Graph {
        critical_path: critical_path.iter().map(|&i| nodes[i].id.clone()).collect(),
        edges,
        nodes,
        top_blockers,
    })
}

/// Unicode unless `ascii` is set, `TERM=dumb`, or the locale (`LC_ALL`, then
/// `LC_CTYPE`, then `LANG`, first one set) names a non-UTF-8 charset. With no
/// locale variable set at all, Unicode. `NO_COLOR` does not affect glyphs.
pub fn use_unicode(ascii: bool, env: &Env) -> bool {
    if ascii || env.get("TERM") == Some("dumb") {
        return false;
    }
    match ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|v| env.get(v))
    {
        None => true,
        Some(l) => {
            let l = l.to_ascii_lowercase();
            l.contains("utf-8") || l.contains("utf8")
        }
    }
}

struct Glyphs {
    branch: &'static str,
    last: &'static str,
    pipe: &'static str,
    blank: &'static str,
    done: &'static str,
    active: &'static str,
    open: &'static str,
    blocked: &'static str,
    abandoned: &'static str,
}

const UNICODE: Glyphs = Glyphs {
    branch: "├─ ",
    last: "└─ ",
    pipe: "│  ",
    blank: "   ",
    done: "●",
    active: "◐",
    open: "○",
    // A lock emoji is double-width in many terminals and breaks alignment.
    blocked: "⊘",
    abandoned: "✕",
};

const ASCII: Glyphs = Glyphs {
    branch: "|-- ",
    last: "`-- ",
    pipe: "|   ",
    blank: "    ",
    done: "[x]",
    active: "[~]",
    open: "[ ]",
    blocked: "[#]",
    abandoned: "[-]",
};

impl Glyphs {
    fn icon(&self, n: &Node) -> &'static str {
        match n.status.as_str() {
            "done" => self.done,
            "abandoned" => self.abandoned,
            _ if n.blocked => self.blocked,
            "active" => self.active,
            _ => self.open,
        }
    }
}

pub fn render_text(g: &Graph, unicode: bool, color: bool) -> String {
    let gl = if unicode { &UNICODE } else { &ASCII };
    let by_id: HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut blockers: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut has_dependent: BTreeSet<&str> = BTreeSet::new();
    for e in &g.edges {
        blockers.entry(&e.to).or_default().push(&e.from);
        has_dependent.insert(&e.from);
    }
    let mut out = String::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();

    // Iterative pre-order walk; each frame is (node, prefix for its children, is_root).
    fn line(gl: &Glyphs, n: &Node, color: bool, again: bool) -> String {
        let mut s = format!("{} {}", gl.icon(n), n.id);
        if n.critical {
            s.push_str(" *");
        }
        s.push(' ');
        s.push_str(&n.title);
        if n.external {
            s.push_str(&format!(" ({}, not listed)", n.status));
        }
        if again {
            s.push_str(" (see above)");
        }
        if color && n.critical {
            format!("\x1b[1m{s}\x1b[0m")
        } else if color && matches!(n.status.as_str(), "done" | "abandoned") {
            format!("\x1b[2m{s}\x1b[0m")
        } else {
            s
        }
    }

    for root in g
        .nodes
        .iter()
        .filter(|n| !has_dependent.contains(n.id.as_str()))
    {
        seen.insert(&root.id);
        out.push_str(&line(gl, root, color, false));
        out.push('\n');
        // Stack of (node id, child prefix); children pushed in reverse so the
        // first child is drawn first.
        let mut stack: Vec<(&str, String, bool)> = vec![];
        fn push_children<'a>(
            stack: &mut Vec<(&'a str, String, bool)>,
            blockers: &HashMap<&'a str, Vec<&'a str>>,
            id: &str,
            prefix: &str,
        ) {
            let kids = blockers.get(id).map(Vec::as_slice).unwrap_or_default();
            for (i, k) in kids.iter().enumerate().rev() {
                stack.push((k, prefix.to_string(), i + 1 == kids.len()));
            }
        }
        push_children(&mut stack, &blockers, &root.id, "");
        while let Some((id, prefix, is_last)) = stack.pop() {
            let n = by_id[id];
            let again = !seen.insert(id);
            out.push_str(&prefix);
            out.push_str(if is_last { gl.last } else { gl.branch });
            out.push_str(&line(gl, n, color, again));
            out.push('\n');
            if !again {
                let child_prefix = format!("{prefix}{}", if is_last { gl.blank } else { gl.pipe });
                push_children(&mut stack, &blockers, id, &child_prefix);
            }
        }
    }
    if !g.critical_path.is_empty() {
        out.push_str(&format!(
            "critical path: {}\n",
            g.critical_path.join(" -> ")
        ));
    }
    if !g.top_blockers.is_empty() {
        let tb: Vec<String> = g
            .top_blockers
            .iter()
            .map(|t| format!("{} (blocks {})", t.id, t.blocks))
            .collect();
        out.push_str(&format!("top blockers: {}\n", tb.join(", ")));
    }
    out.trim_end().to_string()
}

fn mermaid_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn mermaid_label(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '<' | '>') && !c.is_control())
        .collect::<String>()
        .replace('"', "#quot;")
}

pub fn render_mermaid(g: &Graph) -> String {
    let mut out = String::from("flowchart TD\n");
    for n in &g.nodes {
        out.push_str(&format!(
            "    {}[\"{}: {}\"]\n",
            mermaid_id(&n.id),
            mermaid_label(&n.id),
            mermaid_label(&n.title)
        ));
    }
    for e in &g.edges {
        out.push_str(&format!(
            "    {} --> {}\n",
            mermaid_id(&e.from),
            mermaid_id(&e.to)
        ));
    }
    out.push_str("    classDef done fill:#d4edda,stroke:#2e7d32\n");
    out.push_str("    classDef active fill:#fff3cd,stroke:#f9a825\n");
    out.push_str("    classDef ready fill:#ffffff,stroke:#555555\n");
    out.push_str("    classDef blocked fill:#f8d7da,stroke:#c62828\n");
    out.push_str("    classDef external stroke-dasharray:4 2,fill:#eeeeee\n");
    out.push_str("    classDef abandoned fill:#eeeeee,stroke:#999999\n");
    for n in &g.nodes {
        let class = if n.external {
            "external"
        } else {
            match n.status.as_str() {
                "done" => "done",
                "abandoned" => "abandoned",
                _ if n.blocked => "blocked",
                "active" => "active",
                _ => "ready",
            }
        };
        out.push_str(&format!("    class {} {class}\n", mermaid_id(&n.id)));
    }
    let crit: Vec<usize> = g
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            g.critical_path
                .windows(2)
                .any(|w| w[0] == e.from && w[1] == e.to)
        })
        .map(|(i, _)| i)
        .collect();
    if !crit.is_empty() {
        let idx: Vec<String> = crit.iter().map(|i| i.to_string()).collect();
        out.push_str(&format!(
            "    linkStyle {} stroke:#c62828,stroke-width:3px\n",
            idx.join(",")
        ));
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Blocker, Ticket};

    fn t(id: &str, status: &str, blockers: &[(&str, &str)]) -> Ticket {
        Ticket {
            archived_at: None,
            available: false,
            blocked: !blockers.is_empty(),
            blocking_decisions: vec![],
            blocked_by: blockers
                .iter()
                .map(|(b, s)| Blocker {
                    id: b.to_string(),
                    status: s.to_string(),
                    title: format!("title {b}"),
                    worktree: None,
                })
                .collect(),
            claim_touched_at: None,
            claimed_by: None,
            created_at: String::new(),
            criteria: Default::default(),
            done_at: None,
            for_who: "agent".into(),
            id: id.into(),
            parent: None,
            project: Default::default(),
            status: status.into(),
            subtasks: vec![],
            title: format!("title {id}"),
            updated_at: String::new(),
            worktree: None,
        }
    }

    fn diamond() -> Graph {
        // D needs B and C; both need A.
        build(&ListResult {
            tickets: vec![
                t("D", "ready", &[("B", "ready"), ("C", "ready")]),
                t("B", "ready", &[("A", "ready")]),
                t("C", "ready", &[("A", "ready")]),
                t("A", "ready", &[]),
            ],
        })
        .unwrap()
    }

    #[test]
    fn critical_path_and_blockers() {
        let g = diamond();
        assert_eq!(g.critical_path, ["A", "B", "D"]);
        assert_eq!(
            g.top_blockers[0],
            TopBlocker {
                blocks: 3,
                id: "A".into()
            }
        );
        assert_eq!(g.nodes.iter().find(|n| n.id == "B").unwrap().blocks, 1);
    }

    #[test]
    fn diamond_is_drawn_once() {
        let s = render_text(&diamond(), true, false);
        assert_eq!(s.matches("title A").count(), 2, "{s}");
        assert_eq!(s.matches("(see above)").count(), 1, "{s}");
        assert!(s.contains("critical path: A -> B -> D"));
    }

    #[test]
    fn external_blocker_becomes_a_stub() {
        let g = build(&ListResult {
            tickets: vec![t("X", "ready", &[("Z", "abandoned")])],
        })
        .unwrap();
        let z = g.nodes.iter().find(|n| n.id == "Z").unwrap();
        assert!(z.external && z.for_who.is_none());
        assert!(g.critical_path.is_empty());
    }

    #[test]
    fn cycle_is_an_error() {
        let r = build(&ListResult {
            tickets: vec![
                t("A", "ready", &[("B", "ready")]),
                t("B", "ready", &[("A", "ready")]),
            ],
        });
        assert!(matches!(r, Err(Error::Other(_))));
    }

    #[test]
    fn mermaid_escapes_labels() {
        let mut g = diamond();
        g.nodes[0].title = "say \"hi\" <b>".into();
        let m = render_mermaid(&g);
        assert!(m.contains("say #quot;hi#quot; b"), "{m}");
        assert!(m.contains("    A --> B"), "{m}");
    }

    #[test]
    fn glyphs_follow_locale() {
        let env = |pairs: &[(&str, &str)]| Env {
            vars: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        };
        assert!(use_unicode(false, &env(&[])));
        assert!(use_unicode(false, &env(&[("LANG", "en_US.UTF-8")])));
        assert!(!use_unicode(false, &env(&[("LANG", "C")])));
        assert!(!use_unicode(
            false,
            &env(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")])
        ));
        assert!(!use_unicode(true, &env(&[])));
        assert!(!use_unicode(false, &env(&[("TERM", "dumb")])));
        assert!(use_unicode(false, &env(&[("NO_COLOR", "1")])));
    }
}
