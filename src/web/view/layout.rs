//! The page shell: skip link, navigation, optional banner, main, help
//! overlay, footer. The theme follows the system setting through
//! `tokens.css`; there is no toggle.

use maud::{DOCTYPE, Markup, html};

use super::components;
use crate::web::assets;

/// The five screens of the nav, in nav order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    List,
    Board,
    Decisions,
    Ledger,
    Graph,
}

impl Route {
    pub const ALL: [Route; 5] = [
        Route::List,
        Route::Board,
        Route::Decisions,
        Route::Ledger,
        Route::Graph,
    ];

    pub fn path(self) -> &'static str {
        match self {
            Route::List => "/",
            Route::Board => "/board",
            Route::Decisions => "/decisions",
            Route::Ledger => "/ledger",
            Route::Graph => "/graph",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Route::List => "List",
            Route::Board => "Board",
            Route::Decisions => "Decisions",
            Route::Ledger => "Ledger",
            Route::Graph => "Graph",
        }
    }

    /// Whether the screen is served. A screen ticket flips its own arm to
    /// `true`; until then the nav shows it disabled instead of a dead link.
    pub fn built(self) -> bool {
        match self {
            Route::List | Route::Board | Route::Decisions | Route::Ledger => true,
            Route::Graph => false,
        }
    }
}

/// The open-Decisions banner shows on every page except the queue itself.
pub fn show_banner(route: Route) -> bool {
    route != Route::Decisions
}

pub struct PageCtx<'a> {
    pub route: Route,
    pub role: &'a str,
    /// `(open Decisions, blocked Tickets)`; `None` when none are open.
    pub banner: Option<(usize, usize)>,
}

pub fn page(ctx: &PageCtx, title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="color-scheme" content="light dark";
                title { (title) " - Scree" }
                link rel="stylesheet" href=(assets::TOKENS_HREF);
                link rel="stylesheet" href=(assets::APP_HREF);
                script type="module" src=(assets::DATASTAR_HREF) {}
                script defer src=(assets::SHELL_HREF) {}
                script defer src=(assets::ERRORS_HREF) {}
            }
            body {
                a class="skip" href="#main" { "Skip to content" }
                header class="bar" {
                    span class="brand" { "Scree" }
                    nav aria-label="Screens" {
                        @for r in Route::ALL {
                            @if !r.built() {
                                span class="nav-item nav-disabled" aria-disabled="true"
                                    title="Not built yet" { (r.label()) }
                            } @else if r == ctx.route {
                                a class="nav-item" href=(r.path()) aria-current="page" { (r.label()) }
                            } @else {
                                a class="nav-item" href=(r.path()) { (r.label()) }
                            }
                        }
                    }
                }
                @if let Some((open, blocked)) = ctx.banner.filter(|_| show_banner(ctx.route)) {
                    (components::banner(open, blocked))
                }
                main id="main" { (body) }
                (components::help_overlay())
                footer {
                    "role: " (ctx.role) " - press " kbd { "?" } " for keys"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_rule_is_everywhere_but_the_queue() {
        for r in Route::ALL {
            assert_eq!(show_banner(r), r != Route::Decisions, "{r:?}");
        }
    }

    fn render(route: Route, banner: Option<(usize, usize)>) -> String {
        let ctx = PageCtx {
            route,
            role: "human",
            banner,
        };
        page(&ctx, "T", html! { p { "x" } }).into_string()
    }

    #[test]
    fn decisions_route_hides_the_banner() {
        assert!(render(Route::List, Some((1, 1))).contains("class=\"banner\""));
        assert!(!render(Route::Decisions, Some((1, 1))).contains("class=\"banner\""));
        assert!(!render(Route::List, None).contains("class=\"banner\""));
    }

    #[test]
    fn unbuilt_screens_are_disabled_not_links() {
        let html = render(Route::List, None);
        assert!(html.contains("href=\"/\" aria-current=\"page\""));
        for r in Route::ALL.into_iter().filter(|r| !r.built()) {
            assert!(!html.contains(&format!("href=\"{}\"", r.path())), "{r:?}");
            assert!(html.contains(&format!(
                "aria-disabled=\"true\" title=\"Not built yet\">{}",
                r.label()
            )));
        }
    }
}
