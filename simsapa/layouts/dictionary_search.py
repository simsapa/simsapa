from datetime import datetime
from functools import partial
import math, json, queue
from typing import Any, List, Optional
from pathlib import Path

from PyQt6 import QtCore, QtGui
from PyQt6 import QtWidgets
from PyQt6.QtCore import Qt, QUrl, QTimer, pyqtSignal, QSize
from PyQt6.QtGui import QIcon, QCloseEvent, QPixmap, QAction
from PyQt6.QtWidgets import (QFrame, QLineEdit, QListWidget,
                             QHBoxLayout, QPushButton, QSizePolicy, QTabWidget, QToolBar, QVBoxLayout)
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebEngineCore import QWebEngineSettings

from simsapa import SIMSAPA_PACKAGE_DIR, DetailsTab, logger, ApiAction, ApiMessage, APP_QUEUES, GRAPHS_DIR, TIMER_SPEED, QueryType
from simsapa.assets.ui.dictionary_search_window_ui import Ui_DictionarySearchWindow

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd

from simsapa.app.search.helpers import SearchResult, get_word_for_schema_table_and_uid, get_word_gloss, get_word_meaning
from simsapa.app.types import SearchArea, USutta, UDictWord
from simsapa.app.app_data import AppData
from simsapa.app.search.dictionary_queries import ExactQueryResult
from simsapa.app.dict_link_helpers import add_word_links_to_bold

from simsapa.layouts.gui_helpers import get_search_params
from simsapa.layouts.gui_types import LinkHoverData, DictionarySearchWindowInterface, QExpanding, QMinimum
from simsapa.layouts.gui_queries import GuiSearchQueries
from simsapa.layouts.preview_window import PreviewWindow
from simsapa.layouts.find_panel import FindSearched, FindPanel
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.help_info import show_search_info

from simsapa.layouts.parts.search_bar import HasSearchBar
from simsapa.layouts.parts.memo_dialog import HasMemoDialog
from simsapa.layouts.parts.memos_sidebar import HasMemosSidebar
from simsapa.layouts.parts.links_sidebar import HasLinksSidebar
from simsapa.layouts.parts.deconstructor_list import HasDeconstructorList
from simsapa.layouts.parts.fulltext_list import HasFulltextList
from simsapa.layouts.parts.import_stardict_dialog import HasImportStarDictDialog

