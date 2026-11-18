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

/// Decide the scope from the repository's root checkout and the worktree
/// top level the caller is in.
fn classify(root: &Path, top: &Path) -> Scope {
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

/// Run git in `dir` and return trimmed stdout, or `None` on any failure.
fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// The commit a checkout is on: full sha and branch (`None` when detached).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub sha: String,
    pub branch: Option<String>,
}

/// HEAD of the checkout containing `dir`, through the git CLI. `None` outside
/// a repo, in a repo with no commits, or when git fails.
pub fn head(dir: &Path) -> Option<Head> {
    let sha = git_out(dir, &["rev-parse", "--verify", "-q", "HEAD^{commit}"])?;
    let branch = git_out(dir, &["symbolic-ref", "--short", "-q", "HEAD"]).filter(|b| !b.is_empty());
    Some(Head { sha, branch })
}

/// Resolve `rev` to a full commit sha in the repo containing `dir`.
pub fn resolve_commit(dir: &Path, rev: &str) -> Option<String> {
    if rev.starts_with('-') {
        return None;
    }
    git_out(
        dir,
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
}

/// Detect the scope of `dir`. Git missing, failing, or `dir` absent all read
/// as `Unscoped`, as do bare repos and submodules. The root checkout is the
/// top level when the git dir is the common dir; for a linked worktree it
/// comes from `core.worktree` or the `.git` common dir's parent, so a
/// `--separate-git-dir` repo (git dir not named `.git`) still resolves to
/// its real work tree.
pub fn detect(dir: &Path) -> Scope {
    if git_out(dir, &["rev-parse", "--is-bare-repository"]).as_deref() != Some("false") {
        return Scope::Unscoped;
    }
    // Prints the superproject's path inside a submodule, nothing elsewhere.
    match git_out(dir, &["rev-parse", "--show-superproject-working-tree"]) {
        Some(s) if s.is_empty() => {}
        _ => return Scope::Unscoped,
    }
    let Some(paths) = git_out(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ],
    ) else {
        return Scope::Unscoped;
    };
    let mut lines = paths.lines();
    let (Some(top), Some(git_dir), Some(common)) = (lines.next(), lines.next(), lines.next())
    else {
        return Scope::Unscoped;
    };
    let (Ok(top), Ok(git_dir), Ok(common)) = (
        std::fs::canonicalize(top),
        std::fs::canonicalize(git_dir),
        std::fs::canonicalize(common),
    ) else {
        return Scope::Unscoped;
    };
    // The git dir is the common dir only in the root checkout.
    if git_dir == common {
        return classify(&top, &top);
    }
    // Linked worktree: find the root checkout from the common dir.
    let root = match git_out(&common, &["config", "--get", "core.worktree"]) {
        // Set by `--separate-git-dir`; relative to the common dir.
        Some(w) if !w.is_empty() => std::fs::canonicalize(common.join(w)).ok(),
        _ if common.file_name().is_some_and(|n| n == ".git") => {
            common.parent().map(Path::to_path_buf)
        }
        _ => None,
    };
    match root {
        Some(root) => classify(&root, &top),
        None => Scope::Unscoped,
    }
}

/// The scope a write targets: the caller's own repo when inside one, else
/// the repo named by `--project`, else `Unscoped` (the `LOCAL` Project).
/// The root checkout a scope belongs to, if any.
pub fn root_of(s: &Scope) -> Option<&Path> {
    match s {
        Scope::Checkout { root } | Scope::Worktree { root, .. } => Some(root),
        Scope::Unscoped => None,
    }
}

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
