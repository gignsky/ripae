-- Ships lookup, scraped from sc-cargo.space's bundle (index-Cy0P0zyR.js) on 2026-11-08.
-- `ships` already exists from 0002_lookups.sql (id, name, uex_id, created_at); this widens it.
-- sc_cargo_slug + '-official' (+ crate suffix) builds https://sc-cargo.space/#/viewer/...
-- Crate suffix: q w e r t y u = 1 2 4 8 16 24 32 SCU, each followed by its count, sizes in that order.
-- Parked under design/ until roll 8; move to migrations/ and renumber then.

ALTER TABLE ships ADD COLUMN manufacturer  TEXT;
ALTER TABLE ships ADD COLUMN short         TEXT;
ALTER TABLE ships ADD COLUMN sc_cargo_slug TEXT;
ALTER TABLE ships ADD COLUMN capacity_scu  INTEGER;
ALTER TABLE ships ADD COLUMN concept       INTEGER NOT NULL DEFAULT 0;

-- Insert the hulls we don't have yet, then fill the new columns for every row by name
-- (names collate NOCASE in 0002, so existing seeds such as 'Hull A' match 'Hull-A' only if spelled the same --
--  review the existing rows before running; rename old rows rather than ending up with two Hull As).
INSERT OR IGNORE INTO ships (name) VALUES
  ('Hammerhead'),
  ('Idris-M'),
  ('Idris-P'),
  ('Reclaimer'),
  ('Redeemer'),
  ('Tiburon'),
  ('Asgard'),
  ('C8 Pisces'),
  ('C8X Pisces'),
  ('Carrack'),
  ('Liberator [Concept]'),
  ('Paladin'),
  ('Valkyrie'),
  ('CSV-SM'),
  ('MOLE'),
  ('MOTH'),
  ('MPUV-C'),
  ('MPUV-T'),
  ('Raft'),
  ('SRV'),
  ('Mustang Alpha'),
  ('Nomad'),
  ('Pioneer [Concept]'),
  ('A2 Hercules'),
  ('C2 Hercules'),
  ('Intrepid'),
  ('M2 Hercules'),
  ('Caterpillar'),
  ('Clipper'),
  ('Corsair'),
  ('Cutter'),
  ('Cutter Rambler'),
  ('Cutter Scout'),
  ('Golem Ox'),
  ('Mule'),
  ('Vulture'),
  ('Prowler Utility'),
  ('Syulen'),
  ('Tyilui'),
  ('Shiv'),
  ('Fortune'),
  ('Freelancer'),
  ('Freelancer DUR'),
  ('Freelancer MAX'),
  ('Freelancer MIS'),
  ('Hull-A'),
  ('Hull-C'),
  ('Hull-D [Concept]'),
  ('Hull-E [Concept]'),
  ('Reliant Kore'),
  ('Reliant Tana'),
  ('Starfarer'),
  ('Starfarer Gemini'),
  ('Starlancer MAX'),
  ('Starlancer TAC'),
  ('100i'),
  ('125a'),
  ('135c'),
  ('300i'),
  ('315p'),
  ('325a'),
  ('350r'),
  ('400i'),
  ('600i Explorer'),
  ('600i Touring'),
  ('890 Jump'),
  ('M80'),
  ('Apollo Medivac'),
  ('Apollo Triage'),
  ('Arrastra [Concept]'),
  ('Galaxy [Base Concept]'),
  ('Galaxy [Cargo Concept]'),
  ('Hermes'),
  ('Perseus'),
  ('Polaris'),
  ('Salvation'),
  ('Zeus Mk II CL'),
  ('Zeus Mk II ES'),
  ('Zeus Mk II MR [Concept]'),
  ('Cyclone');

