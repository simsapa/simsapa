from typing import List, Optional
from sqlalchemy import (MetaData, Table, Column, Integer,
                        ForeignKey, DateTime, LargeBinary)

from sqlalchemy.sql import func
from sqlalchemy.orm import relationship, mapped_column, Mapped, declarative_base

from simsapa import DbSchemaName

# -------------------------------------------------------------------------
# NOTE: The schema label identifies the attached database. This is the only
# difference between appdata_models.py and userdata_models.py.
# -------------------------------------------------------------------------

metadata = MetaData(schema=DbSchemaName.UserData.value)
Base = declarative_base(metadata=metadata)

assoc_sutta_authors = Table(
    'sutta_authors',
    Base.metadata,
    Column('sutta_id', Integer, ForeignKey("suttas.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('author_id', Integer, ForeignKey("authors.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_sutta_tags = Table(
    'sutta_tags',
    Base.metadata,
    Column('sutta_id', Integer, ForeignKey("suttas.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_sutta_multi_refs = Table(
    'sutta_multi_refs',
    Base.metadata,
    Column('sutta_id', Integer, ForeignKey("suttas.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('multi_ref_id', Integer, ForeignKey("multi_refs.id", ondelete="CASCADE"), primary_key=True, nullable=False),
)

assoc_dict_word_tags = Table(
    'dict_word_tags',
    Base.metadata,
    Column('dict_word_id', Integer, ForeignKey("dict_words.id", ondelete="CASCADE"), primary_key=True, nullable=False),
    Column('tag_id', Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, nullable=False),
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

    id: Mapped[int] = mapped_column(primary_key=True)
    uid: Mapped[str] = mapped_column(unique=True) # sujato
    full_name: Mapped[Optional[str]] # Sujato Bhikkhu
    description: Mapped[Optional[str]] # Translated for SuttaCentral by Sujato Bhikkhu

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    suttas: Mapped[List["Sutta"]] = relationship(secondary=assoc_sutta_authors, back_populates="authors")

class Sutta(Base):
    __tablename__ = "suttas"

    id:  Mapped[int] = mapped_column(primary_key=True)
    uid: Mapped[str] = mapped_column(unique=True) # dn1/pli/ms
    group_path:  Mapped[Optional[str]] # /sutta-pitaka/digha-nikaya/silakkhandha-vagga
    group_index: Mapped[Optional[int]] # 1
    sutta_ref:   Mapped[Optional[str]] # DN 1
    language:    Mapped[Optional[str]] # pli / en
    order_index: Mapped[Optional[int]]

    # sn30.7-16
    sutta_range_group: Mapped[Optional[str]] # sn30
    sutta_range_start: Mapped[Optional[int]] # 7
    sutta_range_end:   Mapped[Optional[int]] # 16

    # --- Content props ---
    title:             Mapped[Optional[str]] # Brahmajāla: The Root of All Things
    title_pali:        Mapped[Optional[str]] # Brahmajāla
    title_trans:       Mapped[Optional[str]] # The Root of All Things
    description:       Mapped[Optional[str]]
    content_plain:     Mapped[Optional[str]] # content in plain text
    content_html:      Mapped[Optional[str]] # content in HTML
    content_json:      Mapped[Optional[str]] # content in Bilara JSON
    content_json_tmpl: Mapped[Optional[str]] # HTML template to wrap around JSON

    # --- Source ---
    source_uid:      Mapped[Optional[str]] # ms, bodhi, than
    source_info:     Mapped[Optional[str]]
    source_language: Mapped[Optional[str]]
    message:         Mapped[Optional[str]]
    copyright:       Mapped[Optional[str]]
    license:         Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())
    indexed_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True))

    authors:    Mapped[List[Author]]     = relationship("Author",       secondary=assoc_sutta_authors,    back_populates="suttas")
    tags:       Mapped[List["Tag"]]      = relationship("Tag",          secondary=assoc_sutta_tags,       back_populates="suttas")
    multi_refs: Mapped[List["MultiRef"]] = relationship("MultiRef",     secondary=assoc_sutta_multi_refs, back_populates="suttas")
    bookmarks:  Mapped[List["Bookmark"]] = relationship("Bookmark",     back_populates="sutta", passive_deletes=True)
    variant:    Mapped["SuttaVariant"]   = relationship("SuttaVariant", back_populates="sutta", passive_deletes=True, uselist=False)
    comment:    Mapped["SuttaComment"]   = relationship("SuttaComment", back_populates="sutta", passive_deletes=True, uselist=False)

class SuttaVariant(Base):
    __tablename__ = "sutta_variants"

    id:        Mapped[int] = mapped_column(primary_key=True)
    sutta_id:  Mapped[int] = mapped_column(ForeignKey("suttas.id", ondelete="CASCADE"), nullable=False)
    sutta_uid: Mapped[str] # dn1/pli/ms

    language:     Mapped[Optional[str]] # pli / en
    source_uid:   Mapped[Optional[str]] # ms, bodhi, than
    content_json: Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    sutta: Mapped[Sutta] = relationship("Sutta", back_populates="variant", uselist=False)

class SuttaComment(Base):
    __tablename__ = "sutta_comments"

    id:        Mapped[int] = mapped_column(primary_key=True)
    sutta_id:  Mapped[int] = mapped_column(ForeignKey("suttas.id", ondelete="CASCADE"), nullable=False)
    sutta_uid: Mapped[str] # dn1/pli/ms

    language:     Mapped[Optional[str]] # pli / en
    source_uid:   Mapped[Optional[str]] # ms, bodhi, than
    content_json: Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    sutta: Mapped[Sutta] = relationship("Sutta", back_populates="comment", uselist=False)

class Dictionary(Base):
    __tablename__ = "dictionaries"

    id:    Mapped[int] = mapped_column(primary_key=True)
    label: Mapped[str] = mapped_column(unique=True)
    title: Mapped[str]

    creator:        Mapped[Optional[str]]
    description:    Mapped[Optional[str]]
    feedback_email: Mapped[Optional[str]]
    feedback_url:   Mapped[Optional[str]]
    version:        Mapped[Optional[str]]
    data_zip_url:   Mapped[Optional[str]]
    info_json_url:  Mapped[Optional[str]]
    url_synced_at:  Mapped[Optional[str]]
    has_update:     Mapped[Optional[bool]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    dict_words: Mapped[List["DictWord"]] = relationship("DictWord", back_populates="dictionary", passive_deletes=True)

class DictWord(Base):
    __tablename__ = "dict_words"

    id: Mapped[int] = mapped_column(primary_key=True)
    dictionary_id: Mapped[int] = mapped_column(ForeignKey("dictionaries.id", ondelete="CASCADE"), nullable=False)

    uid:             Mapped[str] = mapped_column(unique=True)
    word:            Mapped[str]
    source_uid:      Mapped[Optional[str]] # pts, dpd, mw
    word_nom_sg:     Mapped[Optional[str]]
    inflections:     Mapped[Optional[str]]
    phonetic:        Mapped[Optional[str]]
    transliteration: Mapped[Optional[str]]

    # --- Meaning ---
    meaning_order:    Mapped[Optional[int]]
    definition_plain: Mapped[Optional[str]]
    definition_html:  Mapped[Optional[str]]
    summary:          Mapped[Optional[str]]

    # --- Associated words ---
    synonyms: Mapped[Optional[str]]
    antonyms: Mapped[Optional[str]]
    homonyms: Mapped[Optional[str]]
    also_written_as: Mapped[Optional[str]]
    see_also: Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())
    indexed_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True))

    dictionary: Mapped[Dictionary]    = relationship("Dictionary", back_populates="dict_words")
    examples: Mapped[List["Example"]] = relationship("Example",    back_populates="dict_word", passive_deletes=True)
    tags: Mapped[List["Tag"]]         = relationship("Tag",        secondary=assoc_dict_word_tags, back_populates="dict_words")

class Example(Base):
    __tablename__ = "examples"

    id: Mapped[int] = mapped_column(primary_key=True)
    dict_word_id: Mapped[int] = mapped_column(ForeignKey("dict_words.id", ondelete="CASCADE"), nullable=False)

    source_ref:       Mapped[Optional[str]]
    source_title:     Mapped[Optional[str]]
    text_html:        Mapped[Optional[str]]
    translation_html: Mapped[Optional[str]]
    highlight:        Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    dict_word: Mapped[DictWord] = relationship("DictWord", back_populates="examples")

class Document(Base):
    __tablename__ = "documents"

    id: Mapped[int] = mapped_column(primary_key=True)
    filepath: Mapped[str] = mapped_column(unique=True)
    title: Mapped[Optional[str]]
    author: Mapped[Optional[str]]

    # --- Cover ---
    cover_data = Column(LargeBinary)
    cover_width: Mapped[Optional[int]]
    cover_height: Mapped[Optional[int]]
    cover_stride: Mapped[Optional[int]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    tags: Mapped[List["Tag"]] = relationship("Tag", secondary=assoc_document_tags, back_populates="documents")

class Deck(Base):
    __tablename__ = "decks"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(unique=True)

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    memos: Mapped[List["Memo"]] = relationship("Memo", back_populates="deck")

class Memo(Base):
    __tablename__ = "memos"

    id: Mapped[int] = mapped_column(primary_key=True)
    deck_id: Mapped[Optional[int]] = mapped_column(ForeignKey("decks.id", ondelete="CASCADE"))

    # --- Content ---
    fields_json: Mapped[Optional[str]] # Front, Back

    anki_model_name: Mapped[Optional[str]] # Basic, Cloze
    anki_note_id: Mapped[Optional[int]]
    anki_synced_at = Mapped[Optional[DateTime]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    deck: Mapped[Deck] = relationship("Deck", back_populates="memos")
    tags: Mapped[List["Tag"]] = relationship("Tag", secondary=assoc_memo_tags, back_populates="memos")

class MemoAssociation(Base):
    __tablename__ = "memo_associations"

    id: Mapped[int] = mapped_column(primary_key=True)
    memo_id: Mapped[int] = mapped_column(ForeignKey("memos.id", ondelete="CASCADE"), nullable=False)

    associated_table: Mapped[str]
    associated_id: Mapped[int]

    page_number: Mapped[Optional[int]]
    location: Mapped[Optional[str]]

class Annotation(Base):
    __tablename__ = "annotations"

    id: Mapped[int] = mapped_column(primary_key=True)
    ann_type: Mapped[Optional[str]]
    text: Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

class AnnotationAssociation(Base):
    __tablename__ = "annotation_associations"

    id: Mapped[int] = mapped_column(primary_key=True)
    annotation_id: Mapped[Optional[int]] = mapped_column(ForeignKey("annotations.id", ondelete="CASCADE"))
    associated_table: Mapped[str]
    associated_id: Mapped[str]

    page_number: Mapped[Optional[int]]
    location: Mapped[Optional[int]]

class Tag(Base):
    __tablename__ = "tags"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[Optional[str]]

    suttas:     Mapped[List[Sutta]]     = relationship("Sutta",    secondary=assoc_sutta_tags, back_populates="tags")
    dict_words: Mapped[List[DictWord]]  = relationship("DictWord", secondary=assoc_dict_word_tags, back_populates="tags")
    documents:  Mapped[List[Document]]  = relationship("Document", secondary=assoc_document_tags, back_populates="tags")
    memos:      Mapped[List[Memo]]      = relationship("Memo",     secondary=assoc_memo_tags, back_populates="tags")

class Bookmark(Base):
    __tablename__ = "bookmarks"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str]

    quote: Mapped[Optional[str]]
    nth: Mapped[Optional[int]]
    selection_range: Mapped[Optional[str]]

    sutta_id:     Mapped[Optional[int]] = mapped_column(ForeignKey("suttas.id", ondelete="CASCADE"))
    sutta_uid:    Mapped[Optional[str]]
    sutta_schema: Mapped[Optional[str]]
    sutta_ref:    Mapped[Optional[str]]
    sutta_title:  Mapped[Optional[str]]

    comment_text: Mapped[Optional[str]]
    comment_attr_json: Mapped[Optional[str]]

    read_only: Mapped[Optional[bool]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    sutta: Mapped[Sutta] = relationship("Sutta", back_populates="bookmarks")

class Link(Base):
    __tablename__ = "links"

    id: Mapped[int] = mapped_column(primary_key=True)
    label: Mapped[Optional[str]]

    from_table:       Mapped[str]
    from_id:          Mapped[int]
    from_page_number: Mapped[Optional[int]]
    from_target:      Mapped[Optional[str]]

    to_table:       Mapped[str]
    to_id:          Mapped[int]
    to_page_number: Mapped[Optional[int]]
    to_target:      Mapped[Optional[str]]

class GptPrompt(Base):
    __tablename__ = "gpt_prompts"

    id: Mapped[int] = mapped_column(primary_key=True)
    name_path:       Mapped[str] = mapped_column(unique=True)
    prompt_text:     Mapped[Optional[str]]
    show_in_context: Mapped[Optional[bool]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

class GptHistory(Base):
    __tablename__ = "gpt_history"

    id: Mapped[int] = mapped_column(primary_key=True)
    name_path:       Mapped[Optional[str]]
    prompt_text:     Mapped[Optional[str]]
    completion_text: Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

class AppSetting(Base):
    __tablename__ = "app_settings"

    id: Mapped[int] = mapped_column(primary_key=True)
    key: Mapped[str] = mapped_column(unique=True)
    value: Mapped[Optional[str]]

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

class ChallengeCourse(Base):
    __tablename__ = "challenge_courses"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(unique=True)

    description:    Mapped[Optional[str]]
    course_dirname: Mapped[Optional[str]]
    sort_index:     Mapped[int]

    groups: Mapped[List["ChallengeGroup"]] = relationship("ChallengeGroup", back_populates="course")
    challenges: Mapped[List["Challenge"]] = relationship("Challenge", back_populates="course")

class ChallengeGroup(Base):
    __tablename__ = "challenge_groups"

    id: Mapped[int] = mapped_column(primary_key=True)
    course_id: Mapped[Optional[int]] = mapped_column(ForeignKey("challenge_courses.id", ondelete="CASCADE"))

    name:        Mapped[str] = mapped_column(unique=True)
    description: Mapped[Optional[str]]
    sort_index:  Mapped[int]

    course: Mapped[ChallengeCourse] = relationship("ChallengeCourse", back_populates="groups", uselist=False)
    challenges: Mapped[List["Challenge"]] = relationship("Challenge", back_populates="group")

class Challenge(Base):
    __tablename__ = "challenges"

    id: Mapped[int] = mapped_column(primary_key=True)
    course_id: Mapped[Optional[int]] = mapped_column(ForeignKey("challenge_courses.id", ondelete="CASCADE"))
    group_id: Mapped[Optional[int]] = mapped_column(ForeignKey("challenge_groups.id", ondelete="CASCADE"))

    challenge_type: Mapped[str]
    sort_index:     Mapped[int]
    explanation_md: Mapped[Optional[str]]
    question_json:  Mapped[Optional[str]]
    answers_json:   Mapped[Optional[str]]
    level:          Mapped[Optional[int]]

    studied_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True))
    due_at:     Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True))

    anki_model_name: Mapped[Optional[str]] # Basic, Cloze
    anki_note_id:    Mapped[Optional[int]]
    anki_synced_at:  Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True))

    created_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(DateTime(timezone=True), onupdate=func.now())

    course: Mapped[ChallengeCourse] = relationship("ChallengeCourse", back_populates="challenges", uselist=False)
    group: Mapped[ChallengeGroup] = relationship("ChallengeGroup", back_populates="challenges", uselist=False)

