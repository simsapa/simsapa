import re
from datetime import datetime
from typing import Dict, List, Optional

import tantivy

from sqlalchemy import and_, or_, not_

from simsapa import logger
from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.helpers import consistent_nasal_m, expand_quote_to_pattern_str
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.types import SearchParams
from simsapa.app.types import SearchMode, UDictWord, USutta
from simsapa.app.search.helpers import SearchResult, dict_word_to_search_result, sutta_to_search_result
from simsapa.app.search.tantivy_index import TantivySearchQuery

class SearchQueryTask:
    _all_results: List[SearchResult] = []
    _highlighted_result_pages: Dict[int, List[SearchResult]] = dict()

    def __init__(self,
                 ix: tantivy.Index,
                 query_text: str,
                 query_started_time: datetime,
                 params: SearchParams):

        super().__init__()

        self.ix = ix
        self.query_finished_time: Optional[datetime] = None

        self.set_query(query_text,
                       query_started_time,
                       params)

    def set_query(self,
                  query_text: str,
                  query_started_time: datetime,
                  params: SearchParams):

        self.query_text = consistent_nasal_m(query_text.strip().lower())
        self.query_started_time = query_started_time
        self.query_finished_time = None

        self.search_mode = params['mode']
        self._page_len = params['page_len'] if params['page_len'] is not None else 20

        s = params['only_lang']
        self.only_lang = None if s is None else s.lower()
        s = params['only_source']
        self.only_source = None if s is None else s.lower()
        self._all_results = []
        self._highlighted_result_pages = dict()

        self.search_query = TantivySearchQuery(self.ix, self._page_len)

    def query_hits(self) -> int:
        if self.search_mode == SearchMode.FulltextMatch:
            if self.search_query.hits_count is None:
                return 0
            else:
                return self.search_query.hits_count

        else:
            return len(self._all_results)

    def all_results(self) -> List[SearchResult]:
        if self.search_mode == SearchMode.FulltextMatch:
            return self.search_query.get_all_results()

        else:
            return self._all_results

    def _highlight_query_in_content(self, query: str, content: str) -> str:
        ll = len(query)
        n = 0
        a = content.lower().find(query.lower(), n)
        while a != -1:
            highlight = "<span class='match'>" + content[a:a+ll] + "</span>"
            content = content[0:a] + highlight + content[a+ll:-1]

            n = a+len(highlight)
            a = content.lower().find(query.lower(), n)

        return content

    def results_page(self, page_num: int) -> List[SearchResult]:
        if page_num not in self._highlighted_result_pages:
            if self.search_mode == SearchMode.FulltextMatch:
                self._highlighted_result_pages[page_num] = self.search_query.highlighted_results_page(page_num)

            else:
                page_start = page_num * self._page_len
                page_end = page_start + self._page_len

                def _add_highlight(x: SearchResult) -> SearchResult:
                    x['snippet'] = self._highlight_query_in_content(self.query_text, x['snippet'])
                    return x

                page = list(map(_add_highlight, self._all_results[page_start:page_end]))

                self._highlighted_result_pages[page_num] = page

        return self._highlighted_result_pages[page_num]

    def _fragment_around_query(self, query: str, content: str) -> str:
        n = content.lower().find(query.lower())
        if n == -1:
            return content

        prefix = ""
        postfix = ""

        if n <= 20:
            a = 0
        else:
            a = n - 20
            prefix = "... "

        if len(content) <= a+500:
            b = len(content) - 1
        else:
            b = a+500
            postfix = " ..."

        return prefix + content[a:b] + postfix

    def _db_sutta_to_result(self, x: USutta) -> SearchResult:
        if x.content_plain is not None and len(str(x.content_plain)) > 0:
            content = str(x.content_plain)
        else:
            content = str(x.content_html)

        snippet = self._fragment_around_query(self.query_text, content)

        return sutta_to_search_result(x, snippet)

    def _db_word_to_result(self, x: UDictWord) -> SearchResult:
        if x.summary is not None and len(str(x.summary)) > 0:
            content = str(x.summary)
        elif x.definition_plain is not None and len(str(x.definition_plain)) > 0:
            content = str(x.definition_plain)
        else:
            content = str(x.definition_html)

        snippet = self._fragment_around_query(self.query_text, content)

        return dict_word_to_search_result(x, snippet)

    def _sutta_only_in_source(self, x: USutta):
        if self.only_source is not None:
            return str(x.uid).endswith(f'/{self.only_source.lower()}')
        else:
            return True

    def _word_only_in_source(self, x: UDictWord):
        if self.only_source is not None:
            return str(x.uid).endswith(f'/{self.only_source.lower()}')
        else:
            return True

    def run(self):
        logger.info("SearchQueryTask::run()")
        try:
            self._all_results = []
            self._highlighted_result_pages = dict()

            if self.search_mode == SearchMode.FulltextMatch:
                 self.search_query.new_query(self.query_text, self.only_source)
                 self._highlighted_result_pages[0] = self.search_query.highlighted_results_page(0)

            elif self.search_mode == SearchMode.ExactMatch or \
                 self.search_mode == SearchMode.RegexMatch:

                db_eng, db_conn, db_session = get_db_engine_connection_session()

                if self.search_query.is_sutta_index():

                    res_suttas: List[USutta] = []

                    if 'AND' in self.query_text:

                        and_terms = list(map(lambda x: x.strip(), self.query_text.split('AND')))

                        q = db_session.query(Am.Sutta)
                        for i in and_terms:
                            if self.search_mode == SearchMode.ExactMatch:
                                p = expand_quote_to_pattern_str(i)
                            else:
                                p = i
                            q = q.filter(Am.Sutta.content_plain.regexp_match(p))
                        r = q.all()
                        res_suttas.extend(r)

                        q = db_session.query(Um.Sutta)
                        for i in and_terms:
                            if self.search_mode == SearchMode.ExactMatch:
                                p = expand_quote_to_pattern_str(i)
                            else:
                                p = i
                            q = q.filter(Um.Sutta.content_plain.regexp_match(p))
                        r = q.all()
                        res_suttas.extend(r)

                    else:

                        if self.search_mode == SearchMode.ExactMatch:
                            p = expand_quote_to_pattern_str(self.query_text)
                        else:
                            p = self.query_text

                        r = db_session \
                            .query(Am.Sutta) \
                            .filter(Am.Sutta.content_plain.regexp_match(p)) \
                            .all()
                        res_suttas.extend(r)

                        r = db_session \
                            .query(Um.Sutta) \
                            .filter(Um.Sutta.content_plain.regexp_match(p)) \
                            .all()
                        res_suttas.extend(r)

                    if self.only_source is not None:
                        res_suttas = list(filter(self._sutta_only_in_source, res_suttas))

                    self._all_results = list(map(self._db_sutta_to_result, res_suttas))

                elif self.search_query.is_dict_word_index():

                    res: List[UDictWord] = []

                    if 'AND' in self.query_text:

                        and_terms = list(map(lambda x: x.strip(), self.query_text.split('AND')))

                        q = db_session.query(Am.DictWord)
                        for i in and_terms:
                            if self.search_mode == SearchMode.ExactMatch:
                                q = q.filter(Am.DictWord.definition_plain.like(f"%{i}%"))
                            else:
                                q = q.filter(Am.DictWord.definition_plain.regexp_match(i))
                        r = q.all()
                        res.extend(r)

                        q = db_session.query(Um.DictWord)
                        for i in and_terms:
                            if self.search_mode == SearchMode.ExactMatch:
                                q = q.filter(Um.DictWord.definition_plain.like(f"%{i}%"))
                            else:
                                q = q.filter(Um.DictWord.definition_plain.regexp_match(i))
                        r = q.all()
                        res.extend(r)

                    else:

                        q = db_session.query(Am.DictWord)
                        if self.search_mode == SearchMode.ExactMatch:
                            q = q.filter(Am.DictWord.definition_plain.like(f"%{self.query_text}%"))
                        else:
                            q = q.filter(Am.DictWord.definition_plain.regexp_match(self.query_text))

                        r = q.all()
                        res.extend(r)

                        q = db_session.query(Um.DictWord)
                        if self.search_mode == SearchMode.ExactMatch:
                            q = q.filter(Um.DictWord.definition_plain.like(f"%{self.query_text}%"))
                        else:
                            q = q.filter(Um.DictWord.definition_plain.regexp_match(self.query_text))

                        r = q.all()
                        res.extend(r)

                    if self.only_source is not None:
                        res = list(filter(self._word_only_in_source, res))

                    self._all_results = list(map(self._db_word_to_result, res))

                db_conn.close()
                db_session.close()
                db_eng.dispose()

            elif self.search_mode == SearchMode.TitleMatch:
                # NOTE: SearchMode.TitleMatch only applies to suttas.
                db_eng, db_conn, db_session = get_db_engine_connection_session()

                res_suttas: List[USutta] = []

                r = db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.title.like(f"{self.query_text}%")) \
                    .all()
                res_suttas.extend(r)

                r = db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.title.like(f"{self.query_text}%")) \
                    .all()
                res_suttas.extend(r)

                ids = list(map(lambda x: x.id, res_suttas))

                r = db_session \
                    .query(Am.Sutta) \
                    .filter(and_(
                        Am.Sutta.title.like(f"%{self.query_text}%"),
                        not_(Am.Sutta.id.in_(ids)),
                    )) \
                    .all()
                res_suttas.extend(r)

                r = db_session \
                    .query(Um.Sutta) \
                    .filter(and_(
                        Um.Sutta.title.like(f"%{self.query_text}%"),
                        not_(Um.Sutta.id.in_(ids)),
                    )) \
                    .all()
                res_suttas.extend(r)

                if self.only_source is not None:
                    res_suttas = list(filter(self._sutta_only_in_source, res_suttas))

                self._all_results = list(map(self._db_sutta_to_result, res_suttas))

                db_conn.close()
                db_session.close()
                db_eng.dispose()

            elif self.search_mode == SearchMode.HeadwordMatch:
                # NOTE: SearchMode.HeadwordMatch only applies to dictionary words.
                db_eng, db_conn, db_session = get_db_engine_connection_session()

                res: List[UDictWord] = []

                r = db_session \
                    .query(Am.DictWord) \
                    .filter(or_(
                        Am.DictWord.word.like(f"{self.query_text}%"),
                        Am.DictWord.word_nom_sg.like(f"{self.query_text}%"),
                        Am.DictWord.inflections.like(f"{self.query_text}%"),
                        Am.DictWord.phonetic.like(f"{self.query_text}%"),
                        Am.DictWord.transliteration.like(f"{self.query_text}%"),
                        Am.DictWord.also_written_as.like(f"{self.query_text}%"),
                    )) \
                    .all()
                res.extend(r)

                r = db_session \
                    .query(Um.DictWord) \
                    .filter(or_(
                        Um.DictWord.word.like(f"{self.query_text}%"),
                        Um.DictWord.word_nom_sg.like(f"{self.query_text}%"),
                        Um.DictWord.inflections.like(f"{self.query_text}%"),
                        Um.DictWord.phonetic.like(f"{self.query_text}%"),
                        Um.DictWord.transliteration.like(f"{self.query_text}%"),
                        Um.DictWord.also_written_as.like(f"{self.query_text}%"),
                    )) \
                    .all()
                res.extend(r)

                # Sort 'dhamma 01' etc. without the numbers.

                a = list(map(lambda x: (re.sub(r"[ 0-9]+$", "", str(x.word).lower()), x), res))
                b = sorted(a, key=lambda x: x[0])
                res = list(map(lambda x: x[1], b))

                ids = list(map(lambda x: x.id, res))

                r = db_session \
                    .query(Am.DictWord) \
                    .filter(and_(
                        Am.DictWord.word.like(f"%{self.query_text}%"),
                        not_(Am.DictWord.id.in_(ids)),
                    )) \
                    .all()
                res.extend(r)

                r = db_session \
                    .query(Um.DictWord) \
                    .filter(and_(
                        Um.DictWord.word.like(f"%{self.query_text}%"),
                        not_(Um.DictWord.id.in_(ids)),
                    )) \
                    .all()
                res.extend(r)

                if self.only_source is not None:
                    res = list(filter(self._word_only_in_source, res))

                self._all_results = list(map(self._db_word_to_result, res))

                db_conn.close()
                db_session.close()
                db_eng.dispose()

            self.query_finished_time = datetime.now()

        except Exception as e:
            logger.error(e)