UPDATE ships SET manufacturer='Aegis', short='Hammerhead', sc_cargo_slug='hammerhead', capacity_scu=64, concept=0 WHERE name='Hammerhead';
UPDATE ships SET manufacturer='Aegis', short='Idris-M', sc_cargo_slug='idris-m', capacity_scu=1326, concept=0 WHERE name='Idris-M';
UPDATE ships SET manufacturer='Aegis', short='Idris-P', sc_cargo_slug='idris-p', capacity_scu=1374, concept=0 WHERE name='Idris-P';
UPDATE ships SET manufacturer='Aegis', short='Reclaimer', sc_cargo_slug='reclaimer', capacity_scu=420, concept=0 WHERE name='Reclaimer';
UPDATE ships SET manufacturer='Aegis', short='Redeemer', sc_cargo_slug='redeemer', capacity_scu=2, concept=0 WHERE name='Redeemer';
UPDATE ships SET manufacturer='Aegis', short='Tiburon', sc_cargo_slug='tiburon', capacity_scu=64, concept=0 WHERE name='Tiburon';
UPDATE ships SET manufacturer='Anvil', short='Asgard', sc_cargo_slug='asgard', capacity_scu=180, concept=0 WHERE name='Asgard';
UPDATE ships SET manufacturer='Anvil', short='C8 Pisces', sc_cargo_slug='c8 pisces', capacity_scu=4, concept=0 WHERE name='C8 Pisces';
UPDATE ships SET manufacturer='Anvil', short='C8X Pisces', sc_cargo_slug='c8x pisces', capacity_scu=4, concept=0 WHERE name='C8X Pisces';
UPDATE ships SET manufacturer='Anvil', short='Carrack', sc_cargo_slug='carrack', capacity_scu=456, concept=0 WHERE name='Carrack';
UPDATE ships SET manufacturer='Anvil', short='Liberator', sc_cargo_slug='liberator [concept]', capacity_scu=400, concept=1 WHERE name='Liberator [Concept]';
UPDATE ships SET manufacturer='Anvil', short='Paladin', sc_cargo_slug='paladin', capacity_scu=4, concept=0 WHERE name='Paladin';
UPDATE ships SET manufacturer='Anvil', short='Valkyrie', sc_cargo_slug='valkyrie', capacity_scu=90, concept=0 WHERE name='Valkyrie';
UPDATE ships SET manufacturer='Argo', short='CSV-SM', sc_cargo_slug='csv-sm', capacity_scu=4, concept=0 WHERE name='CSV-SM';
UPDATE ships SET manufacturer='Argo', short='MOLE', sc_cargo_slug='mole', capacity_scu=32, concept=0 WHERE name='MOLE';
UPDATE ships SET manufacturer='Argo', short='MOTH', sc_cargo_slug='moth', capacity_scu=224, concept=0 WHERE name='MOTH';
UPDATE ships SET manufacturer='Argo', short='MPUV-C', sc_cargo_slug='mpuv-c', capacity_scu=2, concept=0 WHERE name='MPUV-C';
UPDATE ships SET manufacturer='Argo', short='MPUV-T', sc_cargo_slug='mpuv-t', capacity_scu=16, concept=0 WHERE name='MPUV-T';
UPDATE ships SET manufacturer='Argo', short='Raft', sc_cargo_slug='raft', capacity_scu=192, concept=0 WHERE name='Raft';
UPDATE ships SET manufacturer='Argo', short='SRV', sc_cargo_slug='srv', capacity_scu=12, concept=0 WHERE name='SRV';
UPDATE ships SET manufacturer='Consolidated Outland', short='Mustang Alpha', sc_cargo_slug='mustang alpha', capacity_scu=4, concept=0 WHERE name='Mustang Alpha';
UPDATE ships SET manufacturer='Consolidated Outland', short='Nomad', sc_cargo_slug='nomad', capacity_scu=24, concept=0 WHERE name='Nomad';
UPDATE ships SET manufacturer='Consolidated Outland', short='Pioneer', sc_cargo_slug='pioneer [concept]', capacity_scu=1, concept=1 WHERE name='Pioneer [Concept]';
UPDATE ships SET manufacturer='Crusader', short='A2 Hercules', sc_cargo_slug='a2 hercules', capacity_scu=216, concept=0 WHERE name='A2 Hercules';
UPDATE ships SET manufacturer='Crusader', short='C2 Hercules', sc_cargo_slug='c2 hercules', capacity_scu=696, concept=0 WHERE name='C2 Hercules';
UPDATE ships SET manufacturer='Crusader', short='Intrepid', sc_cargo_slug='intrepid', capacity_scu=8, concept=0 WHERE name='Intrepid';
UPDATE ships SET manufacturer='Crusader', short='M2 Hercules', sc_cargo_slug='m2 hercules', capacity_scu=522, concept=0 WHERE name='M2 Hercules';
UPDATE ships SET manufacturer='Drake', short='Caterpillar', sc_cargo_slug='caterpillar', capacity_scu=576, concept=0 WHERE name='Caterpillar';
UPDATE ships SET manufacturer='Drake', short='Clipper', sc_cargo_slug='clipper', capacity_scu=12, concept=0 WHERE name='Clipper';
UPDATE ships SET manufacturer='Drake', short='Corsair', sc_cargo_slug='corsair', capacity_scu=72, concept=0 WHERE name='Corsair';
UPDATE ships SET manufacturer='Drake', short='Cutter', sc_cargo_slug='cutter', capacity_scu=4, concept=0 WHERE name='Cutter';
UPDATE ships SET manufacturer='Drake', short='Cutter Rambler', sc_cargo_slug='cutter rambler', capacity_scu=2, concept=0 WHERE name='Cutter Rambler';
UPDATE ships SET manufacturer='Drake', short='Cutter Scout', sc_cargo_slug='cutter scout', capacity_scu=2, concept=0 WHERE name='Cutter Scout';
UPDATE ships SET manufacturer='Drake', short='Golem Ox', sc_cargo_slug='golem ox', capacity_scu=64, concept=0 WHERE name='Golem Ox';
UPDATE ships SET manufacturer='Drake', short='Mule', sc_cargo_slug='mule', capacity_scu=1, concept=0 WHERE name='Mule';
UPDATE ships SET manufacturer='Drake', short='Vulture', sc_cargo_slug='vulture', capacity_scu=13, concept=0 WHERE name='Vulture';
UPDATE ships SET manufacturer='Esperia', short='Prowler Utility', sc_cargo_slug='prowler utility', capacity_scu=32, concept=0 WHERE name='Prowler Utility';
UPDATE ships SET manufacturer='Gatac', short='Syulen', sc_cargo_slug='syulen', capacity_scu=6, concept=0 WHERE name='Syulen';
UPDATE ships SET manufacturer='Gatac', short='Tyilui', sc_cargo_slug='tyilui', capacity_scu=96, concept=0 WHERE name='Tyilui';
UPDATE ships SET manufacturer='Grey''s', short='Shiv', sc_cargo_slug='shiv', capacity_scu=32, concept=0 WHERE name='Shiv';
UPDATE ships SET manufacturer='MISC', short='Fortune', sc_cargo_slug='fortune', capacity_scu=16, concept=0 WHERE name='Fortune';
UPDATE ships SET manufacturer='MISC', short='Freelancer', sc_cargo_slug='freelancer', capacity_scu=66, concept=0 WHERE name='Freelancer';
UPDATE ships SET manufacturer='MISC', short='Freelancer DUR', sc_cargo_slug='freelancer dur', capacity_scu=36, concept=0 WHERE name='Freelancer DUR';
UPDATE ships SET manufacturer='MISC', short='Freelancer MAX', sc_cargo_slug='freelancer max', capacity_scu=120, concept=0 WHERE name='Freelancer MAX';
UPDATE ships SET manufacturer='MISC', short='Freelancer MIS', sc_cargo_slug='freelancer mis', capacity_scu=36, concept=0 WHERE name='Freelancer MIS';
UPDATE ships SET manufacturer='MISC', short='Hull-A', sc_cargo_slug='hull-a', capacity_scu=64, concept=0 WHERE name='Hull-A';
UPDATE ships SET manufacturer='MISC', short='Hull-C', sc_cargo_slug='hull-c', capacity_scu=4608, concept=0 WHERE name='Hull-C';
UPDATE ships SET manufacturer='MISC', short='Hull-D', sc_cargo_slug='hull-d [concept]', capacity_scu=6912, concept=1 WHERE name='Hull-D [Concept]';
UPDATE ships SET manufacturer='MISC', short='Hull-E', sc_cargo_slug='hull-e [concept]', capacity_scu=12288, concept=1 WHERE name='Hull-E [Concept]';
UPDATE ships SET manufacturer='MISC', short='Reliant Kore', sc_cargo_slug='reliant kore', capacity_scu=6, concept=0 WHERE name='Reliant Kore';
UPDATE ships SET manufacturer='MISC', short='Reliant Tana', sc_cargo_slug='reliant tana', capacity_scu=1, concept=0 WHERE name='Reliant Tana';
UPDATE ships SET manufacturer='MISC', short='Starfarer', sc_cargo_slug='starfarer', capacity_scu=291, concept=0 WHERE name='Starfarer';
UPDATE ships SET manufacturer='MISC', short='Starfarer Gemini', sc_cargo_slug='starfarer gemini', capacity_scu=291, concept=0 WHERE name='Starfarer Gemini';
UPDATE ships SET manufacturer='MISC', short='Starlancer MAX', sc_cargo_slug='starlancer max', capacity_scu=224, concept=0 WHERE name='Starlancer MAX';
UPDATE ships SET manufacturer='MISC', short='Starlancer TAC', sc_cargo_slug='starlancer tac', capacity_scu=96, concept=0 WHERE name='Starlancer TAC';
UPDATE ships SET manufacturer='Origin', short='100i', sc_cargo_slug='100i', capacity_scu=2, concept=0 WHERE name='100i';
UPDATE ships SET manufacturer='Origin', short='125a', sc_cargo_slug='125a', capacity_scu=2, concept=0 WHERE name='125a';
UPDATE ships SET manufacturer='Origin', short='135c', sc_cargo_slug='135c', capacity_scu=6, concept=0 WHERE name='135c';
UPDATE ships SET manufacturer='Origin', short='300i', sc_cargo_slug='300i', capacity_scu=8, concept=0 WHERE name='300i';
UPDATE ships SET manufacturer='Origin', short='315p', sc_cargo_slug='315p', capacity_scu=12, concept=0 WHERE name='315p';
UPDATE ships SET manufacturer='Origin', short='325a', sc_cargo_slug='325a', capacity_scu=4, concept=0 WHERE name='325a';
UPDATE ships SET manufacturer='Origin', short='350r', sc_cargo_slug='350r', capacity_scu=4, concept=0 WHERE name='350r';
UPDATE ships SET manufacturer='Origin', short='400i', sc_cargo_slug='400i', capacity_scu=42, concept=0 WHERE name='400i';
UPDATE ships SET manufacturer='Origin', short='600i Explorer', sc_cargo_slug='600i explorer', capacity_scu=44, concept=0 WHERE name='600i Explorer';
UPDATE ships SET manufacturer='Origin', short='600i Touring', sc_cargo_slug='600i touring', capacity_scu=20, concept=0 WHERE name='600i Touring';
UPDATE ships SET manufacturer='Origin', short='890 Jump', sc_cargo_slug='890 jump', capacity_scu=388, concept=0 WHERE name='890 Jump';
UPDATE ships SET manufacturer='Origin', short='M80', sc_cargo_slug='m80', capacity_scu=2, concept=0 WHERE name='M80';
UPDATE ships SET manufacturer='RSI', short='Apollo Medivac', sc_cargo_slug='apollo medivac', capacity_scu=32, concept=0 WHERE name='Apollo Medivac';
UPDATE ships SET manufacturer='RSI', short='Apollo Triage', sc_cargo_slug='apollo triage', capacity_scu=32, concept=0 WHERE name='Apollo Triage';
UPDATE ships SET manufacturer='RSI', short='Arrastra', sc_cargo_slug='arrastra [concept]', capacity_scu=576, concept=1 WHERE name='Arrastra [Concept]';
UPDATE ships SET manufacturer='RSI', short='Galaxy', sc_cargo_slug='galaxy [base concept]', capacity_scu=64, concept=1 WHERE name='Galaxy [Base Concept]';
UPDATE ships SET manufacturer='RSI', short='Galaxy', sc_cargo_slug='galaxy [cargo concept]', capacity_scu=576, concept=1 WHERE name='Galaxy [Cargo Concept]';
UPDATE ships SET manufacturer='RSI', short='Hermes', sc_cargo_slug='hermes', capacity_scu=288, concept=0 WHERE name='Hermes';
UPDATE ships SET manufacturer='RSI', short='Perseus', sc_cargo_slug='perseus', capacity_scu=96, concept=0 WHERE name='Perseus';
UPDATE ships SET manufacturer='RSI', short='Polaris', sc_cargo_slug='polaris', capacity_scu=576, concept=0 WHERE name='Polaris';
UPDATE ships SET manufacturer='RSI', short='Salvation', sc_cargo_slug='salvation', capacity_scu=6, concept=0 WHERE name='Salvation';
UPDATE ships SET manufacturer='RSI', short='Zeus Mk II CL', sc_cargo_slug='zeus mk ii cl', capacity_scu=128, concept=0 WHERE name='Zeus Mk II CL';
UPDATE ships SET manufacturer='RSI', short='Zeus Mk II ES', sc_cargo_slug='zeus mk ii es', capacity_scu=32, concept=0 WHERE name='Zeus Mk II ES';
UPDATE ships SET manufacturer='RSI', short='Zeus Mk II MR', sc_cargo_slug='zeus mk ii mr [concept]', capacity_scu=16, concept=1 WHERE name='Zeus Mk II MR [Concept]';
UPDATE ships SET manufacturer='Tumbril', short='Cyclone', sc_cargo_slug='cyclone', capacity_scu=1, concept=0 WHERE name='Cyclone';
