from datetime import datetime
from functools import partial
import math
from typing import List, Optional
from PyQt6 import QtGui
from PyQt6.QtCore import QPoint, QThreadPool, QTimer, QUrl, Qt
from PyQt6.QtGui import QClipboard, QCloseEvent, QIcon, QPixmap, QStandardItemModel, QStandardItem, QScreen
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWidgets import QCompleter, QDialog, QFrame, QBoxLayout, QHBoxLayout, QLabel, QLineEdit, QListWidget, QPushButton, QSizePolicy, QSpacerItem, QSpinBox, QTabWidget, QVBoxLayout, QWidget

from simsapa import READING_BACKGROUND_COLOR, SEARCH_TIMER_SPEED, SIMSAPA_PACKAGE_DIR, logger
from simsapa.app.db.search import SearchResult, dict_word_hit_to_search_result
from simsapa.app.types import AppData, UDictWord, WindowPosSize
from simsapa.layouts.dictionary_queries import DictionaryQueries
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.fulltext_list import HasFulltextList
from .search_query_worker import SearchQueryWorker, SearchRet

CSS_EXTRA_BODY = "body { font-size: 0.82rem; }"

class WordScanPopupState(QWidget, HasFulltextList):

    search_input: QLineEdit
    wrap_layout: QBoxLayout
    content_layout: QVBoxLayout
    qwe: QWebEngineView
    _app_data: AppData
    _layout: QVBoxLayout
    _results: List[SearchResult]
    _clipboard: Optional[QClipboard]
    _autocomplete_model: QStandardItemModel
    _current_words: List[UDictWord]
    _search_timer = QTimer()
    _last_query_time = datetime.now()
    search_query_worker: Optional[SearchQueryWorker] = None

    def __init__(self, app_data: AppData, wrap_layout: QBoxLayout, focus_input: bool = True) -> None:
        super().__init__()

        self.wrap_layout = wrap_layout

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._results: List[SearchResult] = []
        self._current_words = []

        self.page_len = 20

        self.thread_pool = QThreadPool()

        self.queries = DictionaryQueries(self._app_data)
        self._autocomplete_model = QStandardItemModel()

        self._clipboard = self._app_data.clipboard

        self.focus_input = focus_input

        self._ui_setup()
        self._connect_signals()

        self.init_fulltext_list()

    def _ui_setup(self):
        search_box = QHBoxLayout()

        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Type or copy to clipboard")
        self.search_input.setClearButtonEnabled(True)

        self.search_input.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.search_input.setMinimumHeight(35)

        completer = QCompleter(self._autocomplete_model, self)
        completer.setMaxVisibleItems(10)
        completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        completer.setModelSorting(QCompleter.ModelSorting.CaseInsensitivelySortedModel)

        self.search_input.setCompleter(completer)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/search"))

        self.search_button = QPushButton()
        self.search_button.setFixedSize(35, 35)
        self.search_button.setIcon(icon)

        search_box.addWidget(self.search_input)
        search_box.addWidget(self.search_button)

        self.wrap_layout.addLayout(search_box)

        if self.focus_input:
            self.search_input.setFocus()

        self._setup_search_tabs()

    def _init_search_query_worker(self, query: str = ""):
        disabled_labels = self._app_data.app_settings.get('disabled_dict_labels', None)
        self._last_query_time = datetime.now()

        self.search_query_worker = SearchQueryWorker(
            self._app_data.search_indexed.dict_words_index,
            self.page_len,
            dict_word_hit_to_search_result)

        self.search_query_worker.set_query(query,
                                           self._last_query_time,
                                           disabled_labels,
                                           None)

        self.search_query_worker.signals.finished.connect(partial(self._search_query_finished))

    def _get_css_extra(self) -> str:
        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        css_extra = f"html {{ font-size: {font_size}px; }} " + CSS_EXTRA_BODY

        return css_extra

    def _setup_qwe(self):
        self.qwe = QWebEngineView()
        self.qwe.setPage(ReaderWebEnginePage(self))

        self.qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        msg = """
<p>
    Select a word or phrase and copy to clipboard with Ctrl+C.
    When the clipboard content changes, this window will display dictionary lookup results.
</p>
"""
        page_html = self.queries.render_html_page(body=msg, css_extra=self._get_css_extra())
        self._set_qwe_html(page_html)

        self.qwe.show()
        self.content_layout.addWidget(self.qwe, 100)

    def _set_qwe_html(self, html: str):
        self._current_html = html
        self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _show_temp_content_msg(self, html_body: str):
        self.tabs.setCurrentIndex(0)
        page_html = self.queries.render_html_page(body=html_body, css_extra=self._get_css_extra())
        self.qwe.setHtml(page_html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _show_current_html(self):
        self.qwe.setHtml(self._current_html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _render_words(self, words: List[UDictWord]):
        self._current_words = words
        if len(words) == 0:
            return

        page_html = self.queries.words_to_html_page(words, self._get_css_extra())
        self._set_qwe_html(page_html)

    def _setup_search_tabs(self):
        self.tabs_layout = QVBoxLayout()

        self.tabs = QTabWidget()

        self.tabs_layout.addWidget(self.tabs)
        self.tabs_layout.setContentsMargins(0, 0, 0, 0)

        self.wrap_layout.addLayout(self.tabs_layout)

        self._setup_words_tab()
        self._setup_fulltext_tab()

    def _setup_words_tab(self):
        self.tab_word = QWidget()
        self.tab_word.setObjectName("Words")
        self.tab_word.setStyleSheet("QWidget#Words { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.tabs.addTab(self.tab_word, "Words")

        self.content_layout = QVBoxLayout()
        self.tab_word.setLayout(self.content_layout)

        self._setup_qwe()

    def _setup_fulltext_tab(self):
        self.fulltext_tab = QWidget()
        self.fulltext_tab.setObjectName("Fulltext")
        self.fulltext_tab.setStyleSheet("QWidget#Fulltext { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.tabs.addTab(self.fulltext_tab, "Fulltext")

        self.fulltext_tab_layout = QVBoxLayout(self.fulltext_tab)
        self.fulltext_tab_inner_layout = QVBoxLayout()

        self.fulltext_pages_layout = QHBoxLayout()

        self.fulltext_page_input = QSpinBox(self.fulltext_tab)
        self.fulltext_page_input.setMinimum(1)
        self.fulltext_page_input.setMaximum(999)
        self.fulltext_pages_layout.addWidget(self.fulltext_page_input)

        self.fulltext_prev_btn = QPushButton("Prev", self.fulltext_tab)
        self.fulltext_pages_layout.addWidget(self.fulltext_prev_btn)

        self.fulltext_next_btn = QPushButton("Next", self.fulltext_tab)
        self.fulltext_pages_layout.addWidget(self.fulltext_next_btn)

        spacerItem2 = QSpacerItem(40, 20, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.fulltext_pages_layout.addItem(spacerItem2)

        self.fulltext_first_page_btn = QPushButton("First", self.fulltext_tab)
        self.fulltext_pages_layout.addWidget(self.fulltext_first_page_btn)

        self.fulltext_last_page_btn = QPushButton("Last", self.fulltext_tab)
        self.fulltext_pages_layout.addWidget(self.fulltext_last_page_btn)

        self.fulltext_tab_inner_layout.addLayout(self.fulltext_pages_layout)

        self.fulltext_label = QLabel(self.fulltext_tab)
        self.fulltext_tab_inner_layout.addWidget(self.fulltext_label)

        self.fulltext_list = QListWidget(self.fulltext_tab)
        self.fulltext_list.setFrameShape(QFrame.Shape.NoFrame)
        self.fulltext_tab_inner_layout.addWidget(self.fulltext_list)

        self.fulltext_tab_layout.addLayout(self.fulltext_tab_inner_layout)

    def _set_query(self, s: str):
        self.search_input.setText(s)

    def _show_word(self, word: UDictWord):
        self._current_words = [word]
        word_html = self.queries.get_word_html(word)

        page_html = self.queries.render_html_page(
            body = word_html['body'],
            css_head = word_html['css'],
            css_extra = self._get_css_extra(),
            js_head = word_html['js'])

        self._set_qwe_html(page_html)

    def _show_word_by_bword_url(self, url: QUrl):
        # bword://localhost/American%20pasqueflower
        # path: /American pasqueflower
        query = url.path().replace('/', '')
        logger.info(f"Show Word: {query}")
        self._set_query(query)
        self._handle_query()
        self._handle_exact_query()

    def _show_word_by_uid(self, uid: str):
        results = self.queries.get_words_by_uid(uid)
        if len(results) > 0:
            self._show_word(results[0])

    def _update_fulltext_page_btn(self, hits: int):
        if hits == 0:
            self.fulltext_page_input.setMinimum(0)
            self.fulltext_page_input.setMaximum(0)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        elif hits <= self.page_len:
            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(1)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        else:
            pages = math.floor(hits / self.page_len) + 1
            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(pages)
            self.fulltext_first_page_btn.setEnabled(True)
            self.fulltext_last_page_btn.setEnabled(True)

    def _search_query_finished(self, ret: SearchRet):
        if self.search_query_worker is None:
            return

        if self._last_query_time != ret['query_started']:
            return

        self._results = ret['results']

        icon_search = QtGui.QIcon()
        icon_search.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.search_button.setIcon(icon_search)

        if self.search_query_worker.search_query.hits > 0:
            self.tabs.setTabText(1, f"Fulltext ({self.search_query_worker.search_query.hits})")
        else:
            self.tabs.setTabText(1, "Fulltext")

        self.render_fulltext_page()

        if self.search_query_worker.search_query.hits == 1 and self._results[0]['uid'] is not None:
            self._show_word_by_uid(self._results[0]['uid'])

        self._update_fulltext_page_btn(self.search_query_worker.search_query.hits)

    def _start_query_worker(self, query: str):
        self._init_search_query_worker(query)
        if self.search_query_worker is not None:
            self.thread_pool.start(self.search_query_worker)

    def _handle_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        # Not aborting, show the user that the app started processsing
        icon_processing = QtGui.QIcon()
        icon_processing.addPixmap(QtGui.QPixmap(":/stopwatch"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.search_button.setIcon(icon_processing)

        self._start_query_worker(query)

    def _handle_autocomplete_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        self._autocomplete_model.clear()

        a = self.queries.autocomplete_hits(query)

        for i in a:
            self._autocomplete_model.appendRow(QStandardItem(i))

        self._autocomplete_model.sort(0)

    def _handle_exact_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        res = self.queries.word_exact_matches(query)

        self._render_words(res)

    def _handle_result_select(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx < len(self._results):
            word = self.queries.dict_word_from_result(self._results[selected_idx])
            if word is not None:
                self._show_word(word)

    def _handle_clipboard_changed(self):
        if self._clipboard is None:
            return

        text = self._clipboard.text()
        text = text.strip(".,:;!? ")
        if not text.startswith('http') and not text.startswith('uid:'):
            self.search_input.setText(text)
            self._handle_query(min_length=4)
            self._handle_exact_query(min_length=4)

    def _user_typed(self):
        if not self._search_timer.isActive():
            self._search_timer = QTimer()
            self._search_timer.timeout.connect(partial(self._handle_query, min_length=4))
            self._search_timer.setSingleShot(True)

        self._search_timer.start(SEARCH_TIMER_SPEED)

    def highlight_results_page(self, page_num: int) -> List[SearchResult]:
        if self.search_query_worker is None:
            return []
        else:
            return self.search_query_worker.search_query.highlight_results_page(page_num)

    def query_hits(self) -> int:
        if self.search_query_worker is None:
            return 0
        else:
            return self.search_query_worker.search_query.hits

    def _connect_signals(self):
        if self._clipboard is not None:
            self._clipboard.dataChanged.connect(partial(self._handle_clipboard_changed))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textEdited.connect(partial(self._user_typed))

        # FIXME is this useful? completion appears regardless.
        #self.search_input.completer().activated.connect(partial(self._handle_query, min_length=1))

        self.search_input.textEdited.connect(partial(self._handle_autocomplete_query, min_length=4))

        self.search_button.clicked.connect(partial(self._handle_exact_query, min_length=1))
        self.search_input.returnPressed.connect(partial(self._handle_exact_query, min_length=1))
        self.search_input.completer().activated.connect(partial(self._handle_exact_query, min_length=1))

class WordScanPopup(QDialog):
    oldPos: QPoint

    def __init__(self, app_data: AppData, focus_input: bool = True) -> None:
        super().__init__()

        self._app_data: AppData = app_data

        self.wrap_layout = QVBoxLayout()
        self.wrap_layout.setContentsMargins(8, 8, 8, 8)
        self.setLayout(self.wrap_layout)

        self.setWindowTitle("Clipboard Scanning Word Lookup")
        self.setMinimumSize(50, 50)

        self._restore_size_pos()

        flags = Qt.WindowType.Dialog | \
            Qt.WindowType.WindowStaysOnTopHint

        self.setWindowFlags(Qt.WindowType(flags))

        self.setObjectName("WordScanPopup")
        self.setStyleSheet("#WordScanPopup { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self._margin = 8

        self.focus_input = focus_input

        self.s = WordScanPopupState(app_data, self.wrap_layout, self.focus_input)

    def _restore_size_pos(self):
        p: Optional[WindowPosSize] = self._app_data.app_settings.get('word_scan_popup_pos', None)
        if p is not None:
            self.resize(p['width'], p['height'])
            self.move(p['x'], p['y'])
        else:
            self.center()
            self.resize(400, 500)

    def center(self):
        qr = self.frameGeometry()
        cp = QScreen().availableGeometry().center()
        qr.moveCenter(cp)
        self.move(qr.topLeft())

    def _noop(self):
        pass

    def closeEvent(self, event: QCloseEvent):
        qr = self.frameGeometry()
        p = WindowPosSize(
            x = qr.x(),
            y = qr.y(),
            width = qr.width(),
            height = qr.height(),
        )
        self._app_data.app_settings['word_scan_popup_pos'] = p
        self._app_data._save_app_settings()

        if self.s._clipboard is not None:
            self.s._clipboard.dataChanged.connect(partial(self._noop))

        event.accept()
