-- migrations/0012_voyage_crew.sql
-- A voyage's crew: who flew it, and the terms every sale on it splits by.
-- Replaces `voyage_shares` (0007), which only pre-filled each close form
-- and could be changed per sale; now the crew's terms are the one place a
-- split is set. What a sale actually paid stays frozen in `shares` -- this
-- migration doesn't touch that table, so no settled amount changes.
--
-- One row per member per voyage. A member may hold a cut of each sale's
-- profit (`pct`), a flat fee per sale (`flat`), both, or neither (crew
-- without a share). `paid_to` forfeits the aUEC to another member while
-- the credit stays with `user_id`; NULL = they keep it.
CREATE TABLE voyage_crew (
  id         INTEGER PRIMARY KEY,
  voyage_id  INTEGER NOT NULL REFERENCES voyages(id) ON DELETE CASCADE,
  user_id    INTEGER NOT NULL REFERENCES users(id),
  pct        REAL NOT NULL DEFAULT 0 CHECK (pct >= 0),
  flat       REAL NOT NULL DEFAULT 0 CHECK (flat >= 0),
  paid_to    INTEGER REFERENCES users(id),
  reason     TEXT,
  UNIQUE (voyage_id, user_id)
);

CREATE INDEX voyage_crew_voyage ON voyage_crew(voyage_id);

-- 1. Voyage-level shares carry over as they were. A member listed more
--    than once has their rows summed by kind (25 % + 1,000 flat stays
--    25 % + 1,000 flat); the first forfeit and every reason are kept.
INSERT INTO voyage_crew (voyage_id, user_id, pct, flat, paid_to, reason)
SELECT vs.voyage_id, vs.to_user,
  SUM(CASE WHEN vs.kind = 'pct' THEN vs.amount ELSE 0 END),
  SUM(CASE WHEN vs.kind = 'flat' THEN vs.amount ELSE 0 END),
  (SELECT f.paid_to FROM voyage_shares f
    WHERE f.voyage_id = vs.voyage_id AND f.to_user = vs.to_user AND f.paid_to <> f.to_user
    ORDER BY f.id LIMIT 1),
  (SELECT group_concat(r.reason, ' · ') FROM (
     SELECT reason FROM voyage_shares r0
      WHERE r0.voyage_id = vs.voyage_id AND r0.to_user = vs.to_user AND TRIM(COALESCE(r0.reason, '')) <> ''
      ORDER BY r0.id) r)
FROM voyage_shares vs
GROUP BY vs.voyage_id, vs.to_user;

-- 2. Members who only held shares on individual sales join the crew on
--    the terms of their latest sale on that voyage (the split the crew was
--    last flying by), summed by kind the same way.
INSERT INTO voyage_crew (voyage_id, user_id, pct, flat, paid_to, reason)
SELECT x.voyage_id, x.to_user,
  (SELECT COALESCE(SUM(s.amount), 0) FROM shares s WHERE s.load_id = x.load_id AND s.to_user = x.to_user AND s.kind = 'pct'),
  (SELECT COALESCE(SUM(s.amount), 0) FROM shares s WHERE s.load_id = x.load_id AND s.to_user = x.to_user AND s.kind = 'flat'),
  (SELECT s.paid_to FROM shares s
    WHERE s.load_id = x.load_id AND s.to_user = x.to_user AND s.paid_to <> s.to_user
    ORDER BY s.id LIMIT 1),
  (SELECT group_concat(r.reason, ' · ') FROM (
     SELECT reason FROM shares r0
      WHERE r0.load_id = x.load_id AND r0.to_user = x.to_user AND TRIM(COALESCE(r0.reason, '')) <> ''
      ORDER BY r0.id) r)
FROM (
  SELECT l.voyage_id, sh.to_user,
    (SELECT sh2.load_id FROM shares sh2 JOIN loads l2 ON l2.id = sh2.load_id
      WHERE l2.voyage_id = l.voyage_id AND sh2.to_user = sh.to_user
      ORDER BY l2.sold_at DESC, l2.id DESC LIMIT 1) AS load_id
  FROM shares sh JOIN loads l ON l.id = sh.load_id
  GROUP BY l.voyage_id, sh.to_user
) x
WHERE NOT EXISTS (SELECT 1 FROM voyage_crew c WHERE c.voyage_id = x.voyage_id AND c.user_id = x.to_user);

-- 3. Everyone else the voyage names flew it too, without a share: its
--    owner, every load's buyer and holder (the seller, on a sold row), and
--    whoever a forfeited share was paid to. The app keeps this true from
--    here on (repo::sync_crew).
INSERT OR IGNORE INTO voyage_crew (voyage_id, user_id)
SELECT id, logged_by FROM voyages;
INSERT OR IGNORE INTO voyage_crew (voyage_id, user_id)
SELECT voyage_id, submitted_by FROM loads ORDER BY id;
INSERT OR IGNORE INTO voyage_crew (voyage_id, user_id)
SELECT voyage_id, bought_by FROM loads WHERE bought_by IS NOT NULL ORDER BY id;
INSERT OR IGNORE INTO voyage_crew (voyage_id, user_id)
SELECT l.voyage_id, sh.paid_to FROM shares sh JOIN loads l ON l.id = sh.load_id ORDER BY sh.id;
INSERT OR IGNORE INTO voyage_crew (voyage_id, user_id)
SELECT voyage_id, paid_to FROM voyage_shares ORDER BY id;

DROP TABLE voyage_shares;
