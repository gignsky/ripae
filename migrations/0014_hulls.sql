-- migrations/0014_hulls.sql
-- The fleet: actual hulls the crew own, not ship models. `ships` stays the
-- model lookup (manufacturer, capacity, sc-cargo slug); a hull points at it
-- when its model is on the list (`ship_id`) and always carries the model's
-- name in `model` -- a name off the list is kept as typed (pick-lists are
-- suggestions). A member may own several hulls of one model.
--
-- One status at a time: planned (pledged / concept / on order), stored at
-- `location`, out (pulled out at `location`; only its owner can pull it, so
-- there is no pilot column), destroyed (claim pending). A hull carried in
-- another (`carried_in`) follows its carrier: its own status/location are
-- ignored while it rides, and taken over again when it is unloaded.
--
-- `loaner` marks a loaner; `loaner_for` the planned hull it is lent for.
-- `insurance_claim` is the claim cost in aUEC the owner fills in.
-- `source` / `external_ref` are the 0.4 hangar-import hook: NULL for hulls
-- typed in here; an import fills them so its rows can be reconciled later.
--
-- `voyages.hull_id` records which hull flies a voyage (NULL for every voyage
-- recorded before this, which keep working off `ship_id` as before).
CREATE TABLE hulls (
  id              INTEGER PRIMARY KEY,
  owner_id        INTEGER NOT NULL REFERENCES users(id),
  ship_id         INTEGER REFERENCES ships(id),
  model           TEXT NOT NULL,
  nickname        TEXT,
  kind            TEXT NOT NULL DEFAULT 'ship' CHECK (kind IN ('ship','vehicle')),
  status          TEXT NOT NULL DEFAULT 'stored' CHECK (status IN ('planned','stored','out','destroyed')),
  location        TEXT,
  carried_in      INTEGER REFERENCES hulls(id) ON DELETE SET NULL,
  loaner          INTEGER NOT NULL DEFAULT 0 CHECK (loaner IN (0,1)),
  loaner_for      INTEGER REFERENCES hulls(id) ON DELETE SET NULL,
  insurance_claim REAL CHECK (insurance_claim IS NULL OR insurance_claim >= 0),
  insurance_type  TEXT,
  notes           TEXT,
  source          TEXT,
  external_ref    TEXT,
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
  CHECK (carried_in IS NULL OR carried_in <> id),
  CHECK (loaner_for IS NULL OR loaner_for <> id)
);
CREATE INDEX hulls_owner ON hulls(owner_id);
CREATE INDEX hulls_carried_in ON hulls(carried_in);
CREATE UNIQUE INDEX hulls_external ON hulls(source, external_ref) WHERE external_ref IS NOT NULL;

ALTER TABLE voyages ADD COLUMN hull_id INTEGER REFERENCES hulls(id) ON DELETE SET NULL;
