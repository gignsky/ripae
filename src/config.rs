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
    /// Discord sign-in, when all of its variables are set (see
    /// `DiscordConfig::from_env`). Without it the gate is handle + password.
    pub discord: Option<DiscordConfig>,
}

/// The Discord application and the server whose members get in.
#[derive(Clone)]
pub struct DiscordConfig {
    pub client_id: String,
    pub client_secret: String,
    /// The NISC server: being a member of it is what lets you sign in.
    pub guild_id: String,
    /// `RIPAE_PUBLIC_URL` + `/auth/discord/callback`; must be listed as a
    /// redirect on the Discord application exactly as it is here.
    pub redirect_uri: String,
}

impl DiscordConfig {
    /// All four of `RIPAE_DISCORD_CLIENT_ID`, `RIPAE_DISCORD_CLIENT_SECRET`,
    /// `RIPAE_DISCORD_GUILD_ID` and `RIPAE_PUBLIC_URL`, or none of them.
    fn from_env() -> Result<Option<Self>> {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self::from_vars(
            get("RIPAE_DISCORD_CLIENT_ID"),
            get("RIPAE_DISCORD_CLIENT_SECRET"),
            get("RIPAE_DISCORD_GUILD_ID"),
            get("RIPAE_PUBLIC_URL"),
        )
    }

    fn from_vars(
        client_id: Option<String>,
        client_secret: Option<String>,
        guild_id: Option<String>,
        public_url: Option<String>,
    ) -> Result<Option<Self>> {
        match (client_id, client_secret, guild_id, public_url) {
            (None, None, None, None) => Ok(None),
            (Some(client_id), Some(client_secret), Some(guild_id), Some(public_url)) => {
                Ok(Some(Self {
                    client_id: client_id.trim().to_string(),
                    client_secret: client_secret.trim().to_string(),
                    guild_id: guild_id.trim().to_string(),
                    redirect_uri: format!(
                        "{}/auth/discord/callback",
                        public_url.trim().trim_end_matches('/')
                    ),
                }))
            }
            _ => anyhow::bail!(
                "Discord sign-in needs all of RIPAE_DISCORD_CLIENT_ID, \
                 RIPAE_DISCORD_CLIENT_SECRET, RIPAE_DISCORD_GUILD_ID and RIPAE_PUBLIC_URL \
                 (or none of them)"
            ),
        }
    }
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
        let discord = DiscordConfig::from_env()?;

        Ok(Self {
            bind,
            db_path,
            session_secret,
            seed_users,
            log_json,
            cookie_secure,
            discord,
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

    #[test]
    fn discord_is_all_or_nothing() {
        let s = |v: &str| Some(v.to_string());
        assert!(DiscordConfig::from_vars(None, None, None, None).unwrap().is_none());
        let d = DiscordConfig::from_vars(s("123"), s("shh"), s("456"), s("https://ripae.example/"))
            .unwrap()
            .unwrap();
        assert_eq!(d.redirect_uri, "https://ripae.example/auth/discord/callback");
        assert_eq!(d.guild_id, "456");
        // half a Discord setup is a boot error, not a silent fall back to passwords
        assert!(DiscordConfig::from_vars(s("123"), None, s("456"), s("https://x")).is_err());
    }
}
