//! Shared widgets: header, footer hint bar, bordered pane. Each takes plain
//! data and a Theme, so each is snapshot-tested through a TestBackend.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use super::theme::Theme;
use crate::scope::Scope;

const SEP: &str = " | ";

/// Pack `[Key] label` hints greedily into lines no wider than `width`,
/// separated by " | " (Backlog.md's footer logic). At most two lines; hints
/// that do not fit on the second are dropped.
pub fn hint_lines(hints: &[(&str, &str)], width: u16, theme: &Theme) -> Vec<Line<'static>> {
    let width = width as usize;
    let mut lines: Vec<Vec<Span<'static>>> = vec![vec![]];
    let mut used = 0usize;
    for (key, label) in hints {
        let key_text = format!("[{key}]");
        let item_w = key_text.chars().count() + 1 + label.chars().count();
        let sep_w = if used == 0 { 0 } else { SEP.len() };
        if used > 0 && used + sep_w + item_w > width {
            if lines.len() == 2 {
                break;
            }
            lines.push(vec![]);
            used = 0;
        }
        let cur = lines.last_mut().expect("at least one line");
        if used > 0 {
            cur.push(Span::raw(SEP));
            used += SEP.len();
        }
        cur.push(Span::styled(key_text, theme.key_style()));
        cur.push(Span::raw(format!(" {label}")));
        used += item_w;
    }
    lines.into_iter().map(Line::from).collect()
}

/// Rows the footer needs: 1 or 2.
pub fn footer_height(hints: &[(&str, &str)], width: u16, theme: &Theme) -> u16 {
    hint_lines(hints, width, theme).len().max(1) as u16
}

pub fn render_footer(buf: &mut Buffer, area: Rect, hints: &[(&str, &str)], theme: &Theme) {
    Paragraph::new(hint_lines(hints, area.width, theme)).render(area, buf);
}

/// "Worktree <name>", "Project <dir>" or "all Projects".
pub fn scope_label(scope: &Scope) -> String {
    let name = |p: &std::path::Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string())
    };
    match scope {
        Scope::Worktree { worktree, .. } => format!("Worktree {}", name(worktree)),
        Scope::Checkout { root } => format!("Project {}", name(root)),
        Scope::Unscoped => "all Projects".to_string(),
    }
}

