-- 0004: playback position and app settings (SPEC §11.4 PlaybackState,
-- §11.6 AppSettings; STATE.md F2.4, F2.7, F2.8; ADR-0016).

-- One row per item: where the user stopped, in which file (PLAY-02). The row
-- goes with its item or its asset.
CREATE TABLE playback_state (
    item_id        TEXT PRIMARY KEY NOT NULL REFERENCES library_item (id) ON DELETE CASCADE,
    asset_id       TEXT NOT NULL REFERENCES media_asset (id) ON DELETE CASCADE,
    position_ms    INTEGER NOT NULL CHECK (position_ms >= 0),
    duration_ms    INTEGER CHECK (duration_ms IS NULL OR duration_ms > 0),
    last_played_at INTEGER NOT NULL,
    completed_at   INTEGER
) STRICT;
CREATE INDEX playback_state_asset ON playback_state (asset_id);

-- AppSettings as one JSON value per key. Keys and value shapes are checked
-- by the app (vv-persistence::settings); unknown keys are ignored on read.
CREATE TABLE app_setting (
    key        TEXT PRIMARY KEY NOT NULL CHECK (length(key) BETWEEN 1 AND 64),
    value      TEXT NOT NULL CHECK (json_valid(value)),
    updated_at INTEGER NOT NULL
) STRICT;
