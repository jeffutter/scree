//! Fragments every screen reuses. Text is escaped by maud; the only markup a
//! caller passes in is the already escaped body of a dialog.

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

/// A modal `<dialog>` holding one form. `shell.js` opens it from the button
/// carrying `data-dialog-open="ID"`, closes it on Cancel, Esc or a successful
/// save, and re-opens it if a live patch of the region closed it. The form's
/// `err-CONTROL` slot is inside the dialog, so a refusal shows where the user
/// is looking.
pub fn dialog(dialog_id: &str, title: &str, inner: Markup) -> Markup {
    dialog_with(dialog_id, title, "dialog", "Cancel", inner)
}

/// [`dialog`] with its CSS class (`dialog dialog-wide` for the long forms) and
/// the label of the button that closes it.
pub fn dialog_with(
    dialog_id: &str,
    title: &str,
    class: &str,
    close: &str,
    inner: Markup,
) -> Markup {
    html! {
        // A live patch morphs the page toward markup with no `open`, which
        // would close a dialog the user is typing in.
        dialog id=(dialog_id) class=(class) aria-labelledby=(format!("{dialog_id}-title"))
            data-preserve-attr="open" {
            h2 id=(format!("{dialog_id}-title")) { (title) }
            (inner)
            div class="dialog-foot" {
                button type="button" class="btn" data-dialog-close { (close) }
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

/// An action icon: 16x16 strokes in `currentColor`, hidden from assistive
/// technology. The control that holds it must carry its own name (visible text
/// or `aria-label`). Unknown names render nothing, and a test pins the set.
pub fn icon(name: &str) -> Markup {
    html! {
        svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false" {
            @match name {
                "edit" => { path d="M11 2.5l2.5 2.5L5.5 13H3v-2.5z" {} },
                "move" => { path d="M5 13V3M2.5 5.5L5 3l2.5 2.5M11 3v10M8.5 10.5L11 13l2.5-2.5" {} },
                "subtask" => { path d="M4 2v7a2 2 0 002 2h6M9.5 8.5L12 11l-2.5 2.5" {} },
                "blocker" => { circle cx="8" cy="8" r="5.5" {} path d="M4.1 4.1l7.8 7.8" {} },
                "circle" => { circle cx="8" cy="8" r="5.5" {} },
                "circle-check" => { circle cx="8" cy="8" r="5.5" {} path d="M5.5 8.2l1.8 1.8 3.2-3.5" {} },
                "remove" => { path d="M4 4l8 8M12 4l-8 8" {} },
                "tick" => { path d="M4 8.5l2.7 2.7L12 5.5" {} },
                "play" => { path d="M5 3l8 5-8 5z" {} },
                "check" => { path d="M3 8.5l3.5 3.5L13 4.5" {} },
                "abandon" => { circle cx="8" cy="8" r="5.5" {} path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" {} },
                "reopen" => { path d="M3 8a5 5 0 105-5H5M5 1L3 3l2 2" {} },
                "archive" => { path d="M2.5 4h11v3h-11zM3.5 7v6h9V7M6.5 9.5h3" {} },
                "release" => { path d="M9 3H4v10h5M7 8h7M11.5 5.5L14 8l-2.5 2.5" {} },
                "commit" => { circle cx="8" cy="8" r="2.5" {} path d="M1.5 8h4M10.5 8h4" {} },
                "ready" => { path d="M3 8h9M8.5 4.5L12 8l-3.5 3.5" {} },
                "note" => { path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z" {} },
                "person" => { circle cx="8" cy="5.5" r="2.5" {} path d="M3 13.5c.5-2.5 2.5-3.5 5-3.5s4.5 1 5 3.5" {} },
                "robot" => { path d="M3 6h10v7H3zM8 6V3.5M6 9v.5M10 9v.5" {} },
                "plus" => { path d="M8 3v10M3 8h10" {} },
                _ => {},
            }
        }
    }
}

/// The status shape from the design system: draft dashed circle, ready empty
/// circle, available circle with a play triangle, active circle with the right
/// half filled, done filled circle with a tick, blocked rounded square with a
/// dash, abandoned circle with a cross. Decorative (`aria-hidden`): the status
/// word always sits next to it, and is the accessible name. Unknown names get
/// the empty circle.
pub fn status_icon(status: &str) -> Markup {
    html! {
        svg class="icon st" viewBox="0 0 16 16" aria-hidden="true" focusable="false" {
            @match status {
                "draft" => { circle cx="8" cy="8" r="5.5" stroke-dasharray="2 2" {} },
                "available" => { circle cx="8" cy="8" r="5.5" {} path class="st-fill" d="M6.5 5.5l4 2.5-4 2.5z" {} },
                "active" => { circle cx="8" cy="8" r="5.5" {} path class="st-fill" d="M8 2.5a5.5 5.5 0 010 11z" {} },
                "done" => { circle class="st-fill" cx="8" cy="8" r="6" {} path class="st-tick" d="M5.3 8.2l1.9 1.9 3.5-3.7" {} },
                "blocked" => { rect x="2.5" y="2.5" width="11" height="11" rx="2.5" {} path d="M5.5 8h5" {} },
                "abandoned" => { circle cx="8" cy="8" r="5.5" {} path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4" {} },
                _ => { circle cx="8" cy="8" r="5.5" {} },
            }
        }
    }
}

/// The quiet blocked cue: a red icon and a muted label. Red is for blocked.
pub fn blocked_cue(label: &str) -> Markup {
    html! {
        span class="cue cue-blocked" { (status_icon("blocked")) span { (label) } }
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
