from sqlalchemy import (MetaData, Table, Column, Integer, String,
                        ForeignKey, Boolean, DateTime, LargeBinary)

from sqlalchemy.sql import func
from sqlalchemy.orm import relationship
from sqlalchemy.ext.declarative import declarative_base

from simsapa import DbSchemaName

# -------------------------------------------------------------------------
# NOTE: The schema label identifies the attached database. This is the only
# difference between appdata_models.py and userdata_models.py.
# -------------------------------------------------------------------------

metadata = MetaData(schema=DbSchemaName.UserData.value)
Base = declarative_base(metadata=metadata)

assoc_sutta_authors = Table(
    'sutta_authors',
    Base.metadata, # type: ignore
    Column('sutta_id', Integer, ForeignKey("suttas.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('author_id', Integer, ForeignKey("authors.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_sutta_tags = Table(
    'sutta_tags',
    Base.metadata, # type: ignore
    Column('sutta_id', Integer, ForeignKey("suttas.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_sutta_multi_refs = Table(
    'sutta_multi_refs',
    Base.metadata, # type: ignore
    Column('sutta_id', Integer, ForeignKey("suttas.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('multi_ref_id', Integer, ForeignKey("multi_refs.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_dict_word_tags = Table(
    'dict_word_tags',
    Base.metadata, # type: ignore
    Column('dict_word_id', Integer, ForeignKey("dict_words.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_document_tags = Table(
    'document_tags',
    Base.metadata, # type: ignore
    Column('document_id', Integer, ForeignKey("documents.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_memo_tags = Table(
    'memo_tags',
    Base.metadata, # type: ignore
    Column('memo_id', Integer, ForeignKey("memos.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)


class Author(Base): # type: ignore
    __tablename__ = "authors"

    id = Column(Integer, primary_key=True)
    uid = Column(String, nullable=False, unique=True) # sujato
    full_name = Column(String) # Sujato Bhikkhu
    description = Column(String) # Translated for SuttaCentral by Sujato Bhikkhu

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    suttas = relationship("Sutta", secondary=assoc_sutta_authors, back_populates="authors")


class Sutta(Base): # type: ignore
    __tablename__ = "suttas"

    id = Column(Integer, primary_key=True)
    uid = Column(String, nullable=False, unique=True) # dn1/pli/ms
    group_path = Column(String) # /sutta-pitaka/digha-nikaya/silakkhandha-vagga
    group_index = Column(Integer) # 1
    sutta_ref = Column(String) # DN 1
    language = Column(String) # pli / en
    order_index = Column(Integer)

    # sn30.7-16
    sutta_range_group = Column(String) # sn30
    sutta_range_start = Column(Integer) # 7
    sutta_range_end = Column(Integer) # 16

    # --- Content props ---
    title = Column(String) # Brahmajāla: The Root of All Things
    title_pali = Column(String) # Brahmajāla
    title_trans = Column(String) # The Root of All Things
    description = Column(String)
    content_plain = Column(String) # content in plain text
    content_html = Column(String) # content in HTML
    content_json = Column(String) # content in Bilara JSON
    content_json_tmpl = Column(String) # HTML template to wrap around JSON

    # --- Source ---
    source_uid = Column(String) # ms, bodhi, than
    source_info = Column(String)
    source_language = Column(String)
    message = Column(String)
    copyright = Column(String)
    license = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())
    indexed_at = Column(DateTime(timezone=True))

    authors = relationship("Author", secondary=assoc_sutta_authors, back_populates="suttas")
    tags = relationship("Tag", secondary=assoc_sutta_tags, back_populates="suttas")
    bookmarks = relationship("Bookmark", back_populates="sutta", passive_deletes=True)

    variant = relationship("SuttaVariant", back_populates="sutta", passive_deletes=True, uselist=False)
    comment = relationship("SuttaComment", back_populates="sutta", passive_deletes=True, uselist=False)

    multi_refs = relationship("MultiRef", secondary=assoc_sutta_multi_refs, back_populates="suttas")


class SuttaVariant(Base): # type: ignore
    __tablename__ = "sutta_variants"

    id = Column(Integer, primary_key=True)

    sutta_id = Column(Integer, ForeignKey("suttas.id", ondelete="CASCADE"), nullable=False)
    sutta_uid = Column(String, nullable=False) # dn1/pli/ms

    language = Column(String) # pli / en
    source_uid = Column(String) # ms, bodhi, than
    content_json = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    sutta = relationship("Sutta", back_populates="variant", uselist=False)


class SuttaComment(Base): # type: ignore
    __tablename__ = "sutta_comments"

    id = Column(Integer, primary_key=True)
    sutta_id = Column(Integer, ForeignKey("suttas.id", ondelete="CASCADE"), nullable=False)
    sutta_uid = Column(String, nullable=False) # dn1/pli/ms

    language = Column(String) # pli / en
    source_uid = Column(String) # ms, bodhi, than
    content_json = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    sutta = relationship("Sutta", back_populates="comment", uselist=False)


class Dictionary(Base): # type: ignore
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


class DictWord(Base): # type: ignore
    __tablename__ = "dict_words"

    id = Column(Integer, primary_key=True)
    dictionary_id = Column(Integer, ForeignKey("dictionaries.id", ondelete="CASCADE"), nullable=False)
    uid = Column(String, nullable=False, unique=True)
    source_uid = Column(String) # pts, dpd, mw
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
    indexed_at = Column(DateTime(timezone=True))

    dictionary = relationship("Dictionary", back_populates="dict_words")
    examples = relationship("Example", back_populates="dict_word", passive_deletes=True)
    tags = relationship("Tag", secondary=assoc_dict_word_tags, back_populates="dict_words")


class Example(Base): # type: ignore
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


class Document(Base): # type: ignore
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


class Deck(Base): # type: ignore
    __tablename__ = "decks"

    id = Column(Integer, primary_key=True)
    name = Column(String, nullable=False, unique=True)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    memos = relationship("Memo", back_populates="deck")


class Memo(Base): # type: ignore
    __tablename__ = "memos"

    id = Column(Integer, primary_key=True)
    deck_id = Column(Integer, ForeignKey("decks.id", ondelete="CASCADE"))

    # --- Content ---
    fields_json = Column(String) # Front, Back

    anki_model_name = Column(String) # Basic, Cloze
    anki_note_id = Column(Integer)
    anki_synced_at = Column(DateTime)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    deck = relationship("Deck", back_populates="memos")
    tags = relationship("Tag", secondary=assoc_memo_tags, back_populates="memos")


class MemoAssociation(Base): # type: ignore
    __tablename__ = "memo_associations"

    id = Column(Integer, primary_key=True)
    memo_id = Column(Integer, ForeignKey("memos.id", ondelete="CASCADE"), nullable=False)
    associated_table = Column(String, nullable=False)
    associated_id = Column(Integer, nullable=False)

    page_number = Column(Integer)
    location = Column(String)


class Annotation(Base): # type: ignore
    __tablename__ = "annotations"

    id = Column(Integer, primary_key=True)
    ann_type = Column(String)
    text = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())


class AnnotationAssociation(Base): # type: ignore
    __tablename__ = "annotation_associations"

    id = Column(Integer, primary_key=True)
    annotation_id = Column(Integer, ForeignKey("annotations.id", ondelete="CASCADE"))
    associated_table = Column(String, nullable=False)
    associated_id = Column(Integer, nullable=False)

    page_number = Column(Integer)
    location = Column(Integer)


class Tag(Base): # type: ignore
    __tablename__ = "tags"

    id = Column(Integer, primary_key=True)
    name = Column(String)

    suttas = relationship("Sutta", secondary=assoc_sutta_tags, back_populates="tags")
    dict_words = relationship("DictWord", secondary=assoc_dict_word_tags, back_populates="tags")
    documents = relationship("Document", secondary=assoc_document_tags, back_populates="tags")
    memos = relationship("Memo", secondary=assoc_memo_tags, back_populates="tags")


class Bookmark(Base): # type: ignore
    __tablename__ = "bookmarks"

    id = Column(Integer, primary_key=True)
    name = Column(String, nullable=False)
    quote = Column(String)
    nth = Column(Integer)
    selection_range = Column(String)

    sutta_id = Column(Integer, ForeignKey("suttas.id", ondelete="CASCADE"), nullable=True)
    sutta_uid = Column(String, nullable=True)
    sutta_schema = Column(String, nullable=True)
    sutta_ref = Column(String, nullable=True)
    sutta_title = Column(String, nullable=True)

    comment_text = Column(String)
    comment_attr_json = Column(String)

    read_only = Column(Boolean)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    sutta = relationship("Sutta", back_populates="bookmarks")


class Link(Base): # type: ignore
    __tablename__ = "links"

    id = Column(Integer, primary_key=True)
    label = Column(String)

    from_table = Column(String, nullable=False)
    from_id = Column(Integer, nullable=False)
    from_page_number = Column(Integer)
    from_target = Column(String)

    to_table = Column(String, nullable=False)
    to_id = Column(Integer, nullable=False)
    to_page_number = Column(Integer)
    to_target = Column(String)


class GptPrompt(Base): # type: ignore
    __tablename__ = "gpt_prompts"

    id = Column(Integer, primary_key=True)
    name_path = Column(String, nullable=False, unique=True)
    prompt_text = Column(String)
    show_in_context = Column(Boolean)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())


class GptHistory(Base): # type: ignore
    __tablename__ = "gpt_history"

    id = Column(Integer, primary_key=True)
    name_path = Column(String)
    prompt_text = Column(String)
    completion_text = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())


class AppSetting(Base): # type: ignore
    __tablename__ = "app_settings"

    id = Column(Integer, primary_key=True)
    key = Column(String, nullable=False, unique=True)
    value = Column(String)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())


class ChallengeCourse(Base): # type: ignore
    __tablename__ = "challenge_courses"

    id = Column(Integer, primary_key=True)
    name = Column(String, nullable=False, unique=True)
    description = Column(String)
    course_dirname = Column(String)

    sort_index = Column(Integer, nullable=False)

    groups = relationship("ChallengeGroup", back_populates="course")
    challenges = relationship("Challenge", back_populates="course")


class ChallengeGroup(Base): # type: ignore
    __tablename__ = "challenge_groups"

    id = Column(Integer, primary_key=True)
    course_id = Column(Integer, ForeignKey("challenge_courses.id", ondelete="CASCADE"))

    name = Column(String, nullable=False, unique=True)
    description = Column(String)

    sort_index = Column(Integer, nullable=False)

    course = relationship("ChallengeCourse", back_populates="groups", uselist=False)
    challenges = relationship("Challenge", back_populates="group")


class Challenge(Base): # type: ignore
    __tablename__ = "challenges"

    id = Column(Integer, primary_key=True)
    course_id = Column(Integer, ForeignKey("challenge_courses.id", ondelete="CASCADE"))
    group_id = Column(Integer, ForeignKey("challenge_groups.id", ondelete="CASCADE"))

    challenge_type = Column(String, nullable=False)

    sort_index = Column(Integer, nullable=False)

    explanation_md = Column(String)

    question_json = Column(String)
    answers_json = Column(String)

    level = Column(Integer)

    studied_at = Column(DateTime(timezone=True))
    due_at = Column(DateTime(timezone=True))

    anki_model_name = Column(String) # Basic, Cloze
    anki_note_id = Column(Integer)
    anki_synced_at = Column(DateTime)

    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), onupdate=func.now())

    course = relationship("ChallengeCourse", back_populates="challenges", uselist=False)
    group = relationship("ChallengeGroup", back_populates="challenges", uselist=False)


class MultiRef(Base): # type: ignore
    __tablename__ = "multi_refs"

    id = Column(Integer, primary_key=True)

    # lowercase an / mn / kp / etc.
    collection = Column(String, nullable=False)

    # AN 2.52-63
    # PTS 1.77.1-1.80.1
    # PTS i 77.1 - i 80.1

    ref_type = Column(String) # sc / pts / dpr / cst4 / bodhi / verse / trad

    # Ref may contain a list of references, separated by commas.
    ref = Column(String) # sn 1.51 / an 1.77.1-1.80.1 / an i 77.1 - i 80.1
    sutta_uid = Column(String) # sn1.51

    edition = Column(String) # 1st ed. Feer (1884) / 2nd ed. Somaratne (1998) / etc.

    name = Column(String)
    biblio_uid = Column(String)

    nipata_number = Column(Integer) # 1 in AN 1.8.3
    vagga_number = Column(Integer) # 8
    sutta_number = Column(Integer) # 3

    volume = Column(Integer)
    page_start = Column(Integer)
    page_end = Column(Integer)
    par_start = Column(Integer)
    par_end = Column(Integer)

    sutta_start = Column(Integer) # 52 in AN 2.52-63
    sutta_end = Column(Integer) # 63 in AN 2.52-63

    verse_start = Column(Integer)
    verse_end = Column(Integer)

    suttas = relationship("Sutta", secondary=assoc_sutta_multi_refs, back_populates="multi_refs")
