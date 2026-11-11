//! Live updates: when one crew member changes the books, every other open
//! ledger screen refreshes its panels without a reload.
//!
//! Every successful write (any non-GET answered 2xx/3xx) is announced on a
//! broadcast channel; `GET /live` streams those announcements as
//! Server-Sent Events. The payload is the writer's tab id (the
//! `X-Ripae-Tab` header static/ripae.js sends on every htmx request) so a
//! tab can ignore its own changes -- its own responses already updated it.

use std::convert::Infallible;

use axum::extract::{Request, State};
use axum::http::{header, Method};
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::auth::CurrentUser;
use crate::AppState;

pub const TAB_HEADER: &str = "x-ripae-tab";

pub fn channel() -> broadcast::Sender<String> {
    broadcast::channel(64).0
}

/// Middleware: announce every successful write after it's done.
pub async fn announce(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let mut writes = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        && !matches!(req.uri().path(), "/login" | "/logout")
        // test mode announces its own push to live (sandbox::keep)
        && !req.uri().path().starts_with("/test-mode");
    // a write to a test mode sandbox changes nobody else's books
    if writes {
        if let Some(session) = req.extensions().get::<tower_sessions::Session>() {
            writes = crate::sandbox::session_sandbox(session).await.is_none();
        }
    }
    let tab = req
        .headers()
        .get(TAB_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let resp = next.run(req).await;
    let s = resp.status();
    if writes && (s.is_success() || s.is_redirection()) {
        // Nobody listening is not an error.
        let _ = state.live.send(tab);
    }
    resp
}

/// `GET /live` -- the event stream. A lagging receiver missed some
/// announcements; that still just means "something changed".
pub async fn events(State(state): State<AppState>, _user: CurrentUser) -> impl IntoResponse {
    let rx = state.live.subscribe();
    let stream = stream::unfold(rx, |mut rx| async move {
        let tab = match rx.recv().await {
            Ok(tab) => tab,
            Err(RecvError::Lagged(_)) => String::new(),
            Err(RecvError::Closed) => return None,
        };
        // Browsers drop an event whose data is empty (a plain form post
        // carries no tab id), so there's always something after the colon.
        let data = if tab.is_empty() { "-".to_owned() } else { tab };
        Some((Ok::<_, Infallible>(Event::default().event("changed").data(data)), rx))
    });
    (
        // Keep reverse proxies from buffering the stream.
        [(header::HeaderName::from_static("x-accel-buffering"), "no")],
        Sse::new(stream).keep_alive(KeepAlive::default()),
    )
}
