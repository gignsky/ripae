mod auth;
mod changelog;
mod config;
mod db;
mod discord;
mod fleet;
mod live;
mod lookups;
mod stowage;
mod templates;

use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use time::Duration;
use tower_http::services::ServeDir;
use std::sync::Arc;
use tower_sessions::cookie::{Key, SameSite};
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

use config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    /// Write announcements for `GET /live` (see `live`).
    pub live: tokio::sync::broadcast::Sender<String>,
    /// Discord sign-in, when configured (see `config::DiscordConfig`).
    pub discord: Option<Arc<discord::Discord>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    init_tracing(config.log_json);
    // reqwest is built without a crypto provider of its own; use ring, the
    // one sqlx's rustls already pulls in
    let _ = rustls::crypto::ring::default_provider().install_default();

    let pool = db::connect(&config.db_path).await?;
    db::migrate(&pool).await?;
    db::seed_if_empty(&pool, &config).await?;

    let session_store = SqliteStore::new(pool.clone());
    session_store.migrate().await?;

    let session_key = decode_session_key(&config.session_secret)?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)))
        .with_secure(config.cookie_secure)
        // Lax, not the default Strict: the browser has to send the cookie
        // on the redirect back from Discord, or the OAuth state can't be
        // checked. Every write is a POST/PUT/DELETE, which Lax still guards.
        .with_same_site(SameSite::Lax)
        .with_signed(session_key);

    let discord = match config.discord.clone() {
        Some(d) => {
            tracing::info!(guild_id = %d.guild_id, redirect_uri = %d.redirect_uri, "Discord sign-in on");
            Some(Arc::new(discord::Discord::new(d)?))
        }
        None => None,
    };
    let state = AppState { pool, live: live::channel(), discord };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/changelog", get(changelog::changelog))
        .route("/login", get(auth::login_form).post(auth::login))
        .route("/logout", post(auth::logout))
        .route("/auth/discord", get(auth::discord_start))
        .route("/auth/discord/callback", get(auth::discord_callback))
        .route("/", get(stowage::index))
        .route("/live", get(live::events))
        .route("/live/panels", get(stowage::live_panels))
        .route("/board", get(stowage::board))
        .route("/crew/{id}", get(stowage::crew))
        .route("/fleet", get(fleet::page))
        .route("/hulls", post(fleet::create))
        .route("/hulls/new", get(fleet::new_form))
        .route("/hulls/{id}", axum::routing::put(fleet::update).delete(fleet::delete))
        .route("/hulls/{id}/edit", get(fleet::edit_form))
        .route("/hulls/{id}/status", post(fleet::status))
        .route("/hulls/{id}/loaners", post(fleet::loaners))
        .route("/voyages", post(stowage::create_voyage))
        .route(
            "/voyages/{id}",
            axum::routing::put(stowage::voyage_update).delete(stowage::voyage_delete),
        )
        .route("/voyages/{id}/head", get(stowage::voyage_head))
        .route(
            "/voyages/{id}/details",
            get(stowage::voyage_details_form).put(stowage::voyage_details_update),
        )
        .route("/voyages/{id}/edit", get(stowage::voyage_edit_form))
        .route("/voyages/{id}/merge", get(stowage::voyage_merge_chooser))
        .route("/voyages/{id}/move", post(stowage::voyage_move))
        .route("/voyages/{id}/advance", post(stowage::voyage_advance))
        .route("/voyages/{id}/delete", get(stowage::voyage_delete_confirm))
        .route("/voyages/{id}/loads", post(stowage::add_load))
        .route("/voyages/{id}/loads/new", get(stowage::add_load_form))
        .route("/loads/{id}", get(stowage::load_row).delete(stowage::load_delete))
        .route(
            "/loads/{id}/edit",
            get(stowage::load_edit_form).put(stowage::load_update),
        )
        .route("/loads/{id}/move", post(stowage::load_move))
        .route("/loads/{id}/crates", get(stowage::crate_manifest))
        .route("/loads/{id}/where", post(stowage::load_where))
        .route(
            "/loads/{id}/close",
            get(stowage::close_form).post(stowage::close_load),
        )
        .route("/loads/{id}/delete", get(stowage::load_delete_confirm))
        .route("/loads/{id}/sale", get(stowage::sale_edit_form).put(stowage::sale_update))
        .route(
            "/loads/{id}/write-off",
            get(stowage::load_write_off_confirm).post(stowage::load_write_off),
        )
        .route("/crates/{id}/condition", post(stowage::crate_condition))
        .route("/crates/{id}/where", post(stowage::crate_where))
        .nest_service("/static", ServeDir::new("static"))
        .layer(axum::middleware::from_fn_with_state(state.clone(), live::announce))
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
