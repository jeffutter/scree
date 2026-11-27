//! Raw mode, the alternate screen and mouse capture, always undone: by Drop on
//! a normal or error exit, and by a panic hook before the panic message prints.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Whether the TUI currently wants the mouse, so `Suspended` can take it back.
static MOUSE: AtomicBool = AtomicBool::new(false);

/// Report button presses, which include the wheel (1000), in SGR form (1006).
/// Not crossterm's `EnableMouseCapture`: that also reports drags and every
/// pointer move, which nothing here uses and each of which would redraw.
fn write_mouse(w: &mut impl Write, on: bool) -> io::Result<()> {
    w.write_all(if on {
        b"\x1b[?1000h\x1b[?1006h"
    } else {
        b"\x1b[?1006l\x1b[?1000l"
    })?;
    w.flush()
}

/// Capture or release the mouse. Writes only on a change, so it is cheap to
/// call on every turn of the event loop.
pub fn set_mouse(on: bool) -> io::Result<()> {
    if MOUSE.swap(on, Ordering::Relaxed) == on {
        return Ok(());
    }
    write_mouse(&mut io::stdout(), on)
}

/// Write the escape sequences that release the mouse, leave the alternate
/// screen and show the cursor.
pub fn write_restore(w: &mut impl Write) -> io::Result<()> {
    write_mouse(w, false)?;
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

/// The TUI stepped aside so a child process can use the terminal. Dropping
/// it re-enters raw mode and the alternate screen, so every path, including
/// an early return after a failed editor, restores the TUI.
pub struct Suspended(());

impl Suspended {
    pub fn new() -> io::Result<Suspended> {
        disable_raw_mode()?;
        // Constructed first so Drop still resumes if leaving the screen fails.
        let s = Suspended(());
        write_restore(&mut io::stdout())?;
        Ok(s)
    }
}

impl Drop for Suspended {
    fn drop(&mut self) {
        let _ = enable_raw_mode();
        let _ = execute!(io::stdout(), EnterAlternateScreen);
        if MOUSE.load(Ordering::Relaxed) {
            let _ = write_mouse(&mut io::stdout(), true);
        }
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
        assert!(s.contains("\x1b[?1000l"), "{s:?}");
        assert!(s.contains("\x1b[?1006l"), "{s:?}");
    }

    #[test]
    fn mouse_asks_for_buttons_in_sgr_form_only() {
        let written = |on| {
            let mut out = Vec::new();
            super::write_mouse(&mut out, on).unwrap();
            String::from_utf8(out).unwrap()
        };
        assert_eq!(written(true), "\x1b[?1000h\x1b[?1006h");
        assert_eq!(written(false), "\x1b[?1006l\x1b[?1000l");
    }
}
