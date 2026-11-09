-- Dailymotion items go back to generic (renormalized again on the next
-- upgrade), then the CHECK list is narrowed back.
UPDATE library_item
   SET source_platform = 'generic', source_id = NULL, normalization_version = 0
 WHERE source_platform = 'dailymotion';
PRAGMA writable_schema = ON;
UPDATE sqlite_schema
   SET sql = replace(sql, '''vk'', ''dailymotion'', ''generic''', '''vk'', ''generic''')
 WHERE type = 'table' AND name = 'library_item';
PRAGMA writable_schema = RESET;
