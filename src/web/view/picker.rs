//! The scope picker: two selects in a GET form. Choosing a Project submits and
//! the server renders the Worktree select for it, so no script repopulates
//! anything (`shell.js` only submits on change; the Apply button covers the
//! no-script case). The values are the ones `web::scope::parse` reads.

use maud::{Markup, html};

use crate::model::ProjectOverview;
use crate::ticket::ScopeFilter;
use crate::web::scope::{ALL, ROOT, Scope};

/// Whether ids should be shown qualified (`KEY-n`): true when the view can
/// hold tickets of more than one Project.
pub fn qualify_ids(scope: &Scope) -> bool {
    match &scope.filter {
        ScopeFilter::All => {
            scope.overview.projects.len() + usize::from(scope.overview.unscoped_open > 0) > 1
        }
        ScopeFilter::Project { .. } | ScopeFilter::Worktree { .. } => false,
    }
}

/// The picker as a form that GETs `action`.
pub fn picker(scope: &Scope, action: &str) -> Markup {
    let o = &scope.overview;
    let (p_val, w_val): (Option<&str>, Option<&str>) = match &scope.filter {
        ScopeFilter::All => (Some(ALL), None),
        ScopeFilter::Project { key } if key == "LOCAL" => (Some(""), None),
        ScopeFilter::Project { key } => (Some(key), Some(ALL)),
        ScopeFilter::Worktree { key, path } => (Some(key), Some(path.as_deref().unwrap_or(ROOT))),
    };
    let project: Option<&ProjectOverview> = match &scope.filter {
        ScopeFilter::Project { key } | ScopeFilter::Worktree { key, .. } => {
            o.projects.iter().find(|p| &p.key == key)
        }
        ScopeFilter::All => None,
    };
    let total: u32 = o.projects.iter().map(|p| p.open).sum::<u32>() + o.unscoped_open;
    html! {
        form class="picker" method="get" action=(action) data-picker {
            label {
                span class="picker-label" { "Project" }
                select name="p" {
                    option value=(ALL) selected[p_val == Some(ALL)] {
                        "All Projects (" (total) " open)"
                    }
                    @if o.unscoped_open > 0 || p_val == Some("") {
                        option value="" selected[p_val == Some("")] {
                            "No Project (" (o.unscoped_open) " open)"
                        }
                    }
                    @for p in &o.projects {
                        option value=(p.key) selected[p_val == Some(p.key.as_str())] {
                            (p.key) " (" (p.open) " open)"
                        }
                    }
                }
            }
            label {
                span class="picker-label" { "Worktree" }
                select name="w" data-picker-w disabled[project.is_none()] {
                    @if let Some(p) = project {
                        option value=(ALL) selected[w_val == Some(ALL)] {
                            "All Worktrees (" (p.open) ")"
                        }
                        option value=(ROOT) selected[w_val == Some(ROOT)] title=(p.root) {
                            "Root checkout (" (p.root_open) ")"
                        }
                        @for w in &p.worktrees {
                            option value=(w.path) selected[w_val == Some(w.path.as_str())]
                                title=(w.path) { (w.path) " (" (w.open) ")" }
                        }
                    } @else {
                        option value=(ALL) { "All Worktrees" }
                    }
                }
            }
            button class="btn" type="submit" { "Apply" }
            div id="err-scope" {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ScopeOverview, WorktreeCount};

    fn scope(filter: ScopeFilter, unscoped: u32, projects: Vec<ProjectOverview>) -> Scope {
        Scope {
            filter,
            overview: ScopeOverview {
                projects,
                unscoped_open: unscoped,
            },
        }
    }

    fn proj(key: &str) -> ProjectOverview {
        ProjectOverview {
            key: key.into(),
            open: 3,
            root: format!("/r/{key}"),
            root_open: 1,
            worktrees: vec![WorktreeCount {
                open: 2,
                path: "/w/feat".into(),
            }],
        }
    }

    #[test]
    fn all_selects_all_and_disables_worktrees() {
        let h = picker(&scope(ScopeFilter::All, 0, vec![proj("AA")]), "/").into_string();
        assert!(h.contains("<option value=\"@all\" selected>All Projects (3 open)"));
        assert!(h.contains("<select name=\"w\" data-picker-w disabled>"));
        assert!(!h.contains("No Project"), "no unscoped tickets: {h}");
    }

    #[test]
    fn no_project_group_and_counts() {
        let h = picker(&scope(ScopeFilter::All, 4, vec![proj("AA")]), "/").into_string();
        assert!(h.contains("<option value=\"\">No Project (4 open)"));
        assert!(h.contains(">AA (3 open)"));
        let local = picker(
            &scope(
                ScopeFilter::Project {
                    key: "LOCAL".into(),
                },
                4,
                vec![],
            ),
            "/",
        )
        .into_string();
        assert!(local.contains("<option value=\"\" selected>No Project"));
    }

    #[test]
    fn worktree_select_follows_the_project() {
        let f = ScopeFilter::Worktree {
            key: "AA".into(),
            path: Some("/w/feat".into()),
        };
        let h = picker(&scope(f, 0, vec![proj("AA")]), "/").into_string();
        assert!(h.contains("<option value=\"AA\" selected>"));
        assert!(h.contains("<option value=\"/w/feat\" selected"));
        assert!(h.contains(">Root checkout (1)"));
        assert!(!h.contains("<select name=\"w\" data-picker-w disabled>"));
    }

    #[test]
    fn qualify_only_when_spanning_projects() {
        let two = vec![proj("AA"), proj("BB")];
        assert!(qualify_ids(&scope(ScopeFilter::All, 0, two.clone())));
        assert!(qualify_ids(&scope(ScopeFilter::All, 1, vec![proj("AA")])));
        assert!(!qualify_ids(&scope(ScopeFilter::All, 0, vec![proj("AA")])));
        assert!(!qualify_ids(&scope(
            ScopeFilter::Project { key: "AA".into() },
            0,
            two
        )));
    }
}
