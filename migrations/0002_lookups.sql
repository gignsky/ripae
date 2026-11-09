-- migrations/0002_lookups.sql
-- Fixed pick-lists for commodities, terminals (every location field), and
-- ships. `runs` keeps its plain-text columns (they double as price
-- observations, §2); the app only accepts names that exist here and stores
-- this table's spelling. `uex_id` is the 0.4 reconcile hook: match by name,
-- fill it in. Renaming a row here does NOT rewrite past runs -- a reconcile
-- migration must `UPDATE runs SET <col> = new WHERE <col> = old` alongside it.

CREATE TABLE commodities (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  uex_id     INTEGER UNIQUE,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE terminals (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  uex_id     INTEGER UNIQUE,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE ships (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  uex_id     INTEGER UNIQUE,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Starter lists (hand-written; review against the game / UEX).
INSERT INTO commodities (name) VALUES
  ('Agricium'), ('Agricultural Supplies'), ('Aluminum'), ('Astatine'),
  ('Beryl'), ('Bexalite'), ('Borase'), ('Chlorine'), ('Construction Materials'),
  ('Copper'), ('Corundum'), ('Diamond'), ('Distilled Spirits'), ('Fluorine'),
  ('Gold'), ('Hephaestanite'), ('Hydrogen'), ('Inert Materials'), ('Iodine'),
  ('Iron'), ('Laranite'), ('Medical Supplies'), ('Processed Food'),
  ('Quantainium'), ('Quartz'), ('Recycled Material Composite'), ('Scrap'),
  ('Silicon'), ('Stims'), ('Taranite'), ('Tin'), ('Titanium'), ('Tungsten'),
  ('Waste');

INSERT INTO terminals (name) VALUES
  ('Area18 TDD'), ('Lorville TDD'), ('New Babbage TDD'), ('Orison TDD'),
  ('Levski'), ('Grim HEX'),
  ('Everus Harbor'), ('Baijini Point'), ('Port Tressler'), ('Seraphim Station'),
  ('ARC-L1 Wide Forest Station'), ('CRU-L1 Ambitious Dream Station'),
  ('HUR-L1 Green Glade Station'), ('HUR-L2 Faithful Dream Station'),
  ('MIC-L1 Shallow Frontier Station'),
  ('ArcCorp Mining Area 045'), ('ArcCorp Mining Area 141'),
  ('Shubin Mining Facility SCD-1');

INSERT INTO ships (name) VALUES
  ('Avenger Titan'), ('C2 Hercules'), ('Caterpillar'), ('Constellation Andromeda'),
  ('Constellation Taurus'), ('Corsair'), ('Cutlass Black'), ('Freelancer MAX'),
  ('Hull A'), ('Hull C'), ('Ironclad'), ('M2 Hercules'), ('Mercury Star Runner'),
  ('Nomad'), ('RAFT'), ('Starlancer MAX'), ('Zeus Mk II CL');

-- Empty optional fields were stored as '' rather than NULL; normalise them
-- so they don't become blank list entries. (stored_at stays required while
-- status = 'stored', per the runs CHECK.)
UPDATE runs SET destination = NULL WHERE trim(destination) = '';
UPDATE runs SET ship        = NULL WHERE trim(ship) = '';
UPDATE runs SET notes       = NULL WHERE trim(notes) = '';
UPDATE runs SET stored_at   = NULL WHERE trim(stored_at) = '' AND status <> 'stored';

-- Backfill from what's already been logged so no existing run is orphaned.
INSERT OR IGNORE INTO commodities (name)
  SELECT DISTINCT trim(commodity) FROM runs WHERE trim(commodity) <> '';

INSERT OR IGNORE INTO terminals (name)
  SELECT trim(buy_terminal)  FROM runs WHERE trim(buy_terminal) <> ''
  UNION SELECT trim(sell_terminal) FROM runs WHERE trim(sell_terminal) <> ''
  UNION SELECT trim(destination)   FROM runs WHERE trim(destination) <> ''
  UNION SELECT trim(stored_at)     FROM runs WHERE trim(stored_at) <> '';

INSERT OR IGNORE INTO ships (name)
  SELECT DISTINCT trim(ship) FROM runs WHERE trim(ship) <> '';
