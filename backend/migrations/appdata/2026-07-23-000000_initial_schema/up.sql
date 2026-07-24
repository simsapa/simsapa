-- 1.0.0 baseline schema for appdata.sqlite3.
-- Squashed from the pre-1.0.0 migration chain (see docs/appdata-migration-mechanisms.md).
-- FTS5 virtual tables + sync triggers are created separately by scripts/*-fts5-indexes.sql
-- at bootstrap and are deliberately NOT part of this migration.

CREATE TABLE app_settings (
    id INTEGER NOT NULL,
    "key" VARCHAR NOT NULL,
    value VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    UNIQUE ("key")
);

CREATE TABLE suttas (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    sutta_ref VARCHAR NOT NULL,
    nikaya VARCHAR NOT NULL,
    language VARCHAR NOT NULL,
    group_path VARCHAR,
    group_index INTEGER,
    order_index INTEGER,
    sutta_range_group VARCHAR,
    sutta_range_start INTEGER,
    sutta_range_end INTEGER,
    title VARCHAR,
    title_ascii VARCHAR,
    title_pali VARCHAR,
    title_trans VARCHAR,
    description VARCHAR,
    content_plain VARCHAR,
    content_html VARCHAR,
    content_json VARCHAR,
    content_json_tmpl VARCHAR,
    source_uid VARCHAR,
    source_info VARCHAR,
    source_language VARCHAR,
    message VARCHAR,
    copyright VARCHAR,
    license VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    indexed_at DATETIME,
    PRIMARY KEY (id),
    UNIQUE (uid)
);

CREATE TABLE sutta_variants (
    id INTEGER NOT NULL,
    sutta_id INTEGER NOT NULL,
    sutta_uid VARCHAR NOT NULL,
    language VARCHAR,
    source_uid VARCHAR,
    content_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    FOREIGN KEY(sutta_id) REFERENCES suttas (id) ON DELETE CASCADE
);

CREATE TABLE sutta_comments (
    id INTEGER NOT NULL,
    sutta_id INTEGER NOT NULL,
    sutta_uid VARCHAR NOT NULL,
    language VARCHAR,
    source_uid VARCHAR,
    content_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    FOREIGN KEY(sutta_id) REFERENCES suttas (id) ON DELETE CASCADE
);

CREATE TABLE sutta_glosses (
    id INTEGER NOT NULL,
    sutta_id INTEGER NOT NULL,
    sutta_uid VARCHAR NOT NULL,
    language VARCHAR,
    source_uid VARCHAR,
    content_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    FOREIGN KEY(sutta_id) REFERENCES suttas (id) ON DELETE CASCADE
);

CREATE TABLE books (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    document_type VARCHAR NOT NULL,
    title VARCHAR,
    author VARCHAR,
    language VARCHAR,
    file_path VARCHAR,
    metadata_json VARCHAR,
    enable_embedded_css BOOLEAN NOT NULL DEFAULT 1,
    toc_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME, is_user_added BOOLEAN NOT NULL DEFAULT 1,
    PRIMARY KEY (id),
    UNIQUE (uid)
);

CREATE TABLE book_spine_items (
    id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    book_uid VARCHAR NOT NULL,
    spine_item_uid VARCHAR NOT NULL,
    spine_index INTEGER NOT NULL,
    resource_path VARCHAR NOT NULL,
    title VARCHAR,
    language VARCHAR,
    content_html VARCHAR,
    content_plain VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    UNIQUE (spine_item_uid),
    FOREIGN KEY(book_id) REFERENCES books (id) ON DELETE CASCADE
);

CREATE TABLE book_resources (
    id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    book_uid VARCHAR NOT NULL,
    resource_path VARCHAR NOT NULL,
    mime_type VARCHAR,
    content_data BLOB,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    FOREIGN KEY(book_id) REFERENCES books (id) ON DELETE CASCADE
);

CREATE TABLE chanting_recordings (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    section_uid VARCHAR NOT NULL,
    file_name VARCHAR NOT NULL,
    recording_type VARCHAR NOT NULL,
    label VARCHAR,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    markers_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME, volume REAL NOT NULL DEFAULT 1.0, playback_position_ms INTEGER NOT NULL DEFAULT 0, waveform_json TEXT, is_user_added BOOLEAN NOT NULL DEFAULT 1,
    PRIMARY KEY (id),
    UNIQUE (uid),
    FOREIGN KEY(section_uid) REFERENCES chanting_sections (uid) ON DELETE CASCADE
);

CREATE TABLE bookmark_folders (
    id INTEGER NOT NULL,
    name VARCHAR NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_last_session BOOLEAN NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME, is_user_added BOOLEAN NOT NULL DEFAULT 1,
    PRIMARY KEY (id)
);

CREATE TABLE bookmark_items (
    id INTEGER NOT NULL,
    folder_id INTEGER NOT NULL,
    item_uid VARCHAR NOT NULL,
    table_name VARCHAR NOT NULL,
    title VARCHAR,
    tab_group VARCHAR NOT NULL,
    scroll_position REAL NOT NULL DEFAULT 0.0,
    find_query VARCHAR NOT NULL DEFAULT '',
    find_match_index INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME, is_user_added BOOLEAN NOT NULL DEFAULT 1,
    PRIMARY KEY (id),
    FOREIGN KEY(folder_id) REFERENCES bookmark_folders (id) ON DELETE CASCADE
);

CREATE TABLE "chanting_collections" (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    title VARCHAR NOT NULL,
    description VARCHAR,
    language VARCHAR NOT NULL DEFAULT 'pali',
    sort_index INTEGER NOT NULL DEFAULT 0,
    is_user_added BOOLEAN NOT NULL DEFAULT 1,
    metadata_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    UNIQUE (uid)
);

CREATE TABLE "chanting_chants" (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    collection_uid VARCHAR NOT NULL,
    title VARCHAR NOT NULL,
    description VARCHAR,
    sort_index INTEGER NOT NULL DEFAULT 0,
    is_user_added BOOLEAN NOT NULL DEFAULT 1,
    metadata_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    UNIQUE (uid),
    FOREIGN KEY(collection_uid) REFERENCES chanting_collections (uid) ON DELETE CASCADE
);

CREATE TABLE "chanting_sections" (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    chant_uid VARCHAR NOT NULL,
    title VARCHAR NOT NULL,
    content_pali VARCHAR NOT NULL,
    sort_index INTEGER NOT NULL DEFAULT 0,
    is_user_added BOOLEAN NOT NULL DEFAULT 1,
    metadata_json VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id),
    UNIQUE (uid),
    FOREIGN KEY(chant_uid) REFERENCES chanting_chants (uid) ON DELETE CASCADE
);

CREATE TABLE gloss_prompts_history (
    id INTEGER NOT NULL,
    item_type VARCHAR NOT NULL,
    data_json TEXT NOT NULL,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id)
);

CREATE TABLE gloss_word_context_cache (
    id INTEGER NOT NULL,
    word VARCHAR NOT NULL,
    context_hash VARCHAR NOT NULL,
    context_snippet TEXT NOT NULL,
    selected_uid VARCHAR NOT NULL,
    origin VARCHAR NOT NULL,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME, built_in INTEGER NOT NULL DEFAULT 0, deconstruction TEXT,
    PRIMARY KEY (id)
);

CREATE TABLE gloss_phrase_selections (
    id INTEGER NOT NULL,
    phrase VARCHAR NOT NULL,
    word VARCHAR NOT NULL,
    selected_uid VARCHAR NOT NULL,
    PRIMARY KEY (id)
);

CREATE INDEX idx_suttas_language ON suttas(language);

CREATE INDEX idx_sutta_variants_sutta_id ON sutta_variants(sutta_id);

CREATE INDEX idx_sutta_comments_sutta_id ON sutta_comments(sutta_id);

CREATE INDEX idx_sutta_glosses_sutta_id ON sutta_glosses(sutta_id);

CREATE INDEX idx_suttas_language_uid ON suttas(language, uid);

CREATE INDEX idx_suttas_source_uid ON suttas(source_uid);

CREATE INDEX idx_suttas_sutta_ref ON suttas(sutta_ref);

CREATE INDEX idx_suttas_nikaya ON suttas(nikaya);

CREATE INDEX idx_suttas_source_uid_language ON suttas(source_uid, language);

CREATE INDEX idx_suttas_nikaya_language ON suttas(nikaya, language);

CREATE INDEX idx_suttas_title_ascii_language ON suttas(title_ascii, language);

CREATE INDEX idx_books_uid ON books(uid);

CREATE INDEX idx_books_document_type ON books(document_type);

CREATE INDEX idx_books_language ON books(language);

CREATE INDEX idx_book_spine_items_book_id ON book_spine_items(book_id);

CREATE INDEX idx_book_resources_book_id ON book_resources(book_id);

CREATE INDEX idx_book_spine_items_book_uid ON book_spine_items(book_uid);

CREATE INDEX idx_book_spine_items_spine_item_uid ON book_spine_items(spine_item_uid);

CREATE INDEX idx_book_spine_items_language ON book_spine_items(language);

CREATE INDEX idx_book_spine_items_book_uid_spine_index ON book_spine_items(book_uid, spine_index);

CREATE INDEX idx_book_spine_items_resource_path ON book_spine_items(resource_path);

CREATE INDEX idx_book_spine_items_book_uid_resource_path ON book_spine_items(book_uid, resource_path);

CREATE INDEX idx_book_resources_book_uid ON book_resources(book_uid);

CREATE INDEX idx_book_resources_book_uid_resource_path ON book_resources(book_uid, resource_path);

CREATE INDEX idx_chanting_recordings_uid ON chanting_recordings(uid);

CREATE INDEX idx_chanting_recordings_section_uid ON chanting_recordings(section_uid);

CREATE INDEX idx_chanting_recordings_type ON chanting_recordings(recording_type);

CREATE INDEX idx_bookmark_folders_sort_order ON bookmark_folders(sort_order);

CREATE INDEX idx_bookmark_folders_is_last_session ON bookmark_folders(is_last_session);

CREATE INDEX idx_bookmark_items_folder_id ON bookmark_items(folder_id);

CREATE INDEX idx_bookmark_items_folder_sort ON bookmark_items(folder_id, sort_order);

CREATE INDEX idx_chanting_collections_uid ON chanting_collections(uid);

CREATE INDEX idx_chanting_chants_uid ON chanting_chants(uid);

CREATE INDEX idx_chanting_sections_uid ON chanting_sections(uid);

CREATE INDEX idx_chanting_chants_collection_uid ON chanting_chants(collection_uid);

CREATE INDEX idx_chanting_sections_chant_uid ON chanting_sections(chant_uid);

CREATE INDEX idx_chanting_collections_sort_index ON chanting_collections(sort_index);

CREATE INDEX idx_chanting_chants_sort_index ON chanting_chants(collection_uid, sort_index);

CREATE INDEX idx_chanting_sections_sort_index ON chanting_sections(chant_uid, sort_index);

CREATE INDEX idx_gloss_prompts_history_type_updated ON gloss_prompts_history(item_type, updated_at);

CREATE UNIQUE INDEX idx_gloss_phrase_selections_phrase_word
    ON gloss_phrase_selections(phrase, word);

CREATE INDEX idx_gloss_phrase_selections_word
    ON gloss_phrase_selections(word);

CREATE UNIQUE INDEX idx_gloss_word_context_cache_word_hash_tier
    ON gloss_word_context_cache(word, context_hash, built_in);

