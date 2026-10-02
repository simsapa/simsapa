-- A hand-written note shown below a navigation row's summary.
--
-- `blurb` holds SuttaCentral's text and is not ours to edit. This column holds
-- the project's own notes, from `assets/navigation-context-notes.toml`, keyed
-- by `node_uid` and written at bootstrap -- so a node on either spine can
-- carry one, including the CST nodes SuttaCentral has no blurb for.
--
-- NULL on every node that file does not name.
ALTER TABLE navigation_nodes ADD COLUMN context_note TEXT;
