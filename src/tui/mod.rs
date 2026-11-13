//! `scree tui`: an interactive terminal UI over the ticket library.
//!
//! Elm-style: `state::update` is pure and returns Effects; `effects` runs them
//! against the database and feeds Events back; `view` draws; `terminal` owns
//! raw mode and the alternate screen.

mod effects;
mod state;
mod terminal;
mod view;

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

pub fn run(conn: &Connection, cwd: &Path, identity: &Identity) -> Result<()> {
    let mut state = State {
        who: format!("{}/{}", identity.role.as_str(), identity.author),
        ..State::default()
    };
    let _guard = terminal::Guard::enter()?;
    let mut term = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    term.clear()?;

    let mut pending = vec![Effect::Reload];
    while !state.should_quit {
        while let Some(effect) = pending.pop() {
            let ev = effects::run_effect(conn, cwd, identity, effect);
            pending.extend(update(&mut state, ev));
        }
        term.draw(|f| view::draw(f, &state))?;
        if event::poll(Duration::from_millis(250))? {
            let ev = match event::read()? {
                event::Event::Key(k) => Event::Key(k),
                event::Event::Resize(..) => Event::Resize,
                _ => continue,
            };
            pending.extend(update(&mut state, ev));
        }
    }
    Ok(())
}
