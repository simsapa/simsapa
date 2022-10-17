from functools import partial
from typing import Callable, List
from PyQt6.QtGui import QColor

from PyQt6.QtWidgets import QLabel, QListWidget, QListWidgetItem, QPushButton, QSpinBox

from simsapa.app.db.search import SearchResult, SearchQuery
from simsapa.layouts.search_item import SearchItemWidget

class HasFulltextList:
    features: List[str]
    fulltext_label: QLabel
    fulltext_list: QListWidget
    fulltext_prev_btn: QPushButton
    fulltext_next_btn: QPushButton
    fulltext_last_page_btn: QPushButton
    fulltext_first_page_btn: QPushButton
    fulltext_page_input: QSpinBox
    search_query: SearchQuery
    _results: List[SearchResult]
    _handle_query: Callable
    _handle_result_select: Callable
    page_len: int

    def init_fulltext_list(self):
        self.features.append('fulltext_list')
        self.connect_fulltext_list_signals()

        self.fulltext_label.clear()
        self.fulltext_list.clear()

        self.fulltext_list.setUniformItemSizes(True)

        self.fulltext_page_input.setMinimum(0)
        self.fulltext_page_input.setMaximum(0)
        self.fulltext_first_page_btn.setEnabled(False)
        self.fulltext_last_page_btn.setEnabled(False)

    def render_fulltext_page(self):
        page_num = self.fulltext_page_input.value() - 1
        page_start = page_num * self.page_len

        page_end = page_start + self.page_len
        if page_end > self.search_query.hits:
            page_end = self.search_query.hits

        self.fulltext_list.clear()

        if self.search_query.hits == 0:
            self.fulltext_label.clear()
            return

        self._results = self.search_query.highlight_results_page(page_num)

        msg = f"Showing {page_start+1}-{page_end} out of {self.search_query.hits} results"
        self.fulltext_label.setText(msg)

        colors = ["#ffffff", "#efefef"]

        for idx, x in enumerate(self._results):
            w = SearchItemWidget()
            w.setFromResult(x)

            item = QListWidgetItem(self.fulltext_list)
            item.setSizeHint(w.sizeHint())

            n = idx % len(colors)
            item.setBackground(QColor(colors[n]))

            self.fulltext_list.addItem(item)
            self.fulltext_list.setItemWidget(item, w)

    def _show_prev_page(self):
        n = self.fulltext_page_input.value()
        if n > self.fulltext_page_input.minimum():
            self.fulltext_page_input.setValue(n - 1)

    def _show_next_page(self):
        n = self.fulltext_page_input.value()
        if n < self.fulltext_page_input.maximum():
            self.fulltext_page_input.setValue(n + 1)

    def _show_first_page(self):
        self.fulltext_page_input.setValue(1)

    def _show_last_page(self):
        n = self.fulltext_page_input.maximum()
        self.fulltext_page_input.setValue(n)

    def connect_fulltext_list_signals(self):

        self.fulltext_list.itemSelectionChanged.connect(partial(self._handle_result_select))

        self.fulltext_page_input \
            .valueChanged.connect(partial(self.render_fulltext_page))

        self.fulltext_prev_btn.clicked.connect(partial(self._show_prev_page))
        self.fulltext_next_btn.clicked.connect(partial(self._show_next_page))

        self.fulltext_first_page_btn \
            .clicked.connect(partial(self._show_first_page))

        self.fulltext_last_page_btn \
            .clicked.connect(partial(self._show_last_page))
