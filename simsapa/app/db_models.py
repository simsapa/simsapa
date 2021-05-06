from sqlalchemy import Column, Integer, String, ForeignKey, Table, Boolean

from sqlalchemy.orm import relationship, backref

from sqlalchemy.ext.declarative import declarative_base


Base = declarative_base()


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


