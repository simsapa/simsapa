from typing import List, Optional, TypedDict, Union
import re

import tantivy

from sqlalchemy.orm.session import Session

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.helpers import word_uid
from simsapa.app.db import dpd_models as Dpd

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
        uid = word_uid(x.pali_1, 'dpd'),
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
