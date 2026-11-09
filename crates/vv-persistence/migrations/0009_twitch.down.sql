UPDATE library_item
   SET source_platform = 'generic', source_id = NULL, normalization_version = 0
 WHERE source_platform = 'twitch';
PRAGMA writable_schema = ON;
UPDATE sqlite_schema
   SET sql = replace(sql, '''dailymotion'', ''twitch'', ''generic''', '''dailymotion'', ''generic''')
 WHERE type = 'table' AND name = 'library_item';
PRAGMA writable_schema = RESET;
