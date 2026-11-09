-- Add B-tree indexes
CREATE INDEX IF NOT EXISTS idx_dpd_headwords_lemma_clean ON dpd_headwords(lemma_clean);
CREATE INDEX IF NOT EXISTS idx_dpd_headwords_word_ascii ON dpd_headwords(word_ascii);
CREATE INDEX IF NOT EXISTS idx_dpd_headwords_stem ON dpd_headwords(stem);

CREATE INDEX IF NOT EXISTS idx_dpd_roots_root_clean ON dpd_roots(root_clean);
CREATE INDEX IF NOT EXISTS idx_dpd_roots_root_no_sign ON dpd_roots(root_no_sign);
CREATE INDEX IF NOT EXISTS idx_dpd_roots_word_ascii ON dpd_roots(word_ascii);

-- Unique index on bold_definitions.uid. Column is populated by the CLI
-- bootstrap step populate_bold_definitions_derived_columns; this line is a
-- no-op on re-run.
CREATE UNIQUE INDEX IF NOT EXISTS idx_bold_definitions_uid ON bold_definitions(uid);

-- NOTE: Do NOT create composite indexes for OR queries
-- SQLite cannot use multicolumn indexes for OR conditions like:
-- WHERE lemma_clean = X OR word_ascii = Y
-- Individual indexes on each column are needed instead,
-- in OR queries SQLite will use the two indexes separately.

-- Note: dpd_roots.uid already has a UNIQUE constraint which creates an implicit index
-- Note: lookup.lookup_key is PRIMARY KEY so already has an index
-- Note: dpd_headwords.id is PRIMARY KEY so already has an index
-- Note: dpd_headwords.lemma_1 has UNIQUE constraint so already has an index
