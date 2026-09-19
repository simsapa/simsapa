-- The sutta navigation tree: one row per node of the browsable corpus
-- hierarchy, written by the CLI bootstrap from two independent spines
-- (`tree_source` = 'sc' from the sc-data checkout, 'cst' from the
-- tipitaka.org toc files). See docs/sutta-navigation-tree.md.
--
-- This is a new, empty table, so existing installs get it from this migration
-- alone -- no DB version bump and no re-download. Every read must tolerate it
-- being absent or empty: a migration failure at startup is non-fatal by
-- design (docs/database-migrations.md).

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
    blurb TEXT,

    -- The `suttas.uid` this leaf opens. NULL for a branch.
    sutta_uid TEXT,

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

    -- Materialized ancestor path: the ancestors' `node_uid`s from the root down
    -- to the parent, pipe-delimited with a leading and trailing '|'
    -- ('|sutta|sutta/linked|sn|'), and the empty string for a root. This is
    -- what makes the whole-tree filter and its breadcrumb reconstruction one
    -- indexed query instead of a walk up the parent chain per match. No
    -- `node_uid` contains '|'.
    path TEXT NOT NULL DEFAULT ''
);

CREATE UNIQUE INDEX idx_navigation_nodes_node_uid ON navigation_nodes (node_uid);

-- The level query: `WHERE parent_uid = ? ORDER BY sort_index`, covered.
CREATE INDEX idx_navigation_nodes_parent_sort ON navigation_nodes (parent_uid, sort_index);

-- Subtree and breadcrumb queries over the materialized path.
CREATE INDEX idx_navigation_nodes_path ON navigation_nodes (path);

-- The filter matches titles and uids; `sutta_uid` is also how a level's
-- translation counts are gathered.
CREATE INDEX idx_navigation_nodes_sutta_uid ON navigation_nodes (sutta_uid);
