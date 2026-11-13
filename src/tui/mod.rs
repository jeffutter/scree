//! `scree tui`: an interactive terminal UI over the ticket library.
//!
//! Elm-style: `state::update` is pure and returns Effects; `effects` runs them
//! against the database and feeds Events back; `view` draws; `terminal` owns
//! raw mode and the alternate screen.

mod effects;
mod state;
mod terminal;
mod theme;
mod view;
mod watch;
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

pub fn run(
    conn: &mut Connection,
    db_path: &Path,
    cwd: &Path,
    project_root: Option<&Path>,
    identity: &Identity,
    env: &Env,
) -> Result<()> {
    let changes = watch::spawn(db_path)?;
    let mut state = State::starting_in(
        crate::scope::detect(&project_root.map_or_else(|| cwd.to_path_buf(), |p| cwd.join(p))),
        format!("{}/{}", identity.role.as_str(), identity.author),
        theme::Theme::from_env(env),
    );
    let _guard = terminal::Guard::enter()?;
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    term.clear()?;

    update(&mut state, Event::Resize(term.size()?.height));
    let mut pending = vec![Effect::Reload(state.load_spec())];
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
                    term.clear()?;
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
        term.draw(|f| view::draw(f, &mut state))?;
        if event::poll(Duration::from_millis(100))? {
            let ev = match event::read()? {
                event::Event::Key(k) => Event::Key(k),
                event::Event::Resize(_, h) => Event::Resize(h),
                _ => continue,
            };
            pending.extend(update(&mut state, ev));
        }
    }
    Ok(())
}
