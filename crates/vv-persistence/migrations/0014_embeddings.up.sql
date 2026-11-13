-- 0014: embedding models, index generations and chunk vectors (SPEC §16.3,
-- DEC-12; STATE.md F7.1; ADR-0035).

-- Embedding models are local AI models like whisper's (§17.1): the same
-- consent row, now also for provider llama_cpp, ids `embed-*` and GGUF
-- files. Nothing references this table, so it is rebuilt with the wider
-- CHECKs and the rows copied over.
CREATE TABLE model_installation_new (
    id           TEXT PRIMARY KEY NOT NULL CHECK ((id GLOB 'whisper-[a-z0-9]*' OR id GLOB 'embed-[a-z0-9]*') AND length(id) <= 64),
    provider     TEXT NOT NULL CHECK (provider IN ('whisper_cpp', 'llama_cpp')),
    version      TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 64),
    file_name    TEXT NOT NULL CHECK ((file_name GLOB 'ggml-*.bin' OR file_name GLOB '?*.gguf') AND file_name NOT GLOB '*[/\:]*' AND instr(file_name, '..') = 0),
    size_bytes   INTEGER NOT NULL CHECK (size_bytes > 0),
    sha256       TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    license      TEXT NOT NULL CHECK (length(license) BETWEEN 1 AND 200),
    source_url   TEXT NOT NULL CHECK (substr(source_url, 1, 8) = 'https://'),
    status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'missing', 'corrupt')),
    consent_at   INTEGER NOT NULL,
    installed_at INTEGER,
    verified_at  INTEGER,
    updated_at   INTEGER NOT NULL,
    CHECK (status <> 'ready' OR (installed_at IS NOT NULL AND verified_at IS NOT NULL)),
    CHECK ((provider = 'whisper_cpp') = (id GLOB 'whisper-*'))
) STRICT;
INSERT INTO model_installation_new SELECT * FROM model_installation;
DROP TABLE model_installation;
ALTER TABLE model_installation_new RENAME TO model_installation;

-- §16.3 index generation: every vector of one generation comes from one
-- provider, model, version and size, so vectors of different models are
-- never compared. At most one generation answers searches (`active`) and at
-- most one is being built beside it (`building`, AC-24). A finished build
-- becomes active in one transaction and the old one is deleted with its
-- vectors; a canceled build is deleted and the active one is untouched.
CREATE TABLE embedding_generation (
    seq           INTEGER PRIMARY KEY,
    id            TEXT NOT NULL UNIQUE,
    provider      TEXT NOT NULL CHECK (provider IN ('llama_cpp')),
    model_id      TEXT NOT NULL CHECK (model_id GLOB 'embed-*'),
    -- The pinned model file (its SHA-256): a new file is a new generation.
    model_version TEXT NOT NULL CHECK (length(model_version) BETWEEN 1 AND 64),
    dims          INTEGER NOT NULL CHECK (dims BETWEEN 8 AND 4096),
    status        TEXT NOT NULL CHECK (status IN ('building', 'active')),
    created_at    INTEGER NOT NULL,
    activated_at  INTEGER,
    CHECK ((status = 'active') = (activated_at IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX embedding_generation_one ON embedding_generation (status);

-- One vector per chunk and generation: little-endian f32, L2-normalized,
-- `dims` long. `text_hash` is the SHA-256 (hex, first 32) of the exact text
-- that was embedded (the source fingerprint of §16.3). Chunks are rewritten
-- when their transcript is, and their vectors go with them.
CREATE TABLE chunk_embedding (
    generation_seq INTEGER NOT NULL REFERENCES embedding_generation (seq) ON DELETE CASCADE,
    chunk_seq      INTEGER NOT NULL REFERENCES search_chunk (seq) ON DELETE CASCADE,
    text_hash      TEXT NOT NULL CHECK (length(text_hash) = 32),
    vector         BLOB NOT NULL CHECK (length(vector) BETWEEN 32 AND 16384 AND length(vector) % 4 = 0),
    PRIMARY KEY (generation_seq, chunk_seq)
) STRICT;
CREATE INDEX chunk_embedding_chunk ON chunk_embedding (chunk_seq);
