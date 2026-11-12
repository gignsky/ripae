use argon2::password_hash::PasswordVerifier;
use argon2::Argon2;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use base64::Engine;
use rand::Rng;
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_sessions::Session;

use crate::discord;
use crate::templates::{render, LoginTemplate};
use crate::AppState;

const SESSION_USER_ID: &str = "user_id";
/// Set once the welcome splash has played for this session, so it doesn't
/// replay on every page load (RIPAE-ROLL7.md §3, 7.5).
const SESSION_SPLASH_SEEN: &str = "splash_seen";
/// Set by a sign-in for the redirect that follows it: the gate was the
/// splash, so the ledger it lands on doesn't play it again, even when the
/// redirect chain came back from Discord (a cross-site landing).
const SESSION_JUST_SIGNED_IN: &str = "just_signed_in";
/// The OAuth `state` of a Discord sign-in in flight; the callback must
/// echo it back (the CSRF guard on the round trip).
const SESSION_OAUTH_STATE: &str = "oauth_state";

const DOOR_SHUT: &str = "The door does not open. Check the handle and password.";
const NOT_VOUCHED: &str = "Discord did not vouch for you. Try again.";
const NOT_A_MEMBER: &str = "The door does not open. NISC members only.";
const NO_SESSION: &str = "Could not start a session. Try again.";

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: i64,
    /// The handle lives on the leaderboard, not the rail. With Discord
    /// sign-in it is the member's Discord username.
    #[allow(dead_code)]
    pub handle: String,
    pub display_name: String,
    /// Every font a quarter larger for this member (`users.big_type`).
    pub big_type: bool,
}

/// How the splash plays: the gate (no session), the welcome line (first
/// paint of a session), or not at all (already welcomed; `[CLOSE]` replays
/// it in welcome mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashMode {
    Gate,
    Welcome,
    Hidden,
}

impl SplashMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SplashMode::Gate => "gate",
            SplashMode::Welcome => "welcome",
            SplashMode::Hidden => "hidden",
        }
    }
}

/// Without a session the splash is the login. With one, opening the site
/// (`landing`) always lands on the welcome splash, whose `[ENTER]` lets the
/// member in; moving around inside the site welcomes them once per
/// session, then stays out of the way.
pub fn splash_mode(user: Option<&CurrentUser>, already_welcomed: bool, landing: bool) -> SplashMode {
    match user {
        None => SplashMode::Gate,
        Some(_) if landing => SplashMode::Welcome,
        Some(_) if already_welcomed => SplashMode::Hidden,
        Some(_) => SplashMode::Welcome,
    }
}

/// Whether a page request is someone opening the site -- typed, bookmarked,
/// a link from Discord -- rather than a move inside it (the rail's nav, the
/// redirect after a form post). Browsers say so in `Sec-Fetch-Site`; one
/// that doesn't send it is judged by its `Referer`.
pub fn is_landing(headers: &HeaderMap) -> bool {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    match get("sec-fetch-site") {
        Some(site) => site != "same-origin",
        None => {
            let host = get("host");
            let from = get("referer")
                .and_then(|r| r.split_once("://"))
                .map(|(_, rest)| rest.split('/').next().unwrap_or(""));
            !matches!((from, host), (Some(f), Some(h)) if f.eq_ignore_ascii_case(h))
        }
    }
}

/// Extracting `CurrentUser` on a route enforces login: a missing or stale
/// session redirects to `/login` instead of reaching the handler.
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Redirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| Redirect::to("/login"))?;
        current_user(state, &session)
            .await
            .ok_or_else(|| Redirect::to("/login"))
    }
}

/// The member behind a session, if any.
pub async fn current_user(state: &AppState, session: &Session) -> Option<CurrentUser> {
    let user_id: i64 = session.get(SESSION_USER_ID).await.ok().flatten()?;
    find_user_by_id(&state.pool, user_id).await.ok().flatten()
}

/// Reads the welcome flag and sets it, so the first `/` of a session plays
/// the welcome splash and later in-site ones don't; a `landing` plays it
/// every time, except straight after signing in at the gate.
pub async fn take_splash_mode(session: &Session, user: &CurrentUser, landing: bool) -> SplashMode {
    let seen: bool = session.get(SESSION_SPLASH_SEEN).await.ok().flatten().unwrap_or(false);
    let just_in: bool = session.remove(SESSION_JUST_SIGNED_IN).await.ok().flatten().unwrap_or(false);
    let mode = splash_mode(Some(user), seen, landing && !just_in);
    if mode == SplashMode::Welcome {
        let _ = session.insert(SESSION_SPLASH_SEEN, true).await;
    }
    mode
}

