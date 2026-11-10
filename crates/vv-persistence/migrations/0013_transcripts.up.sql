-- 0013: transcripts, search chunks and local AI models (SPEC §11.5, §16.2,
-- §17.1; STATE.md F6.1, F6.4; ADR-0026).

-- §11.5 ModelInstallation. A row exists only after the user agreed to a
-- download (AC-20): no row, nothing downloaded. Catalog facts (file, size,
-- hashes, licence, source) are copied from tools/models.json at consent, so
-- what the user agreed to stays on record when the catalog changes; a new
-- catalog version needs a new consent (§17.1).
CREATE TABLE model_installation (
    id           TEXT PRIMARY KEY NOT NULL CHECK (id GLOB 'whisper-[a-z0-9]*' AND length(id) <= 64),
    provider     TEXT NOT NULL CHECK (provider IN ('whisper_cpp')),
    -- The pinned repository commit the file comes from.
    version      TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 64),
    -- A bare file name inside the app's models folder; never a path.
    file_name    TEXT NOT NULL CHECK (file_name GLOB 'ggml-*.bin' AND file_name NOT GLOB '*[/\:]*' AND instr(file_name, '..') = 0),
    size_bytes   INTEGER NOT NULL CHECK (size_bytes > 0),
    sha256       TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    license      TEXT NOT NULL CHECK (length(license) BETWEEN 1 AND 200),
    source_url   TEXT NOT NULL CHECK (substr(source_url, 1, 8) = 'https://'),
    -- pending: agreed, not (yet) a verified file. ready: the file was
    -- hashed and matched. missing/corrupt: found so at a later check.
    status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'missing', 'corrupt')),
    consent_at   INTEGER NOT NULL,
    installed_at INTEGER,
    verified_at  INTEGER,
    updated_at   INTEGER NOT NULL,
    -- A model is ready only with a hash check on record (§17.1).
    CHECK (status <> 'ready' OR (installed_at IS NOT NULL AND verified_at IS NOT NULL))
) STRICT;

-- §11.5 Transcript. `generated_local` needs the media file it was made
-- from; captions from the site and imported files may have none.
CREATE TABLE transcript (
    seq                INTEGER PRIMARY KEY,
    id                 TEXT NOT NULL UNIQUE,
    item_id            TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    asset_id           TEXT REFERENCES media_asset (id) ON DELETE SET NULL,
    -- BCP 47 tag as the source gives it ('tr', 'en', 'en-US'); 'und' when unknown.
    language           TEXT NOT NULL CHECK (length(language) BETWEEN 2 AND 35),
    provider           TEXT NOT NULL CHECK (provider IN ('source_captions', 'whisper_cpp', 'import')),
    model_id           TEXT,
    model_version      TEXT,
    provenance         TEXT NOT NULL CHECK (provenance IN ('source_captions', 'generated_local', 'imported')),
    status             TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'partial', 'ready', 'failed', 'canceled', 'stale')),
    -- What the text was made from (media hash, caption track id); a change
    -- makes the transcript stale (§16.2).
    source_fingerprint TEXT NOT NULL CHECK (length(source_fingerprint) BETWEEN 1 AND 256),
    -- Media time covered so far; for `partial` the processed range (§16.2).
    covered_ms         INTEGER CHECK (covered_ms IS NULL OR covered_ms >= 0),
    -- Source captions: 'manual' or 'automatic' as the site labels them.
    caption_kind       TEXT CHECK (caption_kind IS NULL OR caption_kind IN ('manual', 'automatic')),
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    CHECK (provenance <> 'generated_local' OR (model_id IS NOT NULL AND model_version IS NOT NULL)),
    CHECK ((provenance = 'source_captions') = (provider = 'source_captions')),
    CHECK ((caption_kind IS NULL) OR provenance = 'source_captions')
) STRICT;
CREATE INDEX transcript_item ON transcript (item_id, created_at);
-- One transcript per item, provenance, language and model (§11.5: they must
-- stay apart); regenerating replaces the segments of that one.
CREATE UNIQUE INDEX transcript_identity ON transcript (item_id, provenance, language, ifnull(model_id, ''));

CREATE TABLE transcript_segment (
    seq           INTEGER PRIMARY KEY,
    id            TEXT NOT NULL UNIQUE,
    transcript_id TEXT NOT NULL REFERENCES transcript (id) ON DELETE CASCADE,
    ordinal       INTEGER NOT NULL CHECK (ordinal >= 0),
    start_ms      INTEGER NOT NULL CHECK (start_ms >= 0),
    end_ms        INTEGER NOT NULL,
    text          TEXT NOT NULL CHECK (length(text) BETWEEN 1 AND 10000),
    confidence    REAL CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
    speaker       TEXT,
    CHECK (start_ms < end_ms),
    UNIQUE (transcript_id, ordinal)
) STRICT;

-- §11.5 SearchChunk, transcript rows only for now (OCR, visual and
-- embedding sources come with M5). A chunk joins neighbouring segments of
-- one transcript and keeps the real time range of its first and last one.
CREATE TABLE search_chunk (
    seq              INTEGER PRIMARY KEY,
    id               TEXT NOT NULL UNIQUE,
    item_id          TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    transcript_id    TEXT REFERENCES transcript (id) ON DELETE CASCADE,
    start_segment_id TEXT REFERENCES transcript_segment (id) ON DELETE CASCADE,
    end_segment_id   TEXT REFERENCES transcript_segment (id) ON DELETE CASCADE,
    start_ms         INTEGER CHECK (start_ms IS NULL OR start_ms >= 0),
    end_ms           INTEGER,
    text             TEXT NOT NULL,
    source_type      TEXT NOT NULL CHECK (source_type IN ('metadata', 'transcript', 'ocr', 'visual_description', 'user_note')),
    source_revision  INTEGER NOT NULL DEFAULT 0,
    producer_provider      TEXT,
    producer_model_id      TEXT,
    producer_model_version TEXT,
    CHECK (source_type <> 'transcript' OR (transcript_id IS NOT NULL AND start_segment_id IS NOT NULL
        AND end_segment_id IS NOT NULL AND start_ms IS NOT NULL AND end_ms IS NOT NULL AND start_ms < end_ms))
) STRICT;
CREATE INDEX search_chunk_transcript ON search_chunk (transcript_id, start_ms);
CREATE INDEX search_chunk_item ON search_chunk (item_id);

-- Lexical index of transcript chunks; rowid = search_chunk.seq. Same
-- tokenizer and `ı` folding as search_document (ADR-0012). Written by
-- vv_persistence::transcripts in the transaction that writes the chunks;
-- removed with them by the trigger, also when an item delete cascades.
CREATE VIRTUAL TABLE transcript_search USING fts5 (
    text,
    tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER search_chunk_deleted AFTER DELETE ON search_chunk
BEGIN
    DELETE FROM transcript_search WHERE rowid = OLD.seq;
END;
