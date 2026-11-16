//! Markdown bodies for the detail view: parse with pulldown-cmark, style with
//! the theme, and wrap to the pane width so callers can count the returned
//! lines for scroll and selection exactly as they do for plain text.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

/// Terminal cells one character takes.
fn cell(c: char) -> usize {
    Span::raw(c.to_string()).width()
}

/// Tabs become spaces; other control chars would reach the terminal raw, so
/// drop them.
fn clean(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '\t' => vec![' '; 4],
            c if c.is_control() => vec![],
            c => vec![c],
        })
        .collect()
}

/// One line: the prefix (indent and block markers) as the first span, then the
/// styled characters with equal neighbours merged into one span.
fn build_line(prefix: &str, chars: &[(char, Style)]) -> Line<'static> {
    let mut spans = vec![Span::raw(prefix.to_string())];
    let mut run = String::new();
    let mut run_style = Style::default();
    for &(c, st) in chars {
        if !run.is_empty() && st != run_style {
            spans.push(Span::styled(std::mem::take(&mut run), run_style));
        }
        run_style = st;
        run.push(c);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, run_style));
    }
    Line::from(spans)
}

/// Greedy word wrap of styled text to `width` cells. The first line starts
/// with `first`, the others with `rest` (the same cell width). Runs of spaces
/// collapse, as in rendered Markdown; words wider than the room are split.
fn wrap_spans(text: &[(char, Style)], first: &str, rest: &str, width: usize) -> Vec<Line<'static>> {
    let prefix_w = |p: &str| p.chars().map(cell).sum::<usize>();
    let room = width.saturating_sub(prefix_w(rest)).max(1);

    // Words, each with the style of the space that preceded it.
    let mut words: Vec<(Style, Vec<(char, Style)>)> = vec![];
    let mut sep = Style::default();
    let mut cur: Vec<(char, Style)> = vec![];
    for &(c, st) in text {
        if c == ' ' {
            if !cur.is_empty() {
                let w_sep = if words.is_empty() {
                    Style::default()
                } else {
                    sep
                };
                words.push((w_sep, std::mem::take(&mut cur)));
            }
            sep = st;
        } else {
            cur.push((c, st));
        }
    }
    if !cur.is_empty() {
        let w_sep = if words.is_empty() {
            Style::default()
        } else {
            sep
        };
        words.push((w_sep, cur));
    }

    let mut lines = vec![];
    let mut line: Vec<(char, Style)> = vec![];
    let mut used = 0;
    let flush = |lines: &mut Vec<Line<'static>>, line: &mut Vec<(char, Style)>| {
        let p = if lines.is_empty() { first } else { rest };
        lines.push(build_line(p, line));
        line.clear();
    };
    for (sep, word) in words {
        let w: usize = word.iter().map(|&(c, _)| cell(c)).sum();
        if used > 0 && used + 1 + w <= room {
            line.push((' ', sep));
            line.extend(word);
            used += 1 + w;
            continue;
        }
        if used > 0 {
            flush(&mut lines, &mut line);
            used = 0;
        }
        for (c, st) in word {
            if used > 0 && used + cell(c) > room {
                flush(&mut lines, &mut line);
                used = 0;
            }
            line.push((c, st));
            used += cell(c);
        }
    }
    if used > 0 {
        flush(&mut lines, &mut line);
    }
    lines
}

/// A block container that adds a prefix to every line it holds: `cont` on
/// each line, with `marker` replacing it once on the first (list items).
struct Container {
    cont: String,
    marker: Option<String>,
}

struct Renderer<'a> {
    theme: &'a Theme,
    width: usize,
    indent: &'a str,
    out: Vec<Line<'static>>,
    containers: Vec<Container>,
    /// Counter of each open list: Some(next number) when ordered.
    lists: Vec<Option<u64>>,
    /// Styles of the open inline elements; the last one applies.
    styles: Vec<Style>,
    /// Link targets of the open links, appended in parentheses at their end.
    links: Vec<String>,
    text: Vec<(char, Style)>,
    /// Inside a fenced or indented code block: text is kept verbatim.
    code: Option<String>,
}

