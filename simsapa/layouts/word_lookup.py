from datetime import datetime
from functools import partial
import math, subprocess, json, queue
from typing import List, Optional
from PyQt6 import QtCore
from PyQt6 import QtWidgets
from PyQt6 import QtGui
from PyQt6.QtCore import QSize, QTimer, QUrl, Qt, pyqtSignal
from PyQt6.QtGui import QAction, QClipboard, QCloseEvent, QHideEvent, QIcon, QKeySequence, QPixmap, QShortcut, QScreen
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWidgets import QCheckBox, QFrame, QBoxLayout, QHBoxLayout, QLabel, QLineEdit, QListWidget, QMainWindow, QMenu, QMenuBar, QPushButton, QSizePolicy, QSpacerItem, QSpinBox, QSplitter, QTabWidget, QVBoxLayout, QWidget

from simsapa import IS_SWAY, READING_BACKGROUND_COLOR, SIMSAPA_PACKAGE_DIR, SearchResult, DetailsTab, logger, APP_QUEUES, ApiAction, ApiMessage, TIMER_SPEED, QueryType

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.search.helpers import get_word_for_schema_table_and_uid, get_word_gloss_html, get_word_meaning

from simsapa.app.types import SearchArea, UDictWord
from simsapa.app.app_data import AppData
from simsapa.app.search.dictionary_queries import ExactQueryResult
from simsapa.layouts.find_panel import FindPanel, FindSearched
from simsapa.layouts.gui_helpers import get_search_params

from simsapa.layouts.gui_types import LinkHoverData, WindowPosSize, WindowType, WordLookupInterface, WordLookupStateInterface
from simsapa.layouts.gui_queries import GuiSearchQueries
from simsapa.layouts.preview_window import PreviewWindow
from simsapa.layouts.reader_web import ReaderWebEnginePage

from simsapa.layouts.parts.search_bar import HasSearchBar
from simsapa.layouts.parts.deconstructor_list import HasDeconstructorList
from simsapa.layouts.parts.fulltext_list import HasFulltextList

CSS_EXTRA_BODY = ""

