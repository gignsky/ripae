//! The version badge's target: CHANGELOG.md, embedded at compile time and
//! rendered to HTML once. Public (no login) so the badge works on the login
//! page too.

use std::sync::OnceLock;

use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

use crate::templates::{render, ChangelogTemplate};

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
pub async fn changelog(headers: HeaderMap) -> Response {
    if headers.contains_key("hx-request") {
        Html(changelog_html()).into_response()
    } else {
        render(&ChangelogTemplate {
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
        // The running version (minus any -rollN dev suffix) must have an
        // entry, so a release can't ship without its changelog.
        let release = VERSION.split('-').next().unwrap();
        assert!(
            html.contains(&format!("<h2>{release}")),
            "CHANGELOG.md has no '## {release}' entry"
        );
    }
}
