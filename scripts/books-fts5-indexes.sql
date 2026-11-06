-- FTS5 fulltext search for book_spine_items table

-- Drop existing FTS table and triggers if they exist
DROP TRIGGER IF EXISTS book_spine_items_fts_insert;
DROP TRIGGER IF EXISTS book_spine_items_fts_update;
DROP TRIGGER IF EXISTS book_spine_items_fts_delete;
DROP TABLE IF EXISTS book_spine_items_fts;

-- Create the FTS5 virtual table with trigram tokenizer
-- The trigram tokenizer enables efficient substring matching like LIKE '%query%'
-- detail='none' reduces index size by not storing term positions
--
-- PERFORMANCE: the source row id (book_spine_items.id) is stored as the FTS5
-- `rowid`, NOT as a separate UNINDEXED column. FTS5 has no secondary indexes,
-- so a `WHERE spine_item_id = ?` lookup against an UNINDEXED column is a FULL
-- TABLE SCAN; the per-row delete/update triggers would then scan the whole FTS
-- table once per affected row. Using the rowid makes those lookups O(log n).
-- Queries join `f.rowid = book_spine_items.id` instead of the old
-- `f.spine_item_id`.
CREATE VIRTUAL TABLE book_spine_items_fts USING fts5(
    book_uid UNINDEXED,        -- Store book_uid for filtering, but don't index for search
    language UNINDEXED,        -- Store language for filtering, but don't index for search
    title,                     -- Index title for search
    content_plain,             -- The field we want to search
    tokenize='trigram',        -- Use trigram tokenizer for substring search
    detail='none'              -- Reduce index size
);

-- Populate the FTS table with existing data
-- Only insert rows where content_plain is not NULL.
-- `rowid` is set to book_spine_items.id so deletes/updates can use the fast rowid path.
INSERT INTO book_spine_items_fts (rowid, book_uid, language, title, content_plain)
SELECT id, book_uid, language, title, content_plain
FROM book_spine_items
WHERE content_plain IS NOT NULL;

-- Create triggers to keep FTS table in sync with main table

-- Trigger for INSERT operations
CREATE TRIGGER book_spine_items_fts_insert
AFTER INSERT ON book_spine_items
WHEN NEW.content_plain IS NOT NULL
BEGIN
    INSERT INTO book_spine_items_fts (rowid, book_uid, language, title, content_plain)
    VALUES (NEW.id, NEW.book_uid, NEW.language, NEW.title, NEW.content_plain);
END;

-- Trigger for UPDATE operations
CREATE TRIGGER book_spine_items_fts_update
AFTER UPDATE ON book_spine_items
BEGIN
    -- Delete old entry if it exists (fast rowid lookup)
    DELETE FROM book_spine_items_fts WHERE rowid = OLD.id;

    -- Insert new entry if content_plain is not NULL
    INSERT INTO book_spine_items_fts (rowid, book_uid, language, title, content_plain)
    SELECT NEW.id, NEW.book_uid, NEW.language, NEW.title, NEW.content_plain
    WHERE NEW.content_plain IS NOT NULL;
END;

-- Trigger for DELETE operations (fast rowid lookup)
CREATE TRIGGER book_spine_items_fts_delete
AFTER DELETE ON book_spine_items
BEGIN
    DELETE FROM book_spine_items_fts WHERE rowid = OLD.id;
END;

-- Optimize the FTS index
INSERT INTO book_spine_items_fts(book_spine_items_fts) VALUES('optimize');
