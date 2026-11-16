//! Markdown bodies for the detail view: parse with pulldown-cmark, style with
//! the theme, and wrap to the pane width so callers can count the returned
//! lines for scroll and selection exactly as they do for plain text.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::text::{cell, clean};
use super::theme::Theme;

/// A space that does not separate words; stands for a space inside inline code.
const NBSP: char = '\u{a0}';

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
        run.push(if c == NBSP { ' ' } else { c });
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
            while used > 0 && used + cell(c) > room {
                // Prefer breaking an over-long code span at one of its spaces.
                let tail = match line
                    .iter()
                    .rposition(|&(ch, _)| ch == NBSP)
                    .filter(|&i| i > 0)
                {
                    Some(i) => {
                        let t = line.split_off(i + 1);
                        line.pop();
                        t
                    }
                    None => vec![],
                };
                flush(&mut lines, &mut line);
                used = tail.iter().map(|&(ch, _)| cell(ch)).sum();
                line = tail;
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
    /// Open links: target (appended in parentheses at the end unless the link
    /// text already is the target) and the text seen so far inside the link.
    /// Kept apart from `text`, which a soft break empties.
    links: Vec<(String, String)>,
    /// Index in `out` of the latest gap line, which counts as blank even
    /// though a quote container gives it a `>` marker.
    gap_line: Option<usize>,
    text: Vec<(char, Style)>,
    /// Inside a fenced or indented code block: text is kept verbatim.
    code: Option<String>,
    /// Cells seen so far in the open table row, to place the " | " separators.
    cells: usize,
    /// Open headings: a soft break there is a space, not a new line.
    joined: usize,
    /// Where the open table's header lines start in `out`.
    head_start: usize,
}

