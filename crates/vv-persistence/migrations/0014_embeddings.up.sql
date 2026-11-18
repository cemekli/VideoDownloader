-- 0014: index generations and chunk vectors for search by meaning (SPEC
-- §16.3, DEC-12, AI-C07; STATE.md F7.1; ADR-0035).
--
-- The vectors come from the user's own embedding API (an OpenAI-compatible
-- `/v1/embeddings`, such as Ollama): VideoVault hosts no model.

-- §16.3 index generation: every vector of one generation comes from one
-- provider, API address, model and size, so vectors of different models
-- are never compared. At most one generation answers searches (`active`)
-- and at most one is being built beside it (`building`, AC-24). A finished
-- build becomes active in one transaction and the old one is deleted with
-- its vectors; a canceled build is deleted and the active one is untouched.
CREATE TABLE embedding_generation (
    seq           INTEGER PRIMARY KEY,
    id            TEXT NOT NULL UNIQUE,
    provider      TEXT NOT NULL CHECK (provider IN ('openai_compatible')),
    -- The model name the API was asked for (`bge-m3`).
    model_id      TEXT NOT NULL CHECK (length(model_id) BETWEEN 1 AND 200),
    -- The API address: another server is another generation.
    model_version TEXT NOT NULL CHECK (length(model_version) BETWEEN 1 AND 2048),
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
