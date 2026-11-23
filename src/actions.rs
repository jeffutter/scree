//! UI-neutral mutations shared by the TUI and the web server.
//!
//! Each function is a thin wrapper over one `ticket::*` call: plain typed
//! inputs in, the updated `TicketDetail` or the library's typed `Error` out.
//! Nothing here formats a message; each front end words its own status line
//! and maps `Error` variants to its own response (the web: Usage 400,
//! NotFound 404, Refused 422, Conflict 409).
//!
//! Unlike the CLI, none of these sweeps closed Worktrees first; the caller
//! decides when to sweep.

use rusqlite::Connection;

use crate::error::Result;
use crate::identity::Identity;
use crate::model::TicketDetail;
use crate::ticket::{self, Action, CommitLink, EditFields, Place};

fn act(conn: &mut Connection, id: &str, action: Action, who: &Identity) -> Result<TicketDetail> {
    ticket::transition(conn, id, action, who)
}

pub fn ready(conn: &mut Connection, id: &str, who: &Identity) -> Result<TicketDetail> {
    act(conn, id, Action::Ready, who)
}

pub fn draft(conn: &mut Connection, id: &str, who: &Identity) -> Result<TicketDetail> {
    act(conn, id, Action::Draft, who)
}

pub fn start(conn: &mut Connection, id: &str, who: &Identity) -> Result<TicketDetail> {
    act(conn, id, Action::Start, who)
}

/// `handoff` replaces the stored note when given; `None` keeps it.
pub fn release(
    conn: &mut Connection,
    id: &str,
    handoff: Option<String>,
    who: &Identity,
) -> Result<TicketDetail> {
    act(conn, id, Action::Release { handoff }, who)
}

/// `commits` are linked as part of finishing the ticket.
pub fn done(
    conn: &mut Connection,
    id: &str,
    force: bool,
    commits: Vec<CommitLink>,
    who: &Identity,
) -> Result<TicketDetail> {
    act(conn, id, Action::Done { force, commits }, who)
}

/// An empty or blank `reason` is `Error::Usage`.
pub fn abandon(
    conn: &mut Connection,
    id: &str,
    reason: String,
    who: &Identity,
) -> Result<TicketDetail> {
    act(conn, id, Action::Abandon { reason }, who)
}

pub fn reopen(conn: &mut Connection, id: &str, who: &Identity) -> Result<TicketDetail> {
    act(conn, id, Action::Reopen, who)
}

pub fn archive(conn: &mut Connection, id: &str, archived: bool) -> Result<TicketDetail> {
    ticket::archive(conn, id, archived)
}

/// Link commits to a ticket in any state.
pub fn link_commits(conn: &mut Connection, id: &str, links: &[CommitLink]) -> Result<TicketDetail> {
    ticket::link(conn, id, links)
}

fn criterion_ref(id: &str, number: i64) -> String {
    format!("{id}#{number}")
}

pub fn criterion_check(
    conn: &mut Connection,
    id: &str,
    number: i64,
    evidence: Option<&str>,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::check(conn, &criterion_ref(id, number), evidence, who)
}

pub fn criterion_uncheck(
    conn: &mut Connection,
    id: &str,
    number: i64,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::uncheck(conn, &criterion_ref(id, number), who)
}

pub fn criterion_add(
    conn: &mut Connection,
    id: &str,
    text: &str,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::criteria_add(conn, id, text, who)
}

pub fn criterion_remove(
    conn: &mut Connection,
    id: &str,
    number: i64,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::criteria_rm(conn, &criterion_ref(id, number), who)
}

pub fn comment_add(
    conn: &mut Connection,
    id: &str,
    body: &str,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::comment_add(conn, id, body, who)
}

pub fn comment_edit(
    conn: &mut Connection,
    id: &str,
    comment_id: i64,
    body: &str,
    expected_updated_at: Option<&str>,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::comment_edit(conn, id, comment_id, body, who, expected_updated_at)
}

