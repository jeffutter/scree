use std::fmt::Write;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::identity::{Env, Role};

/// The `{"tickets": [...]}` shape returned by `list` and `next`.
#[derive(Debug, Serialize)]
pub struct ListResult {
    pub tickets: Vec<serde_json::Value>,
}

/// The `next` result: the list plus what was held back.
#[derive(Debug, Serialize)]
pub struct NextResult {
    #[serde(flatten)]
    pub list: ListResult,
    pub human_pending: i64,
    /// Present only when `tickets` is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_pending: Option<i64>,
}

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
/// aligned, and a `[blocked by ...]` line under a blocked ticket. Reads
/// fields from the JSON value and tolerates missing ones.
pub fn render_text_list(list: &ListResult, color: bool) -> String {
    let field = |t: &serde_json::Value, k: &str| match t.get(k) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let id_w = list
        .tickets
        .iter()
        .map(|t| field(t, "id").len())
        .max()
        .unwrap_or(0);
    let st_w = list
        .tickets
        .iter()
        .map(|t| field(t, "status").len())
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for t in &list.tickets {
        let status = field(t, "status");
        let mut line = format!(
            "{:<id_w$}  {:<st_w$}  {}",
            field(t, "id"),
            status,
            field(t, "title")
        );
        let blocked = t.get("blocked").and_then(|b| b.as_bool()).unwrap_or(false);
        if blocked {
            line.push_str(" [blocked]");
        }
        if t.get("for").and_then(|f| f.as_str()) == Some("human") {
            line.push_str(" [human]");
        }
        let dim = color && (status == "done" || status == "abandoned");
        if dim {
            let _ = writeln!(out, "\x1b[2m{}\x1b[0m", line.trim_end());
        } else {
            let _ = writeln!(out, "{}", line.trim_end());
        }
        if blocked && let Some(bs) = t.get("blocked_by").and_then(|b| b.as_array()) {
            let names: Vec<String> = bs
                .iter()
                .map(|b| {
                    let mut s = field(b, "id");
                    let wt = field(b, "worktree");
                    if !wt.is_empty() {
                        let _ = write!(s, " in {wt}");
                    }
                    if field(b, "status") == "abandoned" {
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
/// subtasks and comments. Reads fields from the JSON and tolerates gaps.
pub fn render_text_ticket(t: &serde_json::Value) -> String {
    let s = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let mut out = String::new();
    let mut head = format!("{}  {}  {}", s(t, "id"), s(t, "status"), s(t, "title"));
    if t.get("blocked").and_then(|b| b.as_bool()).unwrap_or(false) {
        head.push_str(" [blocked]");
    }
    if s(t, "for") == "human" {
        head.push_str(" [human]");
    }
    let _ = writeln!(out, "{head}");
    let _ = writeln!(out, "for: {}", s(t, "for"));
    // LOCAL tickets belong to no repository, so they have no worktree to name.
    if !s(t, "id").starts_with("LOCAL-") {
        let wt = s(t, "worktree");
        let _ = writeln!(
            out,
            "worktree: {}",
            if wt.is_empty() {
                "(root checkout)"
            } else {
                &wt
            }
        );
    }
    if let Some(c) = t.get("claimed_by").filter(|c| !c.is_null()) {
        let _ = writeln!(out, "claimed by: {} ({})", s(c, "author"), s(c, "session"));
    }
    let _ = writeln!(
        out,
        "created: {}  updated: {}",
        s(t, "created_at"),
        s(t, "updated_at")
    );
    for (label, key) in [("Description", "description"), ("Plan", "plan")] {
        let body = s(t, key);
        if !body.is_empty() {
            let _ = write!(out, "\n{label}:\n{body}\n");
        }
    }
    if let Some(cs) = t
        .get("criteria")
        .and_then(|c| c.as_array())
        .filter(|c| !c.is_empty())
    {
        let _ = writeln!(out, "\nAcceptance criteria:");
        for c in cs {
            let mark = if c.get("checked").and_then(|b| b.as_bool()).unwrap_or(false) {
                "x"
            } else {
                " "
            };
            let n = c.get("number").and_then(|n| n.as_i64()).unwrap_or(0);
            let _ = write!(out, "[{mark}] #{n} {}", s(c, "text"));
            let ev = s(c, "evidence");
            if !ev.is_empty() {
                let _ = write!(out, " ({ev})");
            }
            let _ = writeln!(out);
        }
    }
    for (label, key) in [("Blocked by", "blocked_by"), ("Subtasks", "subtasks")] {
        if let Some(xs) = t
            .get(key)
            .and_then(|c| c.as_array())
            .filter(|c| !c.is_empty())
        {
            let _ = writeln!(out, "\n{label}:");
            for x in xs {
                let _ = writeln!(out, "{}  {}  {}", s(x, "id"), s(x, "status"), s(x, "title"));
            }
        }
    }
    if let Some(cs) = t
        .get("comments")
        .and_then(|c| c.as_array())
        .filter(|c| !c.is_empty())
    {
        let _ = writeln!(out, "\nComments:");
        for c in cs {
            let _ = writeln!(
                out,
                "{} {}: {}",
                s(c, "created_at"),
                s(c, "author"),
                s(c, "body")
            );
        }
    }
    out.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    fn sample() -> ListResult {
        ListResult {
            tickets: vec![
                json!({"id": "SCR-2", "status": "active", "title": "First", "for": "human"}),
                json!({"id": "SCR-12", "status": "ready", "title": "Second", "blocked": true,
                    "blocked_by": [{"id": "SCR-9", "status": "ready", "worktree": "feat-b"},
                                   {"id": "SCR-8", "status": "abandoned"}]}),
                json!({"id": "SCR-1", "status": "done", "title": "Old"}),
            ],
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
    fn text_tolerates_skeleton_shape() {
        let l = ListResult {
            tickets: vec![json!({"id": 7})],
        };
        assert_eq!(render_text_list(&l, false), "7");
        assert_eq!(render_text_list(&ListResult { tickets: vec![] }, false), "");
    }
}
