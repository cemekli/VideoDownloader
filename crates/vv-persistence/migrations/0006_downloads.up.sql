-- 0006: download engine (STATE.md F3.4–F3.9; SPEC DL-01/DL-02, §13.1
-- storage_move; ADR-0021).

-- The format list the last metadata run reported, for the quality presets
-- (DL-02). Only allowlisted fields (`vv-engine::formats::StoredFormat`): no
-- format URL, manifest URL or HTTP header ever reaches this table.
CREATE TABLE remote_formats (
    item_id       TEXT PRIMARY KEY NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    fetched_at    INTEGER NOT NULL,
    extractor_key TEXT,
    tool_version  TEXT NOT NULL,
    formats       TEXT NOT NULL CHECK (json_valid(formats) AND json_type(formats) = 'array')
) STRICT;

-- storage_move checkpoints (§13.1): one row per asset and job. The step is
-- written after the file work it names is done, so a restart knows exactly
-- what is left: `copied` is verified again, `switched` is cleaned up.
CREATE TABLE storage_move_step (
    job_id         TEXT NOT NULL REFERENCES job (id) ON DELETE CASCADE,
    asset_id       TEXT NOT NULL,
    step           TEXT NOT NULL CHECK (step IN ('pending', 'copied', 'verified', 'switched', 'cleaned', 'cleanup_pending', 'skipped')),
    source_root_id TEXT NOT NULL,
    source_ref     TEXT NOT NULL,
    target_ref     TEXT NOT NULL,
    size_bytes     INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    content_hash   TEXT,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY (job_id, asset_id)
) STRICT;
CREATE INDEX storage_move_step_cleanup ON storage_move_step (step) WHERE step = 'cleanup_pending';
