-- Which mūla texts have an aṭṭhakathā and a ṭīkā, from tipitaka-xml-parser's
-- `cst_commentaries` (its docs/opentipitaka-export.md, "The commentary table").
-- The bootstrap copies the rows of verified files; the reader's m | a | t buttons look
-- them up first (`related_text_lookup()`). See docs/sutta-navigation-tree.md.
CREATE TABLE cst_commentaries (
    id INTEGER PRIMARY KEY NOT NULL,
    -- The mūla text code (`tha-ap450`, `an11.22-981`), the uid without `/pli/…`.
    mula_code TEXT NOT NULL,
    mula_file TEXT NOT NULL,
    -- `att` or `tik`.
    kind TEXT NOT NULL,
    -- `has` or `none`.
    status TEXT NOT NULL,
    -- The full uid of the record that covers it (`ud2.3.att/pli/cst`; the fragments
    -- table holds the text code, `ud2.3.att`), NULL for `none`.
    text_code TEXT,
    -- The commentary file, NULL for `none`.
    cst_file TEXT
);
CREATE INDEX idx_cst_commentaries_mula_code ON cst_commentaries (mula_code);
