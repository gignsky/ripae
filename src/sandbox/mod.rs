//! Test mode: a member's own throwaway copy of the books.
//!
//! `[ENTER TEST MODE]` copies the live database (`VACUUM INTO`) into a
//! *baseline* file and a *sandbox* file in the sandbox directory, and puts
//! the sandbox's id in the member's session. From then on every request
//! from that session that goes through the [`Db`] extractor reads and
//! writes the sandbox; everyone else stays on live. Sign-in, users and the
//! session store always stay on live (they go through `AppState::pool` on
//! the router's own state, never through `Db`).
//!
//! Leaving: `[DISCARD]` deletes both files. `[KEEP / PUSH LIVE]` shows the
//! sandbox's ledger rows against the baseline in words (`describe`), then
//! applies them to live in one transaction, refusing if live changed any
//! of the same rows in the meantime (`merge`). Either way the files go.
//!
//! Sandboxes live 24 h from creation (the id carries the creation time);
//! the boot sweep and an hourly sweep delete anything older, plus files
//! no id accounts for. One per session; at most [`MAX_OPEN`] at a time.

pub mod describe;
pub mod merge;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use rand::Rng;
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tower_sessions::Session;

use crate::auth::{CurrentUser, SplashMode};
use crate::templates::{render, TestModeTemplate};
use crate::{db, AppState};
use describe::{describe, Group, Names};
use merge::{snapshot_pool, Outcome};

/// The session key holding the sandbox id.
const SESSION_SANDBOX: &str = "sandbox_id";
/// How long a sandbox may live, counted from its creation.
pub const TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Sandboxes on disk at once, across every member (each is a copy of the
/// whole database).
pub const MAX_OPEN: usize = 32;

/// One member's sandbox: the copy they write, and the copy it started as.
pub struct Sandbox {
    pub pool: SqlitePool,
    pub base: SqlitePool,
}

/// Every sandbox, on disk and (once used since boot) open.
pub struct Sandboxes {
    dir: PathBuf,
    open: Mutex<HashMap<String, Arc<Sandbox>>>,
}

/// `<unix seconds>-<32 hex>`: the creation time, so expiry needs no
/// metadata, and enough randomness that ids can't be guessed.
fn new_id(now: u64) -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{now}-{hex}")
}

