-- migrations/0008_load_bought_by.sql
-- `loads.submitted_by` is the buyer while a load is open but becomes the
-- seller once it's sold, so a sold row lost who bought it. `bought_by`
-- keeps the buyer for good. Open loads already know theirs; a sold row's
-- real buyer was never recorded, so it starts out as the voyage's owner
-- (who buys by default) and the Sale form can correct it.
ALTER TABLE loads ADD COLUMN bought_by INTEGER REFERENCES users(id);
UPDATE loads SET bought_by = submitted_by WHERE sold_at IS NULL;
UPDATE loads SET bought_by = (SELECT logged_by FROM voyages WHERE voyages.id = loads.voyage_id) WHERE sold_at IS NOT NULL;
