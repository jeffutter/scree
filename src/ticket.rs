use rusqlite::Connection;

use crate::error::Result;
use crate::output::ListResult;

/// Skeleton `list`: the default statuses, no scoping or ticket shape yet.
pub fn list(conn: &Connection) -> Result<ListResult> {
    let mut stmt = conn.prepare(
        "SELECT id FROM tickets WHERE status IN ('draft', 'ready', 'active') ORDER BY position, number",
    )?;
    let tickets = stmt
        .query_map([], |r| r.get::<_, i64>(0))?
        .map(|id| id.map(|id| serde_json::json!({ "id": id })))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ListResult { tickets })
}