/// The creation time of a well-formed id; `None` for anything else (which
/// also keeps a stray session value from naming a path).
fn id_created(id: &str) -> Option<u64> {
    let (secs, hex) = id.split_once('-')?;
    let ok = !secs.is_empty()
        && secs.bytes().all(|b| b.is_ascii_digit())
        && hex.len() == 32
        && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    ok.then(|| secs.parse().ok()).flatten()
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn expired(id: &str, now: u64) -> bool {
    id_created(id).map_or(true, |t| now.saturating_sub(t) >= TTL.as_secs())
}

/// The sandbox id a file in the sandbox directory belongs to
/// (`<id>.db`, `<id>.base.db`, and their `-journal` / `-wal` / `-shm`).
fn file_id(name: &str) -> Option<&str> {
    let stem = name
        .trim_end_matches("-journal")
        .trim_end_matches("-wal")
        .trim_end_matches("-shm");
    stem.strip_suffix(".base.db").or_else(|| stem.strip_suffix(".db"))
}

impl Sandboxes {
    /// `RIPAE_SANDBOX_DIR`, else a `sandboxes/` directory next to the live
    /// database (same volume, so `VACUUM INTO` stays cheap).
    pub fn dir_for(db_path: &str) -> PathBuf {
        if let Some(d) = std::env::var_os("RIPAE_SANDBOX_DIR").filter(|d| !d.is_empty()) {
            return PathBuf::from(d);
        }
        match Path::new(db_path).parent() {
            Some(p) if db_path != ":memory:" => p.join("sandboxes"),
            _ => std::env::temp_dir().join("ripae-sandboxes"),
        }
    }

    pub fn new(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir).with_context(|| format!("creating sandbox dir {}", dir.display()))?;
        Ok(Self { dir, open: Mutex::new(HashMap::new()) })
    }

    fn paths(&self, id: &str) -> (PathBuf, PathBuf) {
        (self.dir.join(format!("{id}.db")), self.dir.join(format!("{id}.base.db")))
    }

    fn remove_files(&self, id: &str) {
        let (sb, base) = self.paths(id);
        for p in [sb, base] {
            for suffix in ["", "-journal", "-wal", "-shm"] {
                let mut f = p.clone().into_os_string();
                f.push(suffix);
                let _ = std::fs::remove_file(PathBuf::from(f));
            }
        }
    }

    /// Ids with files on disk.
    fn on_disk(&self) -> Vec<String> {
        let mut ids: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(file_id).map(str::to_string))
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    async fn open_pools(&self, id: &str) -> Result<Sandbox> {
        let (sb, base) = self.paths(id);
        let base = db::connect(base.to_str().context("sandbox path")?).await?;
        db::migrate(&base).await?;
        let pool = db::connect(sb.to_str().context("sandbox path")?).await?;
        db::migrate(&pool).await?;
        Ok(Sandbox { pool, base })
    }

    /// A fresh copy of `live`.
    pub async fn create(&self, live: &SqlitePool) -> Result<String> {
        let mut open = self.open.lock().await;
        if self.on_disk().len() >= MAX_OPEN {
            anyhow::bail!("too many sandboxes open");
        }
        let id = new_id(unix_now());
        let (sb, base) = self.paths(&id);
        let made = async {
            sqlx::query("VACUUM INTO ?").bind(base.to_str()).execute(live).await?;
            let base_pool = db::connect(base.to_str().context("sandbox path")?).await?;
            // the copies carry no one's sign-in
            let _ = sqlx::query("DELETE FROM tower_sessions").execute(&base_pool).await;
            sqlx::query("VACUUM INTO ?").bind(sb.to_str()).execute(&base_pool).await?;
            base_pool.close().await;
            self.open_pools(&id).await
        }
        .await;
        match made {
            Ok(s) => {
                open.insert(id.clone(), Arc::new(s));
                tracing::info!(sandbox = %id, "test mode sandbox created");
                Ok(id)
            }
            Err(e) => {
                self.remove_files(&id);
                Err(e)
            }
        }
    }

    /// The sandbox, if it still exists and hasn't expired. Reopens one
    /// made before a restart.
    pub async fn get(&self, id: &str) -> Option<Arc<Sandbox>> {
        id_created(id)?;
        if expired(id, unix_now()) {
            self.destroy(id).await;
            return None;
        }
        let mut open = self.open.lock().await;
        if let Some(s) = open.get(id) {
            return Some(s.clone());
        }
        let (sb, base) = self.paths(id);
        if !sb.exists() || !base.exists() {
            return None;
        }
        match self.open_pools(id).await {
            Ok(s) => {
                let s = Arc::new(s);
                open.insert(id.to_string(), s.clone());
                Some(s)
            }
            Err(e) => {
                tracing::warn!(sandbox = %id, error = %e, "could not reopen sandbox");
                None
            }
        }
    }

    /// Close and delete. A no-op for an id that's already gone.
    pub async fn destroy(&self, id: &str) {
        if id_created(id).is_none() {
            return;
        }
        let gone = self.open.lock().await.remove(id);
        if let Some(s) = gone {
            s.pool.close().await;
            s.base.close().await;
        }
        self.remove_files(id);
        tracing::info!(sandbox = %id, "test mode sandbox destroyed");
    }

    /// Deletes every expired sandbox and every stray file. Returns how many
    /// went.
    pub async fn sweep(&self) -> usize {
        let now = unix_now();
        let mut n = 0;
        for id in self.on_disk() {
            let (sb, base) = self.paths(&id);
            let orphan = !sb.exists() || !base.exists();
            if expired(&id, now) || orphan {
                self.destroy(&id).await;
                n += 1;
            }
        }
        n
    }

    /// Boot sweep, then hourly.
    pub fn spawn_sweeper(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60 * 60));
            loop {
                tick.tick().await;
                let n = me.sweep().await;
                if n > 0 {
                    tracing::info!(swept = n, "expired test mode sandboxes deleted");
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// per-request database
// ---------------------------------------------------------------------------

/// The app state with `pool` pointing at the books this request should see:
/// the session's sandbox in test mode, live otherwise. Ledger handlers take
/// `Db(state)` where they used to take `State(state)`.
///
/// A session whose sandbox has gone (expired, swept) is put back on live.
/// A *write* from it is refused rather than let through to live, since the
/// page that sent it still says TEST.
pub struct Db(pub AppState);

impl FromRequestParts<AppState> for Db {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let Ok(session) = Session::from_request_parts(parts, state).await else {
            return Ok(Db(state.clone()));
        };
        let Some(id) = session_sandbox(&session).await else {
            return Ok(Db(state.clone()));
        };
        match state.sandboxes.get(&id).await {
            Some(sb) => Ok(Db(AppState { pool: sb.pool.clone(), sandbox: Some(id), ..state.clone() })),
            None => {
                let _ = session.remove::<String>(SESSION_SANDBOX).await;
                if matches!(parts.method, Method::GET | Method::HEAD) {
                    Ok(Db(state.clone()))
                } else {
                    Err(ended())
                }
            }
        }
    }
}

/// The session's sandbox id, if it has one.
pub async fn session_sandbox(session: &Session) -> Option<String> {
    session.get::<String>(SESSION_SANDBOX).await.ok().flatten()
}

fn ended() -> Response {
    let mut r = (
        StatusCode::CONFLICT,
        "Test mode has ended (the sandbox expired or was discarded); nothing was saved. Reload the page.",
    )
        .into_response();
    r.headers_mut().insert("HX-Refresh", HeaderValue::from_static("true"));
    r
}

/// Drops the session's sandbox, if any (Discard, Log out).
pub async fn leave(state: &AppState, session: &Session) {
    if let Ok(Some(id)) = session.remove::<String>(SESSION_SANDBOX).await {
        state.sandboxes.destroy(&id).await;
    }
}

// ---------------------------------------------------------------------------
// handlers
// ---------------------------------------------------------------------------

/// `POST /test-mode` -- into a fresh sandbox (or back to the one this
/// session already has: one per session).
pub async fn enter(State(state): State<AppState>, user: CurrentUser, session: Session) -> Response {
    if let Some(id) = session_sandbox(&session).await {
        if state.sandboxes.get(&id).await.is_some() {
            return Redirect::to("/").into_response();
        }
    }
    match state.sandboxes.create(&state.pool).await {
        Ok(id) => {
            if session.insert(SESSION_SANDBOX, &id).await.is_err() {
                state.sandboxes.destroy(&id).await;
                return (StatusCode::INTERNAL_SERVER_ERROR, "Could not start test mode. Try again.").into_response();
            }
            tracing::info!(user = user.id, sandbox = %id, "entered test mode");
            Redirect::to("/").into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "could not create a test mode sandbox");
            (StatusCode::SERVICE_UNAVAILABLE, "Test mode is unavailable right now. Try again later.").into_response()
        }
    }
}

