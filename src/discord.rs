//! Discord sign-in (0.3): the NISC server is the door. OAuth2 with the
//! `identify` and `guilds.members.read` scopes is enough to learn who a
//! member is and whether they're in the server, without a bot. Their
//! Discord username becomes the handle, their server nickname (else their
//! Discord display name) the name on the ledger, and their avatar goes on
//! the service record -- all refreshed on every sign-in.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::config::DiscordConfig;

const API: &str = "https://discord.com/api/v10";
const AUTHORIZE: &str = "https://discord.com/oauth2/authorize";
const CDN: &str = "https://cdn.discordapp.com";
const SCOPES: &str = "identify guilds.members.read";

pub struct Discord {
    config: DiscordConfig,
    http: reqwest::Client,
}

/// Who Discord says signed in, as Ripae will show them.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub discord_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    global_name: Option<String>,
    avatar: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GuildMember {
    nick: Option<String>,
    /// A server-specific avatar, if they set one.
    avatar: Option<String>,
}

impl Discord {
    pub fn new(config: DiscordConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("ripae/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("could not build the Discord HTTP client")?;
        Ok(Self { config, http })
    }

    /// Where `[SIGN IN WITH DISCORD]` sends the browser.
    pub fn authorize_url(&self, state: &str) -> String {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("scope", SCOPES)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("state", state)
            // don't re-ask a member who already authorised Ripae
            .append_pair("prompt", "none")
            .finish();
        format!("{AUTHORIZE}?{query}")
    }

    /// Trades the callback's `code` for the member's profile. `Ok(None)`
    /// means Discord knows them but they aren't in the NISC server.
    pub async fn member(&self, code: &str) -> Result<Option<Profile>> {
        let token: TokenResponse = self
            .http
            .post(format!("{API}/oauth2/token"))
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.config.redirect_uri.as_str()),
            ])
            .send()
            .await?
            .error_for_status()
            .context("Discord refused the authorisation code")?
            .json()
            .await?;

        let user: DiscordUser = self
            .http
            .get(format!("{API}/users/@me"))
            .bearer_auth(&token.access_token)
            .send()
            .await?
            .error_for_status()
            .context("Discord would not say who signed in")?
            .json()
            .await?;

        let resp = self
            .http
            .get(format!("{API}/users/@me/guilds/{}/member", self.config.guild_id))
            .bearer_auth(&token.access_token)
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let member: GuildMember = resp
            .error_for_status()
            .context("Discord would not say whether they're in the server")?
            .json()
            .await?;

        Ok(Some(profile(&self.config.guild_id, user, member)))
    }
}

/// Server nickname over Discord display name over username; server avatar
/// over account avatar.
fn profile(guild_id: &str, user: DiscordUser, member: GuildMember) -> Profile {
    let display_name = [member.nick, user.global_name]
        .into_iter()
        .flatten()
        .map(|n| n.trim().to_string())
        .find(|n| !n.is_empty())
        .unwrap_or_else(|| user.username.clone());
    let avatar_url = match (member.avatar, user.avatar) {
        (Some(hash), _) => Some(format!("{CDN}/guilds/{guild_id}/users/{}/avatars/{hash}.png?size=128", user.id)),
        (None, Some(hash)) => Some(format!("{CDN}/avatars/{}/{hash}.png?size=128", user.id)),
        (None, None) => None,
    };
    Profile { discord_id: user.id, username: user.username, display_name, avatar_url }
}

/// The `users.id` this Discord member signs in as: the account already
/// linked to them, or a new one. Their handle, name and avatar are
/// refreshed from `p` either way. (Until 0.3.4 an account from before
/// Discord could be claimed with its password; every member has claimed
/// theirs, so that path is gone.)
pub async fn sign_in(pool: &SqlitePool, p: &Profile) -> Result<i64> {
    let owner: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE discord_id = ?")
        .bind(&p.discord_id)
        .fetch_optional(pool)
        .await?;
    let id = match owner {
        Some(o) => o,
        None => {
            let handle = free_handle(pool, p, None).await?;
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO users (handle, display_name, password_hash, discord_id, avatar_url) \
                 VALUES (?, ?, '', ?, ?) RETURNING id",
            )
            .bind(&handle)
            .bind(&p.display_name)
            .bind(&p.discord_id)
            .bind(&p.avatar_url)
            .fetch_one(pool)
            .await?;
            tracing::info!(user_id = id, %handle, "new crew member from Discord");
            return Ok(id);
        }
    };

    let handle = free_handle(pool, p, Some(id)).await?;
    sqlx::query("UPDATE users SET handle = ?, display_name = ?, avatar_url = ? WHERE id = ?")
        .bind(&handle)
        .bind(&p.display_name)
        .bind(&p.avatar_url)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(id)
}

