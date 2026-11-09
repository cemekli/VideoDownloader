-- Reddit becomes a known platform (SPEC §6, v1.8). Same in-place widening
-- of the platform CHECK as migrations 0008 and 0009; existing generic
-- Reddit bookmarks move over through renormalization (version 4).
PRAGMA writable_schema = ON;
UPDATE sqlite_schema
   SET sql = replace(sql, '''twitch'', ''generic''', '''twitch'', ''reddit'', ''generic''')
 WHERE type = 'table' AND name = 'library_item';
PRAGMA writable_schema = RESET;
