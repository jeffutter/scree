//! Escaping Markdown renderer for ticket text (plans, handoffs, comments).
//!
//! Ticket text comes from other parties, so nothing in it may become live
//! markup. The event stream from pulldown-cmark is rewritten before it reaches
//! the HTML writer: raw HTML becomes text (which the writer escapes), links
//! survive only for `http`, `https` and relative targets, and images collapse
//! to their alt text so no `<img>` is emitted and nothing is fetched.

use maud::{Markup, PreEscaped};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};

/// Render `src` as sanitized HTML.
pub fn render(src: &str) -> Markup {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    // One entry per open Link: whether its Start was kept, so the matching
    // End is dropped for exactly the dropped Starts.
    let mut links: Vec<bool> = Vec::new();
    let events = Parser::new_ext(src, options).filter_map(|ev| match ev {
        Event::Html(s) | Event::InlineHtml(s) => Some(Event::Text(s)),
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        Event::Start(Tag::Link { ref dest_url, .. }) => {
            let ok = link_allowed(dest_url);
            links.push(ok);
            ok.then_some(ev)
        }
        Event::End(TagEnd::Link) => links.pop().unwrap_or(false).then_some(ev),
        other => Some(other),
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    // Every byte came from the writer over sanitized events.
    PreEscaped(out)
}

/// True for `http:`, `https:` and scheme-less (relative) targets. Browsers
/// ignore ASCII whitespace and control characters inside a URL, so strip them
/// before looking for a scheme.
fn link_allowed(url: &str) -> bool {
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_ascii_control())
        .collect();
    match cleaned.find([':', '/', '?', '#']) {
        Some(i) if cleaned.as_bytes()[i] == b':' => {
            let scheme = cleaned[..i].to_ascii_lowercase();
            scheme == "http" || scheme == "https"
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(src: &str) -> String {
        render(src).into_string()
    }

    #[test]
    fn kept_links() {
        assert!(html("[a](http://x.test/p)").contains("href=\"http://x.test/p\""));
        assert!(html("[a](HTTPS://x.test)").contains("href=\"HTTPS://x.test\""));
        assert!(html("[a](/tickets/SCR-1)").contains("href=\"/tickets/SCR-1\""));
        assert!(html("[a](../x)").contains("href=\"../x\""));
        assert!(html("[a](#frag)").contains("href=\"#frag\""));
        assert!(html("[a](x.html?q=a:b)").contains("href=\"x.html?q=a:b\""));
    }

    #[test]
    fn dropped_links_keep_text() {
        for url in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "data:text/html,x",
            "vbscript:x",
            "mailto:a@b.test",
            "&#106;avascript:alert(1)",
            "java&#9;script:alert(1)",
            "%20javascript:alert(1)",
        ] {
            let out = html(&format!("[label](<{url}>)"));
            assert!(!out.contains("<a"), "{url}: {out}");
            assert!(out.contains("label"), "{url}: {out}");
        }
        let out = html("[label](<java\tscript:alert(1)>)");
        assert!(!out.contains("<a"), "{out}");
        let out = html("[label](<  javascript:x>)");
        assert!(!out.contains("<a"), "{out}");
    }

    #[test]
    fn images_become_alt_text() {
        let out = html("![the alt](http://x.test/i.png)");
        assert!(!out.contains("<img"), "{out}");
        assert!(out.contains("the alt"), "{out}");
    }

    #[test]
    fn raw_html_is_escaped() {
        let out = html("<script>alert(1)</script>\n\nhi <b onclick=x>y</b>");
        assert!(!out.contains("<script") && !out.contains("<b"), "{out}");
        assert!(out.contains("&lt;script&gt;"), "{out}");
    }
}