pub fn comment_remove(
    conn: &mut Connection,
    id: &str,
    comment_id: i64,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::comment_rm(conn, id, comment_id, who)
}

pub fn dep_add(
    conn: &mut Connection,
    id: &str,
    blocker: &str,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::dep_add(conn, id, blocker, who)
}

pub fn dep_remove(
    conn: &mut Connection,
    id: &str,
    blocker: &str,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::dep_rm(conn, id, blocker, who)
}

/// Edit title, description, plan, `for` or parent. A stale
/// `expected_updated_at` is `Error::Conflict` and changes nothing.
pub fn edit(
    conn: &mut Connection,
    id: &str,
    fields: &EditFields,
    who: &Identity,
) -> Result<TicketDetail> {
    ticket::edit(conn, id, fields, who)
}

/// Set only the title (an empty title is refused by the library).
pub fn edit_title(
    conn: &mut Connection,
    id: &str,
    title: &str,
    who: &Identity,
) -> Result<TicketDetail> {
    let fields = EditFields {
        title: Some(title),
        description: None,
        plan: None,
        append: false,
        for_who: None,
        parent: None,
        expected_updated_at: None,
    };
    ticket::edit(conn, id, &fields, who)
}

/// Move `id` before or after `anchor` among its siblings.
pub fn reorder(
    conn: &mut Connection,
    id: &str,
    anchor: &str,
    before: bool,
) -> Result<TicketDetail> {
    let place = if before {
        Place::Before(anchor)
    } else {
        Place::After(anchor)
    };
    ticket::reorder(conn, id, place)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ForWho, NewStatus};
    use crate::error::Error;
    use crate::identity::Role;
    use crate::scope::Scope;
    use crate::ticket::{NewTicket, ParentChange};

    fn who() -> Identity {
        Identity {
            role: Role::Human,
            author: "pat".into(),
            session: None,
        }
    }

    fn agent() -> Identity {
        Identity {
            role: Role::Agent,
            author: "a".into(),
            session: Some("s".into()),
        }
    }

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn add(conn: &mut Connection, status: NewStatus, criteria: &[String]) -> String {
        ticket::add(
            conn,
            &Scope::Unscoped,
            &NewTicket {
                title: "t",
                description: "",
                plan: "",
                criteria,
                for_who: ForWho::Agent,
                status,
                key: None,
                parent: None,
                blocked_by: &[],
            },
        )
        .unwrap()
        .ticket
        .id
    }

    fn ready_ticket(conn: &mut Connection) -> String {
        add(conn, NewStatus::Ready, &[])
    }

    fn status(conn: &Connection, id: &str) -> String {
        ticket::show(conn, id).unwrap().ticket.status
    }

    #[test]
    fn ready_and_draft() {
        let (_d, mut c) = db();
        let id = add(&mut c, NewStatus::Draft, &[]);
        assert_eq!(ready(&mut c, &id, &who()).unwrap().ticket.status, "ready");
        assert_eq!(draft(&mut c, &id, &who()).unwrap().ticket.status, "draft");
        assert!(matches!(
            ready(&mut c, "NOPE-99", &who()),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn start_and_release_with_handoff() {
        let (_d, mut c) = db();
        let id = ready_ticket(&mut c);
        assert_eq!(
            start(&mut c, &id, &agent()).unwrap().ticket.status,
            "active"
        );
        // A second caller loses the claim.
        let other = Identity {
            role: Role::Agent,
            author: "o".into(),
            session: Some("o".into()),
        };
        assert!(matches!(
            start(&mut c, &id, &other),
            Err(Error::Conflict(_))
        ));
        let d = release(&mut c, &id, Some("pick up at step 2".into()), &agent()).unwrap();
        assert_eq!(d.ticket.status, "ready");
        assert_eq!(d.handoff, "pick up at step 2");
    }

    #[test]
    fn done_takes_commits_and_refuses_open_criteria() {
        let (_d, mut c) = db();
        let id = ready_ticket(&mut c);
        let links = vec![CommitLink {
            sha: "a".repeat(40),
            branch: Some("main".into()),
        }];
        let d = done(&mut c, &id, false, links, &agent()).unwrap();
        assert_eq!(d.ticket.status, "done");
        assert_eq!(d.commits.len(), 1);
        let open = add(&mut c, NewStatus::Ready, &["x".to_string()]);
        assert!(matches!(
            done(&mut c, &open, false, vec![], &agent()),
            Err(Error::Refused(_))
        ));
        assert_eq!(status(&c, &open), "ready");
        assert_eq!(
            done(&mut c, &open, true, vec![], &agent())
                .unwrap()
                .ticket
                .status,
            "done"
        );
    }

    #[test]
    fn abandon_reason_is_an_input() {
        let (_d, mut c) = db();
        let id = ready_ticket(&mut c);
        assert!(matches!(
            abandon(&mut c, &id, "  ".into(), &who()),
            Err(Error::Usage(_))
        ));
        assert_eq!(status(&c, &id), "ready");
        let d = abandon(&mut c, &id, "superseded".into(), &who()).unwrap();
        assert_eq!(d.ticket.status, "abandoned");
        assert!(d.comments.iter().any(|m| m.body.contains("superseded")));
    }

    #[test]
    fn reopen_closed_and_refuse_open() {
        let (_d, mut c) = db();
        let id = ready_ticket(&mut c);
        assert!(reopen(&mut c, &id, &who()).is_err());
        done(&mut c, &id, false, vec![], &agent()).unwrap();
        assert_eq!(reopen(&mut c, &id, &who()).unwrap().ticket.status, "ready");
        assert!(matches!(
            reopen(&mut c, "NOPE-99", &who()),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn archive_and_unarchive() {
        let (_d, mut c) = db();
        let parent = ready_ticket(&mut c);
        let child = ticket::add(
            &mut c,
            &Scope::Unscoped,
            &NewTicket {
                title: "kid",
                description: "",
                plan: "",
                criteria: &[],
                for_who: ForWho::Agent,
                status: NewStatus::Ready,
                key: None,
                parent: Some(&parent),
                blocked_by: &[],
            },
        )
        .unwrap()
        .ticket
        .id;
        assert!(
            archive(&mut c, &parent, true)
                .unwrap()
                .ticket
                .archived_at
                .is_some()
        );
        // A subtask cannot be unarchived under an archived parent.
        assert!(matches!(
            archive(&mut c, &child, false),
            Err(Error::Refused(_))
        ));
        assert!(
            archive(&mut c, &parent, false)
                .unwrap()
                .ticket
                .archived_at
                .is_none()
        );
        assert!(matches!(
            archive(&mut c, "NOPE-99", true),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn link_commits_links_and_refuses_missing() {
        let (_d, mut c) = db();
        let id = ready_ticket(&mut c);
        let l = [CommitLink {
            sha: "b".repeat(40),
            branch: None,
        }];
        assert_eq!(link_commits(&mut c, &id, &l).unwrap().commits.len(), 1);
        assert!(matches!(
            link_commits(&mut c, "NOPE-99", &l),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn criteria_round_trip_and_refusals() {
        let (_d, mut c) = db();
        let id = add(&mut c, NewStatus::Ready, &["one".to_string()]);
        let d = criterion_check(&mut c, &id, 1, Some("ran"), &who()).unwrap();
        assert!(d.criteria[0].checked);
        assert!(!criterion_uncheck(&mut c, &id, 1, &who()).unwrap().criteria[0].checked);
        let d = criterion_add(&mut c, &id, "two", &who()).unwrap();
        assert_eq!(d.criteria.len(), 2);
        assert_eq!(
            criterion_remove(&mut c, &id, 2, &who())
                .unwrap()
                .criteria
                .len(),
            1
        );
        assert!(matches!(
            criterion_check(&mut c, &id, 9, None, &who()),
            Err(Error::NotFound(_) | Error::Usage(_))
        ));
        assert!(criterion_uncheck(&mut c, "NOPE-99", 1, &who()).is_err());
        assert!(matches!(
            criterion_add(&mut c, &id, " ", &who()),
            Err(Error::Usage(_))
        ));
        assert!(criterion_remove(&mut c, &id, 9, &who()).is_err());
    }

    #[test]
    fn comments_add_edit_remove() {
        let (_d, mut c) = db();
        let id = ready_ticket(&mut c);
        let d = comment_add(&mut c, &id, "hello", &who()).unwrap();
        let cid = d.comments[0].id;
        let d = comment_edit(&mut c, &id, cid, "hi", None, &who()).unwrap();
        assert_eq!(d.comments[0].body, "hi");
        // A stale expected_updated_at is a Conflict.
        assert!(matches!(
            comment_edit(&mut c, &id, cid, "x", Some("1999-01-01T00:00:00Z"), &who()),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            comment_edit(&mut c, &id, 9999, "x", None, &who()),
            Err(Error::NotFound(_))
        ));
        assert!(
            comment_remove(&mut c, &id, cid, &who())
                .unwrap()
                .comments
                .is_empty()
        );
        assert!(matches!(
            comment_remove(&mut c, &id, cid, &who()),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            comment_add(&mut c, &id, " ", &who()),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn deps_add_remove_and_cycle_refused() {
        let (_d, mut c) = db();
        let a = ready_ticket(&mut c);
        let b = ready_ticket(&mut c);
        assert_eq!(
            dep_add(&mut c, &a, &b, &who())
                .unwrap()
                .ticket
                .blocked_by
                .len(),
            1
        );
        assert!(matches!(
            dep_add(&mut c, &b, &a, &who()),
            Err(Error::Refused(_))
        ));
        assert!(
            dep_remove(&mut c, &a, &b, &who())
                .unwrap()
                .ticket
                .blocked_by
                .is_empty()
        );
        assert!(matches!(
            dep_remove(&mut c, &a, "NOPE-99", &who()),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn edit_reaches_for_and_parent_and_guards_stale() {
        let (_d, mut c) = db();
        let parent = ready_ticket(&mut c);
        let id = ready_ticket(&mut c);
        let f = EditFields {
            title: None,
            description: Some("d"),
            plan: None,
            append: false,
            for_who: Some(ForWho::Human),
            parent: Some(ParentChange::Under(&parent)),
            expected_updated_at: None,
        };
        let d = edit(&mut c, &id, &f, &who()).unwrap();
        assert_eq!(d.description, "d");
        assert_eq!(d.ticket.for_who, "human");
        assert!(d.ticket.parent.is_some());
        let stale = EditFields {
            title: Some("z"),
            description: None,
            plan: None,
            append: false,
            for_who: None,
            parent: None,
            expected_updated_at: Some("1999-01-01T00:00:00Z"),
        };
        let new_id = d.ticket.id.clone();
        assert!(matches!(
            edit(&mut c, &new_id, &stale, &who()),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            edit_title(&mut c, &new_id, "  ", &who()),
            Err(Error::Usage(_))
        ));
        assert_eq!(
            edit_title(&mut c, &new_id, " new ", &who())
                .unwrap()
                .ticket
                .title,
            "new"
        );
        assert!(matches!(
            edit_title(&mut c, "NOPE-99", "x", &who()),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn reorder_moves_and_refuses_closed() {
        let (_d, mut c) = db();
        let a = ready_ticket(&mut c);
        let b = ready_ticket(&mut c);
        reorder(&mut c, &b, &a, true).unwrap();
        c.execute("UPDATE tickets SET status = 'done'", []).unwrap();
        assert!(matches!(
            reorder(&mut c, &b, &a, false),
            Err(Error::Refused(_))
        ));
    }
}
