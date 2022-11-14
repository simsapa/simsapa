import re
from datetime import datetime
from typing import Callable, Dict, List, Optional

from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from whoosh.index import FileIndex

from sqlalchemy.sql.elements import and_, not_

from simsapa import logger
from simsapa.app.db_helpers import get_db_engine_connection_session
from simsapa.app.helpers import compactPlainText, compactRichText
from ..app.db.search import SearchQuery, SearchResult, dict_word_to_search_result, sutta_to_search_result
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import Labels, SearchMode, UDictWord, USutta


class WorkerSignals(QObject):
    finished = pyqtSignal()


class SearchQueryWorker(QRunnable):
    signals: WorkerSignals
    _all_results: List[SearchResult] = []
    _highlighted_result_pages: Dict[int, List[SearchResult]] = dict()

    def __init__(self, ix: FileIndex, page_len: int, search_mode: SearchMode, hit_to_result_fn: Callable):
        super().__init__()
        self.signals = WorkerSignals()
        self._page_len = page_len
        self.search_mode = search_mode

        self.query = ""
        self.query_started = datetime.now()
        self.disabled_labels = None
        self.only_source = None

        self.search_query = SearchQuery(ix, self._page_len, hit_to_result_fn)

    def set_query(self, query: str, query_started: datetime, disabled_labels: Labels, only_source: Optional[str] = None):
        self.query = query
        self.query_started = query_started
        self.disabled_labels = disabled_labels
        self.only_source = only_source
        self._all_results = []
        self._highlighted_result_pages = dict()

    def query_hits(self):
        if self.search_mode == SearchMode.FulltextMatch:
            return self.search_query.hits
        else:
            return len(self._all_results)

    def all_results(self) -> List[SearchResult]:
        if self.search_mode == SearchMode.FulltextMatch:
            return self.search_query.get_all_results(highlight=False)
        else:
            return self._all_results

    def results_page(self, page_num: int) -> List[SearchResult]:
        if self.search_mode == SearchMode.FulltextMatch:
            if page_num not in self._highlighted_result_pages:
                self._highlighted_result_pages[page_num] = self.search_query.highlight_results_page(page_num)
            return self._highlighted_result_pages[page_num]
        else:
            page_start = page_num * self._page_len
            page_end = page_start + self._page_len
            return self._all_results[page_start:page_end]

    def highlight_results_page(self, page_num: int) -> List[SearchResult]:
        return self.results_page(page_num)

    def _db_sutta_to_result(self, x: USutta) -> SearchResult:
        if x.content_plain is not None and len(str(x.content_plain)) > 0:
            snippet = compactPlainText(str(x.content_plain))
        else:
            snippet = compactRichText(str(x.content_html))

        return sutta_to_search_result(x, snippet)

    def _db_word_to_result(self, x: UDictWord) -> SearchResult:
        if x.summary is not None and len(str(x.summary)) > 0:
            snippet = str(x.summary)
        elif x.definition_plain is not None and len(str(x.definition_plain)) > 0:
            snippet = compactPlainText(str(x.definition_plain))
        else:
            snippet = compactRichText(str(x.definition_html))

        return dict_word_to_search_result(x, snippet)

    def _sutta_only_in_source(self, x: USutta):
        if self.only_source is not None:
            return str(x.uid).endswith(f'/{self.only_source.lower()}')
        else:
            return True

    def _sutta_not_in_disabled(self, x: USutta):
        if self.disabled_labels is not None:
            for schema in self.disabled_labels.keys():
                for label in self.disabled_labels[schema]:
                    if x.metadata.schema == schema and str(x.uid).endswith(f'/{label.lower()}'):
                        return False
            return True
        else:
            return True

    def _word_only_in_source(self, x: UDictWord):
        if self.only_source is not None:
            return str(x.uid).endswith(f'/{self.only_source.lower()}')
        else:
            return True

    def _word_not_in_disabled(self, x: UDictWord):
        if self.disabled_labels is not None:
            for schema in self.disabled_labels.keys():
                for label in self.disabled_labels[schema]:
                    if x.metadata.schema == schema and str(x.uid).endswith(f'/{label.lower()}'):
                        return False
            return True
        else:
            return True

    @pyqtSlot()
    def run(self):
        logger.info("SearchQueryWorker::run()")
        try:
            self._all_results = []
            self._highlighted_result_pages = dict()

            if self.search_mode == SearchMode.FulltextMatch:
                 self.search_query.new_query(self.query, self.disabled_labels, self.only_source)
                 self._highlighted_result_pages[0] = self.search_query.highlight_results_page(0)

            elif self.search_mode == SearchMode.ExactMatch:
                _, _, db_session = get_db_engine_connection_session()

                if self.search_query.ix.indexname == 'suttas':

                    res_suttas: List[USutta] = []

                    r = db_session \
                        .query(Am.Sutta) \
                        .filter(Am.Sutta.content_html.like(f"%{self.query}%")) \
                        .all()
                    res_suttas.extend(r)

                    r = db_session \
                        .query(Um.Sutta) \
                        .filter(Um.Sutta.content_html.like(f"%{self.query}%")) \
                        .all()
                    res_suttas.extend(r)

                    if self.only_source is not None:
                        res_suttas = list(filter(self._sutta_only_in_source, res_suttas))

                    elif self.disabled_labels is not None:
                        res_suttas = list(filter(self._sutta_not_in_disabled, res_suttas))

                    self._all_results = list(map(self._db_sutta_to_result, res_suttas))

                elif self.search_query.ix.indexname == 'dict_words':

                    res: List[UDictWord] = []

                    r = db_session \
                        .query(Am.DictWord) \
                        .filter(Am.DictWord.definition_html.like(f"%{self.query}%")) \
                        .all()
                    res.extend(r)

                    r = db_session \
                        .query(Um.DictWord) \
                        .filter(Um.DictWord.definition_html.like(f"%{self.query}%")) \
                        .all()
                    res.extend(r)

                    if self.only_source is not None:
                        res = list(filter(self._word_only_in_source, res))

                    elif self.disabled_labels is not None:
                        res = list(filter(self._word_not_in_disabled, res))

                    self._all_results = list(map(self._db_word_to_result, res))

                db_session.close()

            elif self.search_mode == SearchMode.TitleMatch:
                # NOTE: SearchMode.TitleMatch only applies to suttas.
                _, _, db_session = get_db_engine_connection_session()

                res_suttas: List[USutta] = []

                r = db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.title.like(f"{self.query}%")) \
                    .all()
                res_suttas.extend(r)

                r = db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.title.like(f"{self.query}%")) \
                    .all()
                res_suttas.extend(r)

                ids = list(map(lambda x: x.id, res_suttas))

                r = db_session \
                    .query(Am.Sutta) \
                    .filter(and_(
                        Am.Sutta.title.like(f"%{self.query}%"),
                        not_(Am.Sutta.id.in_(ids)),
                    )) \
                    .all()
                res_suttas.extend(r)

                r = db_session \
                    .query(Um.Sutta) \
                    .filter(and_(
                        Um.Sutta.title.like(f"%{self.query}%"),
                        not_(Um.Sutta.id.in_(ids)),
                    )) \
                    .all()
                res_suttas.extend(r)

                if self.only_source is not None:
                    res_suttas = list(filter(self._sutta_only_in_source, res_suttas))

                elif self.disabled_labels is not None:
                    res_suttas = list(filter(self._sutta_not_in_disabled, res_suttas))

                self._all_results = list(map(self._db_sutta_to_result, res_suttas))

                db_session.close()

            elif self.search_mode == SearchMode.HeadwordMatch:
                # NOTE: SearchMode.HeadwordMatch only applies to dictionary words.
                _, _, db_session = get_db_engine_connection_session()

                res: List[UDictWord] = []

                r = db_session \
                    .query(Am.DictWord) \
                    .filter(Am.DictWord.word.like(f"{self.query}%")) \
                    .all()
                res.extend(r)

                r = db_session \
                    .query(Um.DictWord) \
                    .filter(Um.DictWord.word.like(f"{self.query}%")) \
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
                        Am.DictWord.word.like(f"%{self.query}%"),
                        not_(Am.DictWord.id.in_(ids)),
                    )) \
                    .all()
                res.extend(r)

                r = db_session \
                    .query(Um.DictWord) \
                    .filter(and_(
                        Um.DictWord.word.like(f"%{self.query}%"),
                        not_(Um.DictWord.id.in_(ids)),
                    )) \
                    .all()
                res.extend(r)

                if self.only_source is not None:
                    res = list(filter(self._word_only_in_source, res))

                elif self.disabled_labels is not None:
                    res = list(filter(self._word_not_in_disabled, res))

                self._all_results = list(map(self._db_word_to_result, res))

                db_session.close()

            logger.info("signals.finished.emit()")
            self.signals.finished.emit()

        except Exception as e:
            logger.error(e)
