import re
from datetime import datetime
from typing import Dict, List, Optional

import tantivy

from sqlalchemy import and_, or_, not_

from simsapa import logger, SearchResult
from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.helpers import consistent_niggahita
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.types import SearchArea, SearchParams, SearchMode, UDictWord, USutta
from simsapa.app.search.helpers import dict_word_to_search_result, dpd_lookup, sutta_to_search_result, unique_search_results
from simsapa.app.search.tantivy_index import TantivySearchQuery

class SearchQueryTask:
    _highlighted_result_pages: Dict[int, List[SearchResult]] = dict()
    _db_all_results: List[SearchResult] = []
    _db_query_hits_count = 0

    def __init__(self,
                 lang: str,
                 ix: tantivy.Index,
                 query_text_orig: str,
                 query_started_time: datetime,
                 params: SearchParams,
                 area: SearchArea):

        self.ix = ix
        self.query_text = consistent_niggahita(query_text_orig)
        self.query_started_time = query_started_time
        self.query_finished_time: Optional[datetime] = None

        self.search_mode = params['mode']
        self.search_area = area

        self._page_len = params['page_len'] if params['page_len'] is not None else 20

        self.lang = lang
        self.lang_include = params['lang_include']
        s = params['source']
        self.source = None if s is None else s.lower()
        self.source_include = params['source_include']
        self._highlighted_result_pages = dict()

        self.enable_regex = params['enable_regex']
        self.fuzzy_distance = params['fuzzy_distance']

        self.search_query = TantivySearchQuery(self.ix, params)

    def query_hits(self) -> Optional[int]:
        if self.search_mode == SearchMode.Combined:
            a = len(self._db_all_results)
            b = self.search_query.hits_count
            if b is not None:
                a += b

            return a

        elif self.search_mode == SearchMode.FulltextMatch:
            return self.search_query.hits_count

        elif self.search_mode == SearchMode.DpdIdMatch or \
             self.search_mode == SearchMode.DpdLookup or \
             self.search_mode == SearchMode.UidMatch:

            return len(self._db_all_results)

        elif self.search_mode == SearchMode.ContainsMatch or \
             self.search_mode == SearchMode.RegExMatch:
            return self._db_query_hits_count

        else:
            return len(self._db_all_results)

    def all_results(self) -> List[SearchResult]:
        if self.search_mode == SearchMode.Combined:
            res = []
            res.extend(self._db_all_results)
            res.extend(self.search_query.get_all_results())
            return res

        elif self.search_mode == SearchMode.FulltextMatch:
            return self.search_query.get_all_results()

        else:
            return self._db_all_results

    def _highlight_text(self, query: str, content: str) -> str:
        pat = re.compile(f"({query})")
        return pat.sub(r"<span class='match'>\1</span>", content)

    def _highlight_query_in_content(self, query: str, content: str) -> str:
        if 'AND' in query:
            terms = list(map(lambda x: x.strip(), query.split('AND')))
        else:
            terms = [query]

        for i in terms:
            content = self._highlight_text(i, content)

        return content

    def results_page(self, page_num: int) -> List[SearchResult]:
        # If this results page has been calculated before, return it.
        if page_num in self._highlighted_result_pages:
            return self._highlighted_result_pages[page_num]

        # Otherwise, run the queries and return the results page.

        if self.search_mode == SearchMode.Combined:
            res: List[SearchResult] = []

            # Display all DPD Lookup results (not many) on the first (0 index) results page.
            if page_num == 0:
                def _boost(i: SearchResult) -> SearchResult:
                    if i['score'] is None:
                        i['score'] = 10000
                    else:
                        i['score'] += 10000
                    return i

                # Run DPD Lookup and boost results to the top.
                res.extend([_boost(i) for i in self._dpd_lookup()])
                self._db_all_results = res

            # The Fulltext query has been executed before this, request the
            # results with highlighted snippets.
            res.extend(self.search_query.highlighted_results_page(page_num))

            res = unique_search_results(res)

            self._highlighted_result_pages[page_num] = res

        elif self.search_mode == SearchMode.FulltextMatch:
            res = self.search_query.highlighted_results_page(page_num)
            self._highlighted_result_pages[page_num] = res

        elif self.search_mode == SearchMode.UidMatch:
            res = self.uid_word()
            self._highlighted_result_pages[page_num] = res
            self._db_all_results = res

        elif self.search_mode == SearchMode.DpdIdMatch:
            res = self.dpd_id_word()
            self._highlighted_result_pages[page_num] = res
            self._db_all_results = res

        elif self.search_mode == SearchMode.DpdLookup:
            # Display all DPD Lookup results (not many) on the first (0 index) results page.
            res = self._dpd_lookup()
            self._highlighted_result_pages[0] = res
            self._db_all_results = res

        else:
            def _add_highlight(x: SearchResult) -> SearchResult:
                x['snippet'] = self._highlight_query_in_content(self.query_text, x['snippet'])
                return x

            results = []

            if self.search_area == SearchArea.Suttas:
                results = self.suttas_contains_or_regex_match_page(page_num)

            elif self.search_area == SearchArea.DictWords:
                results = self.dict_words_contains_or_regex_match_page(page_num)

            else:
                logger.error(f"Unknown SearchArea: {self.search_area}")

            self._highlighted_result_pages[page_num] = list(map(_add_highlight, results))

        return self._highlighted_result_pages[page_num]

    def _fragment_around_text(self, query: str, content: str, chars_before = 20, chars_after = 500) -> str:
        n = content.lower().find(query.lower())
        if n == -1:
            return content

        prefix = ""
        postfix = ""

        if n <= chars_before:
            a = 0
        else:
            a = n - chars_before
            prefix = "... "

        if len(content) <= a+chars_after:
            b = len(content) - 1
        else:
            b = a+chars_after
            postfix = " ..."

        return prefix + content[a:b] + postfix

    def _fragment_around_query(self, query: str, content: str) -> str:
        fragment = ""
        if 'AND' in query:
            terms = list(map(lambda x: x.strip(), query.split('AND')))
            before = 10
            after = 50
        else:
            terms = [query]
            before = 20
            after = 500

        for i in terms:
            fragment += self._fragment_around_text(i, content, chars_before=before, chars_after=after)

        return fragment

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

    def _sutta_only_in_source(self, x: USutta) -> bool:
        return str(x.uid).endswith(f'/{self.source}')

    def _sutta_except_in_source(self, x: USutta) -> bool:
        return not self._sutta_only_in_source(x)

    def _word_only_in_source(self, x: UDictWord) -> bool:
        return str(x.uid).endswith(f'/{self.source}')

    def _word_except_in_source(self, x: UDictWord) -> bool:
        return not self._word_only_in_source(x)

    def _run_fulltext_query(self):
        try:
            self.search_query.new_query(self.query_text,
                                        self.source,
                                        self.source_include,
                                        enable_regex = self.enable_regex,
                                        fuzzy_distance = self.fuzzy_distance)

        except ValueError as e:
            # E.g. invalid query syntax error from tantivy
            raise e

        except Exception as e:
            logger.error(f"SearchQueryTask::_fulltext_search(): {e}")
            raise e

    def suttas_contains_or_regex_match_page(self, page_num: int) -> List[SearchResult]:
        db_eng, db_conn, db_session = get_db_engine_connection_session()

        res: List[USutta] = []

        am_query = db_session.query(Am.Sutta)
        um_query = db_session.query(Um.Sutta)

        if self.source is not None:
            if self.source_include:
                am_query = am_query.filter(Am.Sutta.uid.like(f'%/{self.source}'))
                um_query = um_query.filter(Um.Sutta.uid.like(f'%/{self.source}'))
            else:
                am_query = am_query.filter(Am.Sutta.uid.not_like(f'%/{self.source}'))
                um_query = um_query.filter(Um.Sutta.uid.not_like(f'%/{self.source}'))

        try:
            query_total = 0

            if 'AND' in self.query_text:
                terms = list(map(lambda x: x.strip(), self.query_text.split('AND')))
            else:
                terms = [self.query_text]

            for i in terms:
                if self.search_mode == SearchMode.ContainsMatch:
                    am_query = am_query.filter(Am.Sutta.content_plain.contains(i))
                    um_query = um_query.filter(Um.Sutta.content_plain.contains(i))

                elif self.search_mode == SearchMode.RegExMatch:
                    am_query = am_query.filter(Am.Sutta.content_plain.regexp_match(i))
                    um_query = um_query.filter(Um.Sutta.content_plain.regexp_match(i))

                else:
                    logger.error(f"Invalid search mode in suttas_contains_or_regex_match_page(): {self.search_mode}")
                    return []

            query_total += am_query.count()
            query_total += um_query.count()

            am_query = am_query.offset(page_num * self._page_len).limit(self._page_len)
            um_query = um_query.offset(page_num * self._page_len).limit(self._page_len)

            res.extend(am_query.all())
            res.extend(um_query.all())

            self._db_query_hits_count = query_total

        except Exception as e:
            logger.error(f"SearchQueryTask::suttas_contains_or_regex_match_page(): {e}")
            return []

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return list(map(self._db_sutta_to_result, res))

    def dict_words_contains_or_regex_match_page(self, page_num: int) -> List[SearchResult]:
        db_eng, db_conn, db_session = get_db_engine_connection_session()

        res: List[UDictWord] = []

        am_query = db_session.query(Am.DictWord)
        um_query = db_session.query(Um.DictWord)
        dpd_head_query = db_session.query(Dpd.DpdHeadwords)
        dpd_root_query = db_session.query(Dpd.DpdRoots)

        if self.source is not None:
            if self.source_include:
                if self.source == "dpd":
                    # Only DPD
                    am_query = None
                    um_query = None
                else:
                    # Exclude DPD
                    am_query = am_query.filter(Am.DictWord.uid.like(f'%/{self.source}'))
                    um_query = um_query.filter(Um.DictWord.uid.like(f'%/{self.source}'))
                    dpd_head_query = None
                    dpd_root_query = None

            else:
                if self.source == "dpd":
                    # Exclude DPD
                    am_query = am_query.filter(Am.DictWord.uid.not_like(f'%/{self.source}'))
                    um_query = um_query.filter(Um.DictWord.uid.not_like(f'%/{self.source}'))
                    dpd_head_query = None
                    dpd_root_query = None
                else:
                    # Only DPD
                    am_query = None
                    um_query = None

        try:
            query_total = 0

            if 'AND' in self.query_text:
                terms = list(map(lambda x: x.strip(), self.query_text.split('AND')))
            else:
                terms = [self.query_text]

            for i in terms:
                if self.search_mode == SearchMode.ContainsMatch:

                    if am_query is not None:
                        am_query = am_query.filter(Am.DictWord.definition_plain.contains(i))

                    if um_query is not None:
                        um_query = um_query.filter(Um.DictWord.definition_plain.contains(i))

                    if dpd_head_query is not None:
                        dpd_head_query = dpd_head_query.filter(
                            or_(Dpd.DpdHeadwords.lemma_clean.contains(i),
                                Dpd.DpdHeadwords.word_ascii.contains(i),
                                Dpd.DpdHeadwords.meaning_1.contains(i)))

                    if dpd_root_query is not None:
                        dpd_root_query = dpd_root_query.filter(
                            or_(Dpd.DpdRoots.root_clean.contains(i),
                                Dpd.DpdRoots.word_ascii.contains(i),
                                Dpd.DpdRoots.root_meaning.contains(i)))

                elif self.search_mode == SearchMode.RegExMatch:

                    if am_query is not None:
                        am_query = am_query.filter(Am.DictWord.definition_plain.regexp_match(i))

                    if um_query is not None:
                        um_query = um_query.filter(Um.DictWord.definition_plain.regexp_match(i))

                    if dpd_head_query is not None:
                        dpd_head_query = dpd_head_query.filter(
                            or_(Dpd.DpdHeadwords.lemma_clean.regexp_match(i),
                                Dpd.DpdHeadwords.word_ascii.regexp_match(i),
                                Dpd.DpdHeadwords.meaning_1.regexp_match(i)))

                    if dpd_root_query is not None:
                        dpd_root_query = dpd_root_query.filter(
                            or_(Dpd.DpdRoots.root_clean.regexp_match(i),
                                Dpd.DpdRoots.word_ascii.regexp_match(i),
                                Dpd.DpdRoots.root_meaning.regexp_match(i)))

                else:
                    logger.error(f"Invalid search mode in dict_words_contains_or_regex_match_page(): {self.search_mode}")
                    return []

            for q in [am_query,
                      um_query,
                      dpd_head_query,
                      dpd_root_query]:

                if q is not None:
                    query_total += q.count()
                    q = q.offset(page_num * self._page_len).limit(self._page_len)
                    res.extend(q.all())

            self._db_query_hits_count = query_total

        except Exception as e:
            logger.error(f"SearchQueryTask::dict_words_contains_or_regex_match_page(): {e}")

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return list(map(self._db_word_to_result, res))

    def uid_word(self) -> List[SearchResult]:
        uid = self.query_text.lower() \
                             .replace("uid:", "")

        logger.info(f"uid_word() {uid}")

        db_eng, db_conn, db_session = get_db_engine_connection_session()

        if uid.endswith("/dpd"):
            word = db_session.query(Dpd.DpdHeadwords) \
                            .filter(Dpd.DpdHeadwords.uid == uid) \
                            .first()

            if word is None:
                word = db_session.query(Dpd.DpdRoots) \
                                .filter(Dpd.DpdRoots.uid == uid) \
                                .first()

        else:
            word = db_session.query(Am.DictWord) \
                            .filter(Am.DictWord.uid == uid) \
                            .first()

            if word is None:
                word = db_session.query(Um.DictWord) \
                                .filter(Um.DictWord.uid == uid) \
                                .first()

        res_page = []

        if word is not None:
            snippet = str(word.definition_plain)
            if len(snippet) > 100:
                snippet = snippet[0:100] + " ..."

            res = dict_word_to_search_result(word, snippet)
            res_page.append(res)

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return res_page

    def dpd_id_word(self) -> List[SearchResult]:
        if not self.query_text.isdigit():
            return []

        dpd_id = int(self.query_text)

        db_eng, db_conn, db_session = get_db_engine_connection_session()

        dpd_word = db_session.query(Dpd.DpdHeadwords) \
                             .filter(Dpd.DpdHeadwords.id == dpd_id) \
                             .first()

        res_page = []

        if dpd_word is not None:
            snippet = dpd_word.meaning_1 if dpd_word.meaning_1 != "" else dpd_word.meaning_2

            res = dict_word_to_search_result(dpd_word, snippet)
            res_page.append(res)

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return res_page

    def _dpd_lookup(self) -> List[SearchResult]:
        logger.info("_dpd_lookup()")

        # DPD is English.
        if self.lang != "en":
            return []

        db_eng, db_conn, db_session = get_db_engine_connection_session()

        res_page = dpd_lookup(db_session, self.query_text)
        # FIXME implement paging in DPD lookup results.
        res_page = res_page[0:100]

        db_conn.close()
        db_session.close()
        db_eng.dispose()

        return res_page

    def _suttas_title_match(self):
        # SearchMode.TitleMatch only applies to suttas.
        try:
            db_eng, db_conn, db_session = get_db_engine_connection_session()

            res_suttas: List[USutta] = []

            am_query = db_session.query(Am.Sutta)
            um_query = db_session.query(Um.Sutta)

            if self.source is not None:
                if self.source_include:
                    am_query = am_query.filter(Am.Sutta.uid.like(f'%/{self.source}'))
                    um_query = um_query.filter(Um.Sutta.uid.like(f'%/{self.source}'))
                else:
                    am_query = am_query.filter(Am.Sutta.uid.not_like(f'%/{self.source}'))
                    um_query = um_query.filter(Um.Sutta.uid.not_like(f'%/{self.source}'))

            am_query = am_query.filter(Am.Sutta.title.like(f"{self.query_text}%"))
            um_query = um_query.filter(Um.Sutta.title.like(f"{self.query_text}%"))

            res_suttas.extend(am_query.all())
            res_suttas.extend(um_query.all())

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

            self._db_all_results = list(map(self._db_sutta_to_result, res_suttas))

            db_conn.close()
            db_session.close()
            db_eng.dispose()

        except Exception as e:
            logger.error(f"SearchQueryTask::_suttas_title_match(): {e}")

    def _dict_words_headword_match(self):
        # SearchMode.HeadwordMatch only applies to dictionary words.
        try:
            db_eng, db_conn, db_session = get_db_engine_connection_session()

            res: List[UDictWord] = []

            am_query = db_session.query(Am.DictWord)
            um_query = db_session.query(Um.DictWord)

            if self.source is not None:
                if self.source_include:
                    am_query = am_query.filter(Am.DictWord.uid.like(f'%/{self.source}'))
                    um_query = um_query.filter(Um.DictWord.uid.like(f'%/{self.source}'))
                else:
                    am_query = am_query.filter(Am.DictWord.uid.not_like(f'%/{self.source}'))
                    um_query = um_query.filter(Um.DictWord.uid.not_like(f'%/{self.source}'))

            am_query = am_query.filter(or_(
                Am.DictWord.word.like(f"{self.query_text}%"),
                Am.DictWord.word_nom_sg.like(f"{self.query_text}%"),
                Am.DictWord.inflections.like(f"{self.query_text}%"),
                Am.DictWord.phonetic.like(f"{self.query_text}%"),
                Am.DictWord.transliteration.like(f"{self.query_text}%"),
                Am.DictWord.also_written_as.like(f"{self.query_text}%"),
            ))

            um_query = um_query.filter(or_(
                Um.DictWord.word.like(f"{self.query_text}%"),
                Um.DictWord.word_nom_sg.like(f"{self.query_text}%"),
                Um.DictWord.inflections.like(f"{self.query_text}%"),
                Um.DictWord.phonetic.like(f"{self.query_text}%"),
                Um.DictWord.transliteration.like(f"{self.query_text}%"),
                Um.DictWord.also_written_as.like(f"{self.query_text}%"),
            ))

            res.extend(am_query.all())
            res.extend(um_query.all())

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

            self._db_all_results = list(map(self._db_word_to_result, res))

            db_conn.close()
            db_session.close()
            db_eng.dispose()

        except Exception as e:
            logger.error(f"SearchQueryTask::_dict_words_headword_match(): {e}")

    def run(self):
        logger.info("SearchQueryTask::run()")
        self._db_all_results = []
        self._highlighted_result_pages = dict()

        if self.search_mode == SearchMode.Combined or \
           self.search_mode == SearchMode.FulltextMatch:

            self._run_fulltext_query()
            self.results_page(0)

        elif self.search_mode == SearchMode.DpdIdMatch or \
             self.search_mode == SearchMode.DpdLookup or \
             self.search_mode == SearchMode.UidMatch or \
             self.search_mode == SearchMode.ContainsMatch or \
             self.search_mode == SearchMode.RegExMatch:

            self.results_page(0)

        elif self.search_mode == SearchMode.TitleMatch:
            self._suttas_title_match()

        elif self.search_mode == SearchMode.HeadwordMatch:
            self._dict_words_headword_match()

        else:
            logger.error(f"Unknown SearchMode: {self.search_mode}")

        self.query_finished_time = datetime.now()

