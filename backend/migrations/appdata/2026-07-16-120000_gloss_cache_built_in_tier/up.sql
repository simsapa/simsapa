-- Let a user's own selection coexist with the shipped one for the same
-- (word, context_hash) instead of overwriting it. built_in = 1 marks the rows
-- imported at bootstrap from gloss-data-cache/ (origins "built-in-*"), 0 the
-- rows this install created ("user-selected" / "ai-selected"). The local row
-- shadows the shipped one in the resolution chain. Deleting it (shield click,
-- Clear Word-Selection Cache) lets the shipped selection apply again, so
-- curated data stays available for a later word selection.
--
-- NOTE: upgrade_appdata_schema() splits this file on the statement separator,
-- so none may appear inside a statement -- comments included.
ALTER TABLE gloss_word_context_cache ADD COLUMN built_in INTEGER NOT NULL DEFAULT 0;

UPDATE gloss_word_context_cache SET built_in = 1 WHERE origin LIKE 'built-in-%';

DROP INDEX IF EXISTS idx_gloss_word_context_cache_word_hash;

CREATE UNIQUE INDEX IF NOT EXISTS idx_gloss_word_context_cache_word_hash_tier
    ON gloss_word_context_cache(word, context_hash, built_in);
