-- A deconstructor-resolved compound word caches its chosen break-down here as
-- the display string ("words_joined", e.g. "sādhu + iti"), stored on the
-- compound's own row (selected_uid empty). Component-sense rows are ordinary
-- rows keyed on the compound's context hash. The column is nullable so
-- pre-existing rows keep working with deconstruction = NULL.
--
-- NOTE: upgrade_appdata_schema() splits this file on the statement separator,
-- so none may appear inside a statement -- comments included.
ALTER TABLE gloss_word_context_cache ADD COLUMN deconstruction TEXT;
