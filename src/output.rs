use std::fmt::Write;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::identity::{Env, Role};
use crate::model::{ListResult, TicketDetail};

pub fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| Error::Other(e.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Text,
}

/// An explicit flag wins, then `SCREE_FORMAT` (`json` or `text`), then text
/// only for a human on a terminal. Everything else is JSON.
pub fn decide_format(json: bool, text: bool, role: Role, env: &Env) -> Result<Format> {
    match (json, text) {
        (true, true) => return Err(Error::Usage("--json and --text conflict".into())),
        (true, false) => return Ok(Format::Json),
        (false, true) => return Ok(Format::Text),
        _ => {}
    }
    match env.get("SCREE_FORMAT") {
        Some("json") => Ok(Format::Json),
        Some("text") => Ok(Format::Text),
        Some(other) => Err(Error::Usage(format!(
            "SCREE_FORMAT must be json or text, got {other:?}"
        ))),
        None if role == Role::Human && env.stdout_tty => Ok(Format::Text),
        None => Ok(Format::Json),
    }
}

/// Colour only on a terminal, and never when `NO_COLOR` is set non-empty.
pub fn use_color(env: &Env) -> bool {
    env.stdout_tty && env.get("NO_COLOR").is_none()
}

/// One line per ticket: `SCR-12  active  Title [blocked] [human]`, columns
/// aligned, and a `[blocked by ...]` line under a blocked ticket.
pub fn render_text_list(list: &ListResult, color: bool) -> String {
    let id_w = list.tickets.iter().map(|t| t.id.len()).max().unwrap_or(0);
    let st_w = list
        .tickets
        .iter()
        .map(|t| t.status.len())
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for t in &list.tickets {
        let mut line = format!("{:<id_w$}  {:<st_w$}  {}", t.id, t.status, t.title);
        if t.blocked {
            line.push_str(" [blocked]");
        }
        if t.for_who == "human" {
            line.push_str(" [human]");
        }
        let dim = color && (t.status == "done" || t.status == "abandoned");
        if dim {
            let _ = writeln!(out, "\x1b[2m{}\x1b[0m", line.trim_end());
        } else {
            let _ = writeln!(out, "{}", line.trim_end());
        }
        if t.blocked {
            let names: Vec<String> = t
                .blocked_by
                .iter()
                .map(|b| {
                    let mut s = b.id.clone();
                    // The JSON carries the full path; text names the directory.
                    let full = b.worktree.as_deref().unwrap_or("");
                    let wt = std::path::Path::new(full)
                        .file_name()
                        .map_or(full.to_string(), |n| n.to_string_lossy().into_owned());
                    if !wt.is_empty() {
                        let _ = write!(s, " in {wt}");
                    }
                    if b.status == "abandoned" {
                        s.push_str(" (abandoned)");
                    }
                    s
                })
                .collect();
            if !names.is_empty() {
                let _ = writeln!(out, "  [blocked by {}]", names.join(", "));
            }
        }
    }
    out.trim_end_matches('\n').to_string()
}

