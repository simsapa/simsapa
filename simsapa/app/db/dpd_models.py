"""Datebase model for use by SQLAlchemy."""
import re

from typing import Any, Dict, List, Set
from typing import Optional


from sqlalchemy import and_
# from sqlalchemy import case
# from sqlalchemy import null
from sqlalchemy import Column
from sqlalchemy import DateTime
from sqlalchemy import ForeignKey
from sqlalchemy import ForeignKeyConstraint
from sqlalchemy import Integer
from sqlalchemy.ext.hybrid import hybrid_property
# from sqlalchemy.orm import DeclarativeBase
from sqlalchemy.orm import Mapped
from sqlalchemy.orm import mapped_column
from sqlalchemy.orm import relationship
from sqlalchemy.orm import declared_attr
from sqlalchemy.orm import object_session
from sqlalchemy.sql import func

from simsapa.dpd_db.tools.link_generator import generate_simsapa_link
from simsapa.dpd_db.tools.pali_sort_key import pali_sort_key

from sqlalchemy.orm import declarative_base
from sqlalchemy import MetaData
from simsapa import DbSchemaName

metadata = MetaData(schema=DbSchemaName.Dpd.value)
Base = declarative_base(metadata=metadata)

class InflectionTemplates(Base):
    __tablename__ = "inflection_templates"

    pattern: Mapped[str] = mapped_column(primary_key=True)
    like: Mapped[str] = mapped_column(default='')
    data: Mapped[str] = mapped_column(default='')

    def __repr__(self) -> str:
        return f"InflectionTemplates: {self.pattern} {self.like} {self.data}"


