//! The version badge's target: CHANGELOG.md, embedded at compile time and
//! rendered to HTML once. Public (no login) so the badge works on the login
//! page too.

use std::sync::OnceLock;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use tower_sessions::Session;

use crate::auth::{current_user, SplashMode};
use crate::templates::{render, ChangelogTemplate};
use crate::AppState;

/// Shown in the corner of every page; dev builds carry their roll suffix.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const CHANGELOG_MD: &str = include_str!("../CHANGELOG.md");

pub fn changelog_html() -> &'static str {
    static HTML: OnceLock<String> = OnceLock::new();
    HTML.get_or_init(|| {
        let parser = pulldown_cmark::Parser::new(CHANGELOG_MD);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, parser);
        html
    })
}

/// htmx requests (the badge) get just the fragment for the dialog; a plain
/// visit to /changelog gets a full page.
pub async fn changelog(State(state): State<AppState>, session: Session, headers: HeaderMap) -> Response {
    if headers.contains_key("hx-request") {
        Html(changelog_html()).into_response()
    } else {
        let user = current_user(&state, &session).await;
        let shell = crate::stowage::shell(&state, user.as_ref(), "changelog", SplashMode::Hidden, None).await;
        render(&ChangelogTemplate {
            shell,
            body: changelog_html(),
        })
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_renders_and_lists_the_current_version() {
        let html = changelog_html();
        assert!(html.contains("<h1>Changelog</h1>"));
        // A release version must have an entry, so it can't ship without
        // its changelog. Roll builds (-rollN) are exempt: their entry is
        // written on rolling just before promotion, not in the roll.
        if !VERSION.contains('-') {
            assert!(
                html.contains(&format!("<h2>{VERSION}")),
                "CHANGELOG.md has no '## {VERSION}' entry"
            );
        }
    }
}
