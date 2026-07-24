-- 1.0.0 baseline schema for dictionaries.sqlite3.
-- Squashed from the pre-1.0.0 migration chain (see docs/database-migrations.md).
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

CREATE TABLE dict_words (
    id INTEGER NOT NULL,
    dictionary_id INTEGER NOT NULL,
    dict_label VARCHAR NOT NULL,
    uid VARCHAR NOT NULL,
    word VARCHAR NOT NULL,
    word_ascii VARCHAR NOT NULL,
    language VARCHAR,
    word_nom_sg VARCHAR,
    inflections VARCHAR,
    phonetic VARCHAR,
    transliteration VARCHAR,
    meaning_order INTEGER,
    definition_plain VARCHAR,
    definition_html VARCHAR,
    summary VARCHAR,
    synonyms VARCHAR,
    antonyms VARCHAR,
    homonyms VARCHAR,
    also_written_as VARCHAR,
    see_also VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    indexed_at DATETIME,
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

