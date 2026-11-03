-- FTS5 trigram indexes over bold_definitions:
--   bold_definitions_fts      over commentary_plain  — Contains Match
--   bold_definitions_bold_fts over bold, bold_ascii  — DPD Lookup / Headword Match
--
-- Both are external-content tables: the text is read from `bold_definitions`
-- and not stored a second time; only the indexes are stored. Each declares
-- only the columns it searches, since FTS5 reads every declared column of each
-- candidate row from `bold_definitions`. The trigram tokenizer serves
-- `LIKE '%query%'`, and detail='none' stores no term positions.
--
-- The source row id (bold_definitions.id) is the FTS5 `rowid`, which makes
-- the trigger lookups O(log n). Queries join `f.rowid = bd.id` and order by
-- `f.rowid`.
--
-- Every row is indexed, an empty column included (it gives no tokens). FTS5's
-- integrity check of an external-content table expects an index entry for
-- every row of the content table, and fails if a row is left out.
--
-- This script runs after `bold_definitions` is filled, and nothing writes to
-- it at runtime. The triggers keep any later write in step: FTS5 reports no
-- error when an index is out of step with its content table; it returns wrong
-- results. The `'delete'` command removes the index entries of the values it
-- is given, so it must be given exactly what was indexed: the old values.
--
-- Modifying this script requires a re-bootstrap of the DPD DB. See
-- docs/dict-words-fts5-implementation.md.

-- =====================================================================
-- bold_definitions_fts (commentary_plain)
-- =====================================================================

DROP TRIGGER IF EXISTS bold_definitions_fts_insert;
DROP TRIGGER IF EXISTS bold_definitions_fts_update;
DROP TRIGGER IF EXISTS bold_definitions_fts_delete;
DROP TABLE IF EXISTS bold_definitions_fts;

CREATE VIRTUAL TABLE bold_definitions_fts USING fts5(
    commentary_plain,
    content='bold_definitions', content_rowid='id',
    tokenize='trigram',
    detail='none'
);

INSERT INTO bold_definitions_fts (rowid, commentary_plain)
SELECT id, commentary_plain
FROM bold_definitions;

CREATE TRIGGER bold_definitions_fts_insert
AFTER INSERT ON bold_definitions
BEGIN
    INSERT INTO bold_definitions_fts (rowid, commentary_plain)
    VALUES (NEW.id, NEW.commentary_plain);
END;

CREATE TRIGGER bold_definitions_fts_update
AFTER UPDATE OF commentary_plain ON bold_definitions
BEGIN
    INSERT INTO bold_definitions_fts (bold_definitions_fts, rowid, commentary_plain)
    VALUES ('delete', OLD.id, OLD.commentary_plain);

    INSERT INTO bold_definitions_fts (rowid, commentary_plain)
    VALUES (NEW.id, NEW.commentary_plain);
END;

CREATE TRIGGER bold_definitions_fts_delete
AFTER DELETE ON bold_definitions
BEGIN
    INSERT INTO bold_definitions_fts (bold_definitions_fts, rowid, commentary_plain)
    VALUES ('delete', OLD.id, OLD.commentary_plain);
END;

INSERT INTO bold_definitions_fts(bold_definitions_fts) VALUES('optimize');

-- =====================================================================
-- bold_definitions_bold_fts (bold, bold_ascii)
-- =====================================================================

DROP TRIGGER IF EXISTS bold_definitions_bold_fts_insert;
DROP TRIGGER IF EXISTS bold_definitions_bold_fts_update;
DROP TRIGGER IF EXISTS bold_definitions_bold_fts_delete;
DROP TABLE IF EXISTS bold_definitions_bold_fts;

CREATE VIRTUAL TABLE bold_definitions_bold_fts USING fts5(
    bold,
    bold_ascii,
    content='bold_definitions', content_rowid='id',
    tokenize='trigram',
    detail='none'
);

INSERT INTO bold_definitions_bold_fts (rowid, bold, bold_ascii)
SELECT id, bold, bold_ascii
FROM bold_definitions;

CREATE TRIGGER bold_definitions_bold_fts_insert
AFTER INSERT ON bold_definitions
BEGIN
    INSERT INTO bold_definitions_bold_fts (rowid, bold, bold_ascii)
    VALUES (NEW.id, NEW.bold, NEW.bold_ascii);
END;

CREATE TRIGGER bold_definitions_bold_fts_update
AFTER UPDATE OF bold, bold_ascii ON bold_definitions
BEGIN
    INSERT INTO bold_definitions_bold_fts (bold_definitions_bold_fts, rowid, bold, bold_ascii)
    VALUES ('delete', OLD.id, OLD.bold, OLD.bold_ascii);

    INSERT INTO bold_definitions_bold_fts (rowid, bold, bold_ascii)
    VALUES (NEW.id, NEW.bold, NEW.bold_ascii);
END;

CREATE TRIGGER bold_definitions_bold_fts_delete
AFTER DELETE ON bold_definitions
BEGIN
    INSERT INTO bold_definitions_bold_fts (bold_definitions_bold_fts, rowid, bold, bold_ascii)
    VALUES ('delete', OLD.id, OLD.bold, OLD.bold_ascii);
END;

INSERT INTO bold_definitions_bold_fts(bold_definitions_bold_fts) VALUES('optimize');

-- Single VACUUM at the end of the script.
VACUUM;
