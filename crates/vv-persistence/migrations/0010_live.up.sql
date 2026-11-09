-- SPEC §6 (v1.7): a live stream is saved as a bookmark but not downloaded
-- (a Twitch channel page is one). The metadata run records whether the
-- page was live; the Download dialog and the engine refuse it.
ALTER TABLE remote_formats ADD COLUMN is_live INTEGER NOT NULL DEFAULT 0 CHECK (is_live IN (0, 1));
