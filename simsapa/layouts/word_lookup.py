from datetime import datetime
from functools import partial
import math, subprocess, json, queue
from typing import List, Optional
from PyQt6 import QtGui
from PyQt6 import QtCore
from PyQt6 import QtWidgets
from PyQt6.QtCore import QTimer, QUrl, Qt, pyqtSignal
from PyQt6.QtGui import QClipboard, QCloseEvent, QHideEvent, QIcon, QKeySequence, QPixmap, QShortcut, QStandardItemModel, QStandardItem, QScreen
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWidgets import QComboBox, QCompleter, QFrame, QBoxLayout, QHBoxLayout, QLabel, QLineEdit, QListWidget, QPushButton, QSizePolicy, QSpacerItem, QSpinBox, QTabWidget, QVBoxLayout, QWidget

from simsapa import IS_SWAY, READING_BACKGROUND_COLOR, SEARCH_TIMER_SPEED, SIMSAPA_PACKAGE_DIR, logger, APP_QUEUES, ApiAction, ApiMessage, TIMER_SPEED

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.search.helpers import SearchResult

from simsapa.app.types import DictionarySearchModeNameToType, QueryType, SearchArea, SearchMode, UDictWord
from simsapa.app.app_data import AppData
from simsapa.app.types import SearchParams
from simsapa.app.search.dictionary_queries import ExactQueryResult

from simsapa.layouts.gui_types import LinkHoverData, WindowPosSize, WordLookupInterface, WordLookupStateInterface
from simsapa.layouts.gui_queries import GuiSearchQueries
from simsapa.layouts.preview_window import PreviewWindow
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.fulltext_list import HasFulltextList

CSS_EXTRA_BODY = "body { font-size: 0.82rem; }"

