//! Colors, icons and borders. Every decision about NO_COLOR and non-Unicode
//! terminals lives here so widgets never branch on the environment.

use ratatui::style::{Color, Modifier, Style};

use crate::identity::Env;
use crate::output;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub color: bool,
    pub unicode: bool,
}

impl Default for Theme {
    /// Plain and Unicode: what tests and `State::default()` use.
    fn default() -> Self {
        Theme {
            color: false,
            unicode: true,
        }
    }
}

impl Theme {
    #[cfg(test)]
    pub fn new(color: bool, unicode: bool) -> Self {
        Theme { color, unicode }
    }

    /// Color follows `output::use_color` (tty and NO_COLOR). Unicode is off
    /// when the first non-empty of LC_ALL, LC_CTYPE, LANG does not name UTF-8,
    /// or when TERM is `linux` or `dumb`.
    pub fn from_env(env: &Env) -> Self {
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|k| env.get(k))
            .map(str::to_ascii_lowercase);
        let utf8 = locale.is_some_and(|l| l.contains("utf-8") || l.contains("utf8"));
        let term_ok = !matches!(env.get("TERM"), Some("linux" | "dumb"));
        Theme {
            color: output::use_color(env),
            unicode: utf8 && term_ok,
        }
    }

    fn fg(&self, color: Color) -> Style {
        if self.color {
            Style::new().fg(color)
        } else {
            Style::new()
        }
    }

    #[allow(dead_code)] // used by the ticket views that follow
    /// Icon and style for a ticket status. Blocked wins over everything; a
    /// ready ticket that is available (not blocked) is shown distinctly.
    pub fn status_style(
        &self,
        status: &str,
        blocked: bool,
        available: bool,
    ) -> (&'static str, Style) {
        let u = self.unicode;
        if blocked {
            return (if u { "\u{2298}" } else { "!" }, self.fg(Color::Red));
        }
        match status {
            "draft" => (if u { "\u{25cc}" } else { "-" }, self.fg(Color::DarkGray)),
            "ready" if available => (if u { "\u{25c9}" } else { "~" }, self.fg(Color::Cyan)),
            "ready" => (if u { "\u{25cb}" } else { "o" }, Style::new()),
            "active" => (if u { "\u{25d0}" } else { "*" }, self.fg(Color::Yellow)),
            "done" => (if u { "\u{25cf}" } else { "+" }, self.fg(Color::Green)),
            "abandoned" => (if u { "\u{2715}" } else { "x" }, self.fg(Color::Red)),
            _ => ("?", Style::new()),
        }
    }

    /// Active pane border yellow, inactive gray; bold marks active without color.
    pub fn border_style(&self, active: bool) -> Style {
        if self.color {
            Style::new().fg(if active { Color::Yellow } else { Color::Gray })
        } else if active {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        }
    }

    #[allow(dead_code)] // used by the ticket views that follow
    /// Selected row: inverse video, meaningful with or without color.
    pub fn selected_style(&self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }

    pub fn key_style(&self) -> Style {
        self.fg(Color::Cyan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)], tty: bool) -> Env {
        Env {
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            stdin_tty: tty,
            stdout_tty: tty,
        }
    }

    const STATUSES: [(&str, bool, bool); 7] = [
        ("draft", false, false),
        ("ready", false, false),
        ("active", false, false),
        ("done", false, false),
        ("abandoned", false, false),
        ("ready", true, false),
        ("ready", false, true),
    ];

    #[test]
    fn icons_are_distinct_in_both_modes() {
        for unicode in [true, false] {
            let t = Theme::new(true, unicode);
            let mut icons: Vec<_> = STATUSES
                .iter()
                .map(|(s, b, a)| t.status_style(s, *b, *a).0)
                .collect();
            icons.sort();
            icons.dedup();
            assert_eq!(icons.len(), STATUSES.len(), "unicode={unicode}");
        }
    }

    #[test]
    fn colors_match_the_map() {
        let t = Theme::new(true, true);
        let fg = |s, b, a| t.status_style(s, b, a).1.fg;
        assert_eq!(fg("draft", false, false), Some(Color::DarkGray));
        assert_eq!(fg("ready", false, false), None);
        assert_eq!(fg("ready", false, true), Some(Color::Cyan));
        assert_eq!(fg("active", false, false), Some(Color::Yellow));
        assert_eq!(fg("done", false, false), Some(Color::Green));
        assert_eq!(fg("abandoned", false, false), Some(Color::Red));
        assert_eq!(fg("ready", true, false), Some(Color::Red));
    }

    #[test]
    fn no_color_yields_no_colors() {
        let t = Theme::new(false, true);
        for (s, b, a) in STATUSES {
            let st = t.status_style(s, b, a).1;
            assert_eq!((st.fg, st.bg), (None, None));
        }
        for active in [true, false] {
            let st = t.border_style(active);
            assert_eq!((st.fg, st.bg), (None, None));
        }
        assert_eq!(t.key_style().fg, None);
        assert!(t.selected_style().add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn from_env_honors_no_color() {
        assert!(Theme::from_env(&env(&[], true)).color);
        assert!(!Theme::from_env(&env(&[("NO_COLOR", "1")], true)).color);
        assert!(!Theme::from_env(&env(&[], false)).color);
    }

    #[test]
    fn from_env_detects_unicode() {
        let u = |vars| Theme::from_env(&env(vars, true)).unicode;
        assert!(u(&[("LANG", "en_US.UTF-8")]));
        assert!(u(&[("LC_ALL", "C.utf8"), ("LANG", "C")]));
        assert!(!u(&[("LANG", "C")]));
        assert!(!u(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")]));
        assert!(!u(&[]));
        assert!(!u(&[("LANG", "en_US.UTF-8"), ("TERM", "linux")]));
        assert!(!u(&[("LANG", "en_US.UTF-8"), ("TERM", "dumb")]));
    }
}
