//! Where the caller is: not in a repo, in a repo's root checkout, or in a
//! linked worktree. Git does the detection; nothing here parses `.git`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Outside any repository (also bare repos and submodules, which have
    /// no root checkout of their own to scope tickets to).
    Unscoped,
    /// The root checkout of a repository.
    Checkout { root: PathBuf },
    /// A linked worktree; `root` is the repository's root checkout.
    Worktree { root: PathBuf, worktree: PathBuf },
}

/// Decide the scope from git's canonical common dir and worktree top level.
/// The root checkout is the parent of a common dir named `.git`; anything
/// else (bare repo, submodule's `.git/modules/x`) is unscoped.
fn classify(common_dir: &Path, top: &Path) -> Scope {
    if common_dir.file_name().is_none_or(|n| n != ".git") {
        return Scope::Unscoped;
    }
    let Some(root) = common_dir.parent() else {
        return Scope::Unscoped;
    };
    if root == top {
        Scope::Checkout {
            root: root.to_path_buf(),
        }
    } else {
        Scope::Worktree {
            root: root.to_path_buf(),
            worktree: top.to_path_buf(),
        }
    }
}

/// Detect the scope of `dir`. Git missing, failing, or `dir` absent all read
/// as `Unscoped`.
pub fn detect(dir: &Path) -> Scope {
    let Ok(out) = Command::new("git")
        .current_dir(dir)
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
            "--show-toplevel",
        ])
        .output()
    else {
        return Scope::Unscoped;
    };
    if !out.status.success() {
        return Scope::Unscoped;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let (Some(common), Some(top)) = (lines.next(), lines.next()) else {
        return Scope::Unscoped;
    };
    match (std::fs::canonicalize(common), std::fs::canonicalize(top)) {
        (Ok(common), Ok(top)) => classify(&common, &top),
        _ => Scope::Unscoped,
    }
}

/// The scope a write targets: the caller's own repo when inside one, else
/// the repo named by `--project`, else `Unscoped` (the `LOCAL` Project).
pub fn for_write(cwd: &Path, project: Option<&Path>) -> Result<Scope> {
    let here = detect(cwd);
    if here != Scope::Unscoped {
        return Ok(here);
    }
    let Some(p) = project else {
        return Ok(Scope::Unscoped);
    };
    match detect(&cwd.join(p)) {
        Scope::Unscoped => Err(Error::Refused(format!(
            "{} is not inside a git repository",
            p.display()
        ))),
        s => Ok(s),
    }
}

/// Paths are stored as text; refuse a path that is not valid UTF-8.
pub fn path_str(p: &Path) -> Result<String> {
    p.to_str()
        .map(String::from)
        .ok_or_else(|| Error::Refused(format!("path is not valid UTF-8: {}", p.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_cases() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(
            classify(&p("/r/app/.git"), &p("/r/app")),
            Scope::Checkout { root: p("/r/app") }
        );
        assert_eq!(
            classify(&p("/r/app/.git"), &p("/r/wt")),
            Scope::Worktree {
                root: p("/r/app"),
                worktree: p("/r/wt")
            }
        );
        assert_eq!(
            classify(&p("/r/app.git"), &p("/r/wt")),
            Scope::Unscoped,
            "bare repo"
        );
        assert_eq!(
            classify(&p("/r/sup/.git/modules/sub"), &p("/r/sup/sub")),
            Scope::Unscoped,
            "submodule"
        );
    }
}