class WordLookupState(WordLookupStateInterface, HasFulltextList):

    search_input: QLineEdit
    wrap_layout: QBoxLayout
    content_layout: QVBoxLayout
    qwe: QWebEngineView
    _app_data: AppData
    _layout: QVBoxLayout
    _clipboard: Optional[QClipboard]
    _autocomplete_model: QStandardItemModel
    _current_words: List[UDictWord]
    _search_timer = QTimer()
    _last_query_time = datetime.now()

    show_sutta_by_url = pyqtSignal(QUrl)
    show_words_by_url = pyqtSignal(QUrl)

    link_mouseover = pyqtSignal(dict)
    link_mouseleave = pyqtSignal(str)
    hide_preview = pyqtSignal()

    def __init__(self, app_data: AppData, wrap_layout: QBoxLayout, focus_input: bool = True) -> None:
        super().__init__()

        self.wrap_layout = wrap_layout

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._current_words = []

        self._queries = GuiSearchQueries(self._app_data.db_session,
                                         self._app_data.get_search_indexes,
                                         self._app_data.api_url)
        # FIXME do this in a way that font size updates when user changes the value
        self._queries.dictionary_queries.dictionary_font_size = self._app_data.app_settings.get('dictionary_font_size', 18)

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self.page_len = 20

        self._autocomplete_model = QStandardItemModel()

        self._clipboard = self._app_data.clipboard

        self.focus_input = focus_input

        self._setup_ui()
        self._connect_signals()

        self.init_icons()
        self.init_fulltext_list()

    def handle_messages(self):
        # No behaviour atm in this window relies on receiving messages.
        pass

    def init_icons(self):
        search_icon = QtGui.QIcon()
        search_icon.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self._normal_search_icon = search_icon

        stopwatch_icon = QtGui.QIcon()
        stopwatch_icon.addPixmap(QtGui.QPixmap(":/stopwatch"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self._stopwatch_icon = stopwatch_icon

        warning_icon = QtGui.QIcon()
        warning_icon.addPixmap(QtGui.QPixmap(":/warning"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self._warning_icon = warning_icon

    def _setup_ui(self):
        search_box = QHBoxLayout()

        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Search in dictionary")
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

        self.search_mode_dropdown = QComboBox()
        items = DictionarySearchModeNameToType.keys()
        self.search_mode_dropdown.addItems(items)
        self.search_mode_dropdown.setFixedHeight(35)

        mode = self._app_data.app_settings.get('word_lookup_search_mode', SearchMode.FulltextMatch)
        values = list(map(lambda x: x[1], DictionarySearchModeNameToType.items()))
        idx = values.index(mode)
        self.search_mode_dropdown.setCurrentIndex(idx)

        search_box.addWidget(self.search_mode_dropdown)

        self.search_extras = QtWidgets.QHBoxLayout()
        search_box.addLayout(self.search_extras)

        self._setup_dict_filter_dropdown()

        self.wrap_layout.addLayout(search_box)

        if self.focus_input:
            self.search_input.setFocus()

        self._setup_search_tabs()

    def _get_filter_labels(self):
        res = []

        r = self._app_data.db_session.query(Am.Dictionary.label.distinct()).all()
        res.extend(r)

        r = self._app_data.db_session.query(Um.Dictionary.label.distinct()).all()
        res.extend(r)

        labels = sorted(set(map(lambda x: str(x[0]).lower(), res)))

        return labels

    def _setup_dict_filter_dropdown(self):
        cmb = QComboBox()
        items = ["Dictionaries",]
        items.extend(self._get_filter_labels())
        idx = self._app_data.app_settings.get('word_lookup_dict_filter_idx', 0)

        cmb.addItems(items)
        cmb.setFixedHeight(35)
        cmb.setCurrentIndex(idx)
        self.dict_filter_dropdown = cmb
        self.search_extras.addWidget(self.dict_filter_dropdown)

    def _get_css_extra(self) -> str:
        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        css_extra = f"html {{ font-size: {font_size}px; }} " + CSS_EXTRA_BODY

        return css_extra

    def _show_url(self, url: QUrl):
        if url.host() == QueryType.suttas:
            self._show_sutta_by_url(url)

        elif url.host() == QueryType.words:
            self._show_words_by_url(url)

    def _show_words_by_url(self, url: QUrl):
        if url.host() != QueryType.words:
            return

        self.show_words_by_url.emit(url)

    def _show_sutta_by_url(self, url: QUrl):
        if url.host() != QueryType.suttas:
            return

        self.show_sutta_by_url.emit(url)

    def _link_mouseover(self, hover_data: LinkHoverData):
        self.link_mouseover.emit(hover_data)

    def _link_mouseleave(self, href: str):
        self.link_mouseleave.emit(href)

    def _emit_hide_preview(self):
        self.hide_preview.emit()

    def _copy_clipboard(self, text: str):
        self._app_data.clipboard_setText(text)

    def _setup_qwe(self):
        self.qwe = QWebEngineView()

        page = ReaderWebEnginePage(self)

        page.helper.mouseover.connect(partial(self._link_mouseover))
        page.helper.mouseleave.connect(partial(self._link_mouseleave))
        page.helper.dblclick.connect(partial(self._lookup_selection_in_dictionary, show_results_tab=True, include_exact_query=False))
        page.helper.hide_preview.connect(partial(self._emit_hide_preview))
        page.helper.copy_clipboard.connect(partial(self._copy_clipboard))

        self.qwe.setPage(page)

        self.qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        msg = """
<p>Type in a word or phrase for dictionary lookup.</p>
<p>When 'Find > Double Click ...' is enabled, double clicking on a word in a text will open this Word Lookup window.</p>
<p>When 'Find > Cliboard Monitoring ...' is enabled, this window will automatically lookup words in the dictionary when the clipboard content changes.</p>
        """
        page_html = self._queries.dictionary_queries.render_html_page(body=msg, css_extra=self._get_css_extra())
        self._set_qwe_html(page_html)

        self.qwe.show()
        self.content_layout.addWidget(self.qwe, 100)

        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

    def results_page(self, page_num: int) -> List[SearchResult]:
        return self._queries.results_page(page_num)

    def query_hits(self) -> int:
        return self._queries.query_hits()

    def _set_qwe_html(self, html: str):
        self._current_html = html
        self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _show_temp_content_msg(self, html_body: str):
        self.tabs.setCurrentIndex(0)
        page_html = self._queries.dictionary_queries.render_html_page(body=html_body, css_extra=self._get_css_extra())
        self.qwe.setHtml(page_html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _show_current_html(self):
        self.qwe.setHtml(self._current_html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _render_words(self, words: List[UDictWord]):
        self._current_words = words
        if len(words) == 0:
            return

        page_html = self._queries.dictionary_queries.words_to_html_page(words, self._get_css_extra())
        self._set_qwe_html(page_html)

    def _setup_search_tabs(self):
        self.tabs_layout = QVBoxLayout()

        self.tabs = QTabWidget()

        self.tabs_layout.addWidget(self.tabs)
        self.tabs_layout.setContentsMargins(0, 0, 0, 0)

        self.wrap_layout.addLayout(self.tabs_layout)

        self._setup_words_tab()
        self._setup_fulltext_tab()
        self.fulltext_results_tab_idx = 1

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
        self.fulltext_tab.setObjectName("Results")
        self.fulltext_tab.setStyleSheet("QWidget#Results { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.tabs.addTab(self.fulltext_tab, "Results")

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

        self.fulltext_loading_bar = QLabel(self.fulltext_tab)
        self.fulltext_loading_bar.setMinimumSize(QtCore.QSize(0, 5))
        self.fulltext_loading_bar.setMaximumSize(QtCore.QSize(16777215, 5))
        self.fulltext_loading_bar.setText("")
        self.fulltext_loading_bar.setObjectName("fulltext_loading_bar")

        self.fulltext_tab_inner_layout.addWidget(self.fulltext_loading_bar)

        self.fulltext_list = QListWidget(self.fulltext_tab)
        self.fulltext_list.setFrameShape(QFrame.Shape.NoFrame)
        self.fulltext_tab_inner_layout.addWidget(self.fulltext_list)

        self.fulltext_tab_layout.addLayout(self.fulltext_tab_inner_layout)

    def _set_query(self, s: str):
        self.search_input.setText(s)

    def _focus_search_input(self):
        self.search_input.setFocus()

    def _show_word(self, word: UDictWord):
        self.tabs.setTabText(0, str(word.uid))

        self._current_words = [word]
        word_html = self._queries.dictionary_queries.get_word_html(word)

        page_html = self._queries.dictionary_queries.render_html_page(
            body = word_html['body'],
            css_head = word_html['css'],
            css_extra = self._get_css_extra(),
            js_head = word_html['js'])

        self._set_qwe_html(page_html)

    def _show_word_by_url(self, url: QUrl):
        # bword://localhost/American%20pasqueflower
        # path: /American pasqueflower
        query = url.path().replace('/', '')
        logger.info(f"Show Word: {query}")
        self.lookup_in_dictionary(query)

    def _show_word_by_uid(self, uid: str):
        results = self._queries.dictionary_queries.get_words_by_uid(uid)
        if len(results) > 0:
            self._show_word(results[0])

    def _get_selection(self) -> Optional[str]:
        text = self.qwe.selectedText()
        # U+2029 Paragraph Separator to blank line
        text = text.replace('\u2029', "\n\n")
        text = text.strip()
        if len(text) > 0:
            return text
        else:
            return None

    def _lookup_selection_in_dictionary(self, show_results_tab = False, include_exact_query = True):
        text = self._get_selection()
        if text is not None:
            self.lookup_in_dictionary(text, show_results_tab, include_exact_query)

    def lookup_in_dictionary(self, query: str, show_results_tab = False, include_exact_query = True):
        self._set_query(query)
        self._handle_query()

        if include_exact_query:
            self._handle_exact_query()

        if show_results_tab:
            self.tabs.setCurrentIndex(1)

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

    def _search_query_finished(self, query_started_time: datetime):
        if query_started_time != self._last_query_time:
            return

        if not self._queries.all_finished():
            return

        self.stop_loading_animation()

        # Restore the search icon, processing finished
        self._show_search_normal_icon()

        hits = self.query_hits()
        if hits > 0:
            self.tabs.setTabText(1, f"Results ({hits})")
        else:
            self.tabs.setTabText(1, "Results")

        self.render_fulltext_page()

        results = self.results_page(0)

        if len(results) > 0 and hits == 1 and results[0]['uid'] is not None:
            self._show_word_by_uid(results[0]['uid'])

        self._update_fulltext_page_btn(hits)

    def _get_search_params(self) -> SearchParams:
        idx = self.dict_filter_dropdown.currentIndex()
        source = self.dict_filter_dropdown.itemText(idx)
        if source == "Dictionaries":
            only_source = None
        else:
            only_source = source

        idx = self.search_mode_dropdown.currentIndex()
        s = self.search_mode_dropdown.itemText(idx)
        mode = DictionarySearchModeNameToType[s]

        return SearchParams(
            mode = mode,
            page_len = self.page_len,
            # FIXME UI for selecting language
            only_lang = None,
            only_source = only_source,
        )

    def _start_query_workers(self, query_text: str = ""):
        if len(query_text) == 0:
            return
        logger.info(f"_start_query_workers(): {query_text}")

        if self._app_data.search_indexes is None:
            return

        params = self._get_search_params()

        if params['mode'] == SearchMode.FulltextMatch:
            try:
                self._app_data.search_indexes.test_correct_query_syntax(SearchArea.DictWords, query_text)

            except ValueError as e:
                self._show_search_warning_icon(str(e))
                return

        self.start_loading_animation()

        self._last_query_time = datetime.now()

        self._queries.start_search_query_workers(
            query_text,
            SearchArea.DictWords,
            self._last_query_time,
            partial(self._search_query_finished),
            self._get_search_params(),
        )

    def _handle_query(self, min_length: int = 4):
        query = self.search_input.text().strip()

        if len(query) < min_length:
            return

        idx = self.dict_filter_dropdown.currentIndex()
        self._app_data.app_settings['word_lookup_dict_filter_idx'] = idx
        self._app_data._save_app_settings()

        # Not aborting, show the user that the app started processsing
        self._show_search_stopwatch_icon()

        self._start_query_workers(query)

    def _show_search_normal_icon(self):
        self.search_button.setIcon(self._normal_search_icon)
        self.search_button.setToolTip("Click to start search")

    def _show_search_stopwatch_icon(self):
        self.search_button.setIcon(self._stopwatch_icon)
        self.search_button.setToolTip("Search query running ...")

    def _show_search_warning_icon(self, warning_msg: str = ''):
        self.search_button.setIcon(self._warning_icon)
        self.search_button.setToolTip(warning_msg)

    def _handle_autocomplete_query(self, min_length: int = 4):
        query = self.search_input.text().strip()

        if len(query) < min_length:
            return

        self._autocomplete_model.clear()

        a = self._queries.dictionary_queries.autocomplete_hits(query)

        # FIXME can these be assigned without a loop?
        for i in a:
            self._autocomplete_model.appendRow(QStandardItem(i))

        # NOTE: completion cache is already sorted.
        # self._autocomplete_model.sort(0)

    def _exact_query_finished(self, q_res: ExactQueryResult):
        logger.info("_exact_query_finished()")

        res: List[UDictWord] = []

        r = self._app_data.db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.id.in_(q_res['appdata_ids'])) \
            .all()
        res.extend(r)

        r = self._app_data.db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.id.in_(q_res['userdata_ids'])) \
            .all()
        res.extend(r)

        query = self.search_input.text().strip()
        self.tabs.setTabText(0, query)

        self.stop_loading_animation()
        self._show_search_normal_icon()

        self._render_words(res)

    def _handle_exact_query(self, min_length: int = 4):
        query_text = self.search_input.text().strip()

        if len(query_text) < min_length:
            return

        self._queries.start_exact_query_worker(
            query_text,
            partial(self._exact_query_finished),
            self._get_search_params(),
        )

    def _handle_result_select(self):
        logger.info("_handle_result_select()")

        if len(self.fulltext_list.selectedItems()) == 0:
            return

        page_num = self.fulltext_page_input.value() - 1
        results = self.results_page(page_num)

        selected_idx = self.fulltext_list.currentRow()
        if selected_idx < len(results):
            word = self._queries.dictionary_queries.dict_word_from_result(results[selected_idx])
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
        self._show_search_normal_icon()
        if not self._app_data.app_settings.get('search_as_you_type', True):
            return

        if not self._search_timer.isActive():
            self._search_timer = QTimer()
            self._search_timer.timeout.connect(partial(self._handle_query, min_length=4))
            self._search_timer.setSingleShot(True)

        self._search_timer.start(SEARCH_TIMER_SPEED)

    def _handle_search_mode_changed(self):
        idx = self.search_mode_dropdown.currentIndex()
        m = self.search_mode_dropdown.itemText(idx)

        self._app_data.app_settings['word_lookup_search_mode'] = DictionarySearchModeNameToType[m]
        self._app_data._save_app_settings()

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.link_mouseover.connect(partial(preview_window.link_mouseover))
        self.link_mouseleave.connect(partial(preview_window.link_mouseleave))
        self.hide_preview.connect(partial(preview_window._do_hide))

    def _connect_signals(self):
        if self._clipboard is not None and self._app_data.app_settings['clipboard_monitoring_for_dict']:
            self._clipboard.dataChanged.connect(partial(self._handle_clipboard_changed))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_button.clicked.connect(partial(self._handle_exact_query, min_length=1))

        self.search_input.textEdited.connect(partial(self._user_typed))
        self.search_input.textEdited.connect(partial(self._handle_autocomplete_query, min_length=4))

        self.search_input.completer().activated.connect(partial(self._handle_query, min_length=1))
        self.search_input.completer().activated.connect(partial(self._handle_exact_query, min_length=1))

        self.search_input.returnPressed.connect(partial(self._handle_query, min_length=1))
        self.search_input.returnPressed.connect(partial(self._handle_exact_query, min_length=1))

        self.dict_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))
        self.dict_filter_dropdown.currentIndexChanged.connect(partial(self._handle_exact_query, min_length=4))

        self.search_mode_dropdown.currentIndexChanged.connect(partial(self._handle_search_mode_changed))

class WordLookup(WordLookupInterface):
    def __init__(self, app_data: AppData, focus_input: bool = True) -> None:
        super().__init__()

        self._app_data: AppData = app_data

        self.wrap_layout = QVBoxLayout()
        self.wrap_layout.setContentsMargins(8, 8, 8, 8)
        self.setLayout(self.wrap_layout)

        self.setWindowTitle("Word Lookup")
        self.setMinimumSize(50, 50)

        if IS_SWAY:
            cmd = """swaymsg 'for_window [title="Word Lookup"] floating enable'"""
            subprocess.Popen(cmd, shell=True)

        self._restore_size_pos()

        flags = Qt.WindowType.Dialog | \
            Qt.WindowType.WindowStaysOnTopHint

        self.setWindowFlags(Qt.WindowType(flags))

        self.setObjectName("WordLookup")
        self.setStyleSheet("#WordLookup { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self._margin = 8

        self.focus_input = focus_input

        self.s = WordLookupState(app_data, self.wrap_layout, self.focus_input)

        self.action_Focus_Search_Input = QShortcut(QKeySequence("Ctrl+L"), self)
        self.action_Focus_Search_Input.activated.connect(partial(self.s._focus_search_input))

    def _restore_size_pos(self):
        p: Optional[WindowPosSize] = self._app_data.app_settings.get('word_lookup_pos', None)
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

    def hideEvent(self, event: QHideEvent) -> None:
        qr = self.frameGeometry()
        p = WindowPosSize(
            x = qr.x(),
            y = qr.y(),
            width = qr.width(),
            height = qr.height(),
        )
        self._app_data.app_settings['word_lookup_pos'] = p
        self._app_data._save_app_settings()

        msg = ApiMessage(queue_id = 'app_windows', action = ApiAction.hidden_word_lookup, data = '')
        s = json.dumps(msg)
        APP_QUEUES['app_windows'].put_nowait(s)

        return super().hideEvent(event)

    def closeEvent(self, _: QCloseEvent) -> None:
        # Don't close, hide the window so it doesn't have to be re-created.
        self.hide()

        # NOTE: if we did close, the clipboard dataChanged signal would also
        # need to be connected to _noop()
        #
        # if self.s._clipboard is not None:
        #     self.s._clipboard.dataChanged.connect(partial(self._noop))
        # return super().closeEvent(event)
