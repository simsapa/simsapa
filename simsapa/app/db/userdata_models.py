import logging as _logging

from sqlalchemy import (MetaData, Table, Column, Integer, String,
                        ForeignKey, Boolean, DateTime, LargeBinary)

from sqlalchemy import func
from sqlalchemy.orm import relationship
from sqlalchemy.ext.declarative import declarative_base

# -------------------------------------------------------------------------
# NOTE: The schema label identifies the attached database. This is the only
# difference between appdata_models.py and userdata_models.py.
# -------------------------------------------------------------------------

metadata = MetaData(schema='userdata')
Base = declarative_base(metadata=metadata)

logger = _logging.getLogger(__name__)

assoc_sutta_authors = Table(
    'sutta_authors',
    Base.metadata,
    Column('sutta_id', Integer, ForeignKey("suttas.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('author_id', Integer, ForeignKey("authors.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_document_tags = Table(
    'document_tags',
    Base.metadata,
    Column('document_id', Integer, ForeignKey("documents.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_memo_tags = Table(
    'memo_tags',
    Base.metadata,
    Column('memo_id', Integer, ForeignKey("memos.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)


class Author(Base):
    __tablename__ = "authors"

    id = Column(Integer, primary_key=True)
    uid = Column(String, nullable=False, unique=True) # sujato
    full_name = Column(String) # Sujato Bhikkhu
    description = Column(String) # Translated for SuttaCentral by Sujato Bhikkhu

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    suttas = relationship("Sutta", secondary=assoc_sutta_authors, back_populates="authors")


class Sutta(Base):
    __tablename__ = "suttas"

    id = Column(Integer, primary_key=True)
    uid = Column(String, nullable=False, unique=True) # dn1/pli/ms
    group_path = Column(String) # /sutta-pitaka/digha-nikaya/silakkhandha-vagga
    group_index = Column(Integer) # 1
    sutta_ref = Column(String) # DN 1
    sutta_ref_pts = Column(String) # DN i 1
    language = Column(String) # pli / en
    order_index = Column(Integer)

    # --- Content props ---
    title = Column(String) # Brahmajāla: The Root of All Things
    title_pali = Column(String) # Brahmajāla
    title_trans = Column(String) # The Root of All Things
    description = Column(String)
    content_plain = Column(String) # content in plain text
    content_html = Column(String) # content in HTML

    # --- Source ---
    source_info = Column(String)
    source_language = Column(String)
    message = Column(String)
    copyright = Column(String)
    license = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    authors = relationship("Author", secondary=assoc_sutta_authors, back_populates="suttas")


class Dictionary(Base):
    __tablename__ = "dictionaries"

    id = Column(Integer, primary_key=True)
    label = Column(String, nullable=False, unique=True)
    title = Column(String, nullable=False)

    creator = Column(String)
    description = Column(String)
    feedback_email = Column(String)
    feedback_url = Column(String)
    version = Column(String)
    data_zip_url = Column(String)
    info_json_url = Column(String)
    url_synced_at = Column(String)
    has_update = Column(Boolean)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    dict_words = relationship("DictWord", back_populates="dictionary", passive_deletes=True)


class DictWord(Base):
    __tablename__ = "dict_words"

    id = Column(Integer, primary_key=True)
    dictionary_id = Column(Integer, ForeignKey("dictionaries.id", ondelete="CASCADE"), nullable=False)
    uid = Column(String, nullable=False, unique=True)
    word = Column(String, nullable=False)
    word_nom_sg = Column(String)
    inflections = Column(String)
    phonetic = Column(String)
    transliteration = Column(String)

    # --- Meaning ---
    meaning_order = Column(Integer)
    definition_plain = Column(String)
    definition_html = Column(String)
    summary = Column(String)

    # --- Associated words ---
    synonyms = Column(String)
    antonyms = Column(String)
    homonyms = Column(String)
    also_written_as = Column(String)
    see_also = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    dictionary = relationship("Dictionary", back_populates="dict_words")
    examples = relationship("Example", back_populates="dict_word", passive_deletes=True)


class Example(Base):
    __tablename__ = "examples"

    id = Column(Integer, primary_key=True)
    dict_word_id = Column(Integer, ForeignKey("dict_words.id", ondelete="CASCADE"), nullable=False)

    source_ref = Column(String)
    source_title = Column(String)

    text_html = Column(String)
    translation_html = Column(String)

    highlight = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    dict_word = relationship("DictWord", back_populates="examples")


class Document(Base):
    __tablename__ = "documents"

    id = Column(Integer, primary_key=True)
    filepath = Column(String, nullable=False, unique=True)
    title = Column(String)
    author = Column(String)

    # --- Cover ---
    cover_data = Column(LargeBinary)
    cover_width = Column(Integer)
    cover_height = Column(Integer)
    cover_stride = Column(Integer)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    tags = relationship("Tag", secondary=assoc_document_tags, back_populates="documents")


class Deck(Base):
    __tablename__ = "decks"

    id = Column(Integer, primary_key=True)
    name = Column(String, nullable=False, unique=True)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    memos = relationship("Memo", back_populates="deck")


class Memo(Base):
    __tablename__ = "memos"

    id = Column(Integer, primary_key=True)
    deck_id = Column(Integer, ForeignKey("decks.id", ondelete="NO ACTION"))

    # --- Content ---
    fields_json = Column(String) # Front, Back

    anki_model_name = Column(String) # Basic, Cloze
    anki_note_id = Column(Integer)
    anki_synced_at = Column(DateTime)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    deck = relationship("Deck", back_populates="memos")
    tags = relationship("Tag", secondary=assoc_memo_tags, back_populates="memos")


class MemoAssociation(Base):
    __tablename__ = "memo_associations"

    id = Column(Integer, primary_key=True)
    memo_id = Column(Integer, ForeignKey("memos.id", ondelete="CASCADE"), nullable=False)
    associated_table = Column(String, nullable=False)
    associated_id = Column(Integer, nullable=False)

    page_number = Column(Integer)
    location = Column(String)


class Annotation(Base):
    __tablename__ = "annotations"

    id = Column(Integer, primary_key=True)
    ann_type = Column(String)
    text = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())


class AnnotationAssociation(Base):
    __tablename__ = "annotation_associations"

    id = Column(Integer, primary_key=True)
    annotation_id = Column(Integer, ForeignKey("annotations.id", ondelete="CASCADE"))
    associated_table = Column(String, nullable=False)
    associated_id = Column(Integer, nullable=False)

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
    from_table = Column(String, nullable=False)
    from_id = Column(Integer, nullable=False)
    from_page_number = Column(Integer)
    to_table = Column(String, nullable=False)
    to_id = Column(Integer, nullable=False)
    to_page_number = Column(Integer)


class AppSetting(Base):
    __tablename__ = "app_settings"

    id = Column(Integer, primary_key=True)
    key = Column(String, nullable=False, unique=True)
    value = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())
