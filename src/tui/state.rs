//! The TUI's model and its pure transition function. Nothing here touches the
//! terminal or the database, so key handling is tested with scripted events.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::model::Ticket;

#[derive(Debug, Default)]
pub struct State {
    pub should_quit: bool,
    /// "role/author" shown in the header.
    pub who: String,
    pub tickets: Vec<Ticket>,
    /// The last load error, shown in the footer.
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Resize,
    Loaded(Result<Vec<Ticket>, String>),
}

/// Work `update` asks the runner to do; the runner feeds the result back as an Event.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Reload,
}

pub fn update(state: &mut State, event: Event) -> Vec<Effect> {
    match event {
        Event::Key(k) if k.kind != KeyEventKind::Release => {
            let ctrl_c =
                k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c');
            if ctrl_c || k.code == KeyCode::Char('q') {
                state.should_quit = true;
            }
            vec![]
        }
        Event::Key(_) | Event::Resize => vec![],
        Event::Loaded(Ok(tickets)) => {
            state.tickets = tickets;
            state.error = None;
            vec![]
        }
        Event::Loaded(Err(e)) => {
            state.error = Some(e);
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventState;

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        })
    }
    fn press(c: char) -> Event {
        key(KeyCode::Char(c), KeyModifiers::NONE, KeyEventKind::Press)
    }

    /// Feed scripted events; return the state and every effect asked for.
    fn drive(events: Vec<Event>) -> (State, Vec<Effect>) {
        let mut s = State::default();
        let mut effects = vec![];
        for e in events {
            effects.extend(update(&mut s, e));
        }
        (s, effects)
    }

    #[test]
    fn q_quits() {
        let (s, fx) = drive(vec![press('q')]);
        assert!(s.should_quit);
        assert!(fx.is_empty());
    }

    #[test]
    fn ctrl_c_quits() {
        let (s, _) = drive(vec![key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        )]);
        assert!(s.should_quit);
    }

    #[test]
    fn other_keys_do_nothing() {
        let (s, fx) = drive(vec![
            press('x'),
            press('c'),
            key(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press),
            Event::Resize,
        ]);
        assert!(!s.should_quit);
        assert!(fx.is_empty());
    }

    #[test]
    fn release_is_ignored() {
        let (s, _) = drive(vec![key(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )]);
        assert!(!s.should_quit);
    }

    #[test]
    fn scripted_sequence_quits_at_q() {
        let (s, _) = drive(vec![press('j'), press('k'), press('q')]);
        assert!(s.should_quit);
    }

    #[test]
    fn load_error_is_kept_then_cleared() {
        let mut s = State::default();
        update(&mut s, Event::Loaded(Err("boom".into())));
        assert_eq!(s.error.as_deref(), Some("boom"));
        update(&mut s, Event::Loaded(Ok(vec![])));
        assert_eq!(s.error, None);
    }
}
