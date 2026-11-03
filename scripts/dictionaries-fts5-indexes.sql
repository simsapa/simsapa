-- FTS5 trigram indexes over dict_words:
--   dict_words_fts      over word, definition_plain — Contains Match
--   dict_words_word_fts over word                   — Headword Match
--
-- Modifying this script requires a re-bootstrap of the dictionaries DB. There
-- is no Diesel migration: the FTS table and triggers are recreated by this
-- script. See docs/dict-words-fts5-implementation.md.

-- =====================================================================
-- dict_words_fts (word, definition_plain)
-- =====================================================================

DROP TRIGGER IF EXISTS dict_words_fts_insert;
DROP TRIGGER IF EXISTS dict_words_fts_update;
DROP TRIGGER IF EXISTS dict_words_fts_delete;
DROP TABLE IF EXISTS dict_words_fts;

-- An external-content table: the text is read from `dict_words` and not stored
-- a second time; only the index is stored. The trigram tokenizer serves
-- `LIKE '%query%'` on either column, and detail='none' stores no term
-- positions.
--
-- It declares only the columns it searches: for every candidate row FTS5 reads
-- each declared column from `dict_words`. The filters (language, dict_label,
-- uid) are read from the joined `dict_words` row. `dict_words` keeps
-- `definition_plain` before its large and optional columns (the dictionaries
-- baseline migration).
--
-- The source row id (dict_words.id) is the FTS5 `rowid`, which makes the
-- trigger lookups O(log n): deleting a dictionary deletes its rows one by one.
-- Queries join `f.rowid = d.id` and order by `f.rowid`.
CREATE VIRTUAL TABLE dict_words_fts USING fts5(
    word,
    definition_plain,
    content='dict_words', content_rowid='id',
    tokenize='trigram',
    detail='none'
);

-- Index the existing rows. A full bootstrap runs this before the DPD import,
-- whose writes then go through the triggers.
--
-- Every row is indexed, a NULL `definition_plain` included (it gives no
-- tokens). FTS5's integrity check of an external-content table expects an
-- index entry for every row of the content table, and fails if a row is left
-- out.
INSERT INTO dict_words_fts (rowid, word, definition_plain)
SELECT id, word, definition_plain
FROM dict_words;

-- Every write to `dict_words.word` or `dict_words.definition_plain` must go
-- through these triggers. FTS5 reports no error when the index is out of step
-- with `dict_words`; it returns wrong results. The `'delete'` command removes
-- the index entries of the values it is given, so it must be given exactly
-- what was indexed: the old values.

CREATE TRIGGER dict_words_fts_insert
AFTER INSERT ON dict_words
BEGIN
    INSERT INTO dict_words_fts (rowid, word, definition_plain)
    VALUES (NEW.id, NEW.word, NEW.definition_plain);
END;

-- Only an update of an indexed column re-tokenizes the row, so renaming a
-- dictionary (dict_label) or marking it indexed (indexed_at) does not.
CREATE TRIGGER dict_words_fts_update
AFTER UPDATE OF word, definition_plain ON dict_words
BEGIN
    INSERT INTO dict_words_fts (dict_words_fts, rowid, word, definition_plain)
    VALUES ('delete', OLD.id, OLD.word, OLD.definition_plain);

    INSERT INTO dict_words_fts (rowid, word, definition_plain)
    VALUES (NEW.id, NEW.word, NEW.definition_plain);
END;

CREATE TRIGGER dict_words_fts_delete
AFTER DELETE ON dict_words
BEGIN
    INSERT INTO dict_words_fts (dict_words_fts, rowid, word, definition_plain)
    VALUES ('delete', OLD.id, OLD.word, OLD.definition_plain);
END;

INSERT INTO dict_words_fts(dict_words_fts) VALUES('optimize');

-- =====================================================================
-- dict_words_word_fts (word)
-- =====================================================================

-- Headword Match searches `word` alone, and needs its own index for it. With
-- detail='none' an index records no column, so `dict_words_fts.word LIKE ?`
-- finds as candidates every row with the query's trigrams in *either* column
-- (27,159 rows for `kamma`, of which 710 match), and FTS5 reads each
-- candidate's row from `dict_words` to check it: thousands of scattered page
-- reads, about 2 s on a cold cache. Over `word` alone the candidates are
-- about the rows that match. The index is about 5 MB.

DROP TRIGGER IF EXISTS dict_words_word_fts_insert;
DROP TRIGGER IF EXISTS dict_words_word_fts_update;
DROP TRIGGER IF EXISTS dict_words_word_fts_delete;
DROP TABLE IF EXISTS dict_words_word_fts;

CREATE VIRTUAL TABLE dict_words_word_fts USING fts5(
    word,
    content='dict_words', content_rowid='id',
    tokenize='trigram',
    detail='none'
);

INSERT INTO dict_words_word_fts (rowid, word)
SELECT id, word
FROM dict_words;

CREATE TRIGGER dict_words_word_fts_insert
AFTER INSERT ON dict_words
BEGIN
    INSERT INTO dict_words_word_fts (rowid, word)
    VALUES (NEW.id, NEW.word);
END;

CREATE TRIGGER dict_words_word_fts_update
AFTER UPDATE OF word ON dict_words
BEGIN
    INSERT INTO dict_words_word_fts (dict_words_word_fts, rowid, word)
    VALUES ('delete', OLD.id, OLD.word);

    INSERT INTO dict_words_word_fts (rowid, word)
    VALUES (NEW.id, NEW.word);
END;

CREATE TRIGGER dict_words_word_fts_delete
AFTER DELETE ON dict_words
BEGIN
    INSERT INTO dict_words_word_fts (dict_words_word_fts, rowid, word)
    VALUES ('delete', OLD.id, OLD.word);
END;

INSERT INTO dict_words_word_fts(dict_words_word_fts) VALUES('optimize');

-- Single VACUUM at the end of the script.
VACUUM;
