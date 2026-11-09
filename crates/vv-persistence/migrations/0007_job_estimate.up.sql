-- STATE.md F3.8: yt-dlp's size estimate while the exact total is unknown
-- (YouTube's chunked downloads report only an estimate). Shown as "about";
-- never stored in bytes_total, which stays exact or NULL (AC-09).
ALTER TABLE job ADD COLUMN bytes_estimate INTEGER CHECK (bytes_estimate IS NULL OR bytes_estimate >= 0);
