-- 0017: text read on screen (SPEC AI-C01…C04, SEA-08, AC-41; STATE.md
-- F7.4; ADR-0039).
--
-- One row per item whose frames were read: what was read (the media file
-- and its fingerprint), with which OCR and language, and how it ended. The
-- text itself is `search_chunk` rows with source_type 'ocr' (one per run
-- of frames that showed the same text, with the time of the first frame
-- and of the frame after the last), indexed in `transcript_search` like
-- transcript chunks. Reading again replaces both.
CREATE TABLE screen_text_run (
    item_id            TEXT PRIMARY KEY NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    asset_id           TEXT REFERENCES media_asset (id) ON DELETE SET NULL,
    provider           TEXT NOT NULL CHECK (provider IN ('windows_ocr', 'test')),
    -- BCP 47 tag of the OCR language ('tr', 'en-US').
    language           TEXT NOT NULL CHECK (length(language) BETWEEN 2 AND 35),
    -- What frames and rules were used; a newer version reads again.
    config_version     INTEGER NOT NULL CHECK (config_version >= 1),
    -- The media file read (hash or size); a change makes the run stale.
    source_fingerprint TEXT NOT NULL CHECK (length(source_fingerprint) BETWEEN 1 AND 256),
    -- ready: frames read (maybe no text). failed: the file could not be
    -- read; not tried again until the file or the version changes.
    status             TEXT NOT NULL CHECK (status IN ('ready', 'failed')),
    error_code         TEXT,
    frames             INTEGER NOT NULL DEFAULT 0 CHECK (frames >= 0),
    chunks             INTEGER NOT NULL DEFAULT 0 CHECK (chunks >= 0),
    created_at         INTEGER NOT NULL,
    CHECK ((status = 'failed') = (error_code IS NOT NULL))
) STRICT;

CREATE INDEX search_chunk_ocr ON search_chunk (item_id, start_ms) WHERE source_type = 'ocr';
