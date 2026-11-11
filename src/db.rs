use anyhow::Result;
use argon2::password_hash::PasswordHasher;
use argon2::Argon2;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::config::{Config, SeedUser};

pub async fn connect(db_path: &str) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    // One connection: SQLite only allows one writer anyway, and a pool of
    // separate `:memory:` connections would each see an empty database.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Seeds the two fleet accounts from `RIPAE_SEED_USERS`, but only while
/// `users` is empty — idempotent across restarts, and a no-op once the var
/// is removed after first boot (per RIPAE-ALPHA.md §6).
pub async fn seed_if_empty(pool: &SqlitePool, config: &Config) -> Result<()> {
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if existing > 0 {
        tracing::debug!("users table already populated; skipping seed");
        return Ok(());
    }
    if config.seed_users.is_empty() {
        tracing::warn!("users table is empty and RIPAE_SEED_USERS is unset; no accounts exist");
        return Ok(());
    }
    for user in &config.seed_users {
        create_user(pool, user).await?;
        tracing::info!(handle = %user.handle, "seeded user");
    }
    Ok(())
}

/// Local dev on a copy of the live database (`scripts/dev.sh live`): every
/// account gets `password`, loses its Discord link and avatar (so the
/// password gate works instead of bouncing to Discord), and stored sessions
/// are dropped. Handles, names and all ledger data are left alone.
pub async fn dev_reset_logins(pool: &SqlitePool, password: &str) -> Result<()> {
    let hash = hash_password(password)?;
    let res = sqlx::query(
        "UPDATE users SET password_hash = ?, discord_id = NULL, avatar_url = NULL",
    )
    .bind(hash)
    .execute(pool)
    .await?;
    // the session table belongs to the session store's own migrate(), which
    // runs after this; on a database with no sessions yet there is nothing to drop
    let _ = sqlx::query("DELETE FROM tower_sessions")
        .execute(pool)
        .await;
    tracing::warn!(
        accounts = res.rows_affected(),
        "RIPAE_DEV_RESET_LOGINS: every account now signs in with the dev password"
    );
    Ok(())
}

async fn create_user(pool: &SqlitePool, user: &SeedUser) -> Result<()> {
    let hash = hash_password(&user.password)?;
    sqlx::query("INSERT INTO users (handle, display_name, password_hash) VALUES (?, ?, ?)")
        .bind(&user.handle)
        .bind(&user.display_name)
        .bind(hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?;
    Ok(hash.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SeedUser;

    async fn test_pool() -> SqlitePool {
        let pool = connect(":memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        pool
    }

    fn seed_config() -> Config {
        Config {
            bind: "0.0.0.0:8080".into(),
            db_path: ":memory:".into(),
            session_secret: "test".into(),
            seed_users: vec![
                SeedUser {
                    handle: "lordgig".into(),
                    display_name: "Lord Gig".into(),
                    password: "pw1".into(),
                },
                SeedUser {
                    handle: "firstofficer".into(),
                    display_name: "First Officer".into(),
                    password: "pw2".into(),
                },
            ],
            log_json: false,
            cookie_secure: true,
            discord: None,
            dev_reset_logins: None,
        }
    }

    #[tokio::test]
    async fn dev_reset_gives_every_account_the_password_and_unlinks_discord() {
        use argon2::password_hash::{PasswordHash, PasswordVerifier};
        let pool = test_pool().await;
        for (h, d) in [("a", Some("1")), ("b", None)] {
            sqlx::query("INSERT INTO users (handle, display_name, password_hash, discord_id) VALUES (?, ?, '', ?)")
                .bind(h)
                .bind(h)
                .bind(d)
                .execute(&pool)
                .await
                .unwrap();
        }
        dev_reset_logins(&pool, "devpw").await.unwrap();
        let rows: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT password_hash, discord_id FROM users")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows.len(), 2);
        for (hash, discord_id) in rows {
            assert!(discord_id.is_none());
            let parsed = PasswordHash::new(&hash).unwrap();
            assert!(argon2::Argon2::default().verify_password(b"devpw", &parsed).is_ok());
        }
    }

    #[tokio::test]
    async fn seeding_creates_both_accounts() {
        let pool = test_pool().await;
        seed_if_empty(&pool, &seed_config()).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn seeding_is_idempotent() {
        let pool = test_pool().await;
        let config = seed_config();
        seed_if_empty(&pool, &config).await.unwrap();
        // Second "boot": same env var still set, but users already exist.
        seed_if_empty(&pool, &config).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "seeding twice must not duplicate accounts");
    }
}
