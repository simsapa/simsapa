-- Single-row store for a CIPS topic index downloaded from within the app.
--
-- The table starts empty, and an empty table means "use the index embedded in
-- this build" (assets/general-index.json). Nothing is written at bootstrap.
--
-- This is a new, empty table rather than a rewrite of an existing one, so
-- existing installs get it from this migration alone on the first launch after
-- the app update -- no DB version bump, no forced re-download, no re-bootstrap.
-- See docs/cips-index-updates.md.

CREATE TABLE topic_index_data (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    index_json TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_etag TEXT,
    csv_line_count INTEGER,
    headword_count INTEGER,
    ref_count INTEGER,
    updated_at TEXT NOT NULL
);