/// The `show` view: header, description, plan, checklist, blockers,
/// subtasks and comments.
pub fn render_text_ticket(d: &TicketDetail) -> String {
    let t = &d.ticket;
    let mut out = String::new();
    let mut head = format!("{}  {}  {}", t.id, t.status, t.title);
    if t.blocked {
        head.push_str(" [blocked]");
    }
    if t.for_who == "human" {
        head.push_str(" [human]");
    }
    let _ = writeln!(out, "{head}");
    let _ = writeln!(out, "for: {}", t.for_who);
    // LOCAL tickets belong to no repository, so they have no worktree to name.
    if !t.id.starts_with("LOCAL-") {
        let wt = t.worktree.as_deref().unwrap_or("");
        let _ = writeln!(
            out,
            "worktree: {}",
            if wt.is_empty() { "(root checkout)" } else { wt }
        );
    }
    if let Some(c) = &t.claimed_by {
        let _ = writeln!(
            out,
            "claimed by: {} ({})",
            c.author,
            c.session.as_deref().unwrap_or("")
        );
    }
    let _ = writeln!(out, "created: {}  updated: {}", t.created_at, t.updated_at);
    for (label, body) in [("Description", &d.description), ("Plan", &d.plan)] {
        if !body.is_empty() {
            let _ = write!(out, "\n{label}:\n{body}\n");
        }
    }
    if !d.criteria.is_empty() {
        let _ = writeln!(out, "\nAcceptance criteria:");
        for c in &d.criteria {
            let mark = if c.checked { "x" } else { " " };
            let _ = write!(out, "[{mark}] #{} {}", c.number, c.text);
            if let Some(ev) = c.evidence.as_deref().filter(|e| !e.is_empty()) {
                let _ = write!(out, " ({ev})");
            }
            let _ = writeln!(out);
        }
    }
    if !t.blocked_by.is_empty() {
        let _ = writeln!(out, "\nBlocked by:");
        for x in &t.blocked_by {
            let _ = writeln!(out, "{}  {}  {}", x.id, x.status, x.title);
        }
    }
    if !t.subtasks.is_empty() {
        let _ = writeln!(out, "\nSubtasks:");
        for x in &t.subtasks {
            let _ = writeln!(out, "{}  {}  {}", x.id, x.status, x.title);
        }
    }
    if !d.comments.is_empty() {
        let _ = writeln!(out, "\nComments:");
        for c in &d.comments {
            let _ = writeln!(out, "{} {}: {}", c.created_at, c.author, c.body);
        }
    }
    out.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Blocker, Ticket};

    fn env(vars: &[(&str, &str)], stdout_tty: bool) -> Env {
        Env {
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            stdin_tty: stdout_tty,
            stdout_tty,
        }
    }

    #[test]
    fn format_decision() {
        let tty = env(&[], true);
        let pipe = env(&[], false);
        let f = |j, t, r, e: &Env| decide_format(j, t, r, e).unwrap();
        assert_eq!(f(false, false, Role::Human, &tty), Format::Text);
        assert_eq!(f(false, false, Role::Human, &pipe), Format::Json);
        assert_eq!(f(false, false, Role::Agent, &tty), Format::Json);
        assert_eq!(f(true, false, Role::Human, &tty), Format::Json);
        assert_eq!(f(false, true, Role::Agent, &pipe), Format::Text);
        let e = env(&[("SCREE_FORMAT", "text")], false);
        assert_eq!(f(false, false, Role::Agent, &e), Format::Text);
        assert_eq!(f(true, false, Role::Agent, &e), Format::Json);
        let e = env(&[("SCREE_FORMAT", "yaml")], false);
        assert!(matches!(
            decide_format(false, false, Role::Agent, &e),
            Err(Error::Usage(_))
        ));
        assert!(matches!(
            decide_format(true, true, Role::Agent, &pipe),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn color_rules() {
        assert!(use_color(&env(&[], true)));
        assert!(!use_color(&env(&[], false)));
        assert!(!use_color(&env(&[("NO_COLOR", "1")], true)));
        assert!(use_color(&env(&[("NO_COLOR", "")], true)));
    }

    fn ticket(id: &str, status: &str, title: &str) -> Ticket {
        Ticket {
            archived_at: None,
            available: false,
            blocked: false,
            blocked_by: vec![],
            claimed_by: None,
            created_at: String::new(),
            done_at: None,
            for_who: "agent".into(),
            id: id.into(),
            parent: None,
            status: status.into(),
            subtasks: vec![],
            title: title.into(),
            updated_at: String::new(),
            worktree: None,
        }
    }

    fn blocker(id: &str, status: &str, worktree: Option<&str>) -> Blocker {
        Blocker {
            id: id.into(),
            status: status.into(),
            title: String::new(),
            worktree: worktree.map(Into::into),
        }
    }

    fn sample() -> ListResult {
        let mut first = ticket("SCR-2", "active", "First");
        first.for_who = "human".into();
        let mut second = ticket("SCR-12", "ready", "Second");
        second.blocked = true;
        second.blocked_by = vec![
            blocker("SCR-9", "ready", Some("/w/feat-b")),
            blocker("SCR-8", "abandoned", None),
        ];
        ListResult {
            tickets: vec![first, second, ticket("SCR-1", "done", "Old")],
        }
    }

    #[test]
    fn text_rendering() {
        let out = render_text_list(&sample(), false);
        assert_eq!(
            out,
            "SCR-2   active  First [human]\n\
             SCR-12  ready   Second [blocked]\n  \
             [blocked by SCR-9 in feat-b, SCR-8 (abandoned)]\n\
             SCR-1   done    Old"
        );
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn text_color_dims_done() {
        let out = render_text_list(&sample(), true);
        assert!(out.contains("\x1b[2mSCR-1   done    Old\x1b[0m"));
    }

    #[test]
    fn text_empty_list() {
        assert_eq!(render_text_list(&ListResult { tickets: vec![] }, false), "");
    }
}
