-- migrations/0007_voyage_shares.sql
-- Profit shares set once for a whole voyage. They're the starting point for
-- every load closed on it -- each close form comes up with them filled in,
-- still editable -- and are never paid out themselves: what's paid lives in
-- `shares`, frozen per sale. Same columns as `shares` minus `paid_auec`.
CREATE TABLE voyage_shares (
  id         INTEGER PRIMARY KEY,
  voyage_id  INTEGER NOT NULL REFERENCES voyages(id) ON DELETE CASCADE,
  to_user    INTEGER NOT NULL REFERENCES users(id),
  paid_to    INTEGER NOT NULL REFERENCES users(id),
  kind       TEXT NOT NULL CHECK (kind IN ('pct','flat')),
  amount     REAL NOT NULL CHECK (amount >= 0),
  reason     TEXT
);

CREATE INDEX voyage_shares_voyage ON voyage_shares(voyage_id);
