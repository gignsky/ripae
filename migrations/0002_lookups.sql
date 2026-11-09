-- migrations/0002_lookups.sql
-- Pick-lists for commodities, terminals (every location field) and ships.
-- Since 0.2 the lists are *suggestions, not gates*: a name on the list is
-- stored with the list's spelling, a name off the list is stored verbatim
-- (ruled 8 Nov 2026; the 0.4 UEX import reconciles names). `uex_id` is the
-- 0.4 hook: match by name, fill it in.
--
-- `ships` carries the sc-cargo.space columns from the start: 80 hulls
-- scraped from the site's bundle (index-Cy0P0zyR.js) on 8 Nov 2026, see
-- design/ships-sc-cargo.csv. `sc_cargo_slug` + '-official' (+ crate suffix)
-- builds https://sc-cargo.space/#/viewer/... ; `short` is the display name
-- without the [Concept] tag. Spellings are the CSV's (`Hull-C`, hyphenated)
-- so the slug and the display agree.

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
  id            INTEGER PRIMARY KEY,
  manufacturer  TEXT NOT NULL,
  name          TEXT NOT NULL UNIQUE COLLATE NOCASE,
  short         TEXT NOT NULL,
  sc_cargo_slug TEXT NOT NULL,
  capacity_scu  INTEGER NOT NULL,
  concept       INTEGER NOT NULL DEFAULT 0,
  uex_id        INTEGER UNIQUE,
  created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Starter lists (hand-written in 0.1.1; review against the game / UEX).
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