class DictionarySearchWindow(DictionarySearchWindowInterface, Ui_DictionarySearchWindow,
                             HasDeconstructorList, HasFulltextList, HasSearchBar, HasMemoDialog,
                             HasLinksSidebar, HasMemosSidebar, HasImportStarDictDialog):

    searchbar_layout: QHBoxLayout
    search_extras: QHBoxLayout
    palibuttons_frame: QFrame
    search_input: QLineEdit
    toggle_pali_btn: QPushButton
    content_layout: QVBoxLayout
    qwe: QWebEngineView
    selected_info: Any
    fulltext_results_tab_idx: int = 0
    rightside_tabs: QTabWidget
    _app_data: AppData
    _queries: GuiSearchQueries
    _current_words: List[UDictWord]
    _search_timer = QTimer()
    _last_query_time = datetime.now()

    show_sutta_by_url = pyqtSignal(QUrl)
    show_words_by_url = pyqtSignal(QUrl)

    lookup_in_new_sutta_window_signal = pyqtSignal(str)
    open_words_new_signal = pyqtSignal(list)
    link_mouseover = pyqtSignal(dict)
    link_mouseleave = pyqtSignal(str)
    page_dblclick = pyqtSignal()
    hide_preview = pyqtSignal()

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("DictionarySearchWindow()")

        self.fulltext_list: QListWidget
        self.recent_list: QListWidget

        self.enable_sidebar = True

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._recent: List[UDictWord] = []
        self._current_words: List[UDictWord] = []

        self._queries = GuiSearchQueries(self._app_data.db_session,
                                         self._app_data.get_search_indexes,
                                         self._app_data.api_url)
        # FIXME do this in a way that font size updates when user changes the value
        self._queries.dictionary_queries.dictionary_font_size = self._app_data.app_settings.get('dictionary_font_size', 18)

        self.page_len = 20

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.selected_info = {}

        self.graph_path: Path = GRAPHS_DIR.joinpath(f"{self.queue_id}.html")

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self._search_mode_setting_key = 'dictionary_search_mode'
        self._language_filter_setting_key = 'dictionary_language_filter_idx'
        self._source_filter_setting_key = 'dictionary_source_filter_idx'

        self._setup_ui()

        self.init_search_bar(wrap_layout            = self.searchbar_layout,
                             search_area            = SearchArea.DictWords,
                             enable_nav_buttons     = True,
                             enable_language_filter = True,
                             enable_search_extras   = True,
                             enable_info_button     = True,
                             input_fixed_size       = QSize(250, 35),
                             icons_height           = 40,
                             focus_input            = True)

        self._setup_show_sidebar_btn()
        self._connect_signals()

        self.init_deconstructor_list()
        self.init_fulltext_list()
        self.init_memo_dialog()
        self.init_memos_sidebar()
        self.init_links_sidebar()
        self.init_stardict_import_dialog()

        self._setup_qwe_context_menu()

    def _lookup_clipboard_in_suttas(self):
        text = self._app_data.clipboard_getText()
        if text is not None:
            self.lookup_in_new_sutta_window_signal.emit(text)

    def _lookup_clipboard_in_dictionary(self):
        self.activateWindow()
        s = self._app_data.clipboard_getText()
        if s is not None:
            self._set_query(s)
            self._handle_query()
            self._handle_exact_query()

    def _lookup_selection_in_suttas(self):
        text = self._get_selection()
        if text is not None:
            self.lookup_in_new_sutta_window_signal.emit(text)

    def _lookup_selection_in_dictionary(self, show_results_tab = False, include_exact_query = True):
        self.activateWindow()
        text = self._get_selection()
        if text is not None:
            self.lookup_in_dictionary(text, show_results_tab, include_exact_query)

    def lookup_in_dictionary(self, query: str, show_results_tab = False, include_exact_query = True, add_recent = True):
        self._set_query(query)
        self._handle_query()

        if include_exact_query:
            self._handle_exact_query(add_recent)

        if show_results_tab:
            self.rightside_tabs.setCurrentIndex(0)

    def _get_selection(self) -> Optional[str]:
        text = self.qwe.selectedText()
        # U+2029 Paragraph Separator to blank line
        text = text.replace('\u2029', "\n\n")
        text = text.strip()
        if len(text) > 0:
            return text
        else:
            return None

    def closeEvent(self, event: QCloseEvent):
        if self.queue_id in APP_QUEUES.keys():
            del APP_QUEUES[self.queue_id]

        msg = ApiMessage(queue_id = 'app_windows',
                         action = ApiAction.remove_closed_window_from_list,
                         data = self.queue_id)
        s = json.dumps(msg)
        APP_QUEUES['app_windows'].put_nowait(s)

        if self.graph_path.exists():
            self.graph_path.unlink()

        event.accept()

    def reinit_index(self):
        self._queries.reinit_indexes()

    def handle_messages(self):
        if self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                msg: ApiMessage = json.loads(s)
                if msg['action'] == ApiAction.show_sutta:
                    info = json.loads(msg['data'])
                    self._show_sutta_from_message(info)

                elif msg['action'] == ApiAction.show_sutta_by_uid:
                    info = json.loads(msg['data'])
                    if 'uid' in info.keys():
                        self._show_sutta_by_uid(info['uid'])

                elif msg['action'] == ApiAction.show_word_by_uid:
                    info = json.loads(msg['data'])
                    if 'uid' in info.keys():
                        self._show_word_by_uid(info['uid'])

                elif msg['action'] == ApiAction.lookup_clipboard_in_dictionary:
                    self._lookup_clipboard_in_dictionary()

                elif msg['action'] == ApiAction.lookup_in_dictionary:
                    text = msg['data']
                    self._set_query(text)
                    self._handle_query()
                    self._handle_exact_query()

                elif msg['action'] == ApiAction.set_selected:
                    info = json.loads(msg['data'])
                    self.selected_info = info

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _setup_ui(self):
        self.setWindowTitle("Dictionary Search - Simsapa")

        self.links_tab_idx = 1
        self.memos_tab_idx = 2

        show = self._app_data.app_settings.get('show_dictionary_sidebar', True)
        self.action_Show_Sidebar.setChecked(show)

        if show:
            self.splitter.setSizes([2000, 2000])
        else:
            self.splitter.setSizes([2000, 0])

        # self._setup_pali_buttons() # TODO: reimplement as hover window
        self._setup_qwe()

        self._find_panel = FindPanel()

        self.find_toolbar = QToolBar()
        self.find_toolbar.addWidget(self._find_panel)

        self.addToolBar(QtCore.Qt.ToolBarArea.BottomToolBarArea, self.find_toolbar)
        self.find_toolbar.hide()

    def _setup_show_sidebar_btn(self):
        if not self.enable_sidebar:
            return

        spacerItem = QtWidgets.QSpacerItem(40, 20, QExpanding, QMinimum)

        self.searchbar_layout.addItem(spacerItem)

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/angles-right"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.show_sidebar_btn = QPushButton()
        self.show_sidebar_btn.setIcon(icon)
        self.show_sidebar_btn.setMinimumSize(QtCore.QSize(40, 40))
        self.show_sidebar_btn.setToolTip("Toggle Sidebar")

        self.searchbar_layout.addWidget(self.show_sidebar_btn)

    def _link_mouseover(self, hover_data: LinkHoverData):
        self.link_mouseover.emit(hover_data)

    def _link_mouseleave(self, href: str):
        self.link_mouseleave.emit(href)

    def _emit_hide_preview(self):
        self.hide_preview.emit()

    def _page_dblclick(self):
        if self._app_data.app_settings['double_click_word_lookup']:
            self.page_dblclick.emit()

    def _copy_clipboard_text(self, text: str):
        self._app_data.clipboard_setText(text)

    def _copy_clipboard_html(self, html: str):
        self._app_data.clipboard_setHtml(html)

    def _copy_gloss(self, db_schema: str, db_table: str, db_uid: str, gloss_keys: str):
        w = get_word_for_schema_table_and_uid(self._app_data.db_session, db_schema, db_table, db_uid)
        self._copy_clipboard_html(get_word_gloss(w, gloss_keys))

    def _copy_meaning(self, db_schema: str, db_table: str, db_uid: str):
        w = get_word_for_schema_table_and_uid(self._app_data.db_session, db_schema, db_table, db_uid)
        self._copy_clipboard_text(get_word_meaning(w))

    def _setup_qwe(self):
        self.qwe = QWebEngineView()

        page = ReaderWebEnginePage(self)
        page.helper.mouseover.connect(partial(self._link_mouseover))
        page.helper.mouseleave.connect(partial(self._link_mouseleave))
        page.helper.dblclick.connect(partial(self._page_dblclick))
        page.helper.hide_preview.connect(partial(self._emit_hide_preview))
        page.helper.copy_clipboard_text.connect(partial(self._copy_clipboard_text))
        page.helper.copy_clipboard_html.connect(partial(self._copy_clipboard_html))
        page.helper.copy_gloss.connect(partial(self._copy_gloss))
        page.helper.copy_meaning.connect(partial(self._copy_meaning))

        self.qwe.setPage(page)

        self.qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.qwe.setHtml(self._queries.dictionary_queries.render_html_page(body=''))
        self.qwe.show()
        self.content_layout.addWidget(self.qwe, 100)

        # Enable dev tools
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

    def _toggle_pali_buttons(self):
        show = self.toggle_pali_btn.isChecked()
        self.palibuttons_frame.setVisible(show)

        self._app_data.app_settings['dictionary_show_pali_buttons'] = show
        self._app_data._save_app_settings()

    def _setup_toggle_pali_button(self):
        icon = QIcon()
        icon.addPixmap(QPixmap(":/keyboard"))

        btn = QPushButton()
        btn.setFixedSize(40, 40)
        btn.setToolTip("Toggle Pali Buttons")
        btn.clicked.connect(partial(self._toggle_pali_buttons))
        btn.setIcon(icon)

        show = self._app_data.app_settings.get('dictionary_show_pali_buttons', False)
        btn.setCheckable(True)
        btn.setChecked(show)

        self.toggle_pali_btn = btn
        self.search_extras.addWidget(self.toggle_pali_btn)

    def _setup_pali_buttons(self):
        palibuttons_layout = QHBoxLayout()
        self.palibuttons_frame.setLayout(palibuttons_layout)

        s = '√'
        btn = QPushButton(s)
        btn.setFixedSize(35, 35)
        btn.clicked.connect(partial(self._append_to_query, s))
        palibuttons_layout.addWidget(btn)

        lowercase = 'ā ī ū ṃ ṁ ṅ ñ ṭ ḍ ṇ ḷ ṛ ṣ ś'.split(' ')

        for i in lowercase:
            btn = QPushButton(i)
            btn.setFixedSize(35, 35)
            btn.clicked.connect(partial(self._append_to_query, i))
            palibuttons_layout.addWidget(btn)

        show = self._app_data.app_settings.get('dictionary_show_pali_buttons', False)
        self.palibuttons_frame.setVisible(show)

    def _search_query_finished(self, query_started_time: datetime):
        logger.info("_search_query_finished()")

        # If it is an old query worker, i.e. not the most recent batch, then
        # ignore the results.
        if query_started_time != self._last_query_time:
            return

        if not self._queries.all_finished():
            return

        self.stop_loading_animation()

        # Restore the search icon, processing finished
        self._show_search_normal_icon()

        hits = self.query_hits()
        if hits is None:
            self.rightside_tabs.setTabText(self.fulltext_results_tab_idx, "Results")
        elif hits > 0:
            self.rightside_tabs.setTabText(self.fulltext_results_tab_idx, f"Results ({hits})")
        else:
            self.rightside_tabs.setTabText(self.fulltext_results_tab_idx, "Results")

        self.render_deconstructor_list_for_query(self.search_input.text().strip())

        results = self.render_fulltext_page()

        if len(results) > 0 and hits == 1 and results[0]['uid'] is not None:
            self._show_word_by_uid(results[0]['uid'])
        else:
            self._render_dict_words_search_results(results[0:10])

        self._update_fulltext_page_btn(hits)

    def _handle_query(self, min_length: int = 4):
        query_text_orig = self.search_input.text().strip()
        logger.info(f"_handle_query(): {query_text_orig}, {min_length}")

        if len(query_text_orig) < min_length:
            return

        idx = self.source_filter_dropdown.currentIndex()
        self._app_data.app_settings['dictionary_source_filter_idx'] = idx
        self._app_data._save_app_settings()

        # Not aborting, show the user that the app started processsing
        self._show_search_stopwatch_icon()

        self._start_query_workers(query_text_orig)

    def _exact_query_finished(self, q_res: ExactQueryResult):
        logger.info("_exact_query_finished()")

        if len(q_res['appdata_uids']) > 0 and q_res['add_recent']:
            word = self._app_data.db_session \
                .query(Am.DictWord) \
                .filter(Am.DictWord.uid == q_res['appdata_uids'][0]) \
                .first()
            if word is not None:
                self._add_recent(word)

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
            get_search_params(self),
        )

    def results_page(self, page_num: int) -> List[SearchResult]:
        return self._queries.results_page(page_num)

    def query_hits(self) -> Optional[int]:
        return self._queries.query_hits()

    def result_pages_count(self) -> Optional[int]:
        return self._queries.result_pages_count()

    def _set_qwe_html(self, html: str):
        self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _add_recent(self, word: UDictWord):
        # de-duplicate: if item already exists, remove it
        if word in self._recent:
            self._recent.remove(word)
        # insert new item on top
        self._recent.insert(0, word)

        # Rebuild Qt recents list
        self.recent_list.clear()

        def _to_title(x: UDictWord):
            return " - ".join([str(x.uid), str(x.word)])

        titles = list(map(lambda x: _to_title(x), self._recent))

        self.recent_list.insertItems(0, titles)

    @QtCore.pyqtSlot(dict)
    def on_searched(self, find_searched: FindSearched):
        if find_searched['flag'] is None:
            self.qwe.findText(find_searched['text'])
        else:
            self.qwe.findText(find_searched['text'], find_searched['flag'])

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
                self._add_recent(word)
                self._show_word(word)

    def _handle_recent_select(self):
        selected_idx = self.recent_list.currentRow()
        word: UDictWord = self._recent[selected_idx]
        self._show_word(word)

    def _select_prev_recent(self):
        selected_idx = self.recent_list.currentRow()
        if selected_idx == -1:
            self.recent_list.setCurrentRow(0)
        elif selected_idx == 0:
            return
        else:
            self.recent_list.setCurrentRow(selected_idx - 1)

    def _select_next_recent(self):
        selected_idx = self.recent_list.currentRow()
        # List is empty or lost focus (no selected item)
        if selected_idx == -1:
            if len(self.recent_list) == 1:
                # Only one viewed item, which is presently being shown, and no
                # next item
                return
            else:
                # The 0 index is already the presently show item
                self.recent_list.setCurrentRow(1)

        elif selected_idx + 1 < len(self.recent_list):
            self.recent_list.setCurrentRow(selected_idx + 1)

    def _select_prev_result(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx == -1:
            self.fulltext_list.setCurrentRow(0)
        elif selected_idx == 0:
            return
        else:
            self.fulltext_list.setCurrentRow(selected_idx - 1)

    def _select_next_result(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx == -1:
            self.fulltext_list.setCurrentRow(0)
        elif selected_idx + 1 < len(self.fulltext_list):
            self.fulltext_list.setCurrentRow(selected_idx + 1)

    def _focus_search_input(self):
        self.search_input.setFocus()

    def _render_words(self, words: List[UDictWord]):
        self._current_words = words
        if len(self._current_words) == 0:
            return

        if len(self._current_words) == 1:
            self.update_memos_list_for_dict_word(self._current_words[0])
            self.show_network_graph(self._current_words[0])

        page_html = self._queries.dictionary_queries.words_to_html_page(words)

        self._set_qwe_html(page_html)

    def _show_word(self, word: UDictWord):
        self.setWindowTitle(f"{self.search_input.text().strip()} ({word.uid}) - Simsapa")
        self._current_words = [word]

        self.update_memos_list_for_dict_word(self._current_words[0])
        self.show_network_graph(self._current_words[0])

        open_details = [DetailsTab.Inflections, DetailsTab.RootInfo]
        word_html = self._queries.dictionary_queries.get_word_html(word, open_details)

        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        css_extra = f"html {{ font-size: {font_size}px; }}"

        body = word_html['body']

        if word.source_uid == "cpd":
            body = add_word_links_to_bold(body)

        page_html = self._queries.dictionary_queries.render_html_page(
            body = body,
            css_head = word_html['css'],
            css_extra = css_extra,
            js_head = word_html['js'])

        self._set_qwe_html(page_html)

    def show_network_graph(self, word: Optional[UDictWord] = None):
        if word is None:
            if len(self._current_words) == 0:
                return
            else:
                word = self._current_words[0]

        self.generate_and_show_graph(None, word, self.queue_id, self.graph_path, self.messages_url)

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

    def _show_selected(self):
        self._show_sutta_from_message(self.selected_info)

    def _show_sutta_from_message(self, info):
        sutta: Optional[USutta] = None

        if 'table' not in info.keys() or 'id' not in info.keys():
            return

        if info['table'] == 'appdata.suttas':
            sutta = self._app_data.db_session \
                .query(Am.Sutta) \
                .filter(Am.Sutta.id == info['id']) \
                .first()

        elif info['table'] == 'userdata.suttas':
            sutta = self._app_data.db_session \
                .query(Um.Sutta) \
                .filter(Um.Sutta.id == info['id']) \
                .first()

        self._app_data.sutta_to_open = sutta
        self.action_Sutta_Search.activate(QAction.ActionEvent.Trigger)

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

    def _show_sutta_by_uid(self, uid: str):
        results: List[USutta] = []

        res = self._app_data.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.uid == uid) \
            .all()
        results.extend(res)

        if len(results) > 0:
            self._app_data.sutta_to_open = results[0]
            self.action_Sutta_Search.activate(QAction.ActionEvent.Trigger)

    def _show_word_by_url(self, url: QUrl, show_results_tab = True, include_exact_query = True):
        # ssp://words/dhammacakkhu
        # path: /dhammacakkhu
        # bword://localhost/American%20pasqueflower
        # path: /American pasqueflower
        query = url.path().strip('/')
        logger.info(f"Show Word: {query}")

        self.lookup_in_dictionary(query,
                                  show_results_tab,
                                  include_exact_query,
                                  add_recent=True)

    def _show_word_by_uid(self, uid: str):
        results = self._queries.dictionary_queries.get_words_by_uid(uid)
        if len(results) > 0:
            self._show_word(results[0])

    def _handle_copy(self):
        text = self._get_selection()
        if text is not None:
            self._app_data.clipboard_setText(text)

    def _handle_paste(self):
        s = self._app_data.clipboard_getText()
        if s is not None:
            self._append_to_query(s)
            self._handle_query()

    def _toggle_dev_tools_inspector(self):
        if self.devToolsAction.isChecked():
            self.dev_view = QWebEngineView()
            self.content_layout.addWidget(self.dev_view, 100)
            self.qwe.page().setDevToolsPage(self.dev_view.page())
        else:
            self.qwe.page().devToolsPage().deleteLater()
            self.dev_view.deleteLater()

    def _handle_open_content_new(self):
        if len(self._current_words) > 0:

            def _f(x: UDictWord):
                return (str(x.metadata.schema), str(x.__tablename__), str(x.uid))

            schemas_tables_uids = list(map(_f, self._current_words))

            self.open_words_new_signal.emit(schemas_tables_uids)
        else:
            logger.warn("No current words")

    def _setup_qwe_context_menu(self):
        self.qwe.setContextMenuPolicy(Qt.ContextMenuPolicy.ActionsContextMenu)

        copyAction = QAction("Copy", self.qwe)
        # NOTE: don't bind Ctrl-C, will be ambiguous to the window menu action
        copyAction.triggered.connect(partial(self._handle_copy))

        self.qwe.addAction(copyAction)

        memoAction = QAction("Create Memo", self.qwe)
        memoAction.triggered.connect(partial(self.handle_create_memo_for_dict_word))

        self.qwe.addAction(memoAction)

        lookupSelectionInSuttas = QAction("Lookup Selection in Suttas", self.qwe)
        lookupSelectionInSuttas.triggered.connect(partial(self._lookup_selection_in_suttas))

        self.qwe.addAction(lookupSelectionInSuttas)

        lookupSelectionInDictionary = QAction("Lookup Selection in Dictionary", self.qwe)
        lookupSelectionInDictionary.triggered.connect(partial(self._lookup_selection_in_dictionary))

        self.qwe.addAction(lookupSelectionInDictionary)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/new-window"))

        open_new_action = QAction("Open in New Window", self.qwe)
        open_new_action.setIcon(icon)
        open_new_action.triggered.connect(partial(self._handle_open_content_new))

        self.qwe.addAction(open_new_action)

        self.devToolsAction = QAction("Show Inspector", self.qwe)
        self.devToolsAction.setCheckable(True)
        self.devToolsAction.triggered.connect(partial(self._toggle_dev_tools_inspector))

        self.qwe.addAction(self.devToolsAction)

    def _handle_show_find_panel(self):
        self.find_toolbar.show()
        self._find_panel.search_input.setFocus()

    def reload_page(self):
        self._render_words(self._current_words)

    def _increase_text_size(self):
        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        self._app_data.app_settings['dictionary_font_size'] = font_size + 2
        self._app_data._save_app_settings()
        self._render_words(self._current_words)

    def _decrease_text_size(self):
        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        if font_size < 5:
            return
        self._app_data.app_settings['dictionary_font_size'] = font_size - 2
        self._app_data._save_app_settings()
        self._render_words(self._current_words)

    def _show_search_result_sizes_dialog(self):
        from simsapa.layouts.search_result_sizes_dialog import SearchResultSizesDialog
        d = SearchResultSizesDialog(self._app_data, self)
        if d.exec():
            self.render_fulltext_page()

    def _toggle_sidebar(self):
        is_on = self.action_Show_Sidebar.isChecked()
        if is_on:
            self.splitter.setSizes([2000, 2000])
        else:
            self.splitter.setSizes([2000, 0])

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.link_mouseover.connect(partial(preview_window.link_mouseover))
        self.link_mouseleave.connect(partial(preview_window.link_mouseleave))
        self.hide_preview.connect(partial(preview_window._do_hide))

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self._handle_close))

        self.recent_list.itemSelectionChanged.connect(partial(self._handle_recent_select))

        self._find_panel.searched.connect(self.on_searched)
        self._find_panel.closed.connect(self.find_toolbar.hide)

        def _handle_sidebar():
            self.action_Show_Sidebar.activate(QAction.ActionEvent.Trigger)

        self.show_sidebar_btn.clicked.connect(partial(_handle_sidebar))

        self.add_memo_button \
            .clicked.connect(partial(self.add_memo_for_dict_word))

        self.action_Copy \
            .triggered.connect(partial(self._handle_copy))

        self.action_Paste \
            .triggered.connect(partial(self._handle_paste))

        self.action_Find_in_Page \
            .triggered.connect(self._handle_show_find_panel)

        self.action_Import_from_StarDict \
            .triggered.connect(partial(self.show_import_from_stardict_dialog))

        self.action_Search_Query_Terms \
            .triggered.connect(partial(show_search_info))

        self.action_Lookup_Clipboard_in_Suttas \
            .triggered.connect(partial(self._lookup_clipboard_in_suttas))

        self.action_Lookup_Clipboard_in_Dictionary \
            .triggered.connect(partial(self._lookup_clipboard_in_dictionary))

        self.action_Previous_Result \
            .triggered.connect(partial(self._select_prev_result))

        self.action_Next_Result \
            .triggered.connect(partial(self._select_next_result))

        self.back_recent_button.clicked.connect(partial(self._select_next_recent))

        self.forward_recent_button.clicked.connect(partial(self._select_prev_recent))

        self.action_Reload_Page \
            .triggered.connect(partial(self.reload_page))

        self.action_Increase_Text_Size \
            .triggered.connect(partial(self._increase_text_size))

        self.action_Decrease_Text_Size \
            .triggered.connect(partial(self._decrease_text_size))

        self.action_Search_Result_Sizes \
            .triggered.connect(partial(self._show_search_result_sizes_dialog))

        self.action_Show_Sidebar \
            .triggered.connect(partial(self._toggle_sidebar))