class WordLookupState(WordLookupStateInterface, HasDeconstructorList, HasFulltextList, HasSearchBar):

    search_input: QLineEdit
    wrap_layout: QBoxLayout
    content_layout: QVBoxLayout
    qwe: QWebEngineView
    _app_data: AppData
    _layout: QVBoxLayout
    _clipboard: Optional[QClipboard]
    _current_words: List[UDictWord]
    _current_results_page: List[SearchResult] = []
    _search_timer = QTimer()
    _last_query_time = datetime.now()

    show_sutta_by_url = pyqtSignal(QUrl)
    show_words_by_url = pyqtSignal(QUrl)

    link_mouseover = pyqtSignal(dict)
    link_mouseleave = pyqtSignal(str)
    hide_preview = pyqtSignal()

    def __init__(self,
                 app_data: AppData,
                 parent_window: QMainWindow,
                 central_widget: QWidget,
                 wrap_layout: QBoxLayout,
                 focus_input: bool = True,
                 enable_regex_fuzzy = True,
                 enable_find_panel = False,
                 language_filter_setting_key = 'word_lookup_language_filter_idx',
                 search_mode_setting_key = 'word_lookup_search_mode',
                 source_filter_setting_key = 'word_lookup_source_filter_idx') -> None:
        super().__init__()

        self.pw = parent_window
        self.cw = central_widget

        self.wrap_layout = wrap_layout

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._current_words = []

        self._queries = GuiSearchQueries(self._app_data.db_session,
                                         None,
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

        self._clipboard = self._app_data.clipboard

        self.focus_input = focus_input

        self.enable_find_panel = enable_find_panel

        self._search_mode_setting_key = search_mode_setting_key
        self._language_filter_setting_key = language_filter_setting_key
        self._source_filter_setting_key = source_filter_setting_key

        self.init_search_bar(wrap_layout            = self.wrap_layout,
                             search_area            = SearchArea.DictWords,
                             enable_nav_buttons     = False,
                             enable_language_filter = True,
                             enable_search_extras   = True,
                             enable_regex_fuzzy     = enable_regex_fuzzy,
                             enable_info_button     = False,
                             enable_sidebar_button  = False,
                             input_fixed_size       = None,
                             icons_height           = 35,
                             focus_input            = self.focus_input,
                             two_rows_layout        = True)

        self._setup_vertical_splitter()
        self._setup_deconstructor_layout()
        self._setup_search_tabs()

        if self.enable_find_panel:
            self._find_panel = FindPanel()

        self._connect_signals()

        self.init_deconstructor_list()
        self.init_fulltext_list()

    def handle_messages(self):
        # No behaviour atm in this window relies on receiving messages.
        pass

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

    def _copy_clipboard_text(self, text: str):
        self._app_data.clipboard_setText(text)

    def _copy_clipboard_html(self, html: str):
        self._app_data.clipboard_setHtml(html)

    def _copy_gloss(self, db_schema: str, db_table: str, db_uid: str, gloss_keys: str):
        w = get_word_for_schema_table_and_uid(self._app_data.db_session, db_schema, db_table, db_uid)
        self._copy_clipboard_html(get_word_gloss_html(w, gloss_keys))

    def _copy_meaning(self, db_schema: str, db_table: str, db_uid: str):
        w = get_word_for_schema_table_and_uid(self._app_data.db_session, db_schema, db_table, db_uid)
        self._copy_clipboard_text(get_word_meaning(w))

    def _setup_qwe(self):
        self.qwe = QWebEngineView()

        page = ReaderWebEnginePage(self)

        page.helper.mouseover.connect(partial(self._link_mouseover))
        page.helper.mouseleave.connect(partial(self._link_mouseleave))
        page.helper.dblclick.connect(partial(self._lookup_selection_in_dictionary, show_results_tab=False))
        page.helper.hide_preview.connect(partial(self._emit_hide_preview))
        page.helper.copy_clipboard_text.connect(partial(self._copy_clipboard_text))
        page.helper.copy_clipboard_html.connect(partial(self._copy_clipboard_html))
        page.helper.copy_gloss.connect(partial(self._copy_gloss))
        page.helper.copy_meaning.connect(partial(self._copy_meaning))
        page.helper.load_more_results.connect(partial(self._load_more_results))

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

    def query_hits(self) -> Optional[int]:
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

    def _setup_vertical_splitter(self):
        self.vert_splitter = QSplitter(self.cw)
        self.wrap_layout.addWidget(self.vert_splitter)

        self.vert_splitter.setHandleWidth(10)
        self.vert_splitter.setMinimumHeight(200)
        self.vert_splitter.setOrientation(QtCore.Qt.Orientation.Vertical)

    def _setup_deconstructor_layout(self):
        self.deconstructor_wrap_widget = QWidget(self.vert_splitter)
        self.deconstructor_wrap_layout = QVBoxLayout(self.deconstructor_wrap_widget)

        self.show_deconstructor = QCheckBox("Deconstructor Results (0)")
        self.show_deconstructor.setChecked(True)
        self.deconstructor_wrap_layout.addWidget(self.show_deconstructor)

        self.deconstructor_frame = QFrame(self.deconstructor_wrap_widget)
        self.deconstructor_frame.setFrameShape(QFrame.Shape.NoFrame)
        self.deconstructor_frame.setFrameShadow(QFrame.Shadow.Raised)
        self.deconstructor_frame.setContentsMargins(0, 0, 0, 0)
        self.deconstructor_frame.setLineWidth(0)
        self.deconstructor_frame.setMinimumHeight(100)
        self.deconstructor_frame.setObjectName("DeconstructorFrame")

        self.deconstructor_wrap_layout.addWidget(self.deconstructor_frame)

    def _setup_search_tabs(self):
        self.tabs_widget = QWidget(self.vert_splitter)
        self.tabs_widget.setContentsMargins(0, 0, 0, 0)

        self.tabs_layout = QVBoxLayout(self.tabs_widget)

        self.tabs = QTabWidget()

        self.tabs_layout.addWidget(self.tabs)
        self.tabs_layout.setContentsMargins(0, 0, 0, 0)

        self._setup_words_tab()
        self._setup_fulltext_tab()
        self.fulltext_results_tab_idx = 1

    def _setup_words_tab(self):
        self.tab_word = QWidget()
        self.tab_word.setObjectName("Words")
        self.tab_word.setStyleSheet("QWidget#Words { color: #000000; background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.tabs.addTab(self.tab_word, "Words")

        self.content_layout = QVBoxLayout()
        self.tab_word.setLayout(self.content_layout)

        self._setup_qwe()

    def _setup_fulltext_tab(self):
        self.fulltext_tab = QWidget()
        self.fulltext_tab.setObjectName("Results")
        self.fulltext_tab.setStyleSheet("QWidget#Results { color: #000000; background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.tabs.addTab(self.fulltext_tab, "Results")

        self.fulltext_tab_layout = QVBoxLayout(self.fulltext_tab)
        self.fulltext_tab_inner_layout = QVBoxLayout()

        self.fulltext_pages_layout = QHBoxLayout()

        self.fulltext_page_input = QSpinBox(self.fulltext_tab)
        self.fulltext_page_input.setMinimum(1)
        self.fulltext_page_input.setMaximum(999)
        self.fulltext_pages_layout.addWidget(self.fulltext_page_input)

        icon_size = QSize(14, 14)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/angle-left"))
        self.fulltext_prev_btn = QPushButton("", self.fulltext_tab)
        self.fulltext_prev_btn.setIcon(icon)
        self.fulltext_prev_btn.setIconSize(icon_size)
        self.fulltext_pages_layout.addWidget(self.fulltext_prev_btn)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/angle-right"))
        self.fulltext_next_btn = QPushButton("", self.fulltext_tab)
        self.fulltext_next_btn.setIcon(icon)
        self.fulltext_next_btn.setIconSize(icon_size)
        self.fulltext_pages_layout.addWidget(self.fulltext_next_btn)

        self.fulltext_label = QLabel(self.fulltext_tab)
        self.fulltext_label.setObjectName("fulltext_label")
        self.fulltext_label.setStyleSheet("#fulltext_label { color: #000000; background-color: %s; }" % READING_BACKGROUND_COLOR)

        # Add next the buttons to save vertical space
        self.fulltext_pages_layout.addWidget(self.fulltext_label)

        spacerItem2 = QSpacerItem(0, 20, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.fulltext_pages_layout.addItem(spacerItem2)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/angles-left"))
        self.fulltext_first_page_btn = QPushButton("", self.fulltext_tab)
        self.fulltext_first_page_btn.setIcon(icon)
        self.fulltext_first_page_btn.setIconSize(icon_size)
        self.fulltext_pages_layout.addWidget(self.fulltext_first_page_btn)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/angles-right"))
        self.fulltext_last_page_btn = QPushButton("", self.fulltext_tab)
        self.fulltext_last_page_btn.setIcon(icon)
        self.fulltext_last_page_btn.setIconSize(icon_size)
        self.fulltext_pages_layout.addWidget(self.fulltext_last_page_btn)

        self.fulltext_tab_inner_layout.addLayout(self.fulltext_pages_layout)

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

    def _show_word(self, word: UDictWord):
        self.tabs.setTabText(0, str(word.uid))

        self._current_words = [word]
        open_details = [DetailsTab.Inflections]
        word_html = self._queries.dictionary_queries.get_word_html(word, open_details)

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

    def _lookup_selection_in_dictionary(self, show_results_tab = False):
        text = self._get_selection()
        if text is not None:
            self.lookup_in_dictionary(text, show_results_tab)

    def lookup_in_dictionary(self, query: str, show_results_tab = False):
        self._set_query(query)
        self._handle_query()

        if show_results_tab:
            self.tabs.setCurrentIndex(1)

    def _update_fulltext_page_btn(self, hits: Optional[int]):
        if hits is None:
            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(99)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        elif hits == 0:
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
        if hits is None:
            self.tabs.setTabText(1, "Results")
        elif hits > 0:
            self.tabs.setTabText(1, f"Results ({hits})")
        else:
            self.tabs.setTabText(1, "Results")

        self.render_deconstructor_list_for_query(self.search_input.text().strip())

        r = self.render_fulltext_page()
        self._current_results_page = r
        self._current_results_page_num = 0
        render_len = self._results_page_render_len

        if len(r) > 0 and hits == 1 and r[0]['uid'] is not None:
            self._show_word_by_uid(r[0]['uid'])
        else:
            start_idx = self._current_results_page_num * render_len
            end_idx = start_idx + render_len
            self._render_dict_words_search_results(r[start_idx:end_idx])

        self._update_fulltext_page_btn(hits)

    def _handle_query(self, min_length: int = 4):
        logger.info("_handle_query()")
        query_text_orig = self.search_input.text().strip()

        if not query_text_orig.isdigit() and len(query_text_orig) < min_length:
            return

        idx = self.source_filter_dropdown.currentIndex()
        self._app_data.app_settings['word_lookup_source_filter_idx'] = idx
        self._app_data._save_app_settings()

        # Not aborting, show the user that the app started processsing
        self._show_search_stopwatch_icon()

        self._start_query_workers(query_text_orig)

    def _exact_query_finished(self, q_res: ExactQueryResult):
        logger.info("_exact_query_finished()")

        res: List[UDictWord] = []

        r = self._app_data.db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.uid.in_(q_res['appdata_uids'])) \
            .all()
        res.extend(r)

        r = self._app_data.db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.uid.in_(q_res['userdata_uids'])) \
            .all()
        res.extend(r)

        r = self._app_data.db_session \
            .query(Dpd.PaliWord) \
            .filter(Dpd.PaliWord.uid.in_(q_res['pali_words_uids'])) \
            .all()
        res.extend(r)

        r = self._app_data.db_session \
            .query(Dpd.PaliRoot) \
            .filter(Dpd.PaliRoot.uid.in_(q_res['pali_roots_uids'])) \
            .all()
        res.extend(r)

        query = self.search_input.text().strip()
        self.tabs.setTabText(0, query)

        self.stop_loading_animation()
        self._show_search_normal_icon()

        self._render_words(res)

    def _handle_exact_query(self, min_length: int = 4):
        logger.info("_handle_exact_query()")
        query_text = self.search_input.text().strip()

        if not query_text.isdigit() and len(query_text) < min_length:
            return

        self._queries.start_exact_query_worker(
            query_text,
            partial(self._exact_query_finished),
            get_search_params(self),
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
            # self._handle_exact_query(min_length=4)

    @QtCore.pyqtSlot(dict)
    def on_searched(self, find_searched: FindSearched):
        if find_searched['flag'] is None:
            self.qwe.findText(find_searched['text'])
        else:
            self.qwe.findText(find_searched['text'], find_searched['flag'])

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.link_mouseover.connect(partial(preview_window.link_mouseover))
        self.link_mouseleave.connect(partial(preview_window.link_mouseleave))
        self.hide_preview.connect(partial(preview_window._do_hide))

    def _connect_signals(self):
        if self._clipboard is not None and self._app_data.app_settings['clipboard_monitoring_for_dict']:
            self._clipboard.dataChanged.connect(partial(self._handle_clipboard_changed))

        if self.enable_find_panel:
            self._find_panel.searched.connect(self.on_searched)

class WordLookup(WordLookupInterface):
    def __init__(self, app_data: AppData, focus_input: bool = True) -> None:
        super().__init__()

        self._app_data: AppData = app_data

        if IS_SWAY:
            cmd = """swaymsg 'for_window [title="Word Lookup"] floating enable'"""
            subprocess.Popen(cmd, shell=True)

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self.wrap_layout = QVBoxLayout()
        self.wrap_layout.setContentsMargins(8, 8, 8, 8)

        self._central_widget.setLayout(self.wrap_layout)

        self.setWindowTitle("Word Lookup - Simsapa")
        self.setMinimumSize(50, 50)

        self._restore_size_pos()

        # NOTE: Don't set Qt.WindowType.Dialog flag, it disables the tiling
        # behaviour in window managers such as Cinnamon.
        self.setWindowFlags(Qt.WindowType(Qt.WindowType.WindowStaysOnTopHint))

        self.setObjectName("WordLookup")
        self.setStyleSheet("#WordLookup { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self._setup_menubar()

        self._margin = 8

        self.focus_input = focus_input

        self.s = WordLookupState(app_data, self, self._central_widget, self.wrap_layout, self.focus_input)

        self.action_Focus_Search_Input = QShortcut(QKeySequence("Ctrl+L"), self)
        self.action_Focus_Search_Input.activated.connect(partial(self.s._focus_search_input))

        self._connect_signals()

    def _setup_menubar(self):
        self.menubar = QMenuBar()
        self.setMenuBar(self.menubar)

        # Shared menu actions will be connected in windows.py::_init_word_lookup()

        # Icons

        close_icon = QtGui.QIcon()
        close_icon.addPixmap(QtGui.QPixmap(":/close"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        book_icon = QtGui.QIcon()
        book_icon.addPixmap(QtGui.QPixmap(":/book"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        dict_icon = QtGui.QIcon()
        dict_icon.addPixmap(QtGui.QPixmap(":/dictionary"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        bookmark_icon = QtGui.QIcon()
        bookmark_icon.addPixmap(QtGui.QPixmap(":/bookmark"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        pen_fancy_icon = QtGui.QIcon()
        pen_fancy_icon.addPixmap(QtGui.QPixmap(":/pen-fancy"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        # === File ===

        self.menu_file = QMenu("&File", self.menubar)
        self.menubar.addMenu(self.menu_file)

        self.action_close_window = QAction("&Close Window")
        self.menu_file.addAction(self.action_close_window)

        self.action_Quit = QAction("&Quit Simsapa")
        self.action_Quit.setIcon(close_icon)
        self.action_Quit.setShortcut("Ctrl+Q")
        self.menu_file.addAction(self.action_Quit)

        # === Find ===

        self.menu_find = QMenu("F&ind", self.menubar)
        self.menubar.addMenu(self.menu_find)

        self.action_Show_Search_Bar = QAction("Show Search &Bar")
        self.action_Show_Search_Bar.setCheckable(True)
        self.action_Show_Search_Bar.setChecked(True)
        self.action_Show_Search_Bar.setShortcut("Ctrl+Shift+/")
        self.menu_find.addAction(self.action_Show_Search_Bar)

        # === Windows ===

        self.menu_windows = QMenu("&Windows", self.menubar)
        self.menubar.addMenu(self.menu_windows)

        self.action_Sutta_Search = QAction("&Sutta Search")
        self.action_Sutta_Search.setIcon(book_icon)
        self.action_Sutta_Search.setShortcut("F5")
        self.menu_windows.addAction(self.action_Sutta_Search)

        self.action_Sutta_Study = QAction("Sutta Study")
        self.action_Sutta_Study.setIcon(book_icon)
        self.action_Sutta_Study.setShortcut("Ctrl+F5")
        self.menu_windows.addAction(self.action_Sutta_Study)

        self.action_Sutta_Index = QAction("Sutta Index")
        self.action_Sutta_Index.setIcon(book_icon)
        self.menu_windows.addAction(self.action_Sutta_Index)

        self.action_Dictionary_Search = QAction("&Dictionary Search")
        self.action_Dictionary_Search.setIcon(dict_icon)
        self.action_Dictionary_Search.setShortcut("F6")
        self.menu_windows.addAction(self.action_Dictionary_Search)

        self.action_Bookmarks = QAction("&Bookmarks")
        self.action_Bookmarks.setIcon(bookmark_icon)
        self.action_Bookmarks.setShortcut("F7")
        self.menu_windows.addAction(self.action_Bookmarks)

        self.action_Ebook_Reader = QAction("&Ebook Reader")
        self.action_Ebook_Reader.setIcon(book_icon)
        self.menu_windows.addAction(self.action_Ebook_Reader)

        self.action_Memos = QAction("&Memos")
        self.action_Memos.setIcon(pen_fancy_icon)
        self.action_Memos.setShortcut("F9")
        self.menu_windows.addAction(self.action_Memos)

    def _restore_size_pos(self):
        p: Optional[WindowPosSize] = self._app_data.app_settings.get('word_lookup_pos', None)
        if p is not None:
            self.resize(p['width'], p['height'])
            self.move(p['x'], p['y'])
        else:
            self.center()
            self.resize(500, 700)

    def center(self):
        qr = self.frameGeometry()
        cp = QScreen().availableGeometry().center()
        qr.moveCenter(cp)
        self.move(qr.topLeft())

    def _noop(self):
        pass

    def _connect_signals(self):
        self.action_close_window.triggered.connect(partial(self._handle_close))

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

    def _handle_close(self):
        # Don't close, hide the window so it doesn't have to be re-created.
        self.hide()

    def closeEvent(self, _: QCloseEvent) -> None:
        self._app_data.save_last_closed_window(WindowType.WordLookup)

        # Don't close, hide the window so it doesn't have to be re-created.
        self.hide()

        # NOTE: if we did close, the clipboard dataChanged signal would also
        # need to be connected to _noop()
        #
        # if self.s._clipboard is not None:
        #     self.s._clipboard.dataChanged.connect(partial(self._noop))
        # return super().closeEvent(event)
