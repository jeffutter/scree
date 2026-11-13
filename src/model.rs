//! The Ticket wire shape from `docs/cli.md` as typed values.
//! `ticket::build` is the only producer; `output` renders from these and the
//! TUI and tests read them back with `Deserialize`.
//!
//! The JSON key order is part of the contract. serde_json has no
//! `preserve_order`, so the old `Value` output was alphabetical; derived
//! structs write keys in declaration order, so fields here are declared
//! alphabetically. `tests/golden.rs` pins the exact bytes.

use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Who did something: the author and an optional session label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claimer {
    pub author: String,
    pub session: Option<String>,
}

/// A dependency or unsettled subtask holding a ticket back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blocker {
    pub id: String,
    pub status: String,
    pub title: String,
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subtask {
    pub id: String,
    pub status: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub checked: bool,
    pub checked_at: Option<String>,
    pub checked_by: Option<Claimer>,
    pub evidence: Option<String>,
    pub number: i64,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub id: i64,
    pub role: Option<String>,
    pub session: Option<String>,
}

/// The list shape: a ticket without description, plan, criteria or comments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ticket {
    pub archived_at: Option<String>,
    pub available: bool,
    pub blocked: bool,
    pub blocked_by: Vec<Blocker>,
    pub claimed_by: Option<Claimer>,
    pub created_at: String,
    pub done_at: Option<String>,
    #[serde(rename = "for")]
    pub for_who: String,
    pub id: String,
    pub parent: Option<String>,
    pub status: String,
    pub subtasks: Vec<Subtask>,
    pub title: String,
    pub updated_at: String,
    pub worktree: Option<String>,
}

/// The `show` shape, also returned by every mutation.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TicketDetail {
    #[serde(flatten)]
    pub ticket: Ticket,
    pub comments: Vec<Comment>,
    pub criteria: Vec<Criterion>,
    pub description: String,
    pub plan: String,
}

// Flattening would write the summary keys first, then the detail keys; the
// wire shape interleaves them alphabetically, so merge through a sorted map.
impl Serialize for TicketDetail {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let to_value =
            |v: serde_json::Result<serde_json::Value>| v.map_err(serde::ser::Error::custom);
        let serde_json::Value::Object(base) = to_value(serde_json::to_value(&self.ticket))? else {
            return Err(serde::ser::Error::custom("ticket is not an object"));
        };
        let mut all: BTreeMap<String, serde_json::Value> = base.into_iter().collect();
        all.insert(
            "comments".into(),
            to_value(serde_json::to_value(&self.comments))?,
        );
        all.insert(
            "criteria".into(),
            to_value(serde_json::to_value(&self.criteria))?,
        );
        all.insert("description".into(), self.description.clone().into());
        all.insert("plan".into(), self.plan.clone().into());
        let mut m = s.serialize_map(Some(all.len()))?;
        for (k, v) in &all {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

/// The `{"tickets": [...]}` shape returned by `list` and `next`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListResult {
    pub tickets: Vec<Ticket>,
}

/// The `next` result: the list plus what was held back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NextResult {
    #[serde(flatten)]
    pub list: ListResult,
    pub human_pending: i64,
    /// Present only when `tickets` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_pending: Option<i64>,
}
