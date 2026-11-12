//! Who is calling: role, author and session, resolved once per run.

use std::collections::HashMap;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Role {
    Agent,
    Human,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Agent => "agent",
            Role::Human => "human",
        }
    }
}

/// The process facts identity and output decisions depend on, injected so
/// tests need no real environment or terminal.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub vars: HashMap<String, String>,
    pub stdin_tty: bool,
    pub stdout_tty: bool,
}

impl Env {
    pub fn from_process() -> Self {
        use std::io::IsTerminal;
        Env {
            // vars() panics on non-UTF-8 entries; skip them instead.
            vars: std::env::vars_os()
                .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
                .collect(),
            stdin_tty: std::io::stdin().is_terminal(),
            stdout_tty: std::io::stdout().is_terminal(),
        }
    }

    /// A variable's value; unset and empty both read as `None`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars
            .get(name)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub role: Role,
    pub author: String,
    pub session: Option<String>,
}

/// Role: `--role` (clap folds `SCREE_ROLE` into it), then an agent marker
/// (`CLAUDECODE` or `AI_AGENT`), then human when stdin and stdout are both
/// terminals. Anything else is an agent.
pub fn resolve_role(flag: Option<Role>, env: &Env) -> Role {
    if let Some(role) = flag {
        return role;
    }
    if env.get("CLAUDECODE").is_some() || env.get("AI_AGENT").is_some() {
        return Role::Agent;
    }
    if env.stdin_tty && env.stdout_tty {
        Role::Human
    } else {
        Role::Agent
    }
}

/// Author defaults to the role name. Session falls back to
/// `CLAUDE_CODE_SESSION_ID`. Empty strings count as unset.
pub fn resolve(
    role: Option<Role>,
    author: Option<String>,
    session: Option<String>,
    env: &Env,
) -> Identity {
    let role = resolve_role(role, env);
    let non_empty = |s: Option<String>| s.filter(|s| !s.is_empty());
    Identity {
        role,
        author: non_empty(author).unwrap_or_else(|| role.as_str().to_string()),
        session: non_empty(session).or_else(|| env.get("CLAUDE_CODE_SESSION_ID").map(String::from)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)], stdin_tty: bool, stdout_tty: bool) -> Env {
        Env {
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            stdin_tty,
            stdout_tty,
        }
    }

    #[test]
    fn flag_beats_everything() {
        let e = env(&[("CLAUDECODE", "1")], true, true);
        assert_eq!(resolve_role(Some(Role::Human), &e), Role::Human);
        let e = env(&[], true, true);
        assert_eq!(resolve_role(Some(Role::Agent), &e), Role::Agent);
    }

    #[test]
    fn marker_beats_tty() {
        assert_eq!(
            resolve_role(None, &env(&[("CLAUDECODE", "1")], true, true)),
            Role::Agent
        );
        assert_eq!(
            resolve_role(None, &env(&[("AI_AGENT", "x")], true, true)),
            Role::Agent
        );
    }

    #[test]
    fn empty_marker_is_unset() {
        assert_eq!(
            resolve_role(None, &env(&[("CLAUDECODE", "")], true, true)),
            Role::Human
        );
    }

    #[test]
    fn human_needs_both_ttys() {
        assert_eq!(resolve_role(None, &env(&[], true, true)), Role::Human);
        assert_eq!(resolve_role(None, &env(&[], false, true)), Role::Agent);
        assert_eq!(resolve_role(None, &env(&[], true, false)), Role::Agent);
    }

    #[test]
    fn unknown_is_agent() {
        assert_eq!(resolve_role(None, &env(&[], false, false)), Role::Agent);
    }

    #[test]
    fn author_defaults_to_role() {
        let e = env(&[], true, true);
        assert_eq!(resolve(None, None, None, &e).author, "human");
        assert_eq!(resolve(None, Some(String::new()), None, &e).author, "human");
        assert_eq!(resolve(None, Some("bob".into()), None, &e).author, "bob");
        let e = env(&[], false, false);
        assert_eq!(resolve(None, None, None, &e).author, "agent");
    }

    #[test]
    fn session_flag_then_claude_env() {
        let e = env(&[("CLAUDE_CODE_SESSION_ID", "abc")], false, false);
        assert_eq!(
            resolve(None, None, None, &e).session.as_deref(),
            Some("abc")
        );
        assert_eq!(
            resolve(None, None, Some("mine".into()), &e)
                .session
                .as_deref(),
            Some("mine")
        );
        assert_eq!(
            resolve(None, None, Some(String::new()), &e)
                .session
                .as_deref(),
            Some("abc")
        );
        assert_eq!(
            resolve(None, None, None, &env(&[], false, false)).session,
            None
        );
    }
}
