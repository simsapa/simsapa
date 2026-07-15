-- Index for per-dictionary word counts and lookups.
--
-- Without this, `count_words_for_dictionary()` (SELECT count(*) FROM
-- dict_words WHERE dictionary_id = ?) is a full table scan of dict_words
-- (~190k rows carrying large HTML blobs, ~0.15 s each). The search bar's
-- Dictionaries panel calls it once per dictionary at startup via
-- `list_dictionaries_without_dpd_and_bold()`, which with many imported
-- dictionaries blocked the GUI thread for seconds before the first window
-- paint. See docs/startup-sequence-and-caches.md ("First paint" section).
CREATE INDEX IF NOT EXISTS dict_words_dictionary_id_idx ON dict_words (dictionary_id);
