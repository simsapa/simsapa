import logging as _logging

from sqlalchemy import (MetaData, Table, Column, Integer, String,
                        ForeignKey, Boolean, DateTime, LargeBinary)

from sqlalchemy.orm import relationship
from sqlalchemy.ext.declarative import declarative_base

# -------------------------------------------------------------------------
# NOTE: The schema label identifies the attached database. This is the only
# difference between appdata_models.py and userdata_models.py.
# -------------------------------------------------------------------------

metadata = MetaData(schema='appdata')
Base = declarative_base(metadata=metadata)

logger = _logging.getLogger(__name__)


class SchemaVersion(Base):
    __tablename__ = "schema_version"
    id = Column(Integer, primary_key=True)
    version = Column(Integer)


assoc_sutta_authors = Table(
    'sutta_authors',
    Base.metadata,
    Column('sutta_id', Integer, ForeignKey("suttas.id")),
    Column('author_id', Integer, ForeignKey("authors.id")),
)

assoc_document_tags = Table(
    'document_tags',
    Base.metadata,
    Column('document_id', Integer, ForeignKey("documents.id")),
    Column('tag_id', Integer, ForeignKey("tags.id")),
)

assoc_memo_tags = Table(
    'memo_tags',
    Base.metadata,
    Column('memo_id', Integer, ForeignKey("memos.id")),
    Column('tag_id', Integer, ForeignKey("tags.id")),
)


class Author(Base):
    __tablename__ = "authors"

    id = Column(Integer, primary_key=True)
    uid = Column(String)
    full_name = Column(String)
    description = Column(String)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)

    suttas = relationship("Sutta", secondary=assoc_sutta_authors, back_populates="authors")


class Sutta(Base):
    __tablename__ = "suttas"

    id = Column(Integer, primary_key=True)
    uid = Column(String)
    group_path = Column(String)
    group_index = Column(Integer)
    sutta_ref = Column(String)
    sutta_ref_pts = Column(String)
    language = Column(String)
    order_index = Column(Integer)

    title = Column(String)
    title_pali = Column(String)
    title_trans = Column(String)
    description = Column(String)
    content_plain = Column(String)
    content_html = Column(String)

    source_info = Column(String)
    source_language = Column(String)
    message = Column(String)
    copyright = Column(String)
    license = Column(String)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)

    authors = relationship("Author", secondary=assoc_sutta_authors, back_populates="suttas")


class Dictionary(Base):
    __tablename__ = "dictionaries"

    id = Column(Integer, primary_key=True)
    label = Column(String)
    title = Column(String)

    creator = Column(String)
    description = Column(String)
    feedback_email = Column(String)
    feedback_url = Column(String)
    version = Column(String)
    data_zip_url = Column(String)
    info_json_url = Column(String)
    url_synced_at = Column(String)
    has_update = Column(Boolean)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)

    dict_words = relationship("DictWord", back_populates="dictionary")


class DictWord(Base):
    __tablename__ = "dict_words"

    id = Column(Integer, primary_key=True)
    dictionary_id = Column(Integer, ForeignKey("dictionaries.id"))
    url_id = Column(String, unique=True)
    word = Column(String)
    word_nom_sg = Column(String)
    inflections = Column(String)
    phonetic = Column(String)
    transliteration = Column(String)

    meaning_order = Column(Integer)
    definition_plain = Column(String)
    definition_html = Column(String)
    summary = Column(String)

    synonyms = Column(String)
    antonyms = Column(String)
    homonyms = Column(String)
    also_written_as = Column(String)
    see_also = Column(String)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)

    dictionary = relationship("Dictionary", back_populates="dict_words")
    examples = relationship("Example", back_populates="dict_word")


class Example(Base):
    __tablename__ = "examples"

    id = Column(Integer, primary_key=True)
    dict_word_id = Column(Integer, ForeignKey("dict_words.id"))

    source_ref = Column(String)
    source_title = Column(String)

    text_html = Column(String)
    translation_html = Column(String)

    highlight = Column(String)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)

    dict_word = relationship("DictWord", back_populates="examples")


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

    tags = relationship("Tag", secondary=assoc_document_tags, back_populates="documents")


class Deck(Base):
    __tablename__ = "decks"

    id = Column(Integer, primary_key=True)
    name = Column(String)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)

    memos = relationship("Memo", back_populates="deck")


class Memo(Base):
    __tablename__ = "memos"

    id = Column(Integer, primary_key=True)
    deck_id = Column(Integer, ForeignKey("decks.id"))

    fields_json = Column(String)

    anki_model_name = Column(String)
    anki_note_id = Column(Integer)
    anki_synced_at = Column(DateTime)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)

    deck = relationship("Deck", back_populates="memos")
    tags = relationship("Tag", secondary=assoc_memo_tags, back_populates="memos")


class MemoAssociation(Base):
    __tablename__ = "memo_associations"

    id = Column(Integer, primary_key=True, autoincrement=True)
    memo_id = Column(Integer, ForeignKey("memos.id"), primary_key=True)
    associated_table = Column(String)
    associated_id = Column(Integer)

    page_number = Column(Integer)
    location = Column(String)


class Annotation(Base):
    __tablename__ = "annotations"

    id = Column(Integer, primary_key=True)
    ann_type = Column(String)
    text = Column(String)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)


class AnnotationAssociation(Base):
    __tablename__ = "annotation_associations"

    id = Column(Integer, primary_key=True)
    annotation_id = Column(Integer, ForeignKey("annotations.id"), primary_key=True)
    associated_table = Column(String)
    associated_id = Column(Integer)

    page_number = Column(Integer)
    location = Column(Integer)


class Tag(Base):
    __tablename__ = "tags"

    id = Column(Integer, primary_key=True)
    name = Column(String)

    documents = relationship("Document", secondary=assoc_document_tags, back_populates="tags")
    memos = relationship("Memo", secondary=assoc_memo_tags, back_populates="tags")


class Link(Base):
    __tablename__ = "links"

    id = Column(Integer, primary_key=True)
    label = Column(String)
    from_table = Column(String)
    from_id = Column(Integer)
    from_page_number = Column(Integer)
    to_table = Column(String)
    to_id = Column(Integer)
    to_page_number = Column(Integer)


class AppSetting(Base):
    __tablename__ = "app_settings"

    id = Column(Integer, primary_key=True)
    key = Column(String)
    value = Column(String)

    created_at = Column(DateTime)
    updated_at = Column(DateTime)