-- All 80 hulls from design/ships-sc-cargo.csv (generated; keep in sync).
INSERT INTO ships (manufacturer, name, short, sc_cargo_slug, capacity_scu, concept) VALUES
  ('Aegis', 'Hammerhead', 'Hammerhead', 'hammerhead', 64, 0),
  ('Aegis', 'Idris-M', 'Idris-M', 'idris-m', 1326, 0),
  ('Aegis', 'Idris-P', 'Idris-P', 'idris-p', 1374, 0),
  ('Aegis', 'Reclaimer', 'Reclaimer', 'reclaimer', 420, 0),
  ('Aegis', 'Redeemer', 'Redeemer', 'redeemer', 2, 0),
  ('Aegis', 'Tiburon', 'Tiburon', 'tiburon', 64, 0),
  ('Anvil', 'Asgard', 'Asgard', 'asgard', 180, 0),
  ('Anvil', 'C8 Pisces', 'C8 Pisces', 'c8 pisces', 4, 0),
  ('Anvil', 'C8X Pisces', 'C8X Pisces', 'c8x pisces', 4, 0),
  ('Anvil', 'Carrack', 'Carrack', 'carrack', 456, 0),
  ('Anvil', 'Liberator [Concept]', 'Liberator', 'liberator [concept]', 400, 1),
  ('Anvil', 'Paladin', 'Paladin', 'paladin', 4, 0),
  ('Anvil', 'Valkyrie', 'Valkyrie', 'valkyrie', 90, 0),
  ('Argo', 'CSV-SM', 'CSV-SM', 'csv-sm', 4, 0),
  ('Argo', 'MOLE', 'MOLE', 'mole', 32, 0),
  ('Argo', 'MOTH', 'MOTH', 'moth', 224, 0),
  ('Argo', 'MPUV-C', 'MPUV-C', 'mpuv-c', 2, 0),
  ('Argo', 'MPUV-T', 'MPUV-T', 'mpuv-t', 16, 0),
  ('Argo', 'Raft', 'Raft', 'raft', 192, 0),
  ('Argo', 'SRV', 'SRV', 'srv', 12, 0),
  ('Consolidated Outland', 'Mustang Alpha', 'Mustang Alpha', 'mustang alpha', 4, 0),
  ('Consolidated Outland', 'Nomad', 'Nomad', 'nomad', 24, 0),
  ('Consolidated Outland', 'Pioneer [Concept]', 'Pioneer', 'pioneer [concept]', 1, 1),
  ('Crusader', 'A2 Hercules', 'A2 Hercules', 'a2 hercules', 216, 0),
  ('Crusader', 'C2 Hercules', 'C2 Hercules', 'c2 hercules', 696, 0),
  ('Crusader', 'Intrepid', 'Intrepid', 'intrepid', 8, 0),
  ('Crusader', 'M2 Hercules', 'M2 Hercules', 'm2 hercules', 522, 0),
  ('Drake', 'Caterpillar', 'Caterpillar', 'caterpillar', 576, 0),
  ('Drake', 'Clipper', 'Clipper', 'clipper', 12, 0),
  ('Drake', 'Corsair', 'Corsair', 'corsair', 72, 0),
  ('Drake', 'Cutter', 'Cutter', 'cutter', 4, 0),
  ('Drake', 'Cutter Rambler', 'Cutter Rambler', 'cutter rambler', 2, 0),
  ('Drake', 'Cutter Scout', 'Cutter Scout', 'cutter scout', 2, 0),
  ('Drake', 'Golem Ox', 'Golem Ox', 'golem ox', 64, 0),
  ('Drake', 'Mule', 'Mule', 'mule', 1, 0),
  ('Drake', 'Vulture', 'Vulture', 'vulture', 13, 0),
  ('Esperia', 'Prowler Utility', 'Prowler Utility', 'prowler utility', 32, 0),
  ('Gatac', 'Syulen', 'Syulen', 'syulen', 6, 0),
  ('Gatac', 'Tyilui', 'Tyilui', 'tyilui', 96, 0),
  ('Grey''s', 'Shiv', 'Shiv', 'shiv', 32, 0),
  ('MISC', 'Fortune', 'Fortune', 'fortune', 16, 0),
  ('MISC', 'Freelancer', 'Freelancer', 'freelancer', 66, 0),
  ('MISC', 'Freelancer DUR', 'Freelancer DUR', 'freelancer dur', 36, 0),
  ('MISC', 'Freelancer MAX', 'Freelancer MAX', 'freelancer max', 120, 0),
  ('MISC', 'Freelancer MIS', 'Freelancer MIS', 'freelancer mis', 36, 0),
  ('MISC', 'Hull-A', 'Hull-A', 'hull-a', 64, 0),
  ('MISC', 'Hull-C', 'Hull-C', 'hull-c', 4608, 0),
  ('MISC', 'Hull-D [Concept]', 'Hull-D', 'hull-d [concept]', 6912, 1),
  ('MISC', 'Hull-E [Concept]', 'Hull-E', 'hull-e [concept]', 12288, 1),
  ('MISC', 'Reliant Kore', 'Reliant Kore', 'reliant kore', 6, 0),
  ('MISC', 'Reliant Tana', 'Reliant Tana', 'reliant tana', 1, 0),
  ('MISC', 'Starfarer', 'Starfarer', 'starfarer', 291, 0),
  ('MISC', 'Starfarer Gemini', 'Starfarer Gemini', 'starfarer gemini', 291, 0),
  ('MISC', 'Starlancer MAX', 'Starlancer MAX', 'starlancer max', 224, 0),
  ('MISC', 'Starlancer TAC', 'Starlancer TAC', 'starlancer tac', 96, 0),
  ('Origin', '100i', '100i', '100i', 2, 0),
  ('Origin', '125a', '125a', '125a', 2, 0),
  ('Origin', '135c', '135c', '135c', 6, 0),
  ('Origin', '300i', '300i', '300i', 8, 0),
  ('Origin', '315p', '315p', '315p', 12, 0),
  ('Origin', '325a', '325a', '325a', 4, 0),
  ('Origin', '350r', '350r', '350r', 4, 0),
  ('Origin', '400i', '400i', '400i', 42, 0),
  ('Origin', '600i Explorer', '600i Explorer', '600i explorer', 44, 0),
  ('Origin', '600i Touring', '600i Touring', '600i touring', 20, 0),
  ('Origin', '890 Jump', '890 Jump', '890 jump', 388, 0),
  ('Origin', 'M80', 'M80', 'm80', 2, 0),
  ('RSI', 'Apollo Medivac', 'Apollo Medivac', 'apollo medivac', 32, 0),
  ('RSI', 'Apollo Triage', 'Apollo Triage', 'apollo triage', 32, 0),
  ('RSI', 'Arrastra [Concept]', 'Arrastra', 'arrastra [concept]', 576, 1),
  ('RSI', 'Galaxy [Base Concept]', 'Galaxy', 'galaxy [base concept]', 64, 1),
  ('RSI', 'Galaxy [Cargo Concept]', 'Galaxy', 'galaxy [cargo concept]', 576, 1),
  ('RSI', 'Hermes', 'Hermes', 'hermes', 288, 0),
  ('RSI', 'Perseus', 'Perseus', 'perseus', 96, 0),
  ('RSI', 'Polaris', 'Polaris', 'polaris', 576, 0),
  ('RSI', 'Salvation', 'Salvation', 'salvation', 6, 0),
  ('RSI', 'Zeus Mk II CL', 'Zeus Mk II CL', 'zeus mk ii cl', 128, 0),
  ('RSI', 'Zeus Mk II ES', 'Zeus Mk II ES', 'zeus mk ii es', 32, 0),
  ('RSI', 'Zeus Mk II MR [Concept]', 'Zeus Mk II MR', 'zeus mk ii mr [concept]', 16, 1),
  ('Tumbril', 'Cyclone', 'Cyclone', 'cyclone', 1, 0);
