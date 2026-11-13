//! Runs an Effect against the ticket library and returns the resulting Event.

use std::path::Path;

use rusqlite::Connection;

use super::state::{Effect, Event};
use crate::cli::ListArgs;
use crate::identity::Identity;
use crate::ticket;

pub fn run_effect(conn: &Connection, cwd: &Path, identity: &Identity, effect: Effect) -> Event {
    match effect {
        Effect::Reload => Event::Loaded(
            ticket::list(conn, cwd, &ListArgs::default(), identity)
                .map(|r| r.tickets)
                .map_err(|e| e.to_string()),
        ),
    }
}
