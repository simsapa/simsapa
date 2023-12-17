from typing import List, Optional, TypedDict, Union, Dict
import re

import tantivy

from sqlalchemy import or_
from sqlalchemy.orm.session import Session
from simsapa import DbSchemaName

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.pali_stemmer import pali_stem
from simsapa.dpd_db.tools.pali_sort_key import pali_sort_key

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord, Dpd.PaliWord]

# TODO same as simsapa.app.types.Labels, but declared here to avoid cirular import
class Labels(TypedDict):
    appdata: List[str]
    userdata: List[str]

class SearchResult(TypedDict):
    # database id
    db_id: int
    # database schema name (appdata or userdata)
    schema_name: str
    # database table name (e.g. suttas or dict_words)
    table_name: str
    uid: Optional[str]
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
        db_id = int(str(x.id)),
        schema_name = x.metadata.schema,
        table_name = 'suttas',
        uid = str(x.uid),
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
        db_id = int(str(x.id)),
        schema_name = x.metadata.schema,
        table_name = 'dict_words',
        uid = str(x.uid),
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

def dpd_pali_word_to_search_result(x: Dpd.PaliWord, snippet: str) -> SearchResult:
    return SearchResult(
        db_id = int(str(x.id)),
        schema_name = 'dpd',
        table_name = 'pali_words',
        uid = str(x.uid),
        source_uid = 'dpd',
        title = str(x.pali_1),
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

def dpd_deconstructor_query(db_session: Session, query_text: str) -> Dict[str, Dpd.PaliWord]:
    pali_words: Dict[str, Dpd.PaliWord] = dict()

    r = db_session.query(Dpd.DpdDeconstructor) \
                    .filter(Dpd.DpdDeconstructor.word == query_text) \
                    .first()
    if r is not None:
        for w in r.headwords_flat:
            for i in inflection_to_pali_words(db_session, w):
                pali_words[i.pali_1] = i

    return pali_words

def dpd_lookup(db_session: Session, query_text: str) -> List[SearchResult]:

    # ![flowchart](https://github.com/digitalpalidictionary/dpd-db/blob/main/tbw/docs/dpd%20lookup%20systen.png)

    pali_words: Dict[str, Dpd.PaliWord] = dict()

    # Query text may be a DPD id number or uid.
    query_text = query_text.replace("/dpd", "")
    if query_text.isdigit():
        r = db_session.query(Dpd.PaliWord) \
                      .filter(Dpd.PaliWord.id == int(query_text)) \
                      .first()
        if r is not None:
            pali_words[r.pali_1] = r

    if len(pali_words) == 0:
        # Lookup word in dpd_i2h (inflections to headwords).
        for i in inflection_to_pali_words(db_session, query_text):
            pali_words[i.pali_1] = i

    if len(pali_words) == 0:
        # i2h result doesn't exist
        # Lookup query text in dpd_deconstructor.

        d = dpd_deconstructor_query(db_session, query_text)
        for i in d.values():
            pali_words[i.pali_1] = i

    if len(pali_words) == 0:
        # It is not in i2h and not in deconstructor.
        # Lookup query_text in pali_words.

        # Word exact match.
        r = db_session.query(Dpd.PaliWord) \
                      .filter(or_(Dpd.PaliWord.pali_clean == query_text,
                                  Dpd.PaliWord.word_ascii == query_text)) \
                      .all()

        if len(r) == 0:
            # Word starts with.
            r = db_session.query(Dpd.PaliWord) \
                          .filter(or_(Dpd.PaliWord.pali_clean.like(f"{query_text}%"),
                                      Dpd.PaliWord.word_ascii.like(f"{query_text}%"))) \
                          .all()

        if len(r) == 0:
            # Stem form exact match.
            stem = pali_stem(query_text)
            r = db_session.query(Dpd.PaliWord) \
                          .filter(Dpd.PaliWord.stem == stem) \
                          .all()

        if len(r) == 0:
            # Stem form starts with.
            stem = pali_stem(query_text)
            r = db_session.query(Dpd.PaliWord) \
                          .filter(Dpd.PaliWord.stem.like(f"{stem}%")) \
                          .all()

        for i in r:
            pali_words[i.pali_1] = i

    res_page = []

    pali_words_values = sorted(pali_words.values(), key=lambda x: pali_sort_key(x.pali_1))

    for w in pali_words_values:
        snippet = w.meaning_1 if w.meaning_1 != "" else w.meaning_2
        snippet += f"<br><i>{w.grammar}</i>"
        res = dpd_pali_word_to_search_result(w, snippet)
        res_page.append(res)

    return res_page

def get_word_for_schema_and_id(db_session: Session, db_schema: str, db_id: int) -> UDictWord:
    if db_schema == DbSchemaName.AppData.value:
        w = db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.id == db_id) \
            .first()

    elif db_schema == DbSchemaName.UserData.value:
        w = db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.id == db_id) \
            .first()

    elif db_schema == DbSchemaName.Dpd.value:
        w = db_session \
            .query(Dpd.PaliWord) \
            .filter(Dpd.PaliWord.id == db_id) \
            .first()

    else:
        raise Exception(f"Unknown schema: {db_schema}")

    assert(w is not None)

    return w

def get_word_gloss(w: UDictWord, gloss_keys_csv: str) -> str:
    html = ""

    if isinstance(w, Am.DictWord) or isinstance(w, Um.DictWord):
        return "<p>Gloss only works for DPD words</p>"

    elif isinstance(w, Dpd.PaliWord):
        html = "<table><tr><td>"

        data = w.as_dict
        values = []

        for k in gloss_keys_csv.split(','):
            k = k.strip()
            if k in data.keys():
                values.append(str(data[k]))

        html += "</td><td>".join(values)

        html += "</td></tr></table>"

    return html

def get_word_meaning(w: UDictWord) -> str:
    if isinstance(w, Am.DictWord) or isinstance(w, Um.DictWord):
        return w.definition_plain if w.definition_plain is not None else ""

    elif isinstance(w, Dpd.PaliWord):
        return w.meaning_1 if w.meaning_1 != "" else w.meaning_2
