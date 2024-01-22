from datetime import datetime
from math import ceil
from typing import List, Optional, Callable
import re

from sqlalchemy.orm.session import Session

from PyQt6.QtCore import QThreadPool

from simsapa import logger, SearchResult

from simsapa.app.search.dictionary_queries import DictionaryQueries, ExactQueryWorker
from simsapa.app.search.query_task import SearchQueryTask
from simsapa.app.search.sutta_queries import SuttaQueries
from simsapa.app.search.tantivy_index import TantivySearchIndexes
from simsapa.app.types import SearchArea, SearchMode, SearchParams

from simsapa.layouts.gui_types import GuiSearchQueriesInterface
from simsapa.layouts.query_worker import SearchQueryWorker

class GuiSearchQueries(GuiSearchQueriesInterface):
    db_session: Session
    api_url: Optional[str] = None
    search_query_workers: List[SearchQueryWorker] = []
    exact_query_worker: Optional[ExactQueryWorker] = None

    sutta_queries: SuttaQueries
    dictionary_queries: DictionaryQueries

    def __init__(self,
                 db_session: Session,
                 search_indexes: Optional[TantivySearchIndexes] = None,
                 search_indexes_getter_fn: Optional[Callable] = None,
                 api_url: Optional[str] = None,
                 page_len = 20):
        logger.profile("GuiSearchQueries::__init__(): start")

        self.db_session = db_session
        self.api_url = api_url
        self.sutta_queries = SuttaQueries(self.db_session)
        self.dictionary_queries = DictionaryQueries(self.db_session, self.api_url)
        self._search_indexes_getter_fn = search_indexes_getter_fn
        self._search_indexes: Optional[TantivySearchIndexes] = None

        if search_indexes is not None:
            self._search_indexes = search_indexes

        self.thread_pool = QThreadPool()

        self._page_len = page_len

        logger.profile("GuiSearchQueries::__init__(): end")

    def reinit_indexes(self):
        self._search_indexes = TantivySearchIndexes(self.db_session)

    def set_search_indexes(self):
        if self._search_indexes_getter_fn is not None:
            self._search_indexes = self._search_indexes_getter_fn()

    def running_queries(self) -> List[SearchQueryWorker]:
        return [i for i in self.search_query_workers if i.task.query_finished_time is None]

    def count_running_queries(self) -> int:
        return len(self.running_queries())

    def all_finished(self) -> bool:
        return (self.count_running_queries() == 0)

    def query_hits(self) -> Optional[int]:
        if len(self.search_query_workers) == 0:
            return 0
        else:
            t = self.search_query_workers[0].task
            if t.enable_regex or t.fuzzy_distance > 0:
                return None

            a = list(filter(None, [i.task.query_hits() for i in self.search_query_workers]))
            return sum(a)

    def start_search_query_workers(self,
                                   query_text_orig: str,
                                   area: SearchArea,
                                   query_started_time: datetime,
                                   finished_fn: Callable,
                                   params: SearchParams):
        self.set_search_indexes()
        if self._search_indexes is None:
            logger.error("self._search_indexes is None")
            return

        assert(self._search_indexes is not None)

        for i in self.search_query_workers:
            i.will_emit_finished = False

        # Create query workers for each language

        # Make sure to empty the workers of a previous query. Otherwise
        # .append(w) would add to a list of workers which start with already
        # deleted threads, and self.thread_pool.start(i) will error.
        self.search_query_workers = []

        if area == SearchArea.Suttas:
            lang_keys = list(self._search_indexes.suttas_lang_index.keys())
            lang_indexes = self._search_indexes.suttas_lang_index
        else:
            lang_keys = list(self._search_indexes.dict_words_lang_index.keys())
            lang_indexes = self._search_indexes.dict_words_lang_index

            if query_text_orig.isdigit():
                # If the query_text is an integer, it is a DPD ID.
                params['mode'] = SearchMode.DpdIdMatch

            elif re.search(r"/[a-z0-9-]+$", query_text_orig.lower()):
                # If the query_text ends with sth like /pts, /sbs-ru, then the
                # user is looking for specific word with a uid.
                params['mode'] = SearchMode.UidMatch

        if params['lang'] is not None:
            if params['lang_include']:
                languages =  [params['lang']]
            else:
                languages = [i for i in lang_keys if i != params['lang']]
        else:
            languages = lang_keys

        for lang in languages:
            # This can happen when the Language dropdown has empty value.
            if lang == "":
                continue

            logger.info(f"SearchQueryWorker for {lang}")

            task = SearchQueryTask(lang,
                                   lang_indexes[lang],
                                   query_text_orig,
                                   query_started_time,
                                   params)

            w = SearchQueryWorker(task, finished_fn)

            self.search_query_workers.append(w)

        for i in self.search_query_workers:
            self.thread_pool.start(i)

    def start_exact_query_worker(self,
                                 query_text: str,
                                 finished_fn: Callable,
                                 params: SearchParams):

        self.exact_query_worker = ExactQueryWorker(
            query_text,
            finished_fn,
            params)

        self.thread_pool.start(self.exact_query_worker)

    def result_pages_count(self) -> Optional[int]:
        # Each query worker stores its result pages, with one page containing
        # page_len items.
        #
        # The total page count is not the total hits / page_len, but the longest
        # page / page_len.
        #
        # Page count may be None in the case of filtered regex or fuzzy queries.
        #
        # When requesting the query's combined pages, the first page of each
        # worker is the first page of the combined results page, the second page
        # of each worker is the second combined page, etc.
        #
        # The query workers have different number of pages (e.g. one worker per
        # language index), hence the first N combined pages may be longer than others.
        #
        # The pages might be:
        #
        # 0: 40 items [20 + 20]
        # 1: 26 items [20 + 6]
        # 2: 20 items [20 + 0]
        # 3: 20 items [20 + 0]
        # ...

        n = self.count_running_queries()
        if n != 0:
            return 0
        else:
            if len(self.search_query_workers) > 0:
                t = self.search_query_workers[0].task
                if t.enable_regex or t.fuzzy_distance > 0:
                    return None

            a = list(filter(None, [i.task.query_hits() for i in self.search_query_workers]))
            worker_max_hits = max(a)
            return ceil(worker_max_hits / self._page_len)

    def results_page(self, page_num: int) -> List[SearchResult]:
        logger.info(f"GuiSearchQueries::results_page(): page_num = {page_num}")
        n = self.count_running_queries()
        if n != 0:
            logger.info(f"Running queries: {n}, return empty results")
            return []
        else:
            a: List[SearchResult] = []
            for i in self.search_query_workers:
                a.extend(i.task.results_page(page_num))

            # The higher the score, the better. Reverse to get descending order.
            res = sorted(a, key=lambda x: x['score'] or 0, reverse = True)

            return res

    def all_results(self, sort_by_score: bool = True) -> List[SearchResult]:
        logger.info(f"GuiSearchQueries::all_results(): sort_by_score = {sort_by_score}")
        n = self.count_running_queries()
        if n != 0:
            logger.info(f"Running queries: {n}, return empty results")
            return []
        else:
            a: List[SearchResult] = []
            for i in self.search_query_workers:
                a.extend(i.task.all_results())

            if sort_by_score:
                # The higher the score, the better. Reverse to get descending order.
                res = sorted(a, key=lambda x: x['score'] or 0, reverse = True)
            else:
                res = a

            return res
