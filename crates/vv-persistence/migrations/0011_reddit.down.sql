UPDATE library_item
   SET source_platform = 'generic', source_id = NULL, normalization_version = 0
 WHERE source_platform = 'reddit';
PRAGMA writable_schema = ON;
UPDATE sqlite_schema
   SET sql = replace(sql, '''twitch'', ''reddit'', ''generic''', '''twitch'', ''generic''')
 WHERE type = 'table' AND name = 'library_item';
PRAGMA writable_schema = RESET;
