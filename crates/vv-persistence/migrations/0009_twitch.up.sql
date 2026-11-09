-- Twitch becomes a known platform (SPEC §6, v1.7). Same in-place widening
-- of the platform CHECK as migration 0008; existing generic Twitch
-- bookmarks move over through renormalization (NORMALIZATION_VERSION 3).
PRAGMA writable_schema = ON;
UPDATE sqlite_schema
   SET sql = replace(sql, '''dailymotion'', ''generic''', '''dailymotion'', ''twitch'', ''generic''')
 WHERE type = 'table' AND name = 'library_item';
PRAGMA writable_schema = RESET;
