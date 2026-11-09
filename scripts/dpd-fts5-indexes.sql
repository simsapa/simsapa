-- Migration: FTS5 fulltext search for dpd_headwords.lemma_1 for efficient "%{}%" LIKE queries
-- This enables fast substring matching for DPD headword searches

-- Drop existing FTS table and triggers if they exist
-- This ensures we start fresh with the new schema
DROP TRIGGER IF EXISTS dpd_headwords_fts_insert;
DROP TRIGGER IF EXISTS dpd_headwords_fts_update;
DROP TRIGGER IF EXISTS dpd_headwords_fts_delete;
DROP TABLE IF EXISTS dpd_headwords_fts;

-- Create the FTS5 virtual table with trigram tokenizer
-- The trigram tokenizer enables efficient substring matching like LIKE '%query%'
-- detail='none' reduces index size by not storing term positions
--
-- PERFORMANCE: the source row id (dpd_headwords.id) is stored as the FTS5
-- `rowid`, NOT as a separate UNINDEXED column. FTS5 has no secondary indexes,
-- so a `WHERE headword_id = ?` lookup against an UNINDEXED column is a FULL
-- TABLE SCAN; the per-row delete/update triggers would then scan the whole FTS
-- table once per affected row. Using the rowid makes those lookups O(log n).
-- Queries select `rowid AS headword_id` instead of the old `headword_id` column.
CREATE VIRTUAL TABLE dpd_headwords_fts USING fts5(
    lemma_1,                -- The primary field we want to search for contains-match
    tokenize='trigram',     -- Use trigram tokenizer for substring search
    detail='none'           -- Reduce index size by not storing term positions
);

-- Populate the FTS table with existing data
-- Only insert rows where lemma_1 is not NULL (though it should always have a value).
-- `rowid` is set to dpd_headwords.id so deletes/updates can use the fast rowid path.
INSERT INTO dpd_headwords_fts (rowid, lemma_1)
SELECT id, lemma_1
FROM dpd_headwords
WHERE lemma_1 IS NOT NULL AND lemma_1 != '';

-- Create triggers to keep FTS table in sync with main table

-- Trigger for INSERT operations
CREATE TRIGGER dpd_headwords_fts_insert
AFTER INSERT ON dpd_headwords
WHEN NEW.lemma_1 IS NOT NULL AND NEW.lemma_1 != ''
BEGIN
    INSERT INTO dpd_headwords_fts (rowid, lemma_1)
    VALUES (NEW.id, NEW.lemma_1);
END;

-- Trigger for UPDATE operations
CREATE TRIGGER dpd_headwords_fts_update
AFTER UPDATE ON dpd_headwords
BEGIN
    -- Delete old entry if it exists (fast rowid lookup)
    DELETE FROM dpd_headwords_fts WHERE rowid = OLD.id;

    -- Insert new entry if lemma_1 is not NULL and not empty
    INSERT INTO dpd_headwords_fts (rowid, lemma_1)
    SELECT NEW.id, NEW.lemma_1
    WHERE NEW.lemma_1 IS NOT NULL AND NEW.lemma_1 != '';
END;

-- Trigger for DELETE operations (fast rowid lookup)
CREATE TRIGGER dpd_headwords_fts_delete
AFTER DELETE ON dpd_headwords
BEGIN
    DELETE FROM dpd_headwords_fts WHERE rowid = OLD.id;
END;

-- Optimize the FTS index
INSERT INTO dpd_headwords_fts(dpd_headwords_fts) VALUES('optimize');
