from typing import List, Optional, Set, Union, Dict
from datetime import datetime
from time import sleep
import re

import tantivy

from sqlalchemy import or_
from sqlalchemy.orm.session import Session
from simsapa import DbSchemaName, SearchResult
from simsapa.app.api import ApiSearchResult

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.helpers import strip_html, root_info_clean_plaintext
from simsapa.app.pali_stemmer import pali_stem
from simsapa.app.types import SearchArea, SearchParams
from simsapa.dpd_db.tools.pali_sort_key import pali_sort_key
from simsapa.layouts.gui_types import GuiSearchQueriesInterface

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord, Dpd.PaliWord, Dpd.PaliRoot]
UDpdWord = Union[Dpd.PaliWord, Dpd.PaliRoot]

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

    i2h = db_session.query(Dpd.InflectionToHeadwords) \
                    .filter(Dpd.InflectionToHeadwords.inflection == query_text) \
                    .first()

    if i2h is not None:
        # i2h result exists
        # Lookup headwords in pali_words.

        r = db_session.query(Dpd.PaliWord) \
                      .filter(Dpd.PaliWord.pali_1.in_(i2h.headwords_list)) \
                      .all()
        if r is not None:
            words.extend(r)

    return words

def dpd_deconstructor_query(db_session: Session, query_text: str, exact_only = True) -> Optional[Dpd.Sandhi]:
    # NOTE: Use exact_only=True as default because 'starts with' matches show confusing additional words.

    # Exact match.
    r = db_session.query(Dpd.Sandhi) \
                    .filter(Dpd.Sandhi.sandhi == query_text) \
                    .first()

    if not exact_only:
        if r is None and len(query_text) >= 4:
            # Match as 'starts with'.
            r = db_session.query(Dpd.Sandhi) \
                        .filter(Dpd.Sandhi.sandhi.like(f"{query_text}%")) \
                        .first()

    if r is None and " " in query_text:
        # If the query contained multiple words, remove spaces to find compound forms.
        r = db_session.query(Dpd.Sandhi) \
                      .filter(Dpd.Sandhi.sandhi == query_text.replace(" ", "")) \
                      .first()

    if not exact_only:
        if r is None and len(query_text) >= 4:
            # No exact match in deconstructor.
            # If query text is long enough, remove the last letter and match as 'starts with'.
            r = db_session.query(Dpd.Sandhi) \
                        .filter(Dpd.Sandhi.sandhi.like(f"{query_text[0:-1]}%")) \
                        .first()

    return r

def dpd_deconstructor_to_pali_words(db_session: Session, query_text: str) -> List[Dpd.PaliWord]:
    pali_words: Dict[str, Dpd.PaliWord] = dict()

    r = dpd_deconstructor_query(db_session, query_text)

    if r is not None:
        for w in r.headwords_flat:
            for i in inflection_to_pali_words(db_session, w):
                pali_words[i.pali_1] = i

    return list(pali_words.values())

def _parse_words(words_res: List[UDpdWord], do_pali_sort = False) -> List[SearchResult]:
    uniq_pali = set()
    uniq_words: List[UDpdWord] = []
    for i in words_res:
        if i.word not in uniq_pali:
            uniq_pali.add(i.word)
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
    if query_text.endswith("/dpd") or query_text.isdigit():
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

    # Add matches from DPD inflections_to_headwords, regardless of earlier results.
    # This will include cases such as:
    # - assa: gen. of ima
    # - assa: imp 2nd sg of assati
    res.extend(inflection_to_pali_words(db_session, query_text))

    if len(res) == 0:
        # Stem form exact match.
        stem = pali_stem(query_text)
        r = db_session.query(Dpd.PaliWord) \
                      .filter(Dpd.PaliWord.stem == stem) \
                      .all()
        res.extend(r)

    if len(res) == 0:
        # If the query contained multiple words, remove spaces to find compound forms.
        nospace_query = query_text.replace(" ", "")
        r = db_session.query(Dpd.PaliWord) \
                      .filter(or_(Dpd.PaliWord.pali_clean == nospace_query,
                                  Dpd.PaliWord.word_ascii == nospace_query)) \
                      .all()
        res.extend(r)

    if len(res) == 0:
        # i2h result doesn't exist.
        # Lookup query text in dpd_deconstructor.
        res.extend(dpd_deconstructor_to_pali_words(db_session, query_text))

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

