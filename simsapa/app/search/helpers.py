from typing import List, Optional, TypedDict, Union, Dict
import re

import tantivy

from sqlalchemy import or_
from sqlalchemy.orm.session import Session
from simsapa import DbSchemaName

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.helpers import strip_html
from simsapa.app.pali_stemmer import pali_stem
from simsapa.dpd_db.tools.pali_sort_key import pali_sort_key

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord, Dpd.PaliWord, Dpd.PaliRoot]
UDpdWord = Union[Dpd.PaliWord, Dpd.PaliRoot]

# TODO same as simsapa.app.types.Labels, but declared here to avoid cirular import
class Labels(TypedDict):
    appdata: List[str]
    userdata: List[str]

class SearchResult(TypedDict):
    uid: str
    # database schema name (appdata or userdata)
    schema_name: str
    # database table name (e.g. suttas or dict_words)
    table_name: str
    source_uid: Optional[str]
    title: str
    ref: Optional[str]
    nikaya: Optional[str]
    author: Optional[str]
    # highlighted snippet
    snippet: str
    # page number in a document
    page_number: Optional[int]
    score: Optional[float]
    rank: Optional[int]

def sutta_to_search_result(x: USutta, snippet: str) -> SearchResult:
    return SearchResult(
        uid = str(x.uid),
        schema_name = x.metadata.schema,
        table_name = 'suttas',
        source_uid = str(x.source_uid),
        title = str(x.title) if x.title else '',
        ref = str(x.sutta_ref) if x.sutta_ref else '',
        nikaya = str(x.nikaya) if x.nikaya else '',
        author = None,
        snippet = snippet,
        page_number = None,
        score = None,
        rank = None,
    )

def dict_word_to_search_result(x: UDictWord, snippet: str) -> SearchResult:
    return SearchResult(
        uid = str(x.uid),
        schema_name = x.metadata.schema,
        table_name = x.__tablename__,
        source_uid = str(x.source_uid),
        title = str(x.word),
        ref = None,
        nikaya = None,
        author = None,
        snippet = snippet,
        page_number = None,
        score = None,
        rank = None,
    )

def search_compact_plain_snippet(content: str,
                                 title: Optional[str] = None,
                                 ref: Optional[str] = None) -> str:

    s = content

    # AN 3.119 Kammantasutta
    # as added to the content when indexing
    if ref is not None and title is not None:
        s = s.replace(f"{ref} {title}", '')

    # 163–182. (- dash and -- en-dash)
    s = re.sub(r'[0-9\.–-]+', '', s)

    # ... Book of the Sixes 5.123.
    s = re.sub(r'Book of the [\w ]+[0-9\.]+', '', s)

    # Connected Discourses on ... 12.55.
    s = re.sub(r'\w+ Discourses on [\w ]+[0-9\.]+', '', s)

    # ...vagga
    s = re.sub(r'[\w -]+vagga', '', s)

    # ... Nikāya 123.
    s = re.sub(r'[\w]+ Nikāya +[0-9\.]*', '', s)

    # SC 1, (SuttaCentral ref link text)
    s = re.sub('SC [0-9]+', '', s)

    # Remove the title from the content, but only the first instance, so as
    # not to remove a common word (e.g.'kamma') from the entire text.
    if title is not None:
        s = s.replace(title, '', 1)

    return s

def search_oneline(content: str) -> str:
    s = content
    # Clean up whitespace so that all text is one line
    s = s.replace("\n", ' ')
    # replace multiple spaces to one
    s = re.sub(r'  +', ' ', s)

    return s

def is_index_empty(__ix__: tantivy.Index) -> bool:
    # FIXME requires dir path as argument
    # if not ix.exists():
    #     return True

    # FIXME returns 0
    # if ix.searcher().num_docs == 0:
    #     return True

    return False

def get_sutta_languages(db_session: Session) -> List[str]:
    res = []

    r = db_session.query(Am.Sutta.language.distinct()).all()
    res.extend(r)

    r = db_session.query(Um.Sutta.language.distinct()).all()
    res.extend(r)

    a = sorted(set(map(lambda x: str(x[0]).lower(), res)))
    langs = list(filter(None, a))

    return langs

def get_dict_word_languages(db_session: Session) -> List[str]:
    res = []

    r = db_session.query(Am.DictWord.language.distinct()).all()
    res.extend(r)

    r = db_session.query(Um.DictWord.language.distinct()).all()
    res.extend(r)

    a = sorted(set(map(lambda x: str(x[0]).lower(), res)))
    langs = list(filter(None, a))

    return langs

