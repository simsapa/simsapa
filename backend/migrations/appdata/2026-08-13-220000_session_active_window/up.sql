-- Which saved session window was in front.
--
-- Without this, restore recreates every window but nothing decides which one
-- the user ends up looking at, so switching windows and then leaving the app
-- brings back the wrong one.
--
-- Nullable: absent on every non-session folder, and on sessions written before
-- this column existed.
ALTER TABLE bookmark_folders ADD COLUMN is_active_window BOOLEAN;
