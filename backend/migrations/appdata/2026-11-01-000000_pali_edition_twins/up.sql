-- The Pāli records whose text is also in the other edition: an MS record
-- (`mn1/pli/ms`) or a mapped CST mūla record (`mn1/pli/cst`) related by
-- reference to a record of the other edition. Filled at CST import.
--
-- The search filters for the Pāli edition preference read it, and the fulltext
-- indexer stores it as the `has_other_edition` field, so both apply one
-- definition. A table rather than a `suttas` column, so that a failed
-- migration cannot break every sutta read.
CREATE TABLE pali_edition_twins (uid TEXT NOT NULL PRIMARY KEY);
