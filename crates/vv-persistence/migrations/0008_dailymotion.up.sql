-- Dailymotion becomes a known platform (SPEC §6, v1.6). SQLite cannot
-- alter a CHECK constraint, and rebuilding library_item would cascade into
-- every table that references it. Widening an IN list changes no stored
-- row, so the table's SQL text is edited in place, as SQLite documents for
-- schema changes that leave the on-disk format alone ("Making Other Kinds
-- Of Table Schema Changes"). Existing generic Dailymotion bookmarks move
-- over through renormalization (NORMALIZATION_VERSION 2).
PRAGMA writable_schema = ON;
UPDATE sqlite_schema
   SET sql = replace(sql, '''vk'', ''generic''', '''vk'', ''dailymotion'', ''generic''')
 WHERE type = 'table' AND name = 'library_item';
PRAGMA writable_schema = RESET;
