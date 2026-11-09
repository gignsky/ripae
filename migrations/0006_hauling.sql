-- migrations/0006_hauling.sql
-- A load is either commodity cargo bought at a terminal ('trade') or the
-- boxes of a hauling contract ('haul'): picked up for free (buy_terminal is
-- the pickup, destination the drop-off) and paid for on delivery. A haul
-- costs nothing, so its "sale" is the contract payout and all of it is
-- profit. `reward` is what the contract promises -- the delivery form's
-- default -- and is never counted until the delivery is recorded.
ALTER TABLE loads ADD COLUMN kind TEXT NOT NULL DEFAULT 'trade' CHECK (kind IN ('trade','haul'));
ALTER TABLE loads ADD COLUMN reward REAL CHECK (reward IS NULL OR reward >= 0);