impl Renderer<'_> {
    fn style(&self) -> Style {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push_text(&mut self, s: &str) {
        let st = self.style();
        let s = clean(s);
        for l in &mut self.links {
            l.1.push_str(&s);
        }
        self.text.extend(s.chars().map(|c| (c, st)));
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
        self.out.is_empty() || self.gap_line == Some(self.out.len() - 1)
    }

    /// One blank line between blocks. Not before the first block, directly
    /// after a list marker, or before a list nested in an item.
    fn gap(&mut self, nested_list: bool) {
        if self.ends_blank() || self.marker_pending() || (nested_list && !self.lists.is_empty()) {
            return;
        }
        // The gap keeps the quote markers of its containers so a quote that
        // spans paragraphs stays one quote.
        let conts: String = self.containers.iter().map(|c| c.cont.as_str()).collect();
        self.out
            .push(Line::from(format!("{}{}", self.indent, conts.trim_end())));
        self.gap_line = Some(self.out.len() - 1);
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

    /// Flush a table row: trailing placeholder NBSPs are dropped, and a row
    /// with nothing else still emits a (blank) line.
    fn flush_table_row(&mut self) {
        while matches!(self.text.last(), Some(&(NBSP, _))) {
            self.text.pop();
        }
        if self.text.is_empty() {
            let (first, _) = self.prefixes();
            self.out.push(Line::from(first.trim_end().to_string()));
        } else {
            self.flush();
        }
    }

    /// Emit a verbatim code block, split at the pane width but never re-flowed.
    fn code_block(&mut self, body: &str) {
        if body.trim_end_matches('\n').is_empty() {
            return;
        }
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
                self.joined += 1;
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
            Tag::Table(_) => {
                self.flush();
                self.gap(false);
            }
            Tag::TableHead | Tag::TableRow => {
                self.flush();
                self.cells = 0;
                if matches!(tag, Tag::TableHead) {
                    self.head_start = self.out.len();
                }
            }
            Tag::TableCell => {
                if self.cells > 0 {
                    // Space, bar, then an NBSP so the gap after the bar
                    // survives wrap_spans even when the cell is empty.
                    let st = self.style();
                    self.text.extend([(' ', st), ('|', st), (NBSP, st)]);
                }
                self.cells += 1;
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
                self.links.push((dest_url.to_string(), String::new()));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(_) => {
                self.flush();
                self.joined -= 1;
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
            TagEnd::TableCell => {
                // An empty leading cell keeps its column.
                if self.cells == 1 && self.text.is_empty() {
                    let st = self.style();
                    self.text.push((NBSP, st));
                }
            }
            TagEnd::TableRow => self.flush_table_row(),
            TagEnd::TableHead => {
                self.flush_table_row();
                // Rule under the header, as wide as its widest line; none when
                // the header rendered nothing.
                let head = &self.out[self.head_start.min(self.out.len())..];
                if let Some(w) = head.iter().map(|l| l.width()).max() {
                    let (first, _) = self.prefixes();
                    let n = w
                        .saturating_sub(first.chars().map(cell).sum())
                        .clamp(1, self.width.max(1));
                    self.out
                        .push(Line::from(format!("{first}{}", "-".repeat(n))));
                }
            }
            TagEnd::Table => self.flush(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link | TagEnd::Image => {
                let (url, label) = self.links.pop().unwrap_or_default();
                self.styles.pop();
                if !url.is_empty() && label != url {
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
                let s = clean(&s);
                for l in &mut self.links {
                    l.1.push_str(&s);
                }
                // NBSP is not a wrap point, so the span keeps runs of spaces;
                // wrap_spans breaks an over-long span at one and build_line
                // turns it back into a plain space for the terminal.
                self.text
                    .extend(s.chars().map(|c| (if c == ' ' { NBSP } else { c }, st)));
            }
            // Ticket text is plain and line-structured; keep its newlines.
            Event::SoftBreak if self.joined > 0 => self.push_text(" "),
            Event::SoftBreak | Event::HardBreak => self.flush(),
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
        gap_line: None,
        text: vec![],
        code: None,
        cells: 0,
        joined: 0,
        head_start: 0,
    };
    for ev in Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES) {
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
            ["  one", "  ", "  two", "  three"]
        );
    }

    #[test]
    fn soft_breaks_keep_their_lines() {
        assert_eq!(plain("Step 1\nStep 2", "", 40), ["Step 1", "Step 2"]);
        assert_eq!(plain("a\nb", "  ", 40), ["  a", "  b"]);
        assert_eq!(plain("- a\n  b", "  ", 40), ["  - a", "    b"]);
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
    fn inline_code_keeps_runs_of_spaces() {
        let lines = markdown_lines("x `a   b` y", "", 40, &Theme::default());
        assert_eq!(texts(&lines), ["x a   b y"]);
    }

    #[test]
    fn inline_code_is_not_split_at_its_spaces() {
        let lines = markdown_lines("aaaa `bb cc`", "", 8, &Theme::default());
        assert_eq!(texts(&lines), ["aaaa", "bb cc"]);
    }

    #[test]
    fn long_inline_code_breaks_at_its_spaces() {
        let lines = markdown_lines(
            "`cargo nextest run --test lifecycle`",
            "",
            12,
            &Theme::default(),
        );
        assert_eq!(
            texts(&lines),
            ["cargo", "nextest run", "--test", "lifecycle"]
        );
        assert!(lines.iter().all(|l| !l.to_string().contains('\u{a0}')));
    }

    #[test]
    fn soft_break_inside_link_text_keeps_the_label() {
        assert_eq!(
            plain("[see\nhttp://x](http://x)", "", 40).concat(),
            "seehttp://x (http://x)"
        );
    }

    #[test]
    fn soft_break_in_a_heading_is_a_space() {
        assert_eq!(plain("Title\nmore\n===", "", 40), ["Title more"]);
    }

    #[test]
    fn table_rule_spans_the_widest_header_line() {
        let out = plain("| aaaa | bbbb |\n|---|---|\n| 1 | 2 |", "", 40);
        assert_eq!(out[1], "-".repeat(out[0].len()));
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
    fn quote_gap_keeps_the_marker() {
        assert_eq!(plain("> a\n>\n> b", "  ", 20), ["  > a", "  >", "  > b"]);
    }

    #[test]
    fn autolinks_do_not_repeat_their_target() {
        assert_eq!(plain("<http://x.y>", "", 40), ["http://x.y"]);
    }

    #[test]
    fn strikethrough_is_styled() {
        let lines = markdown_lines("~~x~~", "", 40, &Theme::default());
        assert_eq!(texts(&lines), ["x"]);
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::CROSSED_OUT))
        );
    }

    #[test]
    fn tables_render_one_line_per_row_with_a_header_rule() {
        assert_eq!(
            plain("| a | b |\n|---|---|\n| 1 | 2 |", "  ", 20),
            ["  a | b", "  -----", "  1 | 2"]
        );
        assert_eq!(
            plain("- | a | b |\n  |---|---|\n  | 1 | 2 |", "  ", 20),
            ["  - a | b", "    -----", "    1 | 2"]
        );
        assert_eq!(
            plain("> | a | b |\n> |---|---|\n> | 1 | 2 |", "", 20),
            ["> a | b", "> -----", "> 1 | 2"]
        );
        // Narrow widths wrap and never panic.
        let _ = plain("| aaaa | bbbb |\n|---|---|\n| 1 | 2 |", "  ", 3);
        assert_eq!(plain("| a |\n|---|\n|  |", "  ", 1).len(), 3);
    }

    #[test]
    fn tables_keep_empty_cells_and_rows() {
        let t = |row: &str| plain(&format!("| h | h | h |\n|---|---|---|\n{row}"), "  ", 20);
        assert_eq!(t("| | b | c |")[2], "    | b | c");
        assert_eq!(t("| b | | |")[2], "  b |  |");
        assert_eq!(t("| a | | c |")[2], "  a |  | c");
        assert_eq!(t("| | | |")[2], "    |  |");
        assert_eq!(plain("| a |\n|---|\n| |", "  ", 20), ["  a", "  -", ""]);
    }

    /// Intended: an empty or whitespace-only body has no lines (a comment
    /// shows only its header); an empty fenced block has no lines either.
    #[test]
    fn empty_bodies_render_no_lines() {
        assert!(plain("", "    ", 20).is_empty());
        assert!(plain("  \n ", "    ", 20).is_empty());
        assert!(plain("```\n```", "    ", 20).is_empty());
    }

    /// Intended: a fenced block in a list item keeps its blank-line
    /// separation from the item text, tight or loose.
    #[test]
    fn fenced_block_in_a_list_item_keeps_its_gap() {
        let want = ["  - item", "  ", "    code", "  - next"];
        assert_eq!(
            plain("- item\n  ```\n  code\n  ```\n- next", "  ", 40),
            want
        );
        assert_eq!(
            plain("- item\n\n  ```\n  code\n  ```\n\n- next", "  ", 40),
            want
        );
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
