-- migrations/0009_discord.sql
-- 0.3: members sign in with Discord (the NISC server is the door). A
-- member's Discord account is `discord_id` (the snowflake, as text); the
-- handle, display name and avatar are refreshed from Discord on every
-- sign-in. Accounts created through Discord have no password, so their
-- `password_hash` is '' (column stays NOT NULL; '' never verifies). The
-- seeded accounts keep theirs until claimed: a password sign-in of an
-- unlinked account goes straight on to Discord to link it.
ALTER TABLE users ADD COLUMN discord_id TEXT;
ALTER TABLE users ADD COLUMN avatar_url TEXT;
CREATE UNIQUE INDEX users_discord_id ON users(discord_id);
