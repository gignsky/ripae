-- migrations/0003_real_dates.sql
-- Dates are now real-world (YYYY-MM-DD from <input type="date">). Convert
-- any in-game SC dates already logged (SC year = real year + 930, same
-- month/day) so they sort alongside new entries. Only rewrites values that
-- start with a 29xx year; reverse with +930 if ever needed.

UPDATE runs
   SET bought_at = printf('%04d', CAST(substr(bought_at, 1, 4) AS INTEGER) - 930)
                   || substr(bought_at, 5)
 WHERE bought_at GLOB '29[0-9][0-9]-*';

UPDATE runs
   SET sold_at = printf('%04d', CAST(substr(sold_at, 1, 4) AS INTEGER) - 930)
                 || substr(sold_at, 5)
 WHERE sold_at GLOB '29[0-9][0-9]-*';