def get_sutta_source_filter_labels(db_session: Session) -> List[str]:
    res = []

    r = db_session.query(Am.Sutta.source_uid.distinct()).all()
    res.extend(r)

    r = db_session.query(Um.Sutta.source_uid.distinct()).all()
    res.extend(r)

    a = sorted(set(map(lambda x: str(x[0]).lower(), res)))
    labels = list(filter(None, a))

    return labels

def get_dict_word_source_filter_labels(db_session: Session) -> List[str]:
    res = []

    r = db_session.query(Am.Dictionary.label.distinct()).all()
    res.extend(r)

    r = db_session.query(Um.Dictionary.label.distinct()).all()
    res.extend(r)

    a = sorted(set(map(lambda x: str(x[0]).lower(), res)))
    labels = list(filter(None, a))

    return labels

def inflection_to_pali_words(db_session: Session, query_text: str) -> List[Dpd.PaliWord]:
    words = []

    i2h = db_session.query(Dpd.DpdI2h) \
                    .filter(Dpd.DpdI2h.word == query_text) \
                    .first()

    if i2h is not None:
        # i2h result exists
        # dpd_ebts has short definitions. For Simsapa, retreive the PaliWords.
        #
        # Lookup headwords in pali_words.

        r = db_session.query(Dpd.PaliWord) \
                      .filter(Dpd.PaliWord.pali_1.in_(i2h.headwords)) \
                      .all()
        if r is not None:
            words.extend(r)

    return words

def dpd_deconstructor_query(db_session: Session, query_text: str, starts_with = False) -> List[Dpd.PaliWord]:
    pali_words: Dict[str, Dpd.PaliWord] = dict()

    if starts_with:
        r = db_session.query(Dpd.DpdDeconstructor) \
                      .filter(Dpd.DpdDeconstructor.word.like(f"{query_text}%")) \
                      .first()

    else:
        r = db_session.query(Dpd.DpdDeconstructor) \
                      .filter(Dpd.DpdDeconstructor.word == query_text) \
                      .first()

    if r is not None:
        for w in r.headwords_flat:
            for i in inflection_to_pali_words(db_session, w):
                pali_words[i.pali_1] = i

    return list(pali_words.values())

def root_info_clean_plaintext(html: str) -> str:
    s = strip_html(html)
    s = s.replace("･", " ")
    s = s.replace("Pāḷi Root:", "")
    s = re.sub(r"Bases:.*$", "", s, flags=re.DOTALL)
    return s

def _parse_words(words_res: List[UDpdWord], do_pali_sort = False) -> List[SearchResult]:
    uniq_pali = []
    uniq_words: List[UDpdWord] = []
    for i in words_res:
        if i.word not in uniq_pali:
            uniq_pali.append(i.word)
            uniq_words.append(i)

    res_page = []

    if do_pali_sort:
        pali_words = sorted(uniq_words, key=lambda x: pali_sort_key(x.word))
    else:
        pali_words = uniq_words

    for w in pali_words:
        if isinstance(w, Dpd.PaliWord):
            snippet = w.meaning_1 if w.meaning_1 != "" else w.meaning_2
            snippet += f" <b>·</b> <i>{strip_html(w.grammar)}</i>"

        elif isinstance(w, Dpd.PaliRoot):
            snippet = w.root_meaning
            snippet += f" <b>·</b> <i>{root_info_clean_plaintext(w.root_info)}</i>"

        else:
            raise Exception(f"Unrecognized word type: {w}")

        r = dict_word_to_search_result(w, snippet)
        res_page.append(r)

    return res_page

