-- Cache of resolved gloss word choices, keyed by the normalized surface form
-- and the hash of the normalized context window.
-- origin: "ai", "user" or "built-in" (bootstrap-shipped rows).
CREATE TABLE gloss_word_context_cache (
    id INTEGER NOT NULL,
    word VARCHAR NOT NULL,
    context_hash VARCHAR NOT NULL,
    context_snippet TEXT NOT NULL,
    selected_uid VARCHAR NOT NULL,
    origin VARCHAR NOT NULL,
    created_at DATETIME DEFAULT (CURRENT_TIMESTAMP),
    updated_at DATETIME,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_gloss_word_context_cache_word_hash
    ON gloss_word_context_cache(word, context_hash);

-- Curated set-phrase selections (bootstrap-seeded): when the normalized phrase
-- occurs in a word's normalized context window, selected_uid applies.
CREATE TABLE gloss_phrase_selections (
    id INTEGER NOT NULL,
    phrase VARCHAR NOT NULL,
    word VARCHAR NOT NULL,
    selected_uid VARCHAR NOT NULL,
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_gloss_phrase_selections_phrase_word
    ON gloss_phrase_selections(phrase, word);

CREATE INDEX IF NOT EXISTS idx_gloss_phrase_selections_word
    ON gloss_phrase_selections(word);