class PaliRoot(Base):
    __tablename__ = "pali_roots"

    root: Mapped[str] = mapped_column(primary_key=True)
    root_in_comps: Mapped[str] = mapped_column(default='')
    root_has_verb: Mapped[str] = mapped_column(default='')
    root_group: Mapped[int] = mapped_column(default=0)
    root_sign: Mapped[str] = mapped_column(default='')
    root_meaning: Mapped[str] = mapped_column(default='')
    sanskrit_root: Mapped[str] = mapped_column(default='')
    sanskrit_root_meaning: Mapped[str] = mapped_column(default='')
    sanskrit_root_class: Mapped[str] = mapped_column(default='')
    root_example: Mapped[str] = mapped_column(default='')
    dhatupatha_num: Mapped[str] = mapped_column(default='')
    dhatupatha_root: Mapped[str] = mapped_column(default='')
    dhatupatha_pali: Mapped[str] = mapped_column(default='')
    dhatupatha_english: Mapped[str] = mapped_column(default='')
    dhatumanjusa_num: Mapped[int] = mapped_column(default=0)
    dhatumanjusa_root: Mapped[str] = mapped_column(default='')
    dhatumanjusa_pali: Mapped[str] = mapped_column(default='')
    dhatumanjusa_english: Mapped[str] = mapped_column(default='')
    dhatumala_root: Mapped[str] = mapped_column(default='')
    dhatumala_pali: Mapped[str] = mapped_column(default='')
    dhatumala_english: Mapped[str] = mapped_column(default='')
    panini_root: Mapped[str] = mapped_column(default='')
    panini_sanskrit: Mapped[str] = mapped_column(default='')
    panini_english: Mapped[str] = mapped_column(default='')
    note: Mapped[str] = mapped_column(default='')
    matrix_test: Mapped[str] = mapped_column(default='')
    root_info: Mapped[str] = mapped_column(default='')
    root_matrix: Mapped[str] = mapped_column(default='')
    # ru_root_meaning: Mapped[str] = mapped_column(default='')
    # ru_sk_root_meaning: Mapped[str] = mapped_column(default='')

    created_at: Mapped[Optional[DateTime]] = mapped_column(
        DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(
        DateTime(timezone=True), onupdate=func.now())

    pw: Mapped[List["PaliWord"]] = relationship(
        back_populates="rt")

    # NOTE: In Simsapa we save these to the db.
    root_clean: Mapped[str] = mapped_column(default='')
    root_no_sign: Mapped[str] = mapped_column(default='')

    @property
    def calc_root_clean(self) -> str:
        return re.sub(r" \d.*$", "", self.root)

    @property
    def calc_root_no_sign(self) -> str:
        return re.sub(r"\d| |√", "", self.root)

    @property
    def root_(self) -> str:
        return self.root.replace(" ", "_")

    @property
    def root_link(self) -> str:
        return self.root.replace(" ", "%20")

    @property
    def root_count(self) -> int:
        db_session = object_session(self)
        if db_session is None:
            raise Exception("No db_session")

        return db_session.query(
            PaliWord
            ).filter(
                PaliWord.root_key == self.root
            ).count()

    @property
    def root_family_list(self) -> list:
        db_session = object_session(self)
        if db_session is None:
            raise Exception("No db_session")

        results = db_session.query(
            PaliWord
        ).filter(
            PaliWord.root_key == self.root
        ).group_by(
            PaliWord.family_root
        ).all()
        family_list = [i.family_root for i in results if i.family_root is not None]
        family_list = sorted(family_list, key=lambda x: pali_sort_key(x))
        return family_list

    def __repr__(self) -> str:
        return f"""PaliRoot: {self.root} {self.root_group} {self.root_sign} ({self.root_meaning})"""

    # === Used in Simsapa ===

    dictionary_id: Mapped[int] = mapped_column(nullable=False)

    uid: Mapped[str] = mapped_column(unique=True)
    word_ascii: Mapped[str]

    @property
    def as_dict(self) -> Dict[str, Any]:
        keys = ['root', 'root_in_comps', 'root_has_verb', 'root_group',
                'root_sign', 'root_meaning', 'sanskrit_root',
                'sanskrit_root_meaning', 'sanskrit_root_class', 'root_example',
                'dhatupatha_num', 'dhatupatha_root', 'dhatupatha_pali',
                'dhatupatha_english', 'dhatumanjusa_num', 'dhatumanjusa_root',
                'dhatumanjusa_pali', 'dhatumanjusa_english', 'dhatumala_root',
                'dhatumala_pali', 'dhatumala_english', 'panini_root',
                'panini_sanskrit', 'panini_english', 'note', 'matrix_test',
                'root_info', 'root_matrix', 'root_clean', 'root_no_sign',
                'dictionary_id', 'uid', 'word_ascii']

        d: Dict[str, Any] = dict()

        for k in keys:
            if k not in self.__dict__.keys():
                raise Exception(f"Key not found in Dpd.PaliRoot: {k}")
            d[k] = self.__dict__[k]

        return d

    # Properties for compatibility with DictWord, when used in a Union.

    @property
    def word(self) -> str:
        return self.root

    @property
    def language(self) -> str:
        return "en"

    @property
    def source_uid(self) -> str:
        return "dpd"

    @property
    def word_nom_sg(self) -> str:
        return ""

    @property
    def inflections(self) -> str:
        return ""

    @property
    def transliteration(self) -> str:
        return ""

    @property
    def meaning_order(self) -> int:
        return 1

    @property
    def definition_plain(self) -> str:
        return self.root_meaning

    @property
    def definition_html(self) -> str:
        return ""

    @property
    def summary(self) -> str:
        return ""

    @property
    def examples(self) -> list:
        return []

    @property
    def synonyms(self) -> list:
        return []

class FamilyRoot(Base):
    __tablename__ = "family_root"
    root_family_key: Mapped[str] = mapped_column(primary_key=True)
    root_key: Mapped[str] = mapped_column()
    root_family: Mapped[str] = mapped_column()
    html: Mapped[str] = mapped_column(default='')
    count: Mapped[int] = mapped_column(default=0)

    __table_args__ = (
        ForeignKeyConstraint(
            ["root_key", "root_family"],
            ["pali_words.root_key", "pali_words.family_root"]
        ),
    )

    @property
    def root_family_link(self) -> str:
        return self.root_family.replace(" ", "%20")

    @property
    def root_family_(self) -> str:
        return self.root_family.replace(" ", "_")

    def __repr__(self) -> str:
        return f"FamilyRoot: {self.root_key} {self.root_family} {self.count}"


class PaliWord(Base):
    __tablename__ = "pali_words"

    id: Mapped[int] = mapped_column(primary_key=True)
    pali_1: Mapped[str] = mapped_column(unique=True)
    pali_2: Mapped[str] = mapped_column(default='')
    pos: Mapped[str] = mapped_column(default='')
    grammar: Mapped[str] = mapped_column(default='')
    derived_from: Mapped[str] = mapped_column(default='')
    neg: Mapped[str] = mapped_column(default='')
    verb: Mapped[str] = mapped_column(default='')
    trans:  Mapped[str] = mapped_column(default='')
    plus_case:  Mapped[str] = mapped_column(default='')

    meaning_1: Mapped[str] = mapped_column(default='')
    meaning_lit: Mapped[str] = mapped_column(default='')
    meaning_2: Mapped[str] = mapped_column(default='')

    non_ia: Mapped[str] = mapped_column(default='')
    sanskrit: Mapped[str] = mapped_column(default='')

    root_key: Mapped[str] = mapped_column(
        ForeignKey("pali_roots.root"), default='')
    root_sign: Mapped[str] = mapped_column(default='')
    root_base: Mapped[str] = mapped_column(default='')

    family_root: Mapped[str] = mapped_column(default='')
    family_word: Mapped[str] = mapped_column(
        ForeignKey("family_word.word_family"), default='')
    family_compound: Mapped[str] = mapped_column(default='')
    family_set: Mapped[str] = mapped_column(default='')

    construction:  Mapped[str] = mapped_column(default='')
    derivative: Mapped[str] = mapped_column(default='')
    suffix: Mapped[str] = mapped_column(default='')
    phonetic: Mapped[str] = mapped_column(default='')
    compound_type: Mapped[str] = mapped_column(default='')
    compound_construction: Mapped[str] = mapped_column(default='')
    non_root_in_comps: Mapped[str] = mapped_column(default='')

    source_1: Mapped[str] = mapped_column(default='')
    sutta_1: Mapped[str] = mapped_column(default='')
    example_1: Mapped[str] = mapped_column(default='')

    source_2: Mapped[str] = mapped_column(default='')
    sutta_2: Mapped[str] = mapped_column(default='')
    example_2: Mapped[str] = mapped_column(default='')

    antonym: Mapped[str] = mapped_column(default='')
    synonym: Mapped[str] = mapped_column(default='')
    variant: Mapped[str] = mapped_column(default='')
    commentary: Mapped[str] = mapped_column(default='')
    notes: Mapped[str] = mapped_column(default='')
    cognate: Mapped[str] = mapped_column(default='')
    link: Mapped[str] = mapped_column(default='')
    origin: Mapped[str] = mapped_column(default='')

    stem: Mapped[str] = mapped_column(default='')
    pattern: Mapped[str] = mapped_column(
        ForeignKey("inflection_templates.pattern"), default='')

    created_at: Mapped[Optional[DateTime]] = mapped_column(
        DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[Optional[DateTime]] = mapped_column(
        DateTime(timezone=True), onupdate=func.now())

    # pali_root
    rt: Mapped[PaliRoot] = relationship(uselist=False)

    # family_root
    fr = relationship(
        FamilyRoot, 
        primaryjoin=and_(
            root_key==FamilyRoot.root_key, 
            family_root==FamilyRoot.root_family),
        uselist=False 
    )

    # derived data
    dd = relationship("DerivedData", uselist=False)

    # sbs
    sbs = relationship("SBS", uselist=False)

    # russion
    ru = relationship("Russian", uselist=False)

    # inflection templates
    it: Mapped[InflectionTemplates] = relationship()


    @hybrid_property
    def root_family_key(self):
        if self.root_key and self.family_root:
            return f"{self.root_key} {self.family_root}"
        else:
            return ""

    # @root_family_key.expression
    # def root_family_key(cls):
    #     return case(
    #         (and_(cls.root_key != null(), cls.family_root != null()),\
    #              cls.root_key + ' ' + cls.family_root), else_="")
    
    @property
    def pali_1_(self) -> str:
        return self.pali_1.replace(" ", "_").replace(".", "_")

    @property
    def pali_link(self) -> str:
        return self.pali_1.replace(" ", "%20")

    # NOTE: In Simsapa we save this to the db.
    pali_clean: Mapped[str] = mapped_column(default='')

    @property
    def calc_pali_clean(self) -> str:
        return re.sub(r" \d.*$", "", self.pali_1)

    @property
    def root_clean(self) -> str:
        try:
            if self.root_key is None:
                return ""
            else:
                return re.sub(r" \d.*$", "", self.root_key)
        except Exception as e:
            print(f"{self.pali_1}: {e}")
            return ""

    @property
    def family_compound_list(self) -> list:
        if self.family_compound:
            return self.family_compound.split(" ")
        else:
            return [self.family_compound]

    @property
    def family_set_list(self) -> list:
        if self.family_set:
            return self.family_set.split("; ")
        else:
            return [self.family_set]

    @property
    def root_count(self) -> int:
        db_session = object_session(self)
        if db_session is None:
            raise Exception("No db_session")

        return db_session.query(
            PaliWord.id
        ).filter(
            PaliWord.root_key == self.root_key
        ).count()

    @property
    def pos_list(self) -> list:
        db_session = object_session(self)
        if db_session is None:
            raise Exception("No db_session")

        pos_db = db_session.query(
            PaliWord.pos
        ).group_by(
            PaliWord.pos
        ).all()
        return sorted([i.pos for i in pos_db])

    @property
    def synonym_list(self) -> list:
        if self.synonym:
            return self.synonym.split(", ")
        else:
            return [self.synonym]

    @property
    def variant_list(self) -> list:
        if self.variant:
            return self.variant.split(", ")
        else:
            return [self.variant]

    @property
    def source_link_1(self) -> str:
        return generate_simsapa_link(self.source_1, self.example_1) if self.source_1 else ""

    @property
    def source_link_2(self) -> str:
        return generate_simsapa_link(self.source_2, self.example_2) if self.source_2 else ""

    @property
    def source_link_sutta(self) -> str:
        if self.meaning_2:
            if self.family_set.startswith("suttas of") or self.family_set == "bhikkhupātimokkha rules":
                unified_pattern = r"\(([A-Z]+)\s?([\d\.]+)\)|([A-Z]+)[\s]?([\d]+)"
                match = re.finditer(unified_pattern, self.meaning_2)
                    
                for m in match:
                    prefix = m.group(1) if m.group(1) else m.group(3)
                    number = m.group(2) if m.group(2) else m.group(4)
                    
                    combined_number = f"{prefix}{number}" if prefix and number else None
                    
                    if combined_number:
                        link = generate_simsapa_link(combined_number)

                        if link:
                            return link

            return ""
        else:
            return ""


    def __repr__(self) -> str:
        return f"""PaliWord: {self.id} {self.pali_1} {self.pos} {
            self.meaning_1}"""

    # === Used in Simsapa ===

    dictionary_id: Mapped[int] = mapped_column(nullable=False)

    uid: Mapped[str] = mapped_column(unique=True)
    word_ascii: Mapped[str]

    @property
    def as_dict(self) -> Dict[str, Any]:
        keys = ['id', 'pali_1', 'pali_2', 'pos', 'grammar', 'derived_from', 'neg',
                'verb', 'trans', 'plus_case', 'meaning_1', 'meaning_lit', 'meaning_2',
                'non_ia', 'sanskrit', 'root_key', 'root_sign', 'root_base', 'family_root',
                'family_word', 'family_compound', 'family_set', 'construction',
                'derivative', 'suffix', 'phonetic', 'compound_type',
                'compound_construction', 'non_root_in_comps', 'source_1', 'sutta_1',
                'example_1', 'source_2', 'sutta_2', 'example_2', 'antonym', 'synonym',
                'variant', 'commentary', 'notes', 'cognate', 'link', 'origin', 'stem',
                'pattern', 'pali_clean', 'dictionary_id', 'uid', 'word_ascii']

        d: Dict[str, Any] = dict()

        for k in keys:
            if k not in self.__dict__.keys():
                raise Exception(f"Key not found in Dpd.PaliWord: {k}")
            d[k] = self.__dict__[k]

        return d

    # Properties for compatibility with DictWord, when used in a Union.

    @property
    def word(self) -> str:
        return self.pali_1

    @property
    def language(self) -> str:
        return "en"

    @property
    def source_uid(self) -> str:
        return "dpd"

    @property
    def word_nom_sg(self) -> str:
        return ""

    @property
    def inflections(self) -> str:
        return ""

    @property
    def transliteration(self) -> str:
        return ""

    @property
    def meaning_order(self) -> int:
        return 1

    @property
    def definition_plain(self) -> str:
        return self.meaning_1 if self.meaning_1 != "" else self.meaning_2

    @property
    def definition_html(self) -> str:
        return ""

    @property
    def summary(self) -> str:
        return ""

    @property
    def examples(self) -> list:
        return []

    @property
    def synonyms(self) -> list:
        return []

class DbInfo(Base):
    """Storing general key-value data such as dpd_release_version and cached
    values, e.g. cf_set, sandhi_contractions as JSON strings."""
    __tablename__ = "db_info"

    id: Mapped[int] = mapped_column(primary_key=True)
    key: Mapped[str] = mapped_column(unique=True)
    value: Mapped[str] = mapped_column(default='')

class DerivedData(Base):
    __tablename__ = "derived_data"

    id: Mapped[int] = mapped_column(
        ForeignKey('pali_words.id'), primary_key=True)
    # pali_1: Mapped[str] = mapped_column(unique=True)
    inflections: Mapped[str] = mapped_column(default='')
    sinhala: Mapped[str] = mapped_column(default='')
    devanagari: Mapped[str] = mapped_column(default='')
    thai: Mapped[str] = mapped_column(default='')
    html_table: Mapped[str] = mapped_column(default='')
    freq_html: Mapped[str] = mapped_column(default='')
    ebt_count: Mapped[int] = mapped_column(default='')

    @property
    def inflections_list(self) -> list:
        if self.inflections:
            return self.inflections.split(",")
        else:
            return []

    @property
    def sinhala_list(self) -> list:
        if self.sinhala:
            return self.sinhala.split(",")
        else:
            return []

    @property
    def devanagari_list(self) -> list:
        if self.devanagari:
            return self.devanagari.split(",")
        else:
            return []

    @property
    def thai_list(self) -> list:
        if self.thai:
            return self.thai.split(",")
        else:
            return []

    def __repr__(self) -> str:
        return f"DerivedData: {self.id} {PaliWord.pali_1} {self.inflections}"


class Sandhi(Base):
    __tablename__ = "sandhi"
    id: Mapped[int] = mapped_column(primary_key=True)
    sandhi: Mapped[str] = mapped_column(unique=True)
    split: Mapped[str] = mapped_column(default='')
    sinhala: Mapped[str] = mapped_column(default='')
    devanagari: Mapped[str] = mapped_column(default='')
    thai: Mapped[str] = mapped_column(default='')

    @property
    def split_list(self) -> list:
        return self.split.split(",")

    @property
    def sinhala_list(self) -> list:
        if self.sinhala:
            return self.sinhala.split(",")
        else:
            return []

    @property
    def devanagari_list(self) -> list:
        if self.devanagari:
            return self.devanagari.split(",")
        else:
            return []

    @property
    def thai_list(self) -> list:
        if self.thai:
            return self.thai.split(",")
        else:
            return []

    def __repr__(self) -> str:
        return f"Sandhi: {self.id} {self.sandhi} {self.split}"

    # === Simsapa ===

    @property
    def headwords(self) -> List[List[str]]:
        """Parses .split to a list of headword combinations (list of lists).

        Example:
        sandhi: kammapattā
        split: kamma + pattā,kamma + apattā,kammi + apattā,kammā + apattā,kammaṃ + apattā
        return: [["kamma", "pattā"], ["kamma", "apattā"], ["kammi", "apattā"]]
        """

        return [[word.strip() for word in line.split("+")] for line in self.split.split(",")]

    @property
    def headwords_flat(self) -> Set[str]:
        """Unique headwords as a flattened set."""
        flat_list = [word for sublist in self.headwords for word in sublist]
        return set(flat_list)


class FamilyCompound(Base):
    __tablename__ = "family_compound"
    id: Mapped[int] = mapped_column(primary_key=True)
    compound_family: Mapped[str] = mapped_column(unique=True)
    html: Mapped[str] = mapped_column(default='')
    count: Mapped[int] = mapped_column(default=0)

    def __repr__(self) -> str:
        return f"FamilyCompound: {self.id} {self.compound_family} {self.count}"


class FamilyWord(Base):
    __tablename__ = "family_word"
    word_family: Mapped[str] = mapped_column(primary_key=True)
    html: Mapped[str] = mapped_column(default='')
    count: Mapped[int] = mapped_column(default=0)

    def __repr__(self) -> str:
        return f"FamilyWord: {self.word_family} {self.count}"


class FamilySet(Base):
    __tablename__ = "family_set"
    set: Mapped[str] = mapped_column(primary_key=True)
    html: Mapped[str] = mapped_column(default='')
    count: Mapped[int] = mapped_column(default=0)

    def __repr__(self) -> str:
        return f"FamilySet: {self.set} {self.count}"


class SBS(Base):
    __tablename__ = "sbs"

    id: Mapped[int] = mapped_column(
        ForeignKey('pali_words.id'), primary_key=True)
    sbs_class_anki: Mapped[int] = mapped_column(default='')
    sbs_class: Mapped[int] = mapped_column(default='')
    sbs_meaning: Mapped[str] = mapped_column(default='')
    sbs_notes: Mapped[str] = mapped_column(default='')
    sbs_source_1: Mapped[str] = mapped_column(default='')
    sbs_sutta_1: Mapped[str] = mapped_column(default='')
    sbs_example_1: Mapped[str] = mapped_column(default='')
    sbs_chant_pali_1: Mapped[str] = mapped_column(default='')
    sbs_chant_eng_1: Mapped[str] = mapped_column(default='')
    sbs_chapter_1: Mapped[str] = mapped_column(default='')
    sbs_source_2: Mapped[str] = mapped_column(default='')
    sbs_sutta_2: Mapped[str] = mapped_column(default='')
    sbs_example_2: Mapped[str] = mapped_column(default='')
    sbs_chant_pali_2: Mapped[str] = mapped_column(default='')
    sbs_chant_eng_2: Mapped[str] = mapped_column(default='')
    sbs_chapter_2: Mapped[str] = mapped_column(default='')
    sbs_source_3: Mapped[str] = mapped_column(default='')
    sbs_sutta_3: Mapped[str] = mapped_column(default='')
    sbs_example_3: Mapped[str] = mapped_column(default='')
    sbs_chant_pali_3: Mapped[str] = mapped_column(default='')
    sbs_chant_eng_3: Mapped[str] = mapped_column(default='')
    sbs_chapter_3: Mapped[str] = mapped_column(default='')
    sbs_source_4: Mapped[str] = mapped_column(default='')
    sbs_sutta_4: Mapped[str] = mapped_column(default='')
    sbs_example_4: Mapped[str] = mapped_column(default='')
    sbs_chant_pali_4: Mapped[str] = mapped_column(default='')
    sbs_chant_eng_4: Mapped[str] = mapped_column(default='')
    sbs_chapter_4: Mapped[str] = mapped_column(default='')
    sbs_category: Mapped[str] = mapped_column(default='')

    def __repr__(self) -> str:
        return f"SBS: {self.id} {self.sbs_chant_pali_1} {self.sbs_class}"

    @declared_attr
    def sbs_chapter_flag(cls):
        return Column(Integer, nullable=True)  # Allow null values

    def calculate_chapter_flag(self):
        for i in range(1, 5):
            chapter_attr = getattr(self, f'sbs_chapter_{i}')
            if chapter_attr and chapter_attr.strip():
                return 1
        return None



class Russian(Base):
    __tablename__ = "russian"

    id: Mapped[int] = mapped_column(
        ForeignKey('pali_words.id'), primary_key=True)
    ru_meaning: Mapped[str] = mapped_column(default="")
    ru_meaning_lit: Mapped[str] = mapped_column(default="")
    ru_notes: Mapped[str] = mapped_column(default='')

    def __repr__(self) -> str:
        return f"Russian: {self.id} {self.ru_meaning}"


class InflectionToHeadwords(Base):
    __tablename__ = "inflection_to_headwords"

    inflection: Mapped[str] = mapped_column(primary_key=True)
    headwords: Mapped[str] = mapped_column(default="")

    @property
    def headwords_list(self) -> List[str]:
        return self.headwords.split(",")

    def __repr__(self) -> str:
        return f"i2h: {self.inflection} {self.headwords}"
