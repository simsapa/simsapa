from sqlalchemy import Column, Integer, String, ForeignKey, Boolean, DateTime, LargeBinary  # type: ignore

from sqlalchemy.orm import relationship, backref  # type: ignore

from sqlalchemy.ext.declarative import declarative_base  # type: ignore


Base = declarative_base()


# === User Data ===

class Document(Base):
    __tablename__ = "documents"
    id = Column(Integer, primary_key=True)
    filepath = Column(String, nullable=False, unique=True)
    title = Column(String)
    author = Column(String)
    cover_data = Column(LargeBinary)
    cover_width = Column(Integer)
    cover_height = Column(Integer)
    cover_stride = Column(Integer)
    created_at = Column(DateTime)
    updated_at = Column(DateTime)
    notes = relationship("Note", backref=backref("note"))


class Note(Base):
    __tablename__ = "notes"
    id = Column(Integer, primary_key=True)
    document_id = Column(Integer, ForeignKey("documents.id"))
    doc_page_number = Column(Integer)
    front = Column(String)
    back = Column(String)
    anki_note_id = Column(Integer)
    anki_synced_at = Column(DateTime)
    created_at = Column(DateTime)
    updated_at = Column(DateTime)


USERDATA_CREATE_SCHEMA_SQL = """
CREATE TABLE `documents` (
  `id`           INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `filepath`     TEXT NOT NULL UNIQUE,
  `title`        VARCHAR,
  `author`       VARCHAR,
  `cover_data`   BLOB,
  `cover_width`  INTEGER,
  `cover_height` INTEGER,
  `cover_stride` INTEGER,
  `created_at`      TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`      TEXT
);

CREATE TABLE `notes` (
  `id`              INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  `document_id`     INTEGER REFERENCES `documents` (`id`) ON DELETE NO ACTION ON UPDATE CASCADE,
  `doc_page_number` INTEGER,
  `front`           TEXT,
  `back`            TEXT,
  `anki_note_id`    INTEGER,
  `anki_synced_at`  TEXT,
  `created_at`      TEXT DEFAULT CURRENT_TIMESTAMP,
  `updated_at`      TEXT
);
"""


# === App Data ===

class Author(Base):
    __tablename__ = "authors"
    id = Column(Integer, primary_key=True)
    uid = Column(String)
    blurb = Column(String)
    long_name = Column(String)
    short_name = Column(String)
    root_texts = relationship("RootText", backref=backref("author"))


class RootText(Base):
    __tablename__ = "root_texts"
    id = Column(Integer, primary_key=True)
    author_id = Column(Integer, ForeignKey("authors.id"))
    uid = Column(String)
    acronym = Column(String)
    volpage = Column(String)
    title = Column(String)
    content_language = Column(String)
    content_plain = Column(String)
    content_html = Column(String)


class Dictionary(Base):
    __tablename__ = "dictionaries"
    id = Column(Integer, primary_key=True)
    label = Column(String)
    title = Column(String)


class DictWord(Base):
    __tablename__ = "dict_words"
    id = Column(Integer, primary_key=True)
    dictionary_id = Column(Integer, ForeignKey("dictionaries.id"))
    word = Column(String)
    word_nom_sg = Column(String)
    inflections = Column(String)
    phonetic = Column(String)
    transliteration = Column(String)
    url_id = Column(String)
    meanings = relationship("Meaning", backref=backref("dict_word"))


class Meaning(Base):
    __tablename__ = "meanings"
    id = Column(Integer, primary_key=True)
    dict_word_id = Column(Integer, ForeignKey("dict_words.id"))
    meaning_order = Column(Integer)
    definition_md = Column(String)
    summary = Column(String)
    synonyms = Column(String)
    antonyms = Column(String)
    homonyms = Column(String)
    also_written_as = Column(String)
    see_also = Column(String)
    comment = Column(String)
    is_root = Column(Boolean)
    root_language = Column(String)
    root_groups = Column(String)
    root_sign = Column(String)
    root_numbered_group = Column(String)


