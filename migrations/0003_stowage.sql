-- migrations/0003_stowage.sql -- RIPAE-0.2.md §4, verbatim.
-- A run is a voyage carrying one or more loads; each load is made of crates.
-- Where a crate is lives on the crate; a load's status is derived in Rust
-- (never stored). Every load is still a price observation (commodity,
-- terminal, price per SCU, date, submitter) -- the per-lot price is stored
-- alongside the derived per-SCU price, never instead of it.

CREATE TABLE voyages (
  id          INTEGER PRIMARY KEY,
  logged_by   INTEGER NOT NULL REFERENCES users(id),
  ship_id     INTEGER REFERENCES ships(id),        -- NULL = no hull yet
  bought_at   TEXT NOT NULL,                       -- real calendar date, ISO-8601
  notes       TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE loads (
  id             INTEGER PRIMARY KEY,
  voyage_id      INTEGER NOT NULL REFERENCES voyages(id),   -- merge/detach = UPDATE this
  submitted_by   INTEGER NOT NULL REFERENCES users(id),     -- on a sold row: who closed it
  -- no status column: derived from crates (see §4 derived figures)
  commodity      TEXT NOT NULL,
  buy_terminal   TEXT NOT NULL,
  price_mode     TEXT NOT NULL CHECK (price_mode IN ('scu','lot')),
  buy_price_scu  REAL NOT NULL CHECK (buy_price_scu >= 0),   -- always stored (derived if mode = lot)
  buy_price_lot  REAL CHECK (buy_price_lot IS NULL OR buy_price_lot >= 0),
  stop_no        INTEGER NOT NULL DEFAULT 1 CHECK (stop_no >= 1),
  destination    TEXT,
  sold_at        TEXT, sell_terminal TEXT,
  sell_price_scu REAL CHECK (sell_price_scu IS NULL OR sell_price_scu >= 0),
  sell_price_lot REAL,
  notes          TEXT,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
  CHECK (sold_at IS NULL OR (sell_terminal IS NOT NULL AND sell_price_scu IS NOT NULL))
);

CREATE TABLE crates (
  id         INTEGER PRIMARY KEY,
  load_id    INTEGER NOT NULL REFERENCES loads(id) ON DELETE CASCADE,
  size_scu   INTEGER NOT NULL CHECK (size_scu IN (1,2,4,8,16,24,32)),
  condition  TEXT NOT NULL DEFAULT 'sound' CHECK (condition IN ('sound','glitched','lost','sold')),
  whereabouts TEXT NOT NULL DEFAULT 'aboard' CHECK (whereabouts IN ('awaiting_pickup','aboard','stored')),
  stored_at  TEXT,                                 -- station, when whereabouts = 'stored'
  sold_at    TEXT,                                 -- set when condition = 'sold'
  CHECK (whereabouts <> 'stored' OR stored_at IS NOT NULL)
);

CREATE TABLE shares (
  id         INTEGER PRIMARY KEY,
  load_id    INTEGER NOT NULL REFERENCES loads(id) ON DELETE CASCADE,
  to_user    INTEGER NOT NULL REFERENCES users(id),   -- who EARNED it (leaderboard credit)
  paid_to    INTEGER NOT NULL REFERENCES users(id),   -- who RECEIVES the aUEC; = to_user unless forfeited
  kind       TEXT NOT NULL CHECK (kind IN ('pct','flat')),
  amount     REAL NOT NULL CHECK (amount >= 0),       -- percent (0-100) or aUEC
  reason     TEXT,
  paid_auec  REAL NOT NULL                            -- frozen at close; 0 on a losing sale (both kinds)
);

CREATE INDEX loads_voyage ON loads(voyage_id);
-- §4 also listed `CREATE INDEX loads_status ON loads(status)`; there is no
-- status column (it is derived), so that line was the document's error.
CREATE INDEX crates_load  ON crates(load_id);
CREATE INDEX shares_load  ON shares(load_id);
