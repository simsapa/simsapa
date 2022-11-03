from datetime import datetime
from typing import Callable, List, Optional, TypedDict

from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from whoosh.index import FileIndex

from simsapa import logger
from ..app.db.search import SearchQuery, SearchResult
from ..app.types import Labels

class SearchRet(TypedDict):
    results: List[SearchResult]
    query_started: datetime


class WorkerSignals(QObject):
    finished = pyqtSignal(dict)


class SearchQueryWorker(QRunnable):
    signals: WorkerSignals

    def __init__(self, ix: FileIndex, page_len: int, hit_to_result_fn: Callable):
        super().__init__()
        self.signals = WorkerSignals()
        self._page_len = page_len

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

    @pyqtSlot()
    def run(self):
        try:
            results: List[SearchResult] = self.search_query.new_query(self.query, self.disabled_labels, self.only_source)

            ret = SearchRet(
                results = results,
                query_started = self.query_started,
            )

            self.signals.finished.emit(ret)

        except Exception as e:
            logger.error(e)
