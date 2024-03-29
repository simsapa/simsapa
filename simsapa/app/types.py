from enum import Enum
from typing import List, Optional, TypedDict, Union, Callable

from simsapa import ShowLabels, SearchResult
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord, Dpd.DpdHeadwords, Dpd.DpdRoots]
ULink = Union[Am.Link, Um.Link]

UDeck = Union[Am.Deck, Um.Deck]
UMemo = Union[Am.Memo, Um.Memo]

UBookmark = Union[Am.Bookmark, Um.Bookmark]
UDocument = Union[Am.Document, Um.Document]

UChallengeCourse = Union[Am.ChallengeCourse, Um.ChallengeCourse]
UChallengeGroup = Union[Am.ChallengeGroup, Um.ChallengeGroup]
UChallenge = Union[Am.Challenge, Um.Challenge]

UMultiRef = Union[Am.MultiRef, Um.MultiRef]

class Labels(TypedDict):
    appdata: List[str]
    userdata: List[str]

class SearchArea(int, Enum):
    Suttas = 0
    DictWords = 1

class SearchMode(int, Enum):
    FulltextMatch = 0
    ContainsMatch = 1
    HeadwordMatch = 2
    TitleMatch = 3
    DpdIdMatch = 4
    DpdLookup = 5
    Combined = 6
    UidMatch = 7
    RegExMatch = 8

AllSearchModeNameToType = {
    "Combined": SearchMode.Combined,
    "Fulltext Match": SearchMode.FulltextMatch,
    "DPD Lookup": SearchMode.DpdLookup,
    "Contains Match": SearchMode.ContainsMatch,
    # FIXME test HeadwordMatch
    # "Headword Match": SearchMode.HeadwordMatch,
    "Title Match": SearchMode.TitleMatch,
    "UID Match": SearchMode.UidMatch,
    "RegEx Match": SearchMode.RegExMatch,
}

SuttaSearchModeNameToType = {
    "Fulltext Match": SearchMode.FulltextMatch,
    "Contains Match": SearchMode.ContainsMatch,
    "Title Match": SearchMode.TitleMatch,
    "RegEx Match": SearchMode.RegExMatch,
}

DictionarySearchModeNameToType = {
    "Combined": SearchMode.Combined,
    "DPD Lookup": SearchMode.DpdLookup,
    "Fulltext Match": SearchMode.FulltextMatch,
    "Contains Match": SearchMode.ContainsMatch,
    # FIXME test HeadwordMatch
    # "Headword Match": SearchMode.HeadwordMatch,
    # "UID Match": SearchMode.UidMatch,
    "RegEx Match": SearchMode.RegExMatch,
}

class SearchParams(TypedDict):
    mode: SearchMode
    page_len: Optional[int]
    lang: Optional[str]
    lang_include: bool
    source: Optional[str]
    source_include: bool
    enable_regex: bool
    fuzzy_distance: int

class SuttaPanelParams(TypedDict):
    sutta_uid: str
    query_text: str
    find_text: str

class LookupPanelParams(TypedDict):
    query_text: str
    find_text: str
    show_results_tab: bool

class SuttaStudyParams(TypedDict):
    sutta_panels: List[SuttaPanelParams]
    lookup_panel: LookupPanelParams

def default_sutta_panel_params() -> SuttaPanelParams:
    return SuttaPanelParams(sutta_uid='', query_text='', find_text='')

def default_lookup_panel_params() -> LookupPanelParams:
    return LookupPanelParams(query_text='', find_text='', show_results_tab=False)

def default_sutta_study_params() -> SuttaStudyParams:
    return SuttaStudyParams(
        sutta_panels = [],
        lookup_panel = default_lookup_panel_params(),
    )

class SuttaQueriesInterface:
    get_sutta_by_url: Callable

class DictionaryQueriesInterface:
    get_words_by_uid: Callable
    words_to_html_page: Callable
    render_html_page: Callable
    get_word_html: Callable
    dict_word_from_result: Callable[[SearchResult], Optional[UDictWord]]

class GraphRequest(TypedDict):
    sutta_uid: Optional[str]
    dict_word_uid: Optional[str]
    distance: int
    queue_id: str
    graph_gen_timestamp: float
    graph_path: str
    messages_url: str
    labels: Optional[ShowLabels]
    min_links: Optional[int]
    width: int
    height: int
