-- 0016: AI tag suggestions (SPEC §11 `AIProposal`, AI-B01…B04; STATE.md
-- F7.3; ADR-0037).
--
-- The suggestions come from the user's own chat API (an OpenAI-compatible
-- `/v1/chat/completions`, such as Ollama): VideoVault hosts no model. A
-- suggestion never changes the item's tags by itself (AI-B02): accepting
-- one adds `library_item_tag (origin = 'ai_accepted', proposal_id)`.

-- One row per suggested value and item. Accepted and rejected rows stay,
-- so a rejected value is never suggested for that item again, whatever
-- model asks (AI-B02); pending rows are replaced by the next run.
CREATE TABLE ai_proposal (
    id                 TEXT PRIMARY KEY NOT NULL,
    item_id            TEXT NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    type               TEXT NOT NULL CHECK (type IN ('tag', 'topic', 'collection')),
    value              TEXT NOT NULL CHECK (length(trim(value)) BETWEEN 1 AND 100),
    -- Like `tag.normalized_name`: "İstanbul" and "istanbul" are one value.
    normalized_value   TEXT NOT NULL CHECK (length(normalized_value) >= 1),
    status             TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'rejected')),
    provider           TEXT NOT NULL CHECK (provider IN ('openai_compatible')),
    model_id           TEXT NOT NULL CHECK (length(model_id) BETWEEN 1 AND 200),
    -- The API address.
    model_version      TEXT NOT NULL CHECK (length(model_version) BETWEEN 1 AND 2048),
    source_fingerprint TEXT NOT NULL CHECK (length(source_fingerprint) = 32),
    created_at         INTEGER NOT NULL,
    decided_at         INTEGER,
    CHECK ((status = 'pending') = (decided_at IS NULL)),
    UNIQUE (item_id, type, normalized_value)
) STRICT;
CREATE INDEX ai_proposal_status ON ai_proposal (status, item_id);

-- The last run per item: the same text with the same model and API is not
-- asked again (AI-B04), also when that run suggested nothing new.
CREATE TABLE ai_proposal_run (
    item_id            TEXT PRIMARY KEY NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    source_fingerprint TEXT NOT NULL CHECK (length(source_fingerprint) = 32),
    model_id           TEXT NOT NULL,
    model_version      TEXT NOT NULL,
    created_at         INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
