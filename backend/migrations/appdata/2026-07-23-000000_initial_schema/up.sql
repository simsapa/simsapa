-- Baseline schema for appdata.sqlite3: the whole schema of the current DB
-- version. It keeps the folder name of the 1.0.0 baseline, so an installed
-- database, which has this version recorded as applied, runs nothing before
-- the upgrade replaces it. Edit it only together with a DB version bump that
-- changes the major or minor version (see docs/database-migrations.md).
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

-- Column order matters for search speed. SQLite reads a row's columns in
-- order and follows a large row's overflow chain from its start, and
-- suttas_fts is an external-content table that reads `content_plain` from this
-- table for every candidate row, while the search filters read `language`,
-- `nikaya` and `source_uid`. So the small columns come first, then
-- `content_plain`, then the large ones, `content_html` last. The `table!` in
-- appdata_schema.rs and the `Sutta` struct follow the same order, because
-- Diesel's `Queryable` maps fields by position. See
-- docs/database-migrations.md.
CREATE TABLE suttas (
    id INTEGER NOT NULL,
    uid VARCHAR NOT NULL,
    sutta_ref VARCHAR NOT NULL,
    nikaya VARCHAR NOT NULL,
    language VARCHAR NOT NULL,
    source_uid VARCHAR,
    source_language VARCHAR,
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
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    indexed_at DATETIME,
    copyright VARCHAR,
    license VARCHAR,
    message VARCHAR,
    source_info VARCHAR,
    content_plain VARCHAR,
    description VARCHAR,
    content_json_tmpl VARCHAR,
    content_json VARCHAR,
    content_html VARCHAR,
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

-- The large text last, `content_plain` before `content_html`, for the reason
-- given at `suttas`: an external-content FTS table reads `content_plain` from
-- here. `table!` and `BookSpineItem` follow the same order.
CREATE TABLE book_spine_items (
    id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    book_uid VARCHAR NOT NULL,
    spine_item_uid VARCHAR NOT NULL,
    spine_index INTEGER NOT NULL,
    resource_path VARCHAR NOT NULL,
    title VARCHAR,
    language VARCHAR,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    content_plain VARCHAR,
    content_html VARCHAR,
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
    -- Per-window metadata of the restored session, which is stored as one
    -- folder per window plus one bookmark_items row per tab. NULL on every
    -- folder that is not a session folder.
    --   window_title      -- user-set window name; NULL or '' means "no custom title"
    --   active_tab_group  -- 'pinned' | 'results' | 'translations'
    --   active_tab_index  -- index within that group, counting only saved tabs
    --   is_active_window  -- which saved window was in front
    window_title TEXT,
    active_tab_group TEXT,
    active_tab_index INTEGER,
    is_active_window BOOLEAN,
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
    built_in INTEGER NOT NULL DEFAULT 0,
    deconstruction TEXT,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
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

-- Single-row store for a CIPS topic index downloaded from within the app.
-- An empty table means "use the index embedded in this build"
-- (assets/general-index.json). Nothing is written at bootstrap. See
-- docs/cips-index-updates.md.
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

-- The sutta navigation tree: one row per node of the browsable corpus
-- hierarchy, written by the CLI bootstrap from two independent spines
-- (`tree_source` = 'sc' from the sc-data checkout, 'cst' from the
-- tipitaka.org toc files). See docs/sutta-navigation-tree.md.
--
-- Every read must tolerate the table being absent or empty: a migration
-- failure at startup is non-fatal by design (docs/database-migrations.md).
CREATE TABLE navigation_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Stable across bootstraps: derived from source identifiers (the SC uid,
    -- or the CST toc file name plus the leaf's index within it), never from a
    -- row id or an enumeration order. The UI persists expansion state and the
    -- breadcrumb path by this value.
    node_uid TEXT NOT NULL,

    -- NULL for a root node.
    parent_uid TEXT,

    -- Order among siblings, taken from the source file's order. The UI never
    -- re-sorts.
    sort_index INTEGER NOT NULL,

    -- 'root', 'group', 'book', 'vagga', 'samyutta', 'sutta', 'file',
    -- 'division', 'fragment', ... Uses `GroupType` names where they apply.
    node_type TEXT NOT NULL,

    -- 'sc' or 'cst': which spine this node came from. The two spines are not
    -- reconciled; a text sitting at a different depth in each is correct.
    tree_source TEXT NOT NULL,

    title_pali TEXT,
    title_en TEXT,

    -- The `suttas.uid` this leaf opens. NULL for a branch.
    sutta_uid TEXT,

    -- The text a leaf opens when it has none of its own, as the target's own
    -- uid. Some SuttaCentral leaves are parallels pages rather than documents:
    -- the Pātimokkha gives every rule a uid (`pli-tv-bu-pm-pj1`), but the text
    -- behind it is the rule inside the Vibhaṅga (`pli-tv-bu-vb-pj1/pli/ms`).
    -- Such a leaf is resolved at bootstrap through
    -- `relationship/parallels.json`. The column lets the row say where its text
    -- comes from, so a reader pressing Open on a bhikkhunī rule does not land
    -- on a bhikkhu rule with nothing to explain why. NULL on every ordinary
    -- node.
    redirect_uid TEXT,

    -- The originating CST XML file. On a fragment row this is the file the
    -- fragment is a portion of, which is what the row renders as "in
    -- s0201a.att.xml".
    cst_file TEXT,

    -- Denormalized so a level can be rendered without a second query per row:
    -- the row needs to know whether to draw a caret before its children are
    -- ever fetched, and a level of several hundred rows would otherwise cost
    -- several hundred COUNT(*)s.
    has_children INTEGER NOT NULL DEFAULT 0,
    -- The number FR-46's branch rows display. `has_children` is kept alongside
    -- it because it is what the caret keys off and what the FR-9 column list
    -- names.
    child_count INTEGER NOT NULL DEFAULT 0,

    -- False for a leaf whose target text is not in the database: the row is
    -- rendered dimmed and non-openable rather than dropped, so the gap is
    -- visible instead of silent.
    is_available INTEGER NOT NULL DEFAULT 1,

    -- Everything the whole-tree filter matches on, folded once at build time:
    -- the Pāli title, the English title, the node uid and the sutta uid, run
    -- through `navigation_tree::normalize_for_search` (consistent_niggahita ->
    -- pali_to_ascii -> lowercase) and joined with a space.
    --
    -- The filter cannot match the title columns directly. SQLite's LIKE folds
    -- case for ASCII only, so 'MAHĀVAGGA' matches nothing, and it does no
    -- accent folding at all, so a reader typing 'samyutta' misses every one of
    -- the 132 'saṁyutta' nodes. Folding at write time and normalizing the query
    -- the same way is what makes 'saccasamyutta' find 'Saccasaṁyuttaṁ'.
    --
    -- Deliberately **not** indexed. The filter is `LIKE '%...%'`, whose leading
    -- wildcard no b-tree can serve: measured, SQLite plans `SCAN
    -- navigation_nodes` with an index present, and the scan is 6 ms over the
    -- 12,622 rows of the full corpus. An index here would cost write time and
    -- space for a plan nothing chooses.
    search_text TEXT NOT NULL DEFAULT '',

    -- Materialized ancestor path: the ancestors' `node_uid`s from the root down
    -- to the parent, pipe-delimited with a leading and trailing '|'
    -- ('|sutta|sutta/linked|sn|'), and the empty string for a root. This is
    -- what makes the whole-tree filter and its breadcrumb reconstruction one
    -- indexed query instead of a walk up the parent chain per match. No
    -- `node_uid` contains '|'.
    path TEXT NOT NULL DEFAULT '',

    -- Document order of the whole tree as one sortable string: each ancestor's
    -- `sort_index` and then the node's own, zero-padded to a fixed width and
    -- pipe-joined ('|000000|000002|000005|').
    --
    -- `path` cannot do this job. It is built from `node_uid`s, so ordering by
    -- it is alphabetical: the filter returned the CST roots before the SC ones
    -- (`|cst:...` < `|sutta|`) and Añña before Aṭṭhakathā, contradicting the
    -- level order the same user had just been browsing. Fixed width is what
    -- makes a plain string compare equal a numeric one, and what puts a parent
    -- before its children (its key is their prefix).
    sort_path TEXT NOT NULL DEFAULT ''
);

CREATE UNIQUE INDEX idx_navigation_nodes_node_uid ON navigation_nodes (node_uid);

-- The level query: `WHERE parent_uid = ? ORDER BY sort_index`, covered.
CREATE INDEX idx_navigation_nodes_parent_sort ON navigation_nodes (parent_uid, sort_index);

-- Subtree and breadcrumb queries over the materialized path.
CREATE INDEX idx_navigation_nodes_path ON navigation_nodes (path);

-- The filter matches titles and uids; `sutta_uid` is also how a level's
-- translation counts are gathered.
CREATE INDEX idx_navigation_nodes_sutta_uid ON navigation_nodes (sutta_uid);

-- The Pāli records whose text is also in the other edition: an MS record
-- (`mn1/pli/ms`) or a mapped CST mūla record (`mn1/pli/cst`) related by
-- reference to a record of the other edition. Filled at CST import.
--
-- The search filters for the Pāli edition preference read it, and the fulltext
-- indexer stores it as the `has_other_edition` field, so both apply one
-- definition. A table rather than a `suttas` column, so that a failed
-- migration cannot break every sutta read.
CREATE TABLE pali_edition_twins (uid TEXT NOT NULL PRIMARY KEY);

-- Old sutta uids and the uids that now carry their text. Ajahn Ṭhānissaro's
-- translations were keyed by the dhammatalks.org file name (his own, often PTS,
-- numbering) and are now keyed by SuttaCentral numbering, e.g.
-- `an10.165/en/thanissaro` -> `an10.176/en/thanissaro`. Filled by the
-- bootstrap's suttas-core step; read where old-numbered uids enter the app
-- (dhammatalks.org links in imported books). See docs/suttas-core-import.md.
CREATE TABLE sutta_uid_renames (
    old_uid TEXT NOT NULL PRIMARY KEY,
    new_uid TEXT NOT NULL,
    reason TEXT NOT NULL
);
