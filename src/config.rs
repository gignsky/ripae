use anyhow::{Context, Result};

/// One seed account parsed out of `RIPAE_SEED_USERS`.
pub struct SeedUser {
    pub handle: String,
    pub display_name: String,
    pub password: String,
}

pub struct Config {
    pub bind: String,
    pub db_path: String,
    pub session_secret: String,
    pub seed_users: Vec<SeedUser>,
    pub log_json: bool,
    /// Mark the session cookie `Secure`. On by default; set
    /// `RIPAE_COOKIE_SECURE=0` when serving over plain HTTP (e.g. a LAN IP),
    /// since browsers drop `Secure` cookies on non-localhost `http://`.
    pub cookie_secure: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind = std::env::var("RIPAE_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let db_path = std::env::var("RIPAE_DB_PATH").context("RIPAE_DB_PATH is required")?;
        let session_secret =
            std::env::var("RIPAE_SESSION_SECRET").context("RIPAE_SESSION_SECRET is required")?;
        let seed_users = match std::env::var("RIPAE_SEED_USERS") {
            Ok(raw) => parse_seed_users(&raw)?,
            Err(_) => Vec::new(),
        };
        let log_json = std::env::var("RIPAE_LOG_JSON").as_deref() == Ok("1");
        let cookie_secure = std::env::var("RIPAE_COOKIE_SECURE").as_deref() != Ok("0");

        Ok(Self {
            bind,
            db_path,
            session_secret,
            seed_users,
            log_json,
            cookie_secure,
        })
    }
}

/// `handle:Display Name:password,handle:Display Name:password`
fn parse_seed_users(raw: &str) -> Result<Vec<SeedUser>> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|entry| {
            let mut parts = entry.splitn(3, ':');
            let handle = parts
                .next()
                .filter(|s| !s.is_empty())
                .with_context(|| format!("RIPAE_SEED_USERS entry missing handle: {entry:?}"))?
                .to_string();
            let display_name = parts
                .next()
                .filter(|s| !s.is_empty())
                .with_context(|| format!("RIPAE_SEED_USERS entry missing display name: {entry:?}"))?
                .to_string();
            let password = parts
                .next()
                .filter(|s| !s.is_empty())
                .with_context(|| format!("RIPAE_SEED_USERS entry missing password: {entry:?}"))?
                .to_string();
            Ok(SeedUser {
                handle,
                display_name,
                password,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_seed_users() {
        let users = parse_seed_users("lordgig:Lord Gig:secret1,firstofficer:First Officer:secret2")
            .unwrap();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].handle, "lordgig");
        assert_eq!(users[0].display_name, "Lord Gig");
        assert_eq!(users[0].password, "secret1");
        assert_eq!(users[1].handle, "firstofficer");
    }

    #[test]
    fn rejects_malformed_entry() {
        assert!(parse_seed_users("lordgig:Lord Gig").is_err());
    }
}
