import re
from datetime import datetime
from typing import Dict, List, Optional

import tantivy

from sqlalchemy import and_, or_, not_
from sqlalchemy.orm.session import Session

from simsapa import logger
from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.helpers import consistent_nasal_m
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.types import SearchParams, SearchMode, UDictWord, USutta
from simsapa.app.search.helpers import SearchResult, dict_word_to_search_result, dpd_pali_word_to_search_result, sutta_to_search_result
from simsapa.app.search.tantivy_index import TantivySearchQuery

class SearchQueryTask:
    _all_results: List[SearchResult] = []
    _highlighted_result_pages: Dict[int, List[SearchResult]] = dict()
    _db_query_hits_count = 0

    def __init__(self,
                 ix: tantivy.Index,
                 query_text_orig: str,
                 query_started_time: datetime,
                 params: SearchParams):

        self.ix = ix
        self.query_text = consistent_nasal_m(query_text_orig)
        self.query_started_time = query_started_time
        self.query_finished_time: Optional[datetime] = None

        # FIXME self.search_mode = params['mode']
        self.search_mode = SearchMode.DpdTbwLookup

        self._page_len = params['page_len'] if params['page_len'] is not None else 20

        s = params['lang']
        self.lang = None if s is None else s.lower()
        self.lang_include = params['lang_include']
        s = params['source']
        self.source = None if s is None else s.lower()
        self.source_include = params['source_include']
        self._all_results = []
        self._highlighted_result_pages = dict()

        self.enable_regex = params['enable_regex']
        self.fuzzy_distance = params['fuzzy_distance']

        self.search_query = TantivySearchQuery(self.ix, params)

    def query_hits(self) -> Optional[int]:
        if self.search_mode == SearchMode.FulltextMatch:
            return self.search_query.hits_count

        elif self.search_mode == SearchMode.ExactMatch:
            return self._db_query_hits_count

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
            if self.search_mode == SearchMode.DpdIdMatch:
                self._highlighted_result_pages[page_num] = self.dpd_id_word()

            elif self.search_mode == SearchMode.FulltextMatch:
                self._highlighted_result_pages[page_num] = self.search_query.highlighted_results_page(page_num)

            elif self.search_mode == SearchMode.DpdTbwLookup:
                self._highlighted_result_pages[page_num] = self.dpd_tbw_lookup()

            else:
                # page_start = page_num * self._page_len
                # page_end = page_start + self._page_len

                def _add_highlight(x: SearchResult) -> SearchResult:
                    x['snippet'] = self._highlight_query_in_content(self.query_text, x['snippet'])
                    return x

                if self.search_query.is_sutta_index():
                    results = self.suttas_exact_match_page(page_num)

                else:
                    results = self.dict_words_exact_match_page(page_num)

                # page = list(map(_add_highlight, results[page_start:page_end]))

                self._highlighted_result_pages[page_num] = list(map(_add_highlight, results))

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
        return str(x.uid).endswith(f'/{self.source}')

    def _sutta_except_in_source(self, x: USutta):
        return not self._sutta_only_in_source(x)

    def _word_only_in_source(self, x: UDictWord):
        return str(x.uid).endswith(f'/{self.source}')

    def _word_except_in_source(self, x: UDictWord):
        return not self._word_only_in_source(x)

    def _tbw_search(self):
        self._highlighted_result_pages[0] = self.results_page(0)

    def _fulltext_search(self):
        try:
            self.search_query.new_query(self.query_text,
                                        self.source,
                                        self.source_include,
                                        enable_regex = self.enable_regex,
                                        fuzzy_distance = self.fuzzy_distance)

            self._highlighted_result_pages[0] = self.search_query.highlighted_results_page(0)

        except ValueError as e:
            # E.g. invalid query syntax error from tantivy
            raise e

        except Exception as e:
            logger.error(f"SearchQueryTask::_fulltext_search(): {e}")

    def suttas_exact_match_page(self, page_num: int) -> List[SearchResult]:
        db_eng, db_conn, db_session = get_db_engine_connection_session()

        res: List[USutta] = []
        try:
            if 'AND' in self.query_text:

                query_total = 0

                and_terms = list(map(lambda x: x.strip(), self.query_text.split('AND')))

                query = db_session.query(Am.Sutta)
                for i in and_terms:
                    query = query.filter(Am.Sutta.content_plain.contains(i))

                query_total += query.count()

                query = query.offset(page_num * self._page_len).limit(self._page_len)
                r = query.all()
                res.extend(r)

                query = db_session.query(Um.Sutta)
                for i in and_terms:
                    query = query.filter(Um.Sutta.content_plain.contains(i))

                query_total += query.count()

                query = query.offset(page_num * self._page_len).limit(self._page_len)
                r = query.all()
                res.extend(r)

                self._db_query_hits_count = query_total

            # there is no 'AND' in self.query_text
            else:

                query_total = 0

                query = db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.content_plain.contains(self.query_text))

                query_total += query.count()

                r = query \
                    .offset(page_num * self._page_len) \
                    .limit(self._page_len) \
                    .all()
                res.extend(r)

                query = db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.content_plain.contains(self.query_text))

                query_total += query.count()

                r = query \
                    .offset(page_num * self._page_len) \
                    .limit(self._page_len) \
                    .all()
                res.extend(r)

                self._db_query_hits_count = query_total

        except Exception as e:
            logger.error(f"SearchQueryTask::suttas_exact_match_page(): {e}")
            return []

        if self.source is not None:
            if self.source_include:
                res = list(filter(self._sutta_only_in_source, res))
            else:
                res = list(filter(self._sutta_except_in_source, res))

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return list(map(self._db_sutta_to_result, res))

    def dict_words_exact_match_page(self, page_num: int) -> List[SearchResult]:
        db_eng, db_conn, db_session = get_db_engine_connection_session()

        res: List[UDictWord] = []
        try:
            if 'AND' in self.query_text:

                query_total = 0

                and_terms = list(map(lambda x: x.strip(), self.query_text.split('AND')))

                query = db_session.query(Am.DictWord)
                for i in and_terms:
                    query = query.filter(Am.DictWord.definition_plain.contains(i))

                query_total += query.count()

                query = query.offset(page_num * self._page_len).limit(self._page_len)
                r = query.all()
                res.extend(r)

                query = db_session.query(Um.DictWord)
                for i in and_terms:
                    query = query.filter(Um.DictWord.definition_plain.contains(i))

                query_total += query.count()

                query = query.offset(page_num * self._page_len).limit(self._page_len)
                r = query.all()
                res.extend(r)

                self._db_query_hits_count = query_total

            # there is no 'AND' in self.query_text
            else:

                query_total = 0

                query = db_session \
                    .query(Am.DictWord) \
                    .filter(Am.DictWord.definition_plain.contains(self.query_text))

                query_total += query.count()

                r = query \
                    .offset(page_num * self._page_len) \
                    .limit(self._page_len) \
                    .all()
                res.extend(r)

                query = db_session \
                    .query(Um.DictWord) \
                    .filter(Um.DictWord.definition_plain.contains(self.query_text))

                query_total += query.count()

                r = query \
                    .offset(page_num * self._page_len) \
                    .limit(self._page_len) \
                    .all()
                res.extend(r)

                self._db_query_hits_count = query_total

        except Exception as e:
            logger.error(f"SearchQueryTask::dict_words_exact_match_page(): {e}")

        if self.source is not None:
            if self.source_include:
                res = list(filter(self._word_only_in_source, res))
            else:
                res = list(filter(self._word_except_in_source, res))

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return list(map(self._db_word_to_result, res))

    def dpd_id_word(self) -> List[SearchResult]:
        dpd_id = int(self.query_text)

        db_eng, db_conn, db_session = get_db_engine_connection_session()

        dpd_word = db_session.query(Dpd.PaliWord) \
                             .filter(Dpd.PaliWord.id == dpd_id) \
                             .first()

        res_page = []

        if dpd_word is not None:
            snippet = dpd_word.meaning_1 if dpd_word.meaning_1 else ""

            res = dpd_pali_word_to_search_result(dpd_word, snippet)
            res_page.append(res)

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return res_page

    def _inflection_to_pali_words(self, db_session: Session, query: str) -> List[Dpd.PaliWord]:
        words = []

        i2h = db_session.query(Dpd.DpdI2h) \
                        .filter(Dpd.DpdI2h.word == query) \
                        .first()

        if i2h is not None:
            # i2h result exists
            # dpd_ebts has short definitions. For Simsapa, retreive the PaliWords.
            #
            # Lookup headwords in pali_words.

            r = db_session.query(Dpd.PaliWord) \
                            .filter(Dpd.PaliWord.pali_1.in_(i2h.headword_list)) \
                            .all()
            if r is not None:
                words.extend(r)

        return words

    def dpd_tbw_lookup(self) -> List[SearchResult]:
        logger.info("dpd_tbw_lookup()")

        db_eng, db_conn, db_session = get_db_engine_connection_session()

        # ![flowchart](https://github.com/digitalpalidictionary/dpd-db/blob/main/tbw/docs/dpd%20lookup%20systen.png)

        pali_words: List[Dpd.PaliWord] = []

        # Lookup word in dpd_i2h (inflections to headwords).

        # word: Inflected form. `phalena`
        # data: pali_1 headwords in TSV list. `phala 1.1   phala 1.2   phala 2.1   phala 2.2   phala 1.3`

        pali_words.extend(self._inflection_to_pali_words(db_session, self.query_text))

        print(len(pali_words))

        if len(pali_words) == 0:
            # i2h result doesn't exist
            # Lookup query text in dpd_deconstructor.

            # word: Inflected compound. `kammapattā`
            # data: List of breakdown. `kamma + pattā<br>kamma + apattā<br>kammi + apattā`

            r = db_session.query(Dpd.DpdDeconstructor) \
                          .filter(Dpd.DpdDeconstructor.word == self.query_text) \
                          .first()
            if r is not None:
                for w in r.headword_list:
                    pali_words.extend(self._inflection_to_pali_words(db_session, w))

        res_page = []

        print(len(pali_words))

        for w in pali_words:
            snippet = w.meaning_1 if w.meaning_1 else ""
            res = dpd_pali_word_to_search_result(w, snippet)
            res_page.append(res)

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return res_page

    def _suttas_title_match(self, db_session: Session):
        # SearchMode.TitleMatch only applies to suttas.
        try:
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

            if self.source is not None:
                if self.source_include:
                    res_suttas = list(filter(self._sutta_only_in_source, res_suttas))
                else:
                    res_suttas = list(filter(self._sutta_except_in_source, res_suttas))

            self._all_results = list(map(self._db_sutta_to_result, res_suttas))

        except Exception as e:
            logger.error(f"SearchQueryTask::_suttas_title_match(): {e}")

    def _dict_words_headword_match(self, db_session: Session):
        # SearchMode.HeadwordMatch only applies to dictionary words.
        try:
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

            if self.source is not None:
                if self.source_include:
                    res = list(filter(self._word_only_in_source, res))
                else:
                    res = list(filter(self._word_except_in_source, res))

            self._all_results = list(map(self._db_word_to_result, res))

        except Exception as e:
            logger.error(f"SearchQueryTask::_dict_words_headword_match(): {e}")

    def run(self):
        logger.info("SearchQueryTask::run()")
        self._all_results = []
        self._highlighted_result_pages = dict()

        db_eng, db_conn, db_session = get_db_engine_connection_session()

        if self.search_mode == SearchMode.FulltextMatch:
            self._fulltext_search()

        elif self.search_mode == SearchMode.ExactMatch:
            self.results_page(0)

        elif self.search_mode == SearchMode.TitleMatch:
            self._suttas_title_match(db_session)

        elif self.search_mode == SearchMode.HeadwordMatch:
            self._dict_words_headword_match(db_session)

        elif self.search_mode == SearchMode.DpdTbwLookup:
            self._tbw_search()

        self.query_finished_time = datetime.now()

        db_conn.close()
        db_session.close()
        db_eng.dispose()