async fn find_user_by_id(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<CurrentUser>> {
    let row = sqlx::query_as::<_, (i64, String, String, bool)>(
        "SELECT id, handle, display_name, big_type FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, handle, display_name, big_type)| CurrentUser {
        id,
        handle,
        display_name,
        big_type,
    }))
}

async fn gate(state: &AppState, error: Option<&'static str>, status: StatusCode) -> Response {
    let shell = crate::stowage::shell(state, None, "login", SplashMode::Gate, error).await;
    (status, render(&LoginTemplate { shell })).into_response()
}

/// `GET /login` -- the splash with the gate. A member with a valid session
/// goes straight to the ledger.
pub async fn login_form(State(state): State<AppState>, session: Session) -> Response {
    if current_user(&state, &session).await.is_some() {
        return Redirect::to("/").into_response();
    }
    gate(&state, None, StatusCode::OK).await
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub handle: String,
    pub password: String,
}

/// `POST /login` -- handle + password, only on a copy without Discord
/// (local dev). With Discord configured, Discord is the only way in: the
/// old accounts have all been claimed (0.3.4), so no password is taken.
pub async fn login(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Response {
    if state.discord.is_some() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match authenticate(&state.pool, form.handle.trim(), &form.password).await {
        Ok(Some(user_id)) => start_session(&state, &session, user_id).await,
        _ => gate(&state, Some(DOOR_SHUT), StatusCode::UNAUTHORIZED).await,
    }
}

#[derive(Debug, Deserialize)]
pub struct DevLoginForm {
    pub user_id: i64,
}

/// `POST /login/dev` -- a one-click sign-in from the gate's account list.
/// Only answers with `RIPAE_DEV_QUICK_LOGIN=1` (local dev).
pub async fn dev_login(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<DevLoginForm>,
) -> Response {
    if !state.dev_quick_login {
        return StatusCode::NOT_FOUND.into_response();
    }
    match find_user_by_id(&state.pool, form.user_id).await {
        Ok(Some(user)) => start_session(&state, &session, user.id).await,
        _ => gate(&state, Some(DOOR_SHUT), StatusCode::UNAUTHORIZED).await,
    }
}

/// Signs `user_id` in on a fresh session id and goes to the ledger.
async fn start_session(state: &AppState, session: &Session, user_id: i64) -> Response {
    // a new id on sign-in, so a session id planted before it is worthless
    if session.cycle_id().await.is_err() || session.insert(SESSION_USER_ID, user_id).await.is_err() {
        return gate(state, Some(NO_SESSION), StatusCode::INTERNAL_SERVER_ERROR).await;
    }
    // the gate itself was the splash: don't play the welcome as well
    let _ = session.insert(SESSION_SPLASH_SEEN, true).await;
    let _ = session.insert(SESSION_JUST_SIGNED_IN, true).await;
    Redirect::to("/").into_response()
}

/// `GET /auth/discord` -- off to Discord, with a fresh `state` to check
/// on the way back.
pub async fn discord_start(State(state): State<AppState>, session: Session) -> Response {
    let Some(discord) = state.discord.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if current_user(&state, &session).await.is_some() {
        return Redirect::to("/").into_response();
    }
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    if session.insert(SESSION_OAUTH_STATE, &token).await.is_err() {
        return gate(&state, Some(NO_SESSION), StatusCode::INTERNAL_SERVER_ERROR).await;
    }
    Redirect::to(&discord.authorize_url(&token)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct DiscordCallback {
    code: Option<String>,
    state: Option<String>,
    /// e.g. `access_denied` when the member pressed Cancel.
    error: Option<String>,
}

/// `GET /auth/discord/callback` -- Discord sends the member back here. In
/// the NISC server: signed in. Not in it, or the round trip doesn't add
/// up: the gate again.
pub async fn discord_callback(
    State(state): State<AppState>,
    session: Session,
    Query(q): Query<DiscordCallback>,
) -> Response {
    let Some(discord) = state.discord.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // single-use, whatever happens next
    let expected: Option<String> = session.remove(SESSION_OAUTH_STATE).await.ok().flatten();
    let code = match (q.code, q.state, expected) {
        (Some(code), Some(got), Some(want)) if got == want => code,
        _ => {
            tracing::info!(error = ?q.error, "Discord sign-in did not complete");
            return gate(&state, Some(NOT_VOUCHED), StatusCode::UNAUTHORIZED).await;
        }
    };
    let profile = match discord.member(&code).await {
        Ok(Some(p)) => p,
        Ok(None) => return gate(&state, Some(NOT_A_MEMBER), StatusCode::FORBIDDEN).await,
        Err(e) => {
            tracing::warn!(error = ?e, "Discord sign-in failed");
            return gate(&state, Some(NOT_VOUCHED), StatusCode::BAD_GATEWAY).await;
        }
    };
    match discord::sign_in(&state.pool, &profile).await {
        Ok(user_id) => start_session(&state, &session, user_id).await,
        Err(e) => {
            tracing::error!(error = ?e, "could not record a Discord sign-in");
            gate(&state, Some(NO_SESSION), StatusCode::INTERNAL_SERVER_ERROR).await
        }
    }
}

/// The account behind a handle and password.
async fn authenticate(pool: &SqlitePool, handle: &str, password: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64, String)>("SELECT id, password_hash FROM users WHERE handle = ?")
    .bind(handle)
    .fetch_optional(pool)
    .await?;
    let Some((id, hash)) = row else {
        return Ok(None);
    };
    match Argon2::default().verify_password(password.as_bytes(), hash.as_str()) {
        Ok(()) => Ok(Some(id)),
        Err(_) => Ok(None),
    }
}

pub async fn logout(State(state): State<AppState>, session: Session) -> Redirect {
    // a sandbox nobody can get back to any more
    crate::sandbox::leave(&state, &session).await;
    let _ = session.flush().await;
    Redirect::to("/login")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    async fn wrong_password_is_rejected() {
        let pool = db::connect(":memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        let hash = db::hash_password("correct-password").unwrap();
        sqlx::query("INSERT INTO users (handle, display_name, password_hash) VALUES ('lordgig', 'Lord Gig', ?)")
            .bind(hash)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(authenticate(&pool, "lordgig", "wrong-password").await.unwrap(), None);
        assert!(authenticate(&pool, "lordgig", "correct-password").await.unwrap().is_some());
        assert_eq!(authenticate(&pool, "nobody", "correct-password").await.unwrap(), None);
        // accounts made through Discord have no password at all
        sqlx::query("INSERT INTO users (handle, display_name, password_hash) VALUES ('jt', 'JT', '')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(authenticate(&pool, "jt", "").await.unwrap(), None);
    }

    #[test]
    fn splash_mode_is_gate_without_session_and_welcome_with_one() {
        let gig = CurrentUser { id: 1, handle: "lordgig".into(), display_name: "Lord Gig".into(), big_type: false };
        assert_eq!(splash_mode(None, false, false), SplashMode::Gate);
        assert_eq!(splash_mode(None, true, true), SplashMode::Gate);
        assert_eq!(splash_mode(Some(&gig), false, false), SplashMode::Welcome);
        // once welcomed, moving around the site keeps the splash hidden
        assert_eq!(splash_mode(Some(&gig), true, false), SplashMode::Hidden);
        // but opening the site always lands on it
        assert_eq!(splash_mode(Some(&gig), true, true), SplashMode::Welcome);
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn landing_is_anything_but_a_move_inside_the_site() {
        assert!(is_landing(&headers(&[("sec-fetch-site", "none")])));
        assert!(is_landing(&headers(&[("sec-fetch-site", "cross-site")])));
        assert!(!is_landing(&headers(&[("sec-fetch-site", "same-origin")])));
        // no Sec-Fetch-Site: the Referer decides
        assert!(is_landing(&headers(&[("host", "ripae.example")])));
        assert!(!is_landing(&headers(&[("host", "ripae.example"), ("referer", "https://ripae.example/board")])));
        assert!(is_landing(&headers(&[("host", "ripae.example"), ("referer", "https://discord.com/channels/1")])));
    }
}
