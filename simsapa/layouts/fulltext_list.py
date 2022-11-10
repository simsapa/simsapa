from functools import partial
from typing import Callable, List
from PyQt6 import QtGui

from PyQt6.QtCore import Qt
from PyQt6.QtGui import QColor, QMovie
from PyQt6.QtWidgets import QLabel, QListWidget, QListWidgetItem, QPushButton, QSpinBox, QTabWidget

from simsapa.app.db.search import SearchResult
from simsapa.app.types import AppData, default_search_result_sizes
from simsapa.layouts.search_item import SearchItemWidget

class HasFulltextList:
    features: List[str]
    fulltext_label: QLabel
    fulltext_loading_bar: QLabel
    fulltext_list: QListWidget
    fulltext_prev_btn: QPushButton
    fulltext_next_btn: QPushButton
    fulltext_last_page_btn: QPushButton
    fulltext_first_page_btn: QPushButton
    fulltext_page_input: QSpinBox
    fulltext_results_tab_idx: int
    _app_data: AppData
    _results: List[SearchResult]
    _handle_query: Callable
    _handle_result_select: Callable
    page_len: int
    query_hits: Callable
    highlight_results_page: Callable
    rightside_tabs: QTabWidget
    tabs: QTabWidget

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

        self._ui_set_search_icon()
        self._ui_setup_loading_bar()

    def _ui_set_search_icon(self):
        icon_search = QtGui.QIcon()
        icon_search.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        if hasattr(self, 'rightside_tabs'):
            self.rightside_tabs.setTabIcon(self.fulltext_results_tab_idx, icon_search)
        elif hasattr(self, 'tabs'):
            self.tabs.setTabIcon(self.fulltext_results_tab_idx, icon_search)

    def _ui_setup_loading_bar(self):
        self.fulltext_loading_bar.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._loading_bar_anim = QMovie(':loading-bar')
        self._loading_bar_empty_anim = QMovie(':loading-bar-empty')

    def start_loading_animation(self):
        self.fulltext_loading_bar.setMovie(self._loading_bar_anim)
        self._loading_bar_anim.start()

        icon_processing = QtGui.QIcon()
        icon_processing.addPixmap(QtGui.QPixmap(":/stopwatch"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        if hasattr(self, 'rightside_tabs'):
            self.rightside_tabs.setTabIcon(self.fulltext_results_tab_idx, icon_processing)
        elif hasattr(self, 'tabs'):
            self.tabs.setTabIcon(self.fulltext_results_tab_idx, icon_processing)

    def stop_loading_animation(self):
        self._loading_bar_anim.stop()
        self.fulltext_loading_bar.setMovie(self._loading_bar_empty_anim)

        self._ui_set_search_icon()

    def render_fulltext_page(self):
        page_num = self.fulltext_page_input.value() - 1
        page_start = page_num * self.page_len

        page_end = page_start + self.page_len
        if page_end > self.query_hits():
            page_end = self.query_hits()

        self.fulltext_list.clear()

        if self.query_hits() == 0:
            self.fulltext_label.clear()
            return

        self._results = self.highlight_results_page(page_num)

        msg = f"Showing {page_start+1}-{page_end} out of {self.query_hits()} results"
        self.fulltext_label.setText(msg)

        colors = ["#ffffff", "#efefef"]

        sizes = self._app_data.app_settings.get('search_result_sizes', default_search_result_sizes())

        for idx, x in enumerate(self._results):
            w = SearchItemWidget(sizes)
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
