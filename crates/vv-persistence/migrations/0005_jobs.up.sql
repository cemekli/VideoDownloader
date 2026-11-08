-- 0005: persistent job queue (SPEC §11 `Job`, §12.3, §13; STATE.md F3.1;
-- ADR-0019).

-- One row per job. A job runs at least once and its output is published
-- only by the worker holding the current generation (§13.2).
CREATE TABLE job (
    id               TEXT PRIMARY KEY NOT NULL,
    type             TEXT NOT NULL CHECK (type IN (
                         'metadata', 'probe_media', 'hash_media', 'download', 'merge_media',
                         'thumbnail', 'extract_audio', 'fetch_captions', 'transcription',
                         'fts_index', 'ocr', 'embedding', 'visual_embedding', 'model_download',
                         'reindex', 'storage_move')),
    item_id          TEXT REFERENCES library_item (id) ON DELETE CASCADE,
    parent_job_id    TEXT REFERENCES job (id) ON DELETE SET NULL,
    status           TEXT NOT NULL CHECK (status IN (
                         'queued', 'blocked', 'running', 'retry_wait', 'pausing', 'paused',
                         'canceling', 'canceled', 'succeeded', 'failed')),
    payload_version  INTEGER NOT NULL DEFAULT 1 CHECK (payload_version >= 1),
    -- Typed, secret-free input (§11 note): no cookies, tokens or signed URLs.
    payload          TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload)),
    idempotency_key  TEXT NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 512),
    priority         INTEGER NOT NULL DEFAULT 0,
    attempts         INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts     INTEGER NOT NULL DEFAULT 4 CHECK (max_attempts >= 1),
    generation       INTEGER NOT NULL DEFAULT 0 CHECK (generation >= 0),
    lease_owner      TEXT,
    lease_expires_at INTEGER,
    next_retry_at    INTEGER,
    cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
    cancel_reason    TEXT CHECK (cancel_reason IS NULL OR cancel_reason IN (
                         'user', 'dependency_failed', 'item_removed', 'shutdown')),
    -- Why a blocked job waits: 'parent' (dependency not done yet), 'offline',
    -- or a domain reason such as 'disk_full' or 'login_required'.
    blocked_reason   TEXT,
    phase            TEXT,
    bytes_done       INTEGER CHECK (bytes_done IS NULL OR bytes_done >= 0),
    bytes_total      INTEGER CHECK (bytes_total IS NULL OR bytes_total >= 0),
    units_done       INTEGER CHECK (units_done IS NULL OR units_done >= 0),
    units_total      INTEGER CHECK (units_total IS NULL OR units_total >= 0),
    checkpoint_ref   TEXT,
    error_code       TEXT,
    redacted_error_message TEXT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    finished_at      INTEGER,
    CHECK ((status = 'blocked') = (blocked_reason IS NOT NULL)),
    CHECK ((status IN ('running', 'pausing', 'canceling')) = (lease_owner IS NOT NULL)),
    CHECK (status <> 'retry_wait' OR next_retry_at IS NOT NULL)
) STRICT;

-- The same work cannot be queued twice at once (§13.2 idempotency key).
CREATE UNIQUE INDEX job_idempotency_open ON job (idempotency_key)
    WHERE status NOT IN ('succeeded', 'failed', 'canceled');
CREATE INDEX job_ready ON job (status, next_retry_at, priority);
CREATE INDEX job_parent ON job (parent_job_id);
CREATE INDEX job_item ON job (item_id);

-- Attempt history: kept when a job is retried by the user (§12.3).
CREATE TABLE job_attempt (
    job_id     TEXT NOT NULL REFERENCES job (id) ON DELETE CASCADE,
    attempt    INTEGER NOT NULL CHECK (attempt >= 1),
    generation INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at   INTEGER,
    outcome    TEXT CHECK (outcome IS NULL OR outcome IN (
                   'succeeded', 'retry', 'failed', 'canceled', 'paused', 'blocked', 'interrupted')),
    error_code TEXT,
    PRIMARY KEY (job_id, attempt)
) STRICT;
