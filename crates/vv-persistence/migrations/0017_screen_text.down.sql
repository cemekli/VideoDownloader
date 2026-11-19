DELETE FROM search_chunk WHERE source_type = 'ocr';
DROP INDEX search_chunk_ocr;
DROP TABLE screen_text_run;