/// Their Discord username, unless another account already has it as a
/// handle (an unclaimed seeded account, say): then the account keeps the
/// handle it has, or a new one gets the username plus the tail of their id.
async fn free_handle(pool: &SqlitePool, p: &Profile, id: Option<i64>) -> Result<String> {
    let taken = |h: String| async move {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE handle = ? AND id IS NOT ?)")
            .bind(h)
            .bind(id)
            .fetch_one(pool)
            .await
    };
    if !taken(p.username.clone()).await? {
        return Ok(p.username.clone());
    }
    if let Some(id) = id {
        return Ok(sqlx::query_scalar("SELECT handle FROM users WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?);
    }
    let tail = &p.discord_id[p.discord_id.len().saturating_sub(4)..];
    let suffixed = format!("{}-{tail}", p.username);
    if !taken(suffixed.clone()).await? {
        return Ok(suffixed);
    }
    Ok(format!("discord-{}", p.discord_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn pool() -> SqlitePool {
        let pool = db::connect(":memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        pool
    }

    async fn seeded(pool: &SqlitePool, handle: &str, name: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO users (handle, display_name, password_hash) VALUES (?, ?, 'x') RETURNING id")
            .bind(handle)
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn member(id: &str, username: &str, name: &str) -> Profile {
        Profile {
            discord_id: id.into(),
            username: username.into(),
            display_name: name.into(),
            avatar_url: Some(format!("https://cdn.example/{id}.png")),
        }
    }

    async fn row(pool: &SqlitePool, id: i64) -> (String, String, Option<String>, Option<String>) {
        sqlx::query_as("SELECT handle, display_name, discord_id, avatar_url FROM users WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[test]
    fn nickname_beats_display_name_beats_username() {
        let user = |g: Option<&str>, a: Option<&str>| DiscordUser {
            id: "42".into(),
            username: "lordgig".into(),
            global_name: g.map(Into::into),
            avatar: a.map(Into::into),
        };
        let m = |n: Option<&str>, a: Option<&str>| GuildMember { nick: n.map(Into::into), avatar: a.map(Into::into) };

        let p = profile("7", user(Some("Gig"), Some("u")), m(Some("Lord Gig"), Some("g")));
        assert_eq!(p.display_name, "Lord Gig");
        assert_eq!(p.avatar_url.as_deref(), Some("https://cdn.discordapp.com/guilds/7/users/42/avatars/g.png?size=128"));

        let p = profile("7", user(Some("Gig"), Some("u")), m(Some("  "), None));
        assert_eq!(p.display_name, "Gig");
        assert_eq!(p.avatar_url.as_deref(), Some("https://cdn.discordapp.com/avatars/42/u.png?size=128"));

        let p = profile("7", user(None, None), m(None, None));
        assert_eq!((p.display_name.as_str(), p.avatar_url), ("lordgig", None));
    }

    #[tokio::test]
    async fn first_sign_in_creates_a_member_and_the_next_finds_them() {
        let pool = pool().await;
        let id = sign_in(&pool, &member("100", "jt", "JT")).await.unwrap();
        assert_eq!(row(&pool, id).await, ("jt".into(), "JT".into(), Some("100".into()), Some("https://cdn.example/100.png".into())));

        // renamed in the server since: same account, new name
        let again = sign_in(&pool, &member("100", "jt", "First Officer JT")).await.unwrap();
        assert_eq!(again, id);
        assert_eq!(row(&pool, id).await.1, "First Officer JT");
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn sign_in_never_takes_over_an_unlinked_account() {
        let pool = pool().await;
        let old = seeded(&pool, "firstofficer", "JT").await;
        // same handle and name as the old account, still a new member
        let id = sign_in(&pool, &member("300", "firstofficer", "JT")).await.unwrap();
        assert_ne!(id, old);
        assert_eq!(row(&pool, old).await.2, None);
        assert_eq!(row(&pool, id).await.2, Some("300".into()));
    }

    #[tokio::test]
    async fn a_taken_handle_is_not_overwritten() {
        let pool = pool().await;
        let seeded_gig = seeded(&pool, "lordgig", "Lord Gig").await;
        // someone else whose Discord username is the unclaimed seeded handle
        let other = sign_in(&pool, &member("1234567", "lordgig", "Not Gig")).await.unwrap();
        assert_ne!(other, seeded_gig);
        assert_eq!(row(&pool, other).await.0, "lordgig-4567");
        assert_eq!(row(&pool, seeded_gig).await.0, "lordgig");

        // an existing account whose new username is taken keeps its handle
        let jt = sign_in(&pool, &member("9", "jt", "JT")).await.unwrap();
        sign_in(&pool, &member("9", "lordgig", "JT")).await.unwrap();
        assert_eq!(row(&pool, jt).await.0, "jt");
    }
}
