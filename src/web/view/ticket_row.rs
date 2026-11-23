//! One ticket as a list row. Every ticket-supplied string goes through maud's
//! escaping; nothing here builds markup from ticket text.

use maud::{Markup, html};

use crate::model::Ticket;

/// Whether the ticket detail screen is served. Until it is, the id is plain
/// text instead of a dead link. The screen's ticket flips this.
const DETAIL_BUILT: bool = false;

/// The `[blocked by ...]` text of a blocked ticket, as the CLI's text list
/// writes it: blocker ids with the Worktree directory name and `(abandoned)`,
/// then open Decision ids. `None` when nothing is named.
pub fn blocked_by(t: &Ticket) -> Option<String> {
    if !t.blocked {
        return None;
    }
    let mut names: Vec<String> = t
        .blocked_by
        .iter()
        .map(|b| {
            let mut s = b.id.clone();
            let full = b.worktree.as_deref().unwrap_or("");
            let wt = std::path::Path::new(full)
                .file_name()
                .map_or(full.to_string(), |n| n.to_string_lossy().into_owned());
            if !wt.is_empty() {
                s.push_str(" in ");
                s.push_str(&wt);
            }
            if b.status == "abandoned" {
                s.push_str(" (abandoned)");
            }
            s
        })
        .collect();
    names.extend(t.blocking_decisions.iter().map(|d| d.id.clone()));
    if names.is_empty() {
        None
    } else {
        Some(format!("[blocked by {}]", names.join(", ")))
    }
}

/// The row: status as text (never colour alone), id, title, and the markers
/// that matter when scanning (human, claimed, blocked). `data-id` is what the
/// keyboard selection keys on.
pub fn row(t: &Ticket) -> Markup {
    let status_class = if t.available {
        "available"
    } else {
        t.status.as_str()
    };
    html! {
        li class="row" data-id=(t.id) {
            span class=(format!("status status-{status_class}")) { (t.status) }
            @if DETAIL_BUILT {
                a class="row-id" href=(format!("/ticket/{}", t.id)) { (t.id) }
            } @else {
                span class="row-id" { (t.id) }
            }
            span class="row-title" { (t.title) }
            @if t.available {
                span class="tag" { "available" }
            }
            @if t.for_who == "human" {
                span class="tag" { "human" }
            }
            @if let Some(c) = &t.claimed_by {
                span class="tag" { "claimed by " (c.author) }
            }
            @if let Some(b) = blocked_by(t) {
                span class="row-blocked" { (b) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Blocker, BlockingDecision};

    fn ticket(title: &str) -> Ticket {
        Ticket {
            archived_at: None,
            available: false,
            blocked: false,
            blocked_by: vec![],
            blocking_decisions: vec![],
            claim_touched_at: None,
            claimed_by: None,
            created_at: String::new(),
            criteria: Default::default(),
            done_at: None,
            for_who: "agent".into(),
            id: "SCR-1".into(),
            parent: None,
            project: Default::default(),
            status: "ready".into(),
            subtasks: vec![],
            title: title.into(),
            updated_at: String::new(),
            worktree: None,
        }
    }

    #[test]
    fn title_is_escaped() {
        let html = row(&ticket("<script>alert(1)</script>")).into_string();
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("data-id=\"SCR-1\""));
    }

    #[test]
    fn blocked_summary_names_blockers_and_decisions() {
        let mut t = ticket("x");
        t.blocked = true;
        t.blocked_by = vec![
            Blocker {
                id: "SCR-9".into(),
                status: "active".into(),
                title: "t".into(),
                worktree: Some("/w/feat-b".into()),
            },
            Blocker {
                id: "SCR-8".into(),
                status: "abandoned".into(),
                title: "t".into(),
                worktree: None,
            },
        ];
        t.blocking_decisions = vec![BlockingDecision {
            id: "D-2".into(),
            question: "q".into(),
        }];
        assert_eq!(
            blocked_by(&t).unwrap(),
            "[blocked by SCR-9 in feat-b, SCR-8 (abandoned), D-2]"
        );
        assert!(blocked_by(&ticket("x")).is_none());
    }

    #[test]
    fn status_is_text_and_markers_show() {
        let mut t = ticket("x");
        t.for_who = "human".into();
        t.available = true;
        let html = row(&t).into_string();
        assert!(html.contains(">ready</span>"));
        assert!(html.contains("status-available"));
        assert!(html.contains(">human</span>"));
    }
}