class MultiRef(Base):
    __tablename__ = "multi_refs"

    id: Mapped[int] = mapped_column(primary_key=True)

    # lowercase an / mn / kp / etc.
    collection: Mapped[str]

    # AN 2.52-63
    # PTS 1.77.1-1.80.1
    # PTS i 77.1 - i 80.1

    ref_type: Mapped[Optional[str]] # sc / pts / dpr / cst4 / bodhi / verse / trad

    # Ref may contain a list of references, separated by commas.
    ref: Mapped[Optional[str]] # sn 1.51 / an 1.77.1-1.80.1 / an i 77.1 - i 80.1
    sutta_uid: Mapped[Optional[str]] # sn1.51

    edition: Mapped[Optional[str]] # 1st ed. Feer (1884) / 2nd ed. Somaratne (1998) / etc.

    name: Mapped[Optional[str]]
    biblio_uid: Mapped[Optional[str]]

    nipata_number: Mapped[Optional[int]] # 1 in AN 1.8.3
    vagga_number: Mapped[Optional[int]] # 8
    sutta_number: Mapped[Optional[int]] # 3

    volume: Mapped[Optional[int]]
    page_start: Mapped[Optional[int]]
    page_end: Mapped[Optional[int]]
    par_start: Mapped[Optional[int]]
    par_end: Mapped[Optional[int]]

    sutta_start: Mapped[Optional[int]] # 52 in AN 2.52-63
    sutta_end: Mapped[Optional[int]] # 63 in AN 2.52-63

    verse_start: Mapped[Optional[int]]
    verse_end: Mapped[Optional[int]]

    suttas: Mapped[List[Sutta]] = relationship("Sutta", secondary=assoc_sutta_multi_refs, back_populates="multi_refs")
