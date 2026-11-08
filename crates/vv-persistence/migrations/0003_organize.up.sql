-- 0003: organization tables, operation log and orphaned media
-- (SPEC §7.3, §11.4; STATE.md F2.1–F2.3; ADR-0015).

-- Manual collections form a tree. Deleting a parent is decided by the app
-- (move children up or delete them, ORG-10), so the FK restricts; a cycle
-- is refused by the app before the write.
CREATE TABLE collection (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
    parent_id  TEXT REFERENCES collection (id) ON DELETE RESTRICT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK (parent_id IS NULL OR parent_id <> id)
) STRICT;
CREATE INDEX collection_parent ON collection (parent_id);

CREATE TABLE tag (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    normalized_name TEXT NOT NULL UNIQUE,
    created_at      INTEGER NOT NULL
) STRICT;

CREATE TABLE library_item_tag (
    item_id     TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    tag_id      TEXT NOT NULL REFERENCES tag (id) ON DELETE CASCADE,
    origin      TEXT NOT NULL DEFAULT 'user' CHECK (origin IN ('user', 'ai_accepted', 'import')),
    proposal_id TEXT,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (item_id, tag_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX library_item_tag_tag ON library_item_tag (tag_id);

CREATE TABLE library_item_collection (
    item_id       TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    collection_id TEXT NOT NULL REFERENCES collection (id) ON DELETE CASCADE,
    added_at      INTEGER NOT NULL,
    PRIMARY KEY (item_id, collection_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX library_item_collection_collection ON library_item_collection (collection_id);

-- Rules are validated by the app against `rule_schema_version` before they
-- are stored; the column only guarantees JSON.
CREATE TABLE smart_collection (
    id                  TEXT PRIMARY KEY NOT NULL,
    name                TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 200),
    rule_schema_version INTEGER NOT NULL CHECK (rule_schema_version >= 1),
    rules               TEXT NOT NULL CHECK (json_valid(rules)),
    sort                TEXT NOT NULL DEFAULT 'added_desc',
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
) STRICT;

-- Undo (§7.3): the last user operation of this app session, with what it
-- changed. `session` is the app run that wrote it; a new run or a newer
-- undoable operation makes it stale.
CREATE TABLE operation_log (
    seq        INTEGER PRIMARY KEY,
    id         TEXT NOT NULL UNIQUE,
    session    TEXT NOT NULL,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL CHECK (json_valid(payload)),
    created_at INTEGER NOT NULL,
    undone_at  INTEGER
) STRICT;

-- Managed media kept when its item was deleted permanently (§7.3, §11.4).
CREATE TABLE orphan_media (
    id                TEXT PRIMARY KEY NOT NULL,
    storage_root_id   TEXT NOT NULL REFERENCES storage_root (id) ON DELETE RESTRICT,
    storage_ref       TEXT NOT NULL CHECK (storage_ref <> '' AND substr(storage_ref, 1, 1) NOT IN ('/', '\') AND storage_ref NOT GLOB '[A-Za-z]:*'),
    size_bytes        INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    content_hash      TEXT,
    former_item_title TEXT,
    orphaned_at       INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX orphan_media_ref ON orphan_media (storage_root_id, storage_ref);
