-- 0002: library schema (SPEC §11.1–§11.7, STATE.md F1.1, ADR-0011).
-- Ids are random UUID text; times are UTC Unix milliseconds; unknown is NULL,
-- never 0. Enumerations are closed with CHECK so a bug cannot invent states.

CREATE TABLE storage_root (
    id               TEXT PRIMARY KEY NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('managed_media', 'models', 'cache', 'external_import')),
    label            TEXT NOT NULL,
    path             TEXT NOT NULL,
    volume_id        TEXT,
    status           TEXT NOT NULL DEFAULT 'unknown' CHECK (status IN ('unknown', 'available', 'disconnected', 'inaccessible')),
    created_at       INTEGER NOT NULL,
    last_verified_at INTEGER
) STRICT;
CREATE UNIQUE INDEX storage_root_kind_path ON storage_root (kind, path);

-- `seq` is the stable integer key the FTS index points at (a plain rowid may
-- change on VACUUM). It never leaves the database; `id` is the public key.
CREATE TABLE library_item (
    seq                    INTEGER PRIMARY KEY,
    id                     TEXT NOT NULL UNIQUE,
    kind                   TEXT NOT NULL CHECK (kind IN ('bookmark', 'remote_video', 'local_video')),
    canonical_url          TEXT,
    original_url           TEXT,
    normalization_version  INTEGER NOT NULL DEFAULT 0 CHECK (normalization_version >= 0),
    normalization_conflict INTEGER NOT NULL DEFAULT 0 CHECK (normalization_conflict IN (0, 1)),
    identity_conflict      INTEGER NOT NULL DEFAULT 0 CHECK (identity_conflict IN (0, 1)),
    source_platform        TEXT NOT NULL CHECK (source_platform IN ('youtube', 'facebook', 'instagram', 'tiktok', 'x', 'vk', 'generic', 'local')),
    source_id              TEXT,
    title                  TEXT,
    description            TEXT,
    creator                TEXT,
    user_overrides         TEXT NOT NULL DEFAULT '{"v":1}' CHECK (json_valid(user_overrides)),
    published_at           INTEGER,
    published_date         TEXT CHECK (published_date IS NULL OR published_date GLOB '[0-9][0-9][0-9][0-9]-[0-1][0-9]-[0-3][0-9]'),
    published_precision    TEXT CHECK (published_precision IN ('datetime', 'date')),
    added_at               INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL,
    duration_ms            INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    language               TEXT,
    visibility             TEXT NOT NULL DEFAULT 'unknown' CHECK (visibility IN ('unknown', 'public', 'unlisted', 'private')),
    remote_availability    TEXT NOT NULL DEFAULT 'unknown' CHECK (remote_availability IN ('unknown', 'available', 'unavailable', 'deleted', 'region_restricted')),
    access_state           TEXT NOT NULL DEFAULT 'unknown' CHECK (access_state IN ('unknown', 'public_ok', 'login_required', 'authorized', 'access_denied', 'extraction_failed', 'unsupported', 'drm_protected')),
    access_profile_id      TEXT,
    access_checked_at      INTEGER,
    metadata_status        TEXT NOT NULL DEFAULT 'pending' CHECK (metadata_status IN ('pending', 'fetching', 'ready', 'partial', 'failed')),
    thumbnail_asset_id     TEXT REFERENCES media_asset (id) ON DELETE SET NULL,
    user_notes             TEXT NOT NULL DEFAULT '',
    is_favorite            INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
    archived_at            INTEGER,
    removed_at             INTEGER,
    last_checked_at        INTEGER,
    metadata_provenance    TEXT CHECK (metadata_provenance IS NULL OR json_valid(metadata_provenance)),
    revision               INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    -- Remote items always have a canonical URL; local items never pretend to.
    CHECK ((kind = 'local_video') = (source_platform = 'local')),
    CHECK (kind = 'local_video' OR canonical_url IS NOT NULL),
    -- §11.1: precision decides which of the two fields holds the value.
    CHECK (published_precision IS NOT 'datetime' OR (published_at IS NOT NULL AND published_date IS NULL)),
    CHECK (published_precision IS NOT 'date' OR (published_date IS NOT NULL AND published_at IS NULL)),
    CHECK (published_precision IS NOT NULL OR (published_at IS NULL AND published_date IS NULL))
) STRICT;

-- §11.7: known remote identity and normalized URL are unique; a concurrent
-- capture race ends on these and returns the existing item.
CREATE UNIQUE INDEX library_item_source ON library_item (source_platform, source_id) WHERE source_id IS NOT NULL;
CREATE UNIQUE INDEX library_item_canonical ON library_item (canonical_url) WHERE canonical_url IS NOT NULL;
CREATE INDEX library_item_added ON library_item (added_at);
CREATE INDEX library_item_platform ON library_item (source_platform);
CREATE INDEX library_item_favorite ON library_item (is_favorite) WHERE is_favorite = 1;
CREATE INDEX library_item_archived ON library_item (archived_at) WHERE archived_at IS NOT NULL;
CREATE INDEX library_item_removed ON library_item (removed_at) WHERE removed_at IS NOT NULL;
CREATE INDEX library_item_renormalize ON library_item (normalization_version, added_at);

