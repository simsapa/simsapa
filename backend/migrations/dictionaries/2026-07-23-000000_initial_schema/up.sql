-- Baseline schema for dictionaries.sqlite3: the whole schema of the current DB
-- version. It keeps the folder name of the 1.0.0 baseline, so an installed
-- database runs nothing before the upgrade replaces it. Edit it only together
-- with a DB version bump that changes the major or minor version (see
-- docs/database-migrations.md).
-- The dict_words_fts FTS5 virtual table + sync triggers are created separately by
-- scripts/dictionaries-fts5-indexes.sql at bootstrap and are NOT part of this migration.

CREATE TABLE dictionaries (
    id INTEGER NOT NULL,
    label VARCHAR NOT NULL,
    title VARCHAR NOT NULL,
    dict_type VARCHAR NOT NULL,
    creator VARCHAR,
    description VARCHAR,
    feedback_email VARCHAR,
    feedback_url VARCHAR,
    version VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME, is_user_imported BOOLEAN NOT NULL DEFAULT 0, language TEXT NULL, indexed_at TIMESTAMP NULL,
    PRIMARY KEY (id),
    UNIQUE (label)
);

-- Column order matters for search speed (see the `suttas` table of the
-- appdata baseline and docs/database-migrations.md §0): the small fixed
-- columns first, then `definition_plain`, which the external-content
-- dict_words_fts reads for every candidate row, then the optional columns,
-- those that can be large last, `definition_html` (DPD's rendered pages) at
-- the end. `table!` in dictionaries_schema.rs and `DictWord` follow this order.
CREATE TABLE dict_words (
    id INTEGER NOT NULL,
    dictionary_id INTEGER NOT NULL,
    dict_label VARCHAR NOT NULL,
    uid VARCHAR NOT NULL,
    word VARCHAR NOT NULL,
    word_ascii VARCHAR NOT NULL,
    language VARCHAR,
    meaning_order INTEGER,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    indexed_at DATETIME,
    definition_plain VARCHAR,
    word_nom_sg VARCHAR,
    phonetic VARCHAR,
    transliteration VARCHAR,
    synonyms VARCHAR,
    antonyms VARCHAR,
    homonyms VARCHAR,
    also_written_as VARCHAR,
    see_also VARCHAR,
    summary VARCHAR,
    inflections VARCHAR,
    definition_html VARCHAR,
    PRIMARY KEY (id),
    FOREIGN KEY(dictionary_id) REFERENCES dictionaries (id) ON DELETE CASCADE,
    UNIQUE (uid)
);

CREATE TABLE dict_resources (
    id INTEGER NOT NULL,
    dictionary_id INTEGER NOT NULL,
    resource_path VARCHAR NOT NULL,
    mime_type VARCHAR,
    content_data BLOB,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    FOREIGN KEY(dictionary_id) REFERENCES dictionaries (id) ON DELETE CASCADE
);

CREATE INDEX dict_words_dict_label_idx ON dict_words(dict_label);

CREATE INDEX dict_words_idx ON dict_words(dict_label, word);

CREATE INDEX dict_words_language_idx ON dict_words(language);

CREATE INDEX dict_resources_dict_id_path_idx ON dict_resources(dictionary_id, resource_path);

CREATE INDEX dict_words_dictionary_id_idx ON dict_words (dictionary_id);

-- The per-headword lookups of Dictionary Contains Match and Headword Match
-- (`word IN (...)`, with or without a language filter). Without it the plan is
-- a skip-scan of dict_words_idx, one probe per dictionary label.
CREATE INDEX dict_words_word_idx ON dict_words(word);
