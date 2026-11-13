//! Raw mode and the alternate screen, always undone: by Drop on a normal or
//! error exit, and by a panic hook before the panic message prints.

use std::io::{self, Write};

use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Write the escape sequences that leave the alternate screen and show the cursor.
pub fn write_restore(w: &mut impl Write) -> io::Result<()> {
    execute!(w, LeaveAlternateScreen, Show)
}

fn restore() {
    // Best effort: the terminal may already be gone.
    let _ = disable_raw_mode();
    let _ = write_restore(&mut io::stdout());
}

/// Holds the terminal in TUI mode; dropping it restores it.
pub struct Guard(());

impl Guard {
    pub fn enter() -> io::Result<Guard> {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            prev(info);
        }));
        enable_raw_mode()?;
        let guard = Guard(());
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(guard)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn restore_leaves_alt_screen_and_shows_cursor() {
        let mut out = Vec::new();
        super::write_restore(&mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\x1b[?1049l"), "{s:?}");
        assert!(s.contains("\x1b[?25h"), "{s:?}");
    }
}
