CREATE TABLE `schema_version` (
  `id`       INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `version`  INTEGER
);

INSERT INTO `schema_version` (`version`) VALUES (1);

CREATE TABLE `authors` (
  `id`           INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `uid`          VARCHAR NOT NULL UNIQUE, -- sujato
  `full_name`    VARCHAR, -- Sujato Bhikkhu
  `description`  TEXT, -- Translated for SuttaCentral by Sujato Bhikkhu
  -------------  Timestamps
  `created_at`   TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`   TEXT
);

CREATE TABLE `suttas` (
  `id`               INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `uid`              VARCHAR NOT NULL UNIQUE, -- dn1/pli/ms
  `group_path`       VARCHAR, -- /sutta-pitaka/digha-nikaya/silakkhandha-vagga
  `group_index`      INTEGER, -- 1
  `sutta_ref`        VARCHAR, -- DN 1
  `sutta_ref_pts`    VARCHAR, -- DN i 1
  `language`         VARCHAR, -- pli, en
  `order_index`      INTEGER,
  -----------------  Content props
  `title`            VARCHAR, -- Brahmajāla: The Root of All Things
  `title_pali`       VARCHAR, -- Brahmajāla
  `title_trans`      VARCHAR, -- The Root of All Things
  `description`      TEXT,
  `content_plain`    TEXT, -- content in plain text
  `content_html`     TEXT, -- content in HTML
  -----------------  Source
  `source_info`      TEXT,
  `source_language`  VARCHAR,
  `message`          TEXT,
  `copyright`        TEXT,
  `license`          TEXT,
  -----------------  Timestamps
  `created_at`       TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`       TEXT
);

CREATE TABLE `sutta_authors` (
  `sutta_id`   INTEGER NOT NULL REFERENCES `suttas` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,
  `author_id`  INTEGER NOT NULL REFERENCES `authors` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,
  PRIMARY KEY (`sutta_id`, `author_id`)
);

CREATE TABLE `dictionaries` (
  `id`             INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `label`          VARCHAR NOT NULL UNIQUE,
  `title`          VARCHAR NOT NULL,
  `creator`        TEXT,
  `description`    TEXT,
  `feedback_email` TEXT,
  `feedback_url`   TEXT,
  `version`        TEXT,
  `data_zip_url`   TEXT,
  `info_json_url`  TEXT,
  `url_synced_at`  TEXT,
  `has_update`     TINYINT(1),
  ---------------  Timestamps
  `created_at`     TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`     TEXT
);

CREATE TABLE `dict_words` (
  `id`               INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `dictionary_id`    INTEGER NOT NULL REFERENCES `dictionaries` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,
  `url_id`           VARCHAR NOT NULL,
  `word`             VARCHAR NOT NULL,
  `word_nom_sg`      VARCHAR,
  `inflections`      VARCHAR,
  `phonetic`         VARCHAR,
  `transliteration`  VARCHAR,
  -----------------  Meaning
  `meaning_order`    INTEGER,
  `definition_plain` VARCHAR,
  `definition_html`  VARCHAR,
  `summary`          VARCHAR,
  -----------------  Associated words
  `synonyms`         VARCHAR,
  `antonyms`         VARCHAR,
  `homonyms`         VARCHAR,
  `also_written_as`  VARCHAR,
  `see_also`         VARCHAR,
  -----------------  Timestamps
  `created_at`       TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`       TEXT
);

CREATE TABLE `examples` (
  `id`                INTEGER PRIMARY KEY AUTOINCREMENT,
  `dict_word_id`      INTEGER NOT NULL REFERENCES `dict_words` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,

  `source_ref`        VARCHAR,
  `source_title`      VARCHAR,

  `text_html`         VARCHAR,
  `translation_html`  VARCHAR,

  `highlight`         TEXT,
  ------------------  Timestamps
  `created_at`        TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`        TEXT
);

CREATE TABLE `documents` (
  `id`            INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `filepath`      TEXT NOT NULL UNIQUE,
  `title`         VARCHAR,
  `author`        VARCHAR,
  --------------  Cover
  `cover_data`    BLOB,
  `cover_width`   INTEGER,
  `cover_height`  INTEGER,
  `cover_stride`  INTEGER,
  --------------  Timestamps
  `created_at`    TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`    TEXT
);

CREATE TABLE `decks` (
  `id`          INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `name`        TEXT NOT NULL UNIQUE,
  ------------  Timestamps
  `created_at`  TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`  TEXT
);

CREATE TABLE `memos` (
  `id`              INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `deck_id`         INTEGER REFERENCES `decks` (`id`) ON DELETE NO ACTION ON UPDATE CASCADE,
  ----------------  Content
  `fields_json`     TEXT, -- Front, Back
  ----------------  Anki
  `anki_model_name` TEXT, -- Basic, Cloze
  `anki_note_id`    INTEGER,
  `anki_synced_at`  TEXT,
  ----------------  Timestamps
  `created_at`      TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`      TEXT
);

CREATE TABLE `tags` (
  `id`   INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `name` VARCHAR
);

CREATE TABLE `memo_tags` (
  `memo_id`  INTEGER NOT NULL REFERENCES `memos` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,
  `tag_id`   INTEGER NOT NULL REFERENCES `tags` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,
  PRIMARY KEY (`memo_id`, `tag_id`)
);

CREATE TABLE `memo_associations` (
  `id`                INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `memo_id`           INTEGER REFERENCES `memos` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,
  `associated_table`  VARCHAR NOT NULL,
  `associated_id`     INTEGER NOT NULL,
  `page_number`       INTEGER,
  `location`          TEXT
);

CREATE TABLE `annotations` (
  `id`               INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `ann_type`         TEXT,
  `text`             TEXT,
  -----------------  Timestamps
  `created_at`       TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`       TEXT
);

CREATE TABLE `annotation_associations` (
  `id`                INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `annotation_id`     INTEGER REFERENCES `annotations` (`id`) ON DELETE CASCADE ON UPDATE CASCADE,
  `associated_table`  VARCHAR NOT NULL,
  `associated_id`     INTEGER NOT NULL,
  `page_number`       INTEGER,
  `location`          TEXT
);

CREATE TABLE `app_settings` (
  `id`          INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `key`         TEXT NOT NULL UNIQUE,
  `value`       TEXT,
  ------------  Timestamps
  `created_at`  TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`  TEXT
);
