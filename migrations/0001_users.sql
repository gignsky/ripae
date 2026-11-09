-- migrations/0001_users.sql
-- 0.2 "Stowage" starts the schema over (ruled 8 Nov 2026: the 0.1 database
-- is wiped on deploy, no data migration). `users` is exactly as 0.1 had it,
-- so the seeding code is unchanged.
CREATE TABLE users (
  id            INTEGER PRIMARY KEY,
  handle        TEXT NOT NULL UNIQUE,     -- e.g. 'lordgig'
  display_name  TEXT NOT NULL,            -- e.g. 'Lord Gig'
  password_hash TEXT NOT NULL,
  created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
