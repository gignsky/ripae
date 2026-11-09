-- migrations/0005_voyage_entered_by.sql
-- `voyages.logged_by` is whose run it is (the crew member picked on New
-- Run: the default buyer and seller). Who actually typed it in -- the
-- bookkeeper, shown as "Logged by" -- wasn't recorded. Existing voyages
-- start out as their owner's; the voyage Edit form can correct them.
ALTER TABLE voyages ADD COLUMN entered_by INTEGER REFERENCES users(id);
UPDATE voyages SET entered_by = logged_by;
