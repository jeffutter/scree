//! Server-rendered pages: the shell in `layout`, shared fragments in
//! `components`. Handlers build a [`layout::PageCtx`] and hand the page body
//! to [`layout::page`]; nothing here touches the database except
//! [`banner_counts`].

pub mod components;
pub mod graph_svg;
pub mod layout;
pub mod markdown;
pub mod picker;
pub mod ticket_row;

use std::collections::HashSet;

use rusqlite::Connection;

use crate::decision;
use crate::error::Result;
use crate::ticket::ScopeFilter;

/// Counts for the open-Decisions banner: `(open Decisions, distinct Tickets
/// they block)`, or `None` when no Decision is open. A Ticket behind two
/// Decisions counts once, so this is the union of the queue's ticket lists,
/// not the sum of `unblocks`.
pub fn banner_counts(conn: &Connection) -> Result<Option<(usize, usize)>> {
    let queue = decision::open_queue_in(conn, &ScopeFilter::All)?;
    if queue.decisions.is_empty() {
        return Ok(None);
    }
    let blocked: HashSet<&str> = queue
        .decisions
        .iter()
        .flat_map(|d| d.tickets.iter().map(String::as_str))
        .collect();
    Ok(Some((queue.decisions.len(), blocked.len())))
}
