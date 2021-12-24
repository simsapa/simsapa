import logging as _logging

from functools import partial
from typing import Callable, List
from PyQt5.QtGui import QColor

from PyQt5.QtWidgets import QLabel, QListWidget, QListWidgetItem, QPushButton, QSpinBox

from simsapa.app.db.search import SearchResult, SearchQuery
from simsapa.layouts.search_item import SearchItemWidget

logger = _logging.getLogger(__name__)

class HasResultsList:
    features: List[str]
    results_label: QLabel
    results_list: QListWidget
    results_last_page_btn: QPushButton
    results_first_page_btn: QPushButton
    results_page_input: QSpinBox
    search_query: SearchQuery
    _results: List[SearchResult]
    _handle_query: Callable
    _handle_result_select: Callable
    page_len: int

    def init_results_list(self):
        self.features.append('results_list')
        self.connect_results_list_signals()

        self.results_label.clear()
        self.results_list.clear()

        self.results_page_input.setMinimum(0)
        self.results_page_input.setMaximum(0)
        self.results_first_page_btn.setEnabled(False)
        self.results_last_page_btn.setEnabled(False)

    def render_results_page(self):
        page_num = self.results_page_input.value() - 1
        page_start = page_num * self.page_len

        page_end = page_start + self.page_len
        if page_end > self.search_query.hits:
            page_end = self.search_query.hits

        self.results_list.clear()

        if self.search_query.hits == 0:
            self.results_label.clear()
            return

        self._results = self.search_query.highlight_results_page(page_num)

        msg = f"Showing {page_start+1}-{page_end} out of {self.search_query.hits}"
        self.results_label.setText(msg)

        colors = ["#ffffff", "#efefef"]

        for idx, x in enumerate(self._results):
            w = SearchItemWidget()
            w.setFromResult(x)

            item = QListWidgetItem(self.results_list)
            item.setSizeHint(w.sizeHint())

            n = idx % len(colors)
            item.setBackground(QColor(colors[n]))

            self.results_list.addItem(item)
            self.results_list.setItemWidget(item, w)

    def _show_first_page(self):
        self.results_page_input.setValue(1)

    def _show_last_page(self):
        n = self.results_page_input.maximum()
        self.results_page_input.setValue(n)

    def connect_results_list_signals(self):

        self.results_list.itemSelectionChanged.connect(partial(self._handle_result_select))

        self.results_page_input \
            .valueChanged.connect(partial(self.render_results_page))

        self.results_first_page_btn \
            .clicked.connect(partial(self._show_first_page))

        self.results_last_page_btn \
            .clicked.connect(partial(self._show_last_page))
