//! Fragments every screen reuses. Text is escaped by maud; none of these take
//! markup from a caller.

use maud::{Markup, html};

/// Shown on every page of a server bound to a network address. Not dismissible.
pub fn network_banner() -> Markup {
    html! {
        div class="net-banner" role="note" {
            "Open to the network: anyone who can reach this address can read and change tickets. There is no login."
        }
    }
}

/// The open-Decisions banner: counts only, never Decision text. The one place
/// amber is used. A static link, not a live region.
pub fn banner(open: usize, blocked: usize) -> Markup {
    let decisions = if open == 1 { "Decision" } else { "Decisions" };
    let verb = if open == 1 { "blocks" } else { "block" };
    let tickets = if blocked == 1 { "Ticket" } else { "Tickets" };
    html! {
        a class="banner" href="/decisions" {
            span class="banner-count" { (open) " open " (decisions) }
            " " (verb) " " (blocked) " " (tickets) ". Answer them."
        }
    }
}

/// Nothing to show: one plain sentence and a hint.
pub fn empty(title: &str, hint: &str) -> Markup {
    html! {
        div class="empty" {
            p class="notice-title" { (title) }
            p class="notice-hint" { (hint) }
        }
    }
}

/// Something failed: what happened and what to do.
pub fn error(title: &str, detail: &str) -> Markup {
    html! {
        div class="notice notice-error" role="alert" {
            p class="notice-title" { (title) }
            p class="notice-hint" { (detail) }
        }
    }
}

/// The message for one control, as a response body for a failed request. The
/// `err-<control>` id is the contract: the page has an element with that id
/// next to the control, and `errors.js` replaces it with this fragment.
pub fn field_error(control: &str, message: &str) -> Markup {
    html! {
        div id=(format!("err-{control}")) class="notice notice-error" role="alert" {
            p class="notice-hint" { (message) }
        }
    }
}

/// Rows a list renders before it offers "show more".
pub const FIRST_RENDER_ROWS: usize = 200;

/// The footer of a capped list: "showing N of M" and a link to the longer
/// list. Empty when everything is shown. `more_href` is the list's own URL
/// with a larger limit, chosen by the caller.
pub fn show_more(shown: usize, total: usize, more_href: &str) -> Markup {
    html! {
        @if shown < total {
            p class="more" {
                "showing " (shown) " of " (total) " "
                a href=(more_href) { "show more" }
            }
        }
    }
}

/// The data changed underneath the reader.
pub fn stale(message: &str) -> Markup {
    html! {
        div class="notice notice-stale" role="status" {
            p class="notice-title" { (message) }
        }
    }
}

/// Every key the UI defines, as `(keys, what it does)`. Each screen ticket
/// adds its own rows here, together with a test; the overlay lists exactly
/// this table.
pub const KEYS: &[(&str, &str)] = &[
    ("?", "Show this help"),
    ("Esc", "Close this help, or clear the selected row"),
    ("j", "Select the next row"),
    ("k", "Select the previous row"),
    ("Enter", "Open the selected row"),
    ("/", "Search tickets"),
];

/// The keyboard help overlay, hidden until `shell.js` opens it on `?`.
pub fn help_overlay() -> Markup {
    html! {
        div id="help" class="overlay" hidden {
            div class="overlay-panel" role="dialog" aria-modal="true"
                aria-labelledby="help-title" tabindex="-1" {
                h2 id="help-title" { "Keyboard shortcuts" }
                table class="keys" {
                    tbody {
                        @for (key, what) in KEYS {
                            tr {
                                th scope="row" { kbd { (key) } }
                                td { (what) }
                            }
                        }
                    }
                }
                button type="button" class="btn" data-help-close { "Close" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_pluralises_and_links() {
        let one = banner(1, 1).into_string();
        assert!(
            one.contains("1 open Decision</span> blocks 1 Ticket."),
            "{one}"
        );
        assert!(one.contains("href=\"/decisions\""));
        let many = banner(10_000, 3).into_string();
        assert!(
            many.contains("10000 open Decisions</span> block 3 Tickets."),
            "{many}"
        );
    }

    #[test]
    fn fragments_escape_their_text() {
        let evil = "<script>alert(1)</script>";
        for html in [
            empty(evil, evil).into_string(),
            error(evil, evil).into_string(),
            stale(evil).into_string(),
        ] {
            assert!(!html.contains("<script>"), "{html}");
            assert!(html.contains("&lt;script&gt;"), "{html}");
        }
    }

    #[test]
    fn field_error_carries_the_control_id_and_escapes() {
        let html = field_error("save", "<i>x</i>").into_string();
        assert!(html.starts_with("<div id=\"err-save\""), "{html}");
        assert!(html.contains("role=\"alert\""));
        assert!(html.contains("&lt;i&gt;"));
    }

    #[test]
    fn show_more_only_when_capped() {
        assert_eq!(FIRST_RENDER_ROWS, 200);
        assert_eq!(show_more(5, 5, "/?limit=10").into_string(), "");
        let html = show_more(200, 450, "/?limit=400").into_string();
        assert!(html.contains("showing 200 of 450"), "{html}");
        assert!(html.contains("href=\"/?limit=400\""));
    }

    #[test]
    fn error_is_an_alert_and_stale_a_status() {
        assert!(error("a", "b").into_string().contains("role=\"alert\""));
        assert!(stale("a").into_string().contains("role=\"status\""));
    }

    #[test]
    fn overlay_lists_exactly_the_defined_keys() {
        let html = help_overlay().into_string();
        assert_eq!(html.matches("<kbd>").count(), KEYS.len());
        for (key, _) in KEYS {
            assert!(html.contains(&format!("<kbd>{key}</kbd>")), "{key}");
        }
    }
}
