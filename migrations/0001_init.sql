-- migrations/0001_init.sql
CREATE TABLE users (
  id            INTEGER PRIMARY KEY,
  handle        TEXT NOT NULL UNIQUE,     -- e.g. 'lordgig'
  display_name  TEXT NOT NULL,            -- e.g. 'Lord Gig'
  password_hash TEXT NOT NULL,
  created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE runs (
  id             INTEGER PRIMARY KEY,
  submitted_by   INTEGER NOT NULL REFERENCES users(id),

  -- lifecycle
  status         TEXT NOT NULL DEFAULT 'in_transit'
                 CHECK (status IN ('in_transit','stored','sold')),

  -- purchase leg (always known)
  bought_at      TEXT NOT NULL,           -- ISO-8601, user-entered
  commodity      TEXT NOT NULL,           -- free text in alpha; e.g. 'Laranite'
  scu            REAL NOT NULL CHECK (scu > 0),
  buy_terminal   TEXT NOT NULL,           -- e.g. 'ArcCorp Mining Area 141'
  buy_price_scu  REAL NOT NULL CHECK (buy_price_scu >= 0),   -- aUEC per SCU

  -- intent & whereabouts (open cargo)
  destination    TEXT,                    -- where it is MEANT to be sold; may be NULL ("undecided")
  stored_at      TEXT,                    -- required when status = 'stored'; the station/hangar holding it
  ship           TEXT,                    -- optional; e.g. 'Hull A' -- the hull carrying it if in_transit

  -- sale leg (NULL until status = 'sold')
  sold_at        TEXT,
  sell_terminal  TEXT,                    -- where it ACTUALLY sold (may differ from destination)
  sell_price_scu REAL CHECK (sell_price_scu IS NULL OR sell_price_scu >= 0),
  scu_sold       REAL CHECK (scu_sold IS NULL OR scu_sold >= 0),  -- may be < scu (loss, seizure, partial)

  notes          TEXT,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now')),

  CHECK (status <> 'stored' OR stored_at IS NOT NULL),
  CHECK (status <> 'sold'   OR (sold_at IS NOT NULL AND sell_terminal IS NOT NULL
                                AND sell_price_scu IS NOT NULL AND scu_sold IS NOT NULL))
);

CREATE INDEX runs_status    ON runs(status);
CREATE INDEX runs_bought_at ON runs(bought_at DESC);
CREATE INDEX runs_commodity ON runs(commodity);
