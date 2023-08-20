from typing import List, Optional, TypedDict, Union
import re

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]

# MN44; MN 118; AN 4.10; Sn 4:2; Dhp 182; Thag 1207; Vism 152
# Must not match part of the path in a url, <a class="link" href="ssp://suttas/mn44/en/sujato">
RE_ALL_BOOK_SUTTA_REF = re.compile(r'(?<!/)\b(DN|MN|SN|AN|Pv|Vv|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]*(\d[\d\.:]*)\b', re.IGNORECASE)
# Vin.iii.40; AN.i.78; D iii 264; SN i 190; M. III. 203.
RE_ALL_PTS_VOL_SUTTA_REF = re.compile(r'(?<!/)\b(D|DN|M|MN|S|SN|A|AN|Pv|Vv|Vin|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]+([ivxIVX]+)[ \.]+(\d[\d\.]*)\b', re.IGNORECASE)

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
