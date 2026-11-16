//! Text primitives shared by the TUI's wrappers (`view::wrap` for plain text,
//! `markdown` for styled text), so the terminal-safety scrub exists once.

use unicode_width::UnicodeWidthChar;

/// Terminal cells one character takes.
pub(super) fn cell(c: char) -> usize {
    c.width().unwrap_or(0)
}

/// Tabs become spaces; other control chars would reach the terminal raw (zero
/// cell width, possible escape codes), so drop them.
pub(super) fn clean(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\t' => vec![' '; 4],
            c if c.is_control() => vec![],
            c => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_counts_terminal_columns() {
        assert_eq!(cell('a'), 1);
        assert_eq!(cell('\u{4e2d}'), 2);
        assert_eq!(cell('\u{25cf}'), 1);
        assert_eq!(cell('\u{301}'), 0);
    }

    #[test]
    fn clean_expands_tabs_and_drops_control_chars() {
        assert_eq!(clean("a\tb"), "a    b");
        assert_eq!(clean("a\r\u{1b}[31mb"), "a[31mb");
    }
}
