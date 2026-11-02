-- Old sutta uids and the uids that now carry their text. Ajahn Ṭhānissaro's
-- translations were keyed by the dhammatalks.org file name (his own, often PTS,
-- numbering) and are now keyed by SuttaCentral numbering, e.g.
-- `an10.165/en/thanissaro` -> `an10.176/en/thanissaro`. Filled by the
-- bootstrap's suttas-core step; read where old-numbered uids enter the app
-- (dhammatalks.org links in imported books). See docs/suttas-core-import.md.
CREATE TABLE sutta_uid_renames (
    old_uid TEXT NOT NULL PRIMARY KEY,
    new_uid TEXT NOT NULL,
    reason TEXT NOT NULL
);
