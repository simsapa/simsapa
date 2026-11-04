-- FTS5 trigram index over suttas.content_plain, for Contains Match.
--
-- Modifying this script requires a re-bootstrap of the appdata DB. There is no
-- Diesel migration: the FTS table and triggers are recreated by this script.
-- See docs/dict-words-fts5-implementation.md.

DROP TRIGGER IF EXISTS suttas_fts_insert;
DROP TRIGGER IF EXISTS suttas_fts_update;
DROP TRIGGER IF EXISTS suttas_fts_delete;
DROP TABLE IF EXISTS suttas_fts;

-- An external-content table: the text is read from `suttas` and not stored a
-- second time; only the index is stored. The trigram tokenizer serves
-- `LIKE '%query%'`, and detail='none' stores no term positions.
--
-- It declares only the column it searches. For every candidate row FTS5 reads
-- each declared column from `suttas`, so a declared column stored after
-- `content_html` / `content_json` would walk their overflow pages on every
-- check. The filters (language, nikaya, source_uid, uid) are read from the
-- joined `suttas` row instead. `suttas` keeps `content_plain` before its large
-- columns for the same reason (the appdata baseline migration).
--
-- The source row id (suttas.id) is the FTS5 `rowid`, which makes the trigger
-- lookups O(log n). Queries join `f.rowid = s.id` and order by `f.rowid`.
CREATE VIRTUAL TABLE suttas_fts USING fts5(
    content_plain,
    content='suttas', content_rowid='id',
    tokenize='trigram',
    detail='none'
);

-- Index the existing rows. A full bootstrap runs this on the empty database,
-- and the insert trigger indexes the imported rows.
--
-- Every row is indexed, a NULL `content_plain` included (it gives no tokens).
-- FTS5's integrity check of an external-content table expects an index entry
-- for every row of the content table, and fails if a row is left out.
INSERT INTO suttas_fts (rowid, content_plain)
SELECT id, content_plain
FROM suttas;

-- Every write to `suttas.content_plain` must go through these triggers. FTS5
-- reports no error when the index is out of step with `suttas`; it returns
-- wrong results. The `'delete'` command removes the index entries of the
-- values it is given, so it must be given exactly what was indexed: the old
-- values.

CREATE TRIGGER suttas_fts_insert
AFTER INSERT ON suttas
BEGIN
    INSERT INTO suttas_fts (rowid, content_plain)
    VALUES (NEW.id, NEW.content_plain);
END;

-- Only an update of the indexed column re-tokenizes the row.
CREATE TRIGGER suttas_fts_update
AFTER UPDATE OF content_plain ON suttas
BEGIN
    INSERT INTO suttas_fts (suttas_fts, rowid, content_plain)
    VALUES ('delete', OLD.id, OLD.content_plain);

    INSERT INTO suttas_fts (rowid, content_plain)
    VALUES (NEW.id, NEW.content_plain);
END;

CREATE TRIGGER suttas_fts_delete
AFTER DELETE ON suttas
BEGIN
    INSERT INTO suttas_fts (suttas_fts, rowid, content_plain)
    VALUES ('delete', OLD.id, OLD.content_plain);
END;

INSERT INTO suttas_fts(suttas_fts) VALUES('optimize');

VACUUM;
