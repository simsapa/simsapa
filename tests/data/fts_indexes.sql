-- --- Index tables ---

CREATE VIRTUAL TABLE fts_suttas USING fts5 (
  content=suttas,
  content_rowid=id,
  content_html
);

-- --- Triggers to keep content synced ---

-- suttas

CREATE TRIGGER suttas_ai AFTER INSERT ON suttas BEGIN
  INSERT INTO fts_suttas
    (rowid, content_html)
    VALUES
    (new.id, new.content_html);
END;

CREATE TRIGGER suttas_ad AFTER DELETE ON suttas BEGIN
  INSERT INTO fts_suttas
    (fts_suttas, rowid, content_html)
    VALUES
    ('delete', old.id, old.content_html);
END;

CREATE TRIGGER suttas_au AFTER UPDATE ON suttas BEGIN
  INSERT INTO fts_suttas
    (fts_suttas, rowid, content_html)
    VALUES
    ('delete', old.id, old.content_html);
  INSERT INTO fts_suttas
    (rowid, content_html)
    VALUES
    (new.id, new.content_html);
END;

