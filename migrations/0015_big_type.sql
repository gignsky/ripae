-- migrations/0015_big_type.sql
-- 0.3.7: larger type for members who need it. `big_type` scales every font
-- on the site by a quarter for that member's sessions only. JT is the first
-- (handle `jtwiatt`, or a display name starting "JT" should Discord have
-- renamed the handle); there is no toggle in the UI yet.
ALTER TABLE users ADD COLUMN big_type INTEGER NOT NULL DEFAULT 0;
UPDATE users SET big_type = 1 WHERE handle = 'jtwiatt' OR display_name LIKE 'JT%';
