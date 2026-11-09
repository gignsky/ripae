use argon2::password_hash::PasswordVerifier;
use argon2::Argon2;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_sessions::Session;

use crate::templates::{render, LoginTemplate};
use crate::AppState;

const SESSION_USER_ID: &str = "user_id";
/// Set once the welcome splash has played for this session, so it doesn't
/// replay on every page load (RIPAE-ROLL7.md §3, 7.5).
const SESSION_SPLASH_SEEN: &str = "splash_seen";

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: i64,
    pub handle: String,
    pub display_name: String,
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

/// Without a session the splash is the login; with one it welcomes the
/// member back once, then stays out of the way.
pub fn splash_mode(user: Option<&CurrentUser>, already_welcomed: bool) -> SplashMode {
    match user {
        None => SplashMode::Gate,
        Some(_) if already_welcomed => SplashMode::Hidden,
        Some(_) => SplashMode::Welcome,
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
/// the welcome splash and later ones don't.
pub async fn take_splash_mode(session: &Session, user: &CurrentUser) -> SplashMode {
    let seen: bool = session.get(SESSION_SPLASH_SEEN).await.ok().flatten().unwrap_or(false);
    let mode = splash_mode(Some(user), seen);
    if mode == SplashMode::Welcome {
        let _ = session.insert(SESSION_SPLASH_SEEN, true).await;
    }
    mode
}

async fn find_user_by_id(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<CurrentUser>> {
    let row = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, handle, display_name FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, handle, display_name)| CurrentUser {
        id,
        handle,
        display_name,
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

pub async fn login(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Response {
    match authenticate(&state.pool, form.handle.trim(), &form.password).await {
        Ok(Some(user_id)) => {
            if session.insert(SESSION_USER_ID, user_id).await.is_err() {
                return gate(
                    &state,
                    Some("Could not start a session. Try again."),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
                .await;
            }
            // the gate itself was the splash: don't play the welcome as well
            let _ = session.insert(SESSION_SPLASH_SEEN, true).await;
            Redirect::to("/").into_response()
        }
        _ => {
            gate(
                &state,
                Some("The door does not open. Check the handle and password."),
                StatusCode::UNAUTHORIZED,
            )
            .await
        }
    }
}

async fn authenticate(
    pool: &SqlitePool,
    handle: &str,
    password: &str,
) -> anyhow::Result<Option<i64>> {
    let row =
        sqlx::query_as::<_, (i64, String)>("SELECT id, password_hash FROM users WHERE handle = ?")
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

pub async fn logout(session: Session) -> Redirect {
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

        assert_eq!(
            authenticate(&pool, "lordgig", "wrong-password")
                .await
                .unwrap(),
            None
        );
        assert!(authenticate(&pool, "lordgig", "correct-password")
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            authenticate(&pool, "nobody", "correct-password")
                .await
                .unwrap(),
            None
        );
    }

    #[test]
    fn splash_mode_is_gate_without_session_and_welcome_with_one() {
        let gig = CurrentUser { id: 1, handle: "lordgig".into(), display_name: "Lord Gig".into() };
        assert_eq!(splash_mode(None, false), SplashMode::Gate);
        assert_eq!(splash_mode(None, true), SplashMode::Gate);
        assert_eq!(splash_mode(Some(&gig), false), SplashMode::Welcome);
        // once welcomed, the splash stays hidden until [CLOSE] replays it
        assert_eq!(splash_mode(Some(&gig), true), SplashMode::Hidden);
    }
}
