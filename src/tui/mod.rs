//! `scree tui`: an interactive terminal UI over the ticket library.
//!
//! Elm-style: `state::update` is pure and returns Effects; `effects` runs them
//! against the database and feeds Events back; `view` draws; `terminal` owns
//! raw mode, the alternate screen and mouse capture.

mod effects;
mod markdown;
mod state;
mod terminal;
mod text;
mod theme;
mod view;
mod widgets;

use std::path::Path;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event;
use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::identity::{Env, Identity};
use state::{Effect, Event, State, update};

/// Refuse before anything touches the terminal or the database.
pub fn check_tty(env: &Env) -> Result<()> {
    if env.stdout_tty && env.stdin_tty {
        Ok(())
    } else {
        Err(Error::Usage(
            "scree tui needs a terminal: stdin and stdout must both be a TTY".into(),
        ))
    }
}

/// A terminal or watcher failure is not a database failure: report it as a
/// plain failure (exit 1), not through `Error::Io`, which means storage.
fn terminal_err(e: std::io::Error) -> Error {
    Error::Other(format!("terminal error: {e}"))
}

pub fn run(
    conn: &mut Connection,
    db_path: &Path,
    cwd: &Path,
    scope: crate::scope::Scope,
    project_root: Option<&Path>,
    identity: &Identity,
    env: &Env,
) -> Result<()> {
    let changes = spawn_watcher(db_path)?;
    let mut state = State::starting_in(
        scope,
        format!("{}/{}", identity.role.as_str(), identity.author),
        theme::Theme::from_env(env),
    );
    let _guard = terminal::Guard::enter().map_err(terminal_err)?;
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout())).map_err(terminal_err)?;
    term.clear().map_err(terminal_err)?;

    update(
        &mut state,
        Event::Resize(term.size().map_err(terminal_err)?.height),
    );
    let mut pending = vec![
        Effect::Reload(state.load_spec()),
        Effect::LoadDecisions(state.mode),
    ];
    while !state.should_quit {
        while let Some(effect) = pending.pop() {
            let ev = match effect {
                Effect::EditExternal { id, field } => {
                    let r = effects::external_edit(conn, &id, field, identity, |path| {
                        let _suspended = terminal::Suspended::new()
                            .map_err(|e| format!("cannot leave the TUI: {e}"))?;
                        effects::run_editor(&effects::editor_command(), path)
                    });
                    // Whatever the editor drew is gone from our screen state.
                    term.clear().map_err(terminal_err)?;
                    Event::ActionDone(r)
                }
                other => effects::run_effect(conn, cwd, project_root, identity, other),
            };
            pending.extend(update(&mut state, ev));
        }
        let mut changed = false;
        while changes.try_recv().is_ok() {
            changed = true;
        }
        if changed {
            pending.extend(update(&mut state, Event::DbChanged));
            continue;
        }
        // Only the detail view scrolls by wheel; elsewhere the terminal keeps
        // the mouse, so text selects as usual.
        terminal::set_mouse(state.detail.is_some()).map_err(terminal_err)?;
        term.draw(|f| view::draw(f, &mut state))
            .map_err(terminal_err)?;
        if event::poll(Duration::from_millis(100)).map_err(terminal_err)? {
            let ev = match event::read().map_err(terminal_err)? {
                event::Event::Key(k) => Event::Key(k),
                event::Event::Mouse(m) => Event::Mouse(m),
                event::Event::Resize(_, h) => Event::Resize(h),
                _ => continue,
            };
            pending.extend(update(&mut state, ev));
        }
    }
    Ok(())
}

/// Signal the event loop once per database change; the watcher thread ends
/// when the receiver is dropped.
fn spawn_watcher(path: &Path) -> Result<std::sync::mpsc::Receiver<()>> {
    let (tx, rx) = std::sync::mpsc::channel();
    crate::watch::spawn_with(
        path.to_path_buf(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        move || tx.send(()).is_ok(),
    )?;
    Ok(rx)
}
