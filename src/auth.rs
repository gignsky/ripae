use argon2::password_hash::PasswordVerifier;
use argon2::Argon2;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_sessions::Session;

use crate::templates::{render, LoginTemplate};
use crate::AppState;

const SESSION_USER_ID: &str = "user_id";

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: i64,
    pub handle: String,
    pub display_name: String,
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
        let user_id: Option<i64> = session
            .get(SESSION_USER_ID)
            .await
            .map_err(|_| Redirect::to("/login"))?;
        let user_id = user_id.ok_or_else(|| Redirect::to("/login"))?;

        find_user_by_id(&state.pool, user_id)
            .await
            .ok()
            .flatten()
            .ok_or_else(|| Redirect::to("/login"))
    }
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

pub async fn login_form() -> Html<String> {
    render(&LoginTemplate { error: None })
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
    match authenticate(&state.pool, &form.handle, &form.password).await {
        Ok(Some(user_id)) => {
            if session.insert(SESSION_USER_ID, user_id).await.is_err() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    render(&LoginTemplate {
                        error: Some("Could not start a session. Try again."),
                    }),
                )
                    .into_response();
            }
            Redirect::to("/").into_response()
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            render(&LoginTemplate {
                error: Some("Wrong handle or password."),
            }),
        )
            .into_response(),
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
}
