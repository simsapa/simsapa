DROP INDEX IF EXISTS idx_gloss_word_context_cache_word_hash_tier;

CREATE UNIQUE INDEX IF NOT EXISTS idx_gloss_word_context_cache_word_hash
    ON gloss_word_context_cache(word, context_hash);

ALTER TABLE gloss_word_context_cache DROP COLUMN built_in;
