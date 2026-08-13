-- Per-window metadata for the restored session.
--
-- The last session is stored as one bookmark_folders row per window plus one
-- bookmark_items row per tab. That carries everything about the *tabs* but
-- nothing about the *window*, so the window's user-set name and which of its
-- tabs was active were silently dropped on the way to storage.
--
-- Nullable, so existing rows (and any folder that is not a session folder)
-- simply have no metadata.
--   window_title      -- user-set window name; NULL or '' means "no custom title"
--   active_tab_group  -- 'pinned' | 'results' | 'translations'
--   active_tab_index  -- index within that group, counting only saved tabs
ALTER TABLE bookmark_folders ADD COLUMN window_title TEXT;
ALTER TABLE bookmark_folders ADD COLUMN active_tab_group TEXT;
ALTER TABLE bookmark_folders ADD COLUMN active_tab_index INTEGER;