/// `POST /test-mode/discard` -- throw the sandbox away, back to live.
pub async fn discard(State(state): State<AppState>, _user: CurrentUser, session: Session) -> Response {
    leave(&state, &session).await;
    Redirect::to("/").into_response()
}

async fn names(pool: &SqlitePool) -> Names {
    let users: Vec<(i64, String)> = sqlx::query_as("SELECT id, display_name FROM users")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    let ships: Vec<(i64, String)> = sqlx::query_as("SELECT id, short FROM ships")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    Names { users: users.into_iter().collect(), ships: ships.into_iter().collect() }
}

fn server_error(e: anyhow::Error) -> Response {
    tracing::error!(error = %e, "test mode failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "Could not read the sandbox.").into_response()
}

async fn page(state: &AppState, user: &CurrentUser, groups: Vec<Group>, conflicts: Vec<String>, status: StatusCode) -> Response {
    let shell = crate::stowage::shell(state, Some(user), "test-mode", SplashMode::Hidden, None).await;
    let empty = groups.is_empty();
    (status, render(&TestModeTemplate { shell, groups, conflicts, empty })).into_response()
}

/// `GET /test-mode/keep` -- what pushing live would do, in words.
pub async fn review(Db(state): Db, user: CurrentUser, session: Session) -> Response {
    let Some(sb) = current(&state, &session).await else { return Redirect::to("/").into_response() };
    let (base, after) = match (snapshot_pool(&sb.base).await, snapshot_pool(&sb.pool).await) {
        (Ok(b), Ok(a)) => (b, a),
        (Err(e), _) | (_, Err(e)) => return server_error(e),
    };
    let changes = merge::diff(&base, &after);
    let groups = describe(&changes, &base, &after, &names(&sb.pool).await);
    page(&state, &user, groups, Vec::new(), StatusCode::OK).await
}

async fn current(state: &AppState, session: &Session) -> Option<Arc<Sandbox>> {
    let id = state.sandbox.clone().or(session_sandbox(session).await)?;
    state.sandboxes.get(&id).await
}

/// `POST /test-mode/keep` -- apply the sandbox to live and leave test
/// mode, or show why it can't be applied.
pub async fn keep(State(live): State<AppState>, Db(state): Db, user: CurrentUser, session: Session) -> Response {
    let Some(sb) = current(&state, &session).await else { return Redirect::to("/").into_response() };
    let (base, after) = match (snapshot_pool(&sb.base).await, snapshot_pool(&sb.pool).await) {
        (Ok(b), Ok(a)) => (b, a),
        (Err(e), _) | (_, Err(e)) => return server_error(e),
    };
    match merge::merge(&live.pool, &base, &after).await {
        Ok(Outcome::Merged { rows }) => {
            tracing::info!(user = user.id, rows, "test mode pushed live");
            leave(&live, &session).await;
            if rows > 0 {
                // everyone else's screens refresh, as after any write
                let _ = live.live.send(String::new());
            }
            Redirect::to("/").into_response()
        }
        Ok(Outcome::Refused(found)) => {
            let now = snapshot_pool(&live.pool).await.unwrap_or_default();
            let names = names(&sb.pool).await;
            let lines = describe::conflict_lines(&found, &base, &after, &now, &names);
            let changes = merge::diff(&base, &after);
            let groups = describe(&changes, &base, &after, &names);
            page(&state, &user, groups, lines, StatusCode::CONFLICT).await
        }
        Err(e) => {
            tracing::error!(error = %e, "test mode merge failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Html("Could not push the test changes live; nothing was written. Discard, or try again.")).into_response()
        }
    }
}

#[cfg(test)]
mod tests;