def dpd_lookup(db_session: Session, query_text: str, do_pali_sort = False) -> List[SearchResult]:
    query_text = query_text.lower()
    query_text = re.sub("[’']ti$", "ti", query_text)

    res: List[UDpdWord] = []

    # Query text may be a DPD id number or uid.
    if query_text.endswith("/dpd"):
        ref = query_text.replace("/dpd", "")
        if ref.isdigit():
            r = db_session.query(Dpd.PaliWord) \
                        .filter(Dpd.PaliWord.id == int(ref)) \
                        .first()
            res.append(r)

        else:
            r = db_session.query(Dpd.PaliRoot) \
                        .filter(Dpd.PaliRoot.uid == query_text) \
                        .first()
            res.append(r)

    if len(res) > 0:
        return _parse_words(res)

    # Word exact match.
    r = db_session.query(Dpd.PaliWord) \
                    .filter(or_(Dpd.PaliWord.pali_clean == query_text,
                                Dpd.PaliWord.word_ascii == query_text)) \
                    .all()
    res.extend(r)

    r = db_session.query(Dpd.PaliRoot) \
                    .filter(or_(Dpd.PaliRoot.root_clean == query_text,
                                Dpd.PaliRoot.root_no_sign == query_text,
                                Dpd.PaliRoot.word_ascii == query_text)) \
                    .all()
    res.extend(r)

    if len(r) == 0:
        # Stem form exact match.
        stem = pali_stem(query_text)
        r = db_session.query(Dpd.PaliWord) \
                        .filter(Dpd.PaliWord.stem == stem) \
                        .all()
        res.extend(r)

    if len(res) == 0:
        # There were no exact results.
        # Lookup word in dpd_i2h (inflections to headwords).
        res.extend(inflection_to_pali_words(db_session, query_text))

    if len(res) == 0:
        # i2h result doesn't exist.
        # Lookup query text in dpd_deconstructor.
        res.extend(dpd_deconstructor_query(db_session, query_text))

        # No exact match in deconstructor.
        # If query text is long enough, remove the last letter and match as 'starts with'.
        if len(res) == 0 and len(query_text) >= 4:
            res.extend(dpd_deconstructor_query(db_session, query_text[0:-1], starts_with=True))

    if len(res) == 0:
        # - no exact match in pali_words or pali_roots
        # - not in i2h
        # - not in deconstructor.
        #
        # Lookup pali_words which start with the query_text.

        # Word starts with.
        r = db_session.query(Dpd.PaliWord) \
                        .filter(or_(Dpd.PaliWord.pali_clean.like(f"{query_text}%"),
                                    Dpd.PaliWord.word_ascii.like(f"{query_text}%"))) \
                        .all()
        res.extend(r)

        if len(r) == 0:
            # Stem form starts with.
            stem = pali_stem(query_text)
            r = db_session.query(Dpd.PaliWord) \
                          .filter(Dpd.PaliWord.stem.like(f"{stem}%")) \
                          .all()
            res.extend(r)

    return _parse_words(res, do_pali_sort)

def get_word_for_schema_table_and_uid(db_session: Session, db_schema: str, db_table: str, db_uid: str) -> UDictWord:
    if db_schema == DbSchemaName.AppData.value:
        w = db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.uid == db_uid) \
            .first()

    elif db_schema == DbSchemaName.UserData.value:
        w = db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.uid == db_uid) \
            .first()

    elif db_schema == DbSchemaName.Dpd.value:
        if db_table == "pali_words":
            w = db_session \
                .query(Dpd.PaliWord) \
                .filter(Dpd.PaliWord.uid == db_uid) \
                .first()

        elif db_table == "pali_roots":
            w = db_session \
                .query(Dpd.PaliRoot) \
                .filter(Dpd.PaliRoot.uid == db_uid) \
                .first()

        else:
            raise Exception(f"Unknown table: {db_table}")

    else:
        raise Exception(f"Unknown schema: {db_schema}")

    assert(w is not None)

    return w

def get_word_gloss(w: UDictWord, gloss_keys_csv: str) -> str:
    html = ""

    if isinstance(w, Am.DictWord) or isinstance(w, Um.DictWord):
        return "<p>Gloss only works for DPD words</p>"

    elif isinstance(w, Dpd.PaliWord) or isinstance(w, Dpd.PaliRoot):
        html = "<table><tr><td>"

        data = w.as_dict
        values = []

        for k in gloss_keys_csv.split(','):
            k = k.strip()
            if k in data.keys():
                values.append(str(data[k]))
            else:
                values.append('')

        html += "</td><td>".join(values)

        html += "</td></tr></table>"

    return html

def get_word_meaning(w: UDictWord) -> str:
    if isinstance(w, Am.DictWord) or isinstance(w, Um.DictWord):
        return w.definition_plain if w.definition_plain is not None else ""

    elif isinstance(w, Dpd.PaliWord):
        return w.meaning_1 if w.meaning_1 != "" else w.meaning_2

    elif isinstance(w, Dpd.PaliRoot):
        return w.root_meaning

    else:
        raise Exception(f"Unrecognized word type: {w}")
