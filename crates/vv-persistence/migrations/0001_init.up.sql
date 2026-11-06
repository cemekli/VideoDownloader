-- 0001: storage skeleton. Library tables arrive in F1.1 (SPEC §11).
CREATE TABLE app_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;
