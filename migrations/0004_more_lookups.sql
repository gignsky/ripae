-- migrations/0004_more_lookups.sql
-- Pick-list names missing from 0002, added on 9 Nov 2026. 0002 is
-- left untouched so its sqlx checksum still matches deployed DBs.
--
-- Ships: missing from the 8 Nov 2026 sc-cargo.space scrape; capacities from
-- starcitizen.tools / Drake marketing. Slugs follow the CSV convention
-- (lowercase name); sc-cargo.space may not have viewers for these yet.

INSERT OR IGNORE INTO ships (manufacturer, name, short, sc_cargo_slug, capacity_scu, concept) VALUES
  ('Drake', 'Ironclad', 'Ironclad', 'ironclad', 2204, 0),
  ('Drake', 'Ironclad Assault', 'Ironclad Assault', 'ironclad assault', 1440, 0),
  ('RSI', 'Centaurus', 'Centaurus', 'centaurus', 240, 0);

-- Terminals: every Stanton commodity terminal UEX lists as live
-- (api.uexcorp.space/2.0/terminals?id_star_system=68, types commodity and
-- commodity_raw, fetched 9 Nov 2026), by UEX display name. The four
-- landing-zone cities are skipped: 0002 already has them as "<city> TDD".
-- Names already in 0002 are skipped by OR IGNORE (NOCASE).
INSERT OR IGNORE INTO terminals (name) VALUES
  ('ArcCorp Mining Area 045'),
  ('ArcCorp Mining Area 048'),
  ('ArcCorp Mining Area 056'),
  ('ArcCorp Mining Area 061'),
  ('ArcCorp Mining Area 141'),
  ('ArcCorp Mining Area 157'),
  ('ARC-L1 Wide Forest Station'),
  ('ARC-L2 Lively Pathway Station'),
  ('ARC-L3 Modern Express Station'),
  ('ARC-L4 Faint Glen Station'),
  ('ARC-L5 Yellow Core Station'),
  ('Baijini Point'),
  ('Brio''s Breaker Yard'),
  ('CRU-L1 Ambitious Dream Station'),
  ('CRU-L4 Shallow Fields Station'),
  ('CRU-L5 Beautiful Glen Station'),
  ('Deakins Research Outpost'),
  ('Devlin Scrap & Salvage'),
  ('Dunboro'),
  ('Everus Harbor'),
  ('Grim HEX'),
  ('HDMS-Anderson'),
  ('HDMS-Bezdek'),
  ('HDMS-Edmond'),
  ('HDMS-Hadley'),
  ('HDMS-Hahn'),
  ('HDMS-Lathan'),
  ('HDMS-Norgaard'),
  ('HDMS-Oparei'),
  ('HDMS-Perlman'),
  ('HDMS-Pinewood'),
  ('HDMS-Ryder'),
  ('HDMS-Stanhope'),
  ('HDMS-Thedus'),
  ('HDMS-Woodruff'),
  ('Hickes Research Outpost'),
  ('HUR-L1 Green Glade Station'),
  ('HUR-L2 Faithful Dream Station'),
  ('HUR-L3 Thundering Express Station'),
  ('HUR-L4 Melodic Fields Station'),
  ('HUR-L5 High Course Station'),
  ('MIC-L1 Shallow Frontier Station'),
  ('MIC-L2 Long Forest Station'),
  ('MIC-L3 Endless Odyssey Station'),
  ('MIC-L4 Red Crossroads Station'),
  ('MIC-L5 Modern Icarus Station'),
  ('Nyx Gateway'),
  ('Picker''s Field'),
  ('Port Tressler'),
  ('Pyro Gateway'),
  ('Rappel'),
  ('Rayari Anvik Research Outpost'),
  ('Rayari Cantwell Research Outpost'),
  ('Rayari Deltana Research Outpost'),
  ('Rayari Kaltag Research Outpost'),
  ('Rayari McGrath Research Outpost'),
  ('Reclamation & Disposal Orinth'),
  ('Samson & Son''s Salvage Center'),
  ('Seraphim Station'),
  ('Shubin Mining Facility SAL-2'),
  ('Shubin Mining Facility SAL-5'),
  ('Shubin Mining Facility SCD-1'),
  ('Shubin Mining Facility SM0-10'),
  ('Shubin Mining Facility SM0-13'),
  ('Shubin Mining Facility SM0-18'),
  ('Shubin Mining Facility SM0-22'),
  ('Shubin Mining Facility SMCa-6'),
  ('Shubin Mining Facility SMCa-8'),
  ('Terra Gateway'),
  ('Terra Mills HydroFarm');