impl Renderer<'_> {
    fn style(&self) -> Style {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push_text(&mut self, s: &str) {
        let st = self.style();
        self.text.extend(clean(s).chars().map(|c| (c, st)));
    }

    /// First-line and continuation prefixes of the next line, consuming the
    /// pending list markers.
    fn prefixes(&mut self) -> (String, String) {
        let mut first = self.indent.to_string();
        let mut rest = self.indent.to_string();
        for c in &mut self.containers {
            first.push_str(&c.marker.take().unwrap_or_else(|| c.cont.clone()));
            rest.push_str(&c.cont);
        }
        (first, rest)
    }

    fn marker_pending(&self) -> bool {
        self.containers.iter().any(|c| c.marker.is_some())
    }

    fn ends_blank(&self) -> bool {
        self.out
            .last()
            .is_none_or(|l| l.spans.iter().all(|s| s.content.trim().is_empty()))
    }

    /// One blank line between blocks. Not before the first block, directly
    /// after a list marker, or before a list nested in an item.
    fn gap(&mut self, nested_list: bool) {
        if self.ends_blank() || self.marker_pending() || (nested_list && !self.lists.is_empty()) {
            return;
        }
        self.out.push(Line::from(self.indent.to_string()));
    }

    /// Emit the collected inline text as wrapped lines.
    fn flush(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text);
        let (first, rest) = self.prefixes();
        self.out
            .extend(wrap_spans(&text, &first, &rest, self.width));
    }

    /// Emit a verbatim code block, split at the pane width but never re-flowed.
    fn code_block(&mut self, body: &str) {
        let style = self.theme.code_style();
        let (mut first, rest) = self.prefixes();
        let room = self
            .width
            .saturating_sub(rest.chars().map(cell).sum::<usize>())
            .max(1);
        for raw in body.trim_end_matches('\n').split('\n') {
            let mut line: Vec<(char, Style)> = vec![];
            let mut used = 0;
            let mut pieces = vec![];
            for c in clean(raw).chars() {
                if used > 0 && used + cell(c) > room {
                    pieces.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push((c, style));
                used += cell(c);
            }
            pieces.push(line);
            for p in pieces {
                self.out.push(build_line(&first, &p));
                first = rest.clone();
            }
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                self.flush();
                self.gap(false);
            }
            Tag::Heading { .. } => {
                self.flush();
                self.gap(false);
                let st = self.theme.key_style().add_modifier(Modifier::BOLD);
                self.styles.push(st);
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.gap(false);
                self.containers.push(Container {
                    cont: "> ".into(),
                    marker: None,
                });
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.gap(false);
                self.code = Some(String::new());
            }
            Tag::List(start) => {
                self.flush();
                self.gap(true);
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => "- ".to_string(),
                };
                self.containers.push(Container {
                    cont: " ".repeat(marker.chars().count()),
                    marker: Some(marker),
                });
            }
            Tag::Emphasis => {
                let st = self.style().add_modifier(Modifier::ITALIC);
                self.styles.push(st);
            }
            Tag::Strong => {
                let st = self.style().add_modifier(Modifier::BOLD);
                self.styles.push(st);
            }
            Tag::Strikethrough => {
                let st = self.style().add_modifier(Modifier::CROSSED_OUT);
                self.styles.push(st);
            }
            Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                let st = self.style().add_modifier(Modifier::UNDERLINED);
                self.styles.push(st);
                self.links.push(dest_url.to_string());
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(_) => {
                self.flush();
                self.styles.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.containers.pop();
            }
            TagEnd::CodeBlock => {
                if let Some(body) = self.code.take() {
                    self.code_block(&body);
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => {
                self.flush();
                if self.marker_pending() {
                    // An empty item still shows its marker.
                    let (first, _) = self.prefixes();
                    self.out.push(Line::from(first.trim_end().to_string()));
                }
                self.containers.pop();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link | TagEnd::Image => {
                let url = self.links.pop().unwrap_or_default();
                self.styles.pop();
                if !url.is_empty() {
                    self.push_text(&format!(" ({url})"));
                }
            }
            _ => {}
        }
    }

    fn event(&mut self, ev: Event) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(s) | Event::Html(s) | Event::InlineHtml(s) => {
                if let Some(code) = &mut self.code {
                    code.push_str(&s);
                } else {
                    // Raw HTML can carry newlines; Text never does.
                    for (i, part) in s.split('\n').enumerate() {
                        if i > 0 {
                            self.flush();
                        }
                        self.push_text(part);
                    }
                }
            }
            Event::Code(s) => {
                let st = self.theme.code_style();
                self.text.extend(clean(&s).chars().map(|c| (c, st)));
            }
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.gap(false);
                let (first, _) = self.prefixes();
                let n = self
                    .width
                    .saturating_sub(first.chars().map(cell).sum())
                    .clamp(1, 20);
                self.out
                    .push(Line::from(format!("{first}{}", "-".repeat(n))));
            }
            _ => {}
        }
    }
}

