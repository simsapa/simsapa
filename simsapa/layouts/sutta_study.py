from functools import partial
from ..app.db.search import SearchQuery, SearchResult, sutta_hit_to_search_result
from simsapa.layouts.dictionary_queries import DictionaryQueries
from PyQt5.QtCore import QUrl
from PyQt5.QtGui import QIcon, QKeySequence, QPixmap
from typing import List, Optional

from PyQt5 import QtCore, QtGui, QtWidgets
from PyQt5.QtWidgets import QCompleter, QFrame, QHBoxLayout, QLabel, QLineEdit, QListWidget, QMainWindow, QPushButton, QSizePolicy, QSpacerItem, QSpinBox, QSplitter, QTabWidget, QVBoxLayout, QWidget
from PyQt5.QtWebEngineWidgets import QWebEnginePage, QWebEngineSettings, QWebEngineView

from simsapa import READING_BACKGROUND_COLOR, SIMSAPA_PACKAGE_DIR, DbSchemaName, logger, ApiAction, ApiMessage
from simsapa import APP_QUEUES, GRAPHS_DIR, TIMER_SPEED
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.sutta_search import SuttaSearchWindowState

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, USutta, UDictWord
from ..assets.ui.sutta_study_window_ui import Ui_SuttaStudyWindow
from .sutta_tab import SuttaTabWidget
from .html_content import html_page
from .word_scan_popup import WordScanPopupState

CSS_EXTRA = "html { font-size: 14px; }"

class SuttaStudyWindow(QMainWindow, Ui_SuttaStudyWindow):

    splitter: QSplitter
    sutta_one_layout: QVBoxLayout
    sutta_two_layout: QVBoxLayout
    dict_layout: QVBoxLayout

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("SuttaStudyWindow()")

        self.features: List[str] = []
        self._app_data: AppData = app_data

        self.page_len = 20
        self.search_query = SearchQuery(
            self._app_data.search_indexed.suttas_index,
            self.page_len,
            sutta_hit_to_search_result,
        )

        self.queries = DictionaryQueries(self._app_data)

        self._ui_setup()
        self._connect_signals()

    def _ui_setup(self):
        show = self._app_data.app_settings.get('show_related_suttas', True)
        self.action_Show_Related_Suttas.setChecked(show)

        # Setup the splitter and the three columns.

        # One

        self.splitter = QSplitter(self.central_widget)
        self.splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)

        self.sutta_one_layout_widget = QWidget(self.splitter)

        self.sutta_one_layout = QVBoxLayout(self.sutta_one_layout_widget)
        self.sutta_one_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(500, 0, QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        self.sutta_one_layout.addItem(spacer)

        self.sutta_one_searchbar_layout = QHBoxLayout();
        self.sutta_one_layout.addLayout(self.sutta_one_searchbar_layout)

        self.sutta_one_tabs_layout = QVBoxLayout();
        self.sutta_one_layout.addLayout(self.sutta_one_tabs_layout)

        self.sutta_one_state = SuttaSearchWindowState(self._app_data,
                                                      self,
                                                      self.sutta_one_searchbar_layout,
                                                      self.sutta_one_tabs_layout,
                                                      None, False, False, False)

        # Two

        self.sutta_two_layout_widget = QWidget(self.splitter)

        self.sutta_two_layout = QVBoxLayout(self.sutta_two_layout_widget)
        self.sutta_two_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(500, 0, QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        self.sutta_two_layout.addItem(spacer)

        self.main_layout.addWidget(self.splitter)

        self.sutta_two_searchbar_layout = QHBoxLayout();
        self.sutta_two_layout.addLayout(self.sutta_two_searchbar_layout)

        self.sutta_two_tabs_layout = QVBoxLayout();
        self.sutta_two_layout.addLayout(self.sutta_two_tabs_layout)

        self.sutta_two_state = SuttaSearchWindowState(self._app_data,
                                                      self,
                                                      self.sutta_two_searchbar_layout,
                                                      self.sutta_two_tabs_layout,
                                                      None, False, False, False)

        # Setup the two sutta layouts.

        # Setup the dictionary search.

        self.dictionary_layout_widget = QWidget(self.splitter)

        self.dictionary_layout = QVBoxLayout(self.dictionary_layout_widget)
        self.dictionary_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(500, 0, QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)
        self.dictionary_layout.addItem(spacer)

        self.dictionary_state = WordScanPopupState(self._app_data, self.dictionary_layout)

    def _lookup_clipboard_in_suttas(self):
        self.activateWindow()
        s = self._app_data.clipboard_getText()
        if s is not None:
            self.sutta_one_state._set_query(s)
            self.sutta_one_state._handle_query()

    def _lookup_clipboard_in_dictionary(self):
        text = self._app_data.clipboard_getText()
        if text is not None:
            self.dictionary_state._set_query(text)
            self.dictionary_state._handle_query()

    def _lookup_selection_in_suttas(self):
        self.activateWindow()
        text = self.sutta_one_state._get_selection()
        if text is not None:
            self.sutta_one_state._set_query(text)
            self.sutta_one_state._handle_query()
        else:
            text = self.sutta_two_state._get_selection()
            if text is not None:
                self.sutta_two_state._set_query(text)
                self.sutta_two_state._handle_query()

    def _lookup_selection_in_dictionary(self):
        text = self.sutta_one_state._get_selection()
        if text is None:
            text = self.sutta_two_state._get_selection()

        if text is not None:
            self.dictionary_state._set_query(text)
            self.dictionary_state._handle_query()
            self.dictionary_state._handle_exact_query()

    def _handle_copy(self):
        text = self.sutta_one_state._get_selection()
        if text is None:
            text = self.sutta_two_state._get_selection()

        if text is not None:
            self._app_data.clipboard_setText(text)

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Copy \
            .triggered.connect(partial(self._handle_copy))