def unique_search_results(results: List[SearchResult]) -> List[SearchResult]:
    keys: Set[str] = set()
    uniq_results = []
    for i in results:
        k = f"{i['title']} {i['schema_name']} {i['uid']}"
        if k not in keys:
            keys.add(k)
            uniq_results.append(i)

    return uniq_results

def suttas_fulltext_search(queries: GuiSearchQueriesInterface,
                           query_text: str,
                           params: SearchParams,
                           page_num = 0,
                           do_pali_sort = False) -> ApiSearchResult:

    _last_query_time = datetime.now()

    def _search_query_finished(__query_started_time__: datetime):
        pass

    if page_num == 0:
        queries.start_search_query_workers(
            query_text,
            SearchArea.Suttas,
            _last_query_time,
            _search_query_finished,
            params,
        )

        while not queries.all_finished():
            # Sleep 10 milliseconds
            sleep(0.01)

    results = unique_search_results(queries.results_page(page_num))

    if do_pali_sort:
        results = sorted(results, key=lambda i: pali_sort_key(f"{i['title']}.{i['schema_name']}.{i['uid']}"))

    res = ApiSearchResult(
        hits = queries.query_hits(),
        results = results,
        deconstructor = [],
    )

    return res

def combined_search(queries: GuiSearchQueriesInterface,
                    query_text: str,
                    params: SearchParams,
                    page_num = 0,
                    do_pali_sort = False) -> ApiSearchResult:
    _last_query_time = datetime.now()

    def _search_query_finished(__query_started_time__: datetime):
        pass

    if page_num == 0:
        queries.start_search_query_workers(
            query_text,
            SearchArea.DictWords,
            _last_query_time,
            _search_query_finished,
            params,
        )

        while not queries.all_finished():
            # Sleep 10 milliseconds
            sleep(0.01)

    results = unique_search_results(queries.results_page(page_num))

    if do_pali_sort:
        results = sorted(results, key=lambda i: pali_sort_key(f"{i['title']}.{i['schema_name']}.{i['uid']}"))

    deconstructor: List[str] = []

    db_eng, db_conn, db_session = get_db_engine_connection_session()

    r = dpd_deconstructor_query(db_session, query_text)
    if r is not None:
        for variation in r.headwords:
            content = " + ".join(variation)
            deconstructor.append(content)

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    res = ApiSearchResult(
        hits = queries.query_hits(),
        results = results,
        deconstructor = deconstructor,
    )

    return res

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

def get_word_gloss_html(w: UDictWord, __gloss_keys_csv__: str) -> str:
    html = "<table><tr><td>"

    data = w.as_dict
    values = []

    # NOTE: ignore gloss_keys_csv argument for now
    #
    # if (uid.endsWith('/dpd')) {
    #   const item_keys = ['uid', 'pali_1', 'pos', 'grammar', 'meaning_1', 'construction'];
    #   item_values = item_keys.map(key => item[key]);

    # } else {
    #   const item_keys = ['uid', 'word', '', '', 'definition_plain', ''];

    if w.uid.endswith('/dpd'):
        item_keys = ['uid', 'pali_1', 'pos', 'grammar', 'meaning_1', 'construction']
    else:
        item_keys = ['uid', 'word', '', '', 'definition_plain', '']

    for k in item_keys:
        k = k.strip()
        if k != '' and k in data.keys():
            s = str(data[k])
            if len(s) <= 100:
                values.append(s)
            else:
                values.append(s[0:100] + " ...")
        else:
            values.append('')

    html += "</td><td>".join(values)

    html += "</td></tr></table>"

    return html

def get_word_meaning(w: UDictWord) -> str:
    s = ""

    if isinstance(w, Am.DictWord) or isinstance(w, Um.DictWord):
        s = w.definition_plain if w.definition_plain is not None else ""

    elif isinstance(w, Dpd.PaliWord):
        s = w.meaning_1 if w.meaning_1 != "" else w.meaning_2

    elif isinstance(w, Dpd.PaliRoot):
        s = w.root_meaning

    else:
        raise Exception(f"Unrecognized word type: {w}")

    if s != "":
        s = strip_html(s)

    return s
