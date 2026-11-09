mod auth;
mod changelog;
mod config;
mod db;
mod lookups;
mod stowage;
mod templates;

use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use time::Duration;
use tower_http::services::ServeDir;
use tower_sessions::cookie::Key;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    init_tracing(config.log_json);

    let pool = db::connect(&config.db_path).await?;
    db::migrate(&pool).await?;
    db::seed_if_empty(&pool, &config).await?;

    let session_store = SqliteStore::new(pool.clone());
    session_store.migrate().await?;

    let session_key = decode_session_key(&config.session_secret)?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)))
        .with_secure(config.cookie_secure)
        .with_signed(session_key);

    let state = AppState { pool };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/changelog", get(changelog::changelog))
        .route("/login", get(auth::login_form).post(auth::login))
        .route("/logout", post(auth::logout))
        .route("/", get(stowage::index))
        .nest_service("/static", ServeDir::new("static"))
        .layer(session_layer)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!(bind = %config.bind, "ripae listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

/// `RIPAE_SESSION_SECRET` is base64-encoded random bytes (§6); `Key`
/// signs the session cookie so a client can't forge or tamper with it.
fn decode_session_key(secret: &str) -> anyhow::Result<Key> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(secret.trim())
        .map_err(|e| anyhow::anyhow!("RIPAE_SESSION_SECRET is not valid base64: {e}"))?;
    if bytes.len() < 64 {
        anyhow::bail!(
            "RIPAE_SESSION_SECRET must decode to at least 64 bytes, got {}",
            bytes.len()
        );
    }
    Ok(Key::from(&bytes))
}

fn init_tracing(json: bool) {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer()).init();
    }
}