CREATE TABLE media_asset (
    id                 TEXT PRIMARY KEY NOT NULL,
    library_item_id    TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    role               TEXT NOT NULL CHECK (role IN ('original', 'thumbnail', 'preview', 'waveform', 'extracted_audio', 'sampled_frame')),
    media_kind         TEXT NOT NULL CHECK (media_kind IN ('video', 'audio', 'image', 'other')),
    storage_kind       TEXT NOT NULL CHECK (storage_kind IN ('managed', 'external_reference')),
    storage_root_id    TEXT NOT NULL REFERENCES storage_root (id) ON DELETE RESTRICT,
    storage_ref        TEXT NOT NULL CHECK (storage_ref <> '' AND substr(storage_ref, 1, 1) NOT IN ('/', '\') AND storage_ref NOT GLOB '[A-Za-z]:*'),
    content_hash       TEXT,
    hash_algorithm     TEXT CHECK (hash_algorithm IN ('sha256')),
    size_bytes         INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    mime_type          TEXT,
    container          TEXT,
    video_codec        TEXT,
    width              INTEGER,
    height             INTEGER,
    audio_tracks       TEXT CHECK (audio_tracks IS NULL OR json_valid(audio_tracks)),
    duration_ms        INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    acquired_at        INTEGER,
    acquisition_status TEXT NOT NULL DEFAULT 'pending' CHECK (acquisition_status IN ('pending', 'acquiring', 'ready', 'failed', 'canceled')),
    integrity_state    TEXT NOT NULL DEFAULT 'unknown' CHECK (integrity_state IN ('unknown', 'verified', 'missing', 'corrupt', 'modified')),
    source_metadata    TEXT CHECK (source_metadata IS NULL OR json_valid(source_metadata)),
    source_fingerprint TEXT,
    -- Derived assets die with their source (cache scope, §11.3).
    source_asset_id    TEXT REFERENCES media_asset (id) ON DELETE CASCADE,
    derivation         TEXT CHECK (derivation IS NULL OR json_valid(derivation)),
    created_at         INTEGER NOT NULL,
    CHECK ((content_hash IS NULL) = (hash_algorithm IS NULL)),
    CHECK (role = 'original' OR derivation IS NOT NULL),
    CHECK (role <> 'original' OR (derivation IS NULL AND source_asset_id IS NULL))
) STRICT;
CREATE INDEX media_asset_item ON media_asset (library_item_id);
CREATE INDEX media_asset_root ON media_asset (storage_root_id);
CREATE INDEX media_asset_source ON media_asset (source_asset_id) WHERE source_asset_id IS NOT NULL;
CREATE INDEX media_asset_hash ON media_asset (content_hash) WHERE content_hash IS NOT NULL;

CREATE TABLE url_alias (
    id                    TEXT PRIMARY KEY NOT NULL,
    item_id               TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    url                   TEXT NOT NULL UNIQUE,
    normalization_version INTEGER NOT NULL CHECK (normalization_version >= 0),
    created_at            INTEGER NOT NULL
) STRICT;
CREATE INDEX url_alias_item ON url_alias (item_id);

CREATE TABLE duplicate_candidate (
    id         TEXT PRIMARY KEY NOT NULL,
    item_a_id  TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    item_b_id  TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    reason     TEXT NOT NULL CHECK (reason IN ('normalization_conflict', 'source_id_conflict', 'content_hash', 'similar_url')),
    status     TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'merged', 'dismissed')),
    created_at INTEGER NOT NULL,
    decided_at INTEGER,
    CHECK (item_a_id <> item_b_id),
    CHECK ((status = 'open') = (decided_at IS NULL))
) STRICT;
-- One open candidate per unordered pair and reason.
CREATE UNIQUE INDEX duplicate_candidate_open_pair ON duplicate_candidate (min(item_a_id, item_b_id), max(item_a_id, item_b_id), reason) WHERE status = 'open';
CREATE INDEX duplicate_candidate_b ON duplicate_candidate (item_b_id);

-- §11.5 SearchDocument: the single lexical source for item metadata, notes
-- and tags. rowid = library_item.seq. Derived data: rebuildable from the
-- tables above. The tokenizer is provisional; the Turkish decision is F1.6.
CREATE VIRTUAL TABLE search_document USING fts5 (
    title, description, creator, notes, tags,
    tokenize = 'unicode61 remove_diacritics 2'
);