/// Render Markdown `text` as lines wrapped to `width` cells, each starting
/// with `indent`. The renderer is best-effort: any input yields some lines.
pub fn markdown_lines(text: &str, indent: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut r = Renderer {
        theme,
        width,
        indent,
        out: vec![],
        containers: vec![],
        lists: vec![],
        styles: vec![],
        links: vec![],
        text: vec![],
        code: None,
    };
    for ev in Parser::new(text) {
        r.event(ev);
    }
    r.flush();
    while r.out.len() > 1 && r.ends_blank() {
        r.out.pop();
    }
    r.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str, indent: &str, width: usize) -> Vec<String> {
        markdown_lines(text, indent, width, &Theme::default())
            .iter()
            .map(|l| l.to_string())
            .collect()
    }

    fn styled(chars: &str) -> Vec<(char, Style)> {
        chars.chars().map(|c| (c, Style::default())).collect()
    }

    fn texts(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn wrap_spans_splits_long_words() {
        let out = wrap_spans(&styled("abcdefgh"), "", "", 3);
        assert_eq!(texts(&out), ["abc", "def", "gh"]);
    }

    #[test]
    fn wrap_spans_indents_and_has_no_trailing_spaces() {
        let out = wrap_spans(&styled("aaaa  bbbb"), "  ", "  ", 9);
        assert_eq!(texts(&out), ["  aaaa", "  bbbb"]);
    }

    #[test]
    fn wrap_spans_wide_chars_in_narrow_width_have_no_blank_lines() {
        let out = wrap_spans(&styled("\u{4e2d}\u{6587}"), "", "", 1);
        assert_eq!(texts(&out), ["\u{4e2d}", "\u{6587}"]);
    }

    #[test]
    fn wrap_spans_keeps_styles_across_a_break() {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let text: Vec<_> = "aaaa bbbb".chars().map(|c| (c, bold)).collect();
        let out = wrap_spans(&text, "", "", 5);
        assert_eq!(out.len(), 2);
        assert!(out[1].spans.iter().any(|s| s.style == bold));
    }

    #[test]
    fn paragraphs_are_separated_by_one_blank_line() {
        assert_eq!(
            plain("one\n\ntwo\nthree", "  ", 40),
            ["  one", "  ", "  two three"]
        );
    }

    #[test]
    fn heading_emphasis_and_code_keep_their_text() {
        let lines = markdown_lines("# Title\n\n**b** *i* `c`", "  ", 40, &Theme::default());
        assert_eq!(texts(&lines), ["  Title", "  ", "  b i c"]);
        let bold = lines[2].spans.iter().find(|s| s.content == "b").unwrap();
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
        let ital = lines[2].spans.iter().find(|s| s.content == "i").unwrap();
        assert!(ital.style.add_modifier.contains(Modifier::ITALIC));
        let code = lines[2].spans.iter().find(|s| s.content == "c").unwrap();
        assert_eq!(code.style, Theme::default().code_style());
    }

    #[test]
    fn lists_hang_their_continuation_lines() {
        assert_eq!(
            plain("- aaa bbb ccc\n- two\n  - nested", "  ", 12),
            ["  - aaa bbb", "    ccc", "  - two", "    - nested"]
        );
        assert_eq!(plain("1. a\n2. b", "", 20), ["1. a", "2. b"]);
    }

    #[test]
    fn quote_prefixes_every_line() {
        assert_eq!(
            plain("> aaa bbb ccc", "  ", 9),
            ["  > aaa", "  > bbb", "  > ccc"]
        );
    }

    #[test]
    fn fenced_code_is_not_reflowed_only_split() {
        assert_eq!(
            plain("```\nfn  main() {}\n\nlet x = 1;\n```", "  ", 10),
            ["  fn  main", "  () {}", "  ", "  let x = ", "  1;"]
        );
    }

    #[test]
    fn links_show_their_target() {
        assert_eq!(plain("[docs](http://x.y)", "", 40), ["docs (http://x.y)"]);
    }

    #[test]
    fn malformed_and_empty_input_do_not_panic() {
        for s in [
            "",
            "```\nunterminated",
            "**bold",
            "> > >",
            "- \n- ",
            "\t\r\n",
        ] {
            let _ = plain(s, "  ", 5);
        }
    }
}
