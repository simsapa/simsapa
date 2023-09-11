from functools import partial
from typing import Callable, List, Optional
from PyQt6 import QtGui

from PyQt6.QtCore import Qt
from PyQt6.QtGui import QColor, QMovie
from PyQt6.QtWidgets import QLabel, QListWidget, QListWidgetItem, QPushButton, QSpinBox, QTabWidget
from simsapa import logger

from simsapa.app.app_data import AppData

from simsapa.layouts.gui_types import default_search_result_sizes
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
    _handle_query: Callable
    _handle_result_select: Callable
    page_len: int
    query_hits: Callable[[], Optional[int]]
    result_pages_count: Callable
    results_page: Callable
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

    def get_page_num(self) -> int:
        return self.fulltext_page_input.value()

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
        query_hits = self.query_hits()
        logger.info(f"render_fulltext_page(), query_hits: {query_hits}")

        if query_hits is not None and query_hits == 0:
            self.fulltext_label.clear()
            return

        page_num = self.fulltext_page_input.value() - 1
        if page_num < 0:
            return

        pages_count = self.fulltext_page_input.maximum()

        self.fulltext_list.clear()

        results = self.results_page(page_num)

        if query_hits is None:
            msg = ""

        else:
            msg = f"Page {page_num+1} / {pages_count} of {query_hits} results"

        self.fulltext_label.setText(msg)

        colors = ["#ffffff", "#efefef"]

        sizes = self._app_data.app_settings.get('search_result_sizes', default_search_result_sizes())

        for idx, x in enumerate(results):
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