pub fn render_header(
    buf: &mut Buffer,
    area: Rect,
    scope: &Scope,
    who: &str,
    count: usize,
    theme: &Theme,
) {
    let sep = if theme.unicode { "  \u{b7}  " } else { "  |  " };
    let line = Line::from(vec![
        Span::styled("scree", Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(sep),
        Span::raw(scope_label(scope)),
        Span::raw(sep),
        Span::raw(who.to_string()),
        Span::raw(sep),
        Span::raw(format!("{count} tickets")),
    ]);
    Paragraph::new(line).render(area, buf);
}

#[allow(dead_code)] // used by the ticket views that follow
/// A bordered pane: yellow border when active, gray otherwise.
pub fn bordered_pane(title: &str, active: bool, theme: &Theme) -> Block<'static> {
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_style(theme.border_style(active))
        .title(title.to_string());
    if !theme.unicode {
        block = block.border_set(ratatui::symbols::border::Set {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            vertical_left: "|",
            vertical_right: "|",
            horizontal_top: "-",
            horizontal_bottom: "-",
        });
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn dump(term: &Terminal<TestBackend>) -> String {
        let buf = term.backend().buffer();
        let a = buf.area;
        (0..a.height)
            .map(|y| {
                let row: String = (0..a.width).map(|x| buf[(x, y)].symbol()).collect();
                row.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    const HINTS: [(&str, &str); 9] = [
        ("j/k", "move"),
        ("Enter", "open"),
        ("/", "search"),
        ("n", "new ticket"),
        ("d", "done"),
        ("c", "comment"),
        ("r", "reload"),
        ("?", "help"),
        ("q", "quit"),
    ];

    fn footer(width: u16, hints: &[(&str, &str)]) -> String {
        let theme = Theme::default();
        let h = footer_height(hints, width, &theme);
        let mut t = Terminal::new(TestBackend::new(width, h)).unwrap();
        t.draw(|f| {
            let a = f.area();
            render_footer(f.buffer_mut(), a, hints, &theme)
        })
        .unwrap();
        dump(&t)
    }

    #[test]
    fn footer_wraps_at_80_and_fits_at_120() {
        let at80 = footer(80, &HINTS);
        let at120 = footer(120, &HINTS);
        assert_eq!(at80.lines().count(), 2);
        assert_eq!(at120.lines().count(), 1);
        insta::assert_snapshot!("footer_80", at80);
        insta::assert_snapshot!("footer_120", at120);
    }

    #[test]
    fn footer_short_hints_stay_on_one_line() {
        insta::assert_snapshot!("footer_short_80", footer(80, &[("q", "quit")]));
    }

    #[test]
    fn footer_never_exceeds_two_lines_or_width() {
        let theme = Theme::default();
        for w in [10u16, 20, 40, 80] {
            let lines = hint_lines(&HINTS, w, &theme);
            assert!(lines.len() <= 2, "width {w}");
            assert_eq!(footer_height(&HINTS, w, &theme) as usize, lines.len());
        }
    }

    #[test]
    fn footer_keys_are_cyan_with_color() {
        let lines = hint_lines(&[("q", "quit")], 40, &Theme::new(true, true));
        assert_eq!(
            lines[0].spans[0].style.fg,
            Some(ratatui::style::Color::Cyan)
        );
    }

    #[test]
    fn header_per_scope() {
        let theme = Theme::default();
        let scopes = [
            Scope::Unscoped,
            Scope::Checkout {
                root: PathBuf::from("/src/scree"),
            },
            Scope::Worktree {
                root: PathBuf::from("/src/scree"),
                worktree: PathBuf::from("/wt/scree.feature"),
            },
        ];
        let mut out = vec![];
        for s in &scopes {
            let mut t = Terminal::new(TestBackend::new(70, 1)).unwrap();
            t.draw(|f| {
                let a = f.area();
                render_header(f.buffer_mut(), a, s, "human/jeff", 3, &theme)
            })
            .unwrap();
            out.push(dump(&t));
        }
        insta::assert_snapshot!("header_scopes", out.join("\n"));
    }

    #[test]
    fn header_ascii_separator() {
        let theme = Theme::new(false, false);
        let mut t = Terminal::new(TestBackend::new(60, 1)).unwrap();
        t.draw(|f| {
            let a = f.area();
            render_header(f.buffer_mut(), a, &Scope::Unscoped, "agent/x", 0, &theme)
        })
        .unwrap();
        insta::assert_snapshot!("header_ascii", dump(&t));
    }

    #[test]
    fn status_icons_unicode_and_ascii() {
        let rows = [
            ("draft", false, false),
            ("ready", false, false),
            ("active", false, false),
            ("done", false, false),
            ("abandoned", false, false),
            ("ready", true, false),
            ("ready", false, true),
        ];
        for (name, unicode) in [
            ("status_icons_unicode", true),
            ("status_icons_ascii", false),
        ] {
            let theme = Theme::new(false, unicode);
            let text: Vec<String> = rows
                .iter()
                .map(|(s, b, a)| {
                    let label = match (b, a) {
                        (true, _) => "blocked",
                        (_, true) => "available",
                        _ => s,
                    };
                    format!("{} {label}", theme.status_style(s, *b, *a).0)
                })
                .collect();
            insta::assert_snapshot!(name, text.join("\n"));
        }
    }

    #[test]
    fn pane_borders() {
        for (name, theme) in [
            ("pane_unicode", Theme::new(true, true)),
            ("pane_ascii", Theme::new(false, false)),
        ] {
            let mut t = Terminal::new(TestBackend::new(12, 3)).unwrap();
            t.draw(|f| f.render_widget(bordered_pane("Tickets", true, &theme), f.area()))
                .unwrap();
            insta::assert_snapshot!(name, dump(&t));
        }
        let mut t = Terminal::new(TestBackend::new(12, 3)).unwrap();
        let theme = Theme::new(true, true);
        t.draw(|f| f.render_widget(bordered_pane("x", true, &theme), f.area()))
            .unwrap();
        assert_eq!(
            t.backend().buffer()[(0, 0)].fg,
            ratatui::style::Color::Yellow
        );
        t.draw(|f| f.render_widget(bordered_pane("x", false, &theme), f.area()))
            .unwrap();
        assert_eq!(t.backend().buffer()[(0, 0)].fg, ratatui::style::Color::Gray);
    }
}
