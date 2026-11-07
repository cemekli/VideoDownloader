-- library_item and media_asset reference each other; empty them in FK order
-- first so the drops do not trip foreign key actions on a dropped table.
DROP TABLE search_document;
DROP TABLE duplicate_candidate;
DROP TABLE url_alias;
UPDATE library_item SET thumbnail_asset_id = NULL;
DELETE FROM media_asset;
DROP TABLE media_asset;
DROP TABLE library_item;
DROP TABLE storage_root;
