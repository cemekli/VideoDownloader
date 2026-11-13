DROP INDEX IF EXISTS chunk_embedding_chunk;
DROP TABLE IF EXISTS chunk_embedding;
DROP INDEX IF EXISTS embedding_generation_one;
DROP TABLE IF EXISTS embedding_generation;
DELETE FROM model_installation WHERE provider <> 'whisper_cpp';
CREATE TABLE model_installation_old (
    id           TEXT PRIMARY KEY NOT NULL CHECK (id GLOB 'whisper-[a-z0-9]*' AND length(id) <= 64),
    provider     TEXT NOT NULL CHECK (provider IN ('whisper_cpp')),
    version      TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 64),
    file_name    TEXT NOT NULL CHECK (file_name GLOB 'ggml-*.bin' AND file_name NOT GLOB '*[/\:]*' AND instr(file_name, '..') = 0),
    size_bytes   INTEGER NOT NULL CHECK (size_bytes > 0),
    sha256       TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
    license      TEXT NOT NULL CHECK (length(license) BETWEEN 1 AND 200),
    source_url   TEXT NOT NULL CHECK (substr(source_url, 1, 8) = 'https://'),
    status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'missing', 'corrupt')),
    consent_at   INTEGER NOT NULL,
    installed_at INTEGER,
    verified_at  INTEGER,
    updated_at   INTEGER NOT NULL,
    CHECK (status <> 'ready' OR (installed_at IS NOT NULL AND verified_at IS NOT NULL))
) STRICT;
INSERT INTO model_installation_old SELECT * FROM model_installation;
DROP TABLE model_installation;
ALTER TABLE model_installation_old RENAME TO model_installation;
