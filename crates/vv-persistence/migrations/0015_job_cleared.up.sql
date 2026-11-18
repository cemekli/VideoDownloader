-- 0015: "Clear finished" on the Downloads and AI Processing screens.
-- A cleared job is only hidden from those lists; the row, its attempts and
-- whatever refers to it stay (Details still shows a transcript's last run).
-- A retry shows the job again.
ALTER TABLE job ADD COLUMN cleared_at INTEGER CHECK (cleared_at IS NULL OR cleared_at >= 0);
