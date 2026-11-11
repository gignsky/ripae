-- migrations/0011_voyage_routes.sql
-- A voyage flies a route: a starting location (seq 0) and an ordered list
-- of stops (seq 1..n), each a terminal. Every load comes aboard at one stop
-- (`pickup_stop`) and comes off at a later one (`drop_stop`; NULL while its
-- destination is undecided). `at_stop` is the stop the voyage last reached.
--
-- `loads.buy_terminal` and `loads.destination` stay as they are -- they are
-- the price observation and what the ledger, the board and the clustering
-- read -- and are kept equal to the route's terminals by every write.
-- `loads.stop_no` is no longer read (drop_stop replaces it) but is kept, so
-- nothing already recorded is lost.
--
-- Existing voyages get their route from their loads in Rust, right after
-- the migrations run (`stowage::repo::backfill_routes`, idempotent: it only
-- touches voyages that have no stops yet): the start is the first load's
-- buy terminal, then any other buy terminal that no load is bound for, then
-- the destinations in their old stop № order. See model::derive_route.
CREATE TABLE voyage_stops (
  id         INTEGER PRIMARY KEY,
  voyage_id  INTEGER NOT NULL REFERENCES voyages(id) ON DELETE CASCADE,
  seq        INTEGER NOT NULL CHECK (seq >= 0),   -- 0 = where the voyage starts
  terminal   TEXT NOT NULL,                       -- '' only for an unknown start
  UNIQUE (voyage_id, seq)
);

ALTER TABLE voyages ADD COLUMN at_stop INTEGER NOT NULL DEFAULT 0 CHECK (at_stop >= 0);
ALTER TABLE loads ADD COLUMN pickup_stop INTEGER NOT NULL DEFAULT 0 CHECK (pickup_stop >= 0);
ALTER TABLE loads ADD COLUMN drop_stop INTEGER CHECK (drop_stop IS NULL OR drop_stop >= 1);
