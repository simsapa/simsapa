from functools import partial
import math
from typing import List, Optional
from pathlib import Path
import json
import queue
import re

from PyQt5 import QtCore
from PyQt5.QtCore import Qt, QUrl, QTimer
from PyQt5.QtGui import QIcon, QKeySequence, QCloseEvent, QPixmap, QStandardItem, QStandardItemModel
from PyQt5.QtWidgets import (QCompleter, QFrame, QLabel, QLineEdit, QListWidget, QMainWindow, QAction,
                             QHBoxLayout, QPushButton, QSizePolicy, QToolBar, QVBoxLayout)
from PyQt5.QtWebEngineWidgets import QWebEnginePage, QWebEngineSettings, QWebEngineView

from simsapa import IS_WINDOWS, SIMSAPA_PACKAGE_DIR, logger
from simsapa import APP_QUEUES, GRAPHS_DIR, TIMER_SPEED
from simsapa.layouts.dictionary_queries import DictionaryQueries
from simsapa.layouts.find_panel import FindPanel
from simsapa.layouts.reader_web import ReaderWebEnginePage
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.db.search import SearchIndexed, SearchQuery, SearchResult, dict_word_hit_to_search_result
from ..app.types import AppData, USutta, UDictWord
from ..assets.ui.dictionary_search_window_ui import Ui_DictionarySearchWindow
from .memo_dialog import HasMemoDialog
from .memos_sidebar import HasMemosSidebar
from .links_sidebar import HasLinksSidebar
from .results_list import HasResultsList
from .import_stardict_dialog import HasImportStarDictDialog
from .help_info import show_search_info, setup_info_button
from .dictionary_select_dialog import DictionarySelectDialog

class DictionarySearchWindow(QMainWindow, Ui_DictionarySearchWindow, HasMemoDialog,
                             HasLinksSidebar, HasMemosSidebar,
                             HasResultsList, HasImportStarDictDialog):

    searchbar_layout: QHBoxLayout
    search_extras: QHBoxLayout
    palibuttons_frame: QFrame
    search_input: QLineEdit
    toggle_pali_btn: QPushButton
    content_layout: QVBoxLayout
    content_html: QWebEngineView
    _app_data: AppData
    _results: List[SearchResult]
    _autocomplete_model: QStandardItemModel
    _current_words: List[UDictWord]

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("DictionarySearchWindow()")

        self.results_list: QListWidget
        self.recent_list: QListWidget

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._results: List[SearchResult] = []
        self._recent: List[UDictWord] = []
        self._current_words: List[UDictWord] = []

        self.page_len = 20
        self.search_query = SearchQuery(
            self._app_data.search_indexed.dict_words_index,
            self.page_len,
            dict_word_hit_to_search_result,
        )

        self.queries = DictionaryQueries(self._app_data)
        self._autocomplete_model = QStandardItemModel()

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.graph_path: Path = GRAPHS_DIR.joinpath(f"{self.queue_id}.html")

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self._ui_setup()
        self._connect_signals()

        self.init_results_list()
        self.init_memo_dialog()
        self.init_memos_sidebar()
        self.init_links_sidebar()
        self.init_stardict_import_dialog()

        self._setup_content_html_context_menu()

        self.statusbar.showMessage("Ready", 3000)

    def _lookup_clipboard_in_suttas(self):
        text = self._app_data.clipboard_getText()
        if text is not None and self._app_data.actions_manager is not None:
            self._app_data.actions_manager.lookup_in_suttas(text)

    def _lookup_clipboard_in_dictionary(self):
        self.activateWindow()
        s = self._app_data.clipboard_getText()
        if s is not None:
            self._set_query(s)
            self._handle_query()

    def _lookup_selection_in_suttas(self):
        text = self._get_selection()
        if text is not None and self._app_data.actions_manager is not None:
            self._app_data.actions_manager.lookup_in_suttas(text)

    def _lookup_selection_in_dictionary(self):
        self.activateWindow()
        text = self._get_selection()
        if text is not None:
            self._set_query(text)
            self._handle_query()

    def _get_selection(self) -> Optional[str]:
        text = self.content_html.selectedText()
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

        if self.graph_path.exists():
            self.graph_path.unlink()

        event.accept()

    def reinit_index(self):
        self._app_data.search_indexed = SearchIndexed()
        self.search_query = SearchQuery(
            self._app_data.search_indexed.dict_words_index,
            self.page_len,
            dict_word_hit_to_search_result,
        )

    def handle_messages(self):
        if self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                data = json.loads(s)
                if data['action'] == 'show_sutta':
                    self._show_sutta_from_message(data['arg'])

                elif data['action'] == 'show_sutta_by_uid':
                    info = data['arg']
                    if 'uid' in info.keys():
                        self._show_sutta_by_uid(info['uid'])

                elif data['action'] == 'show_word_by_uid':
                    info = data['arg']
                    if 'uid' in info.keys():
                        self._show_word_by_uid(info['uid'])

                elif data['action'] == 'lookup_clipboard_in_dictionary':
                    self._lookup_clipboard_in_dictionary()

                elif data['action'] == 'lookup_in_dictionary':
                    text = data['query']
                    self._set_query(text)
                    self._handle_query()

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _ui_setup(self):
        self.status_msg = QLabel("Ready")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.links_tab_idx = 1
        self.memos_tab_idx = 2

        style = """
QWidget { border: 1px solid #272727; }
QWidget:focus { border: 1px solid #1092C3; }
        """

        self.search_input.setStyleSheet(style)

        completer = QCompleter(self._autocomplete_model, self)
        completer.setMaxVisibleItems(20)
        completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        completer.setModelSorting(QCompleter.ModelSorting.CaseInsensitivelySortedModel)

        self.search_input.setCompleter(completer)

        self._setup_dict_select_button()
        self._setup_toggle_pali_button()
        setup_info_button(self.search_extras, self)

        self._setup_pali_buttons()
        self._setup_content_html()

        self.search_input.setFocus()

        self._find_panel = FindPanel()

        self.find_toolbar = QToolBar()
        self.find_toolbar.addWidget(self._find_panel)

        self.addToolBar(QtCore.Qt.ToolBarArea.BottomToolBarArea, self.find_toolbar)
        self.find_toolbar.hide()

    def _setup_content_html(self):
        self.content_html = QWebEngineView()
        self.content_html.setPage(ReaderWebEnginePage(self))

        self.content_html.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_html.setHtml(self.queries.content_html_page(body=''))
        self.content_html.show()
        self.content_layout.addWidget(self.content_html, 100)

        # Enable dev tools
        self.content_html.settings().setAttribute(QWebEngineSettings.JavascriptEnabled, True)
        self.content_html.settings().setAttribute(QWebEngineSettings.LocalContentCanAccessRemoteUrls, True)
        self.content_html.settings().setAttribute(QWebEngineSettings.ErrorPageEnabled, True)
        self.content_html.settings().setAttribute(QWebEngineSettings.PluginsEnabled, True)

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

    def _setup_dict_select_button(self):
        icon = QIcon()
        icon.addPixmap(QPixmap(":/dictionary"))

        btn = QPushButton()
        btn.setFixedSize(40, 40)
        btn.setToolTip("Select Dictionaries")
        btn.clicked.connect(partial(self._show_dict_select_dialog))
        btn.setIcon(icon)

        self.dict_select_btn = btn
        self.search_extras.addWidget(self.dict_select_btn)

    def _show_dict_select_dialog(self):
        d = DictionarySelectDialog(self._app_data, self)

        if d.exec():
            self._handle_query()

    def _set_query(self, s: str):
        self.search_input.setText(s)

    def _append_to_query(self, s: str):
        a = self.search_input.text()
        n = self.search_input.cursorPosition()
        pre = a[:n]
        post = a[n:]
        self.search_input.setText(pre + s + post)
        self.search_input.setCursorPosition(n + len(s))
        self.search_input.setFocus()

    def _handle_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        self._results = self._word_search_query(query)

        if self.search_query.hits > 0:
            self.rightside_tabs.setTabText(0, f"Fulltext ({self.search_query.hits})")
        else:
            self.rightside_tabs.setTabText(0, "Fulltext")

        self.render_results_page()

        if self.search_query.hits == 1 and self._results[0]['uid'] is not None:
            self._show_word_by_uid(self._results[0]['uid'])

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

    def _set_content_html(self, html: str):
        self.content_html.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _add_recent(self, word: UDictWord):
        # de-duplicate: if item already exists, remove it
        if word in self._recent:
            self._recent.remove(word)
        # insert new item on top
        self._recent.insert(0, word)

        # Rebuild Qt recents list
        self.recent_list.clear()
        words = list(map(lambda x: x.word, self._recent))
        self.recent_list.insertItems(0, words) # type: ignore

    @QtCore.pyqtSlot(str, QWebEnginePage.FindFlag)
    def on_searched(self, text: str, flag: QWebEnginePage.FindFlag):
        def callback(found):
            if text and not found:
                self.statusBar().showMessage('Not found')
            else:
                self.statusBar().showMessage('')
        self.content_html.findText(text, flag, callback)

    def _handle_result_select(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx < len(self._results):
            word = self.queries.dict_word_from_result(self._results[selected_idx])
            if word is not None:
                self._show_word(word)
                self._add_recent(word)

    def _handle_recent_select(self):
        selected_idx = self.recent_list.currentRow()
        word: UDictWord = self._recent[selected_idx]
        self._show_word(word)

    def _render_words(self, words: List[UDictWord]):
        self._current_words = words
        if len(self._current_words) == 0:
            return

        self.status_msg.setText(self._current_words[0].word) # type: ignore

        self.update_memos_list_for_dict_word(self._current_words[0])
        self.show_network_graph(self._current_words[0])

        page_html = self.queries.words_to_html_page(words)

        self._set_content_html(page_html)

    def _show_word(self, word: UDictWord):
        self._current_words = [word]
        self.status_msg.setText(self._current_words[0].word) # type: ignore

        self.update_memos_list_for_dict_word(self._current_words[0])
        self.show_network_graph(self._current_words[0])

        word_html = self.queries.get_word_html(word)

        page_html = self.queries.content_html_page(
            body = word_html['body'],
            css_head = word_html['css'],
            js_head = word_html['js'])

        self._set_content_html(page_html)

    def show_network_graph(self, word: UDictWord):
        self.generate_graph_for_dict_word(word, self.queue_id, self.graph_path, self.messages_url)
        self.content_graph.load(QUrl(str(self.graph_path.absolute().as_uri())))

    def _word_search_query(self, query: str) -> List[SearchResult]:
        results = self.search_query.new_query(query, self._app_data.app_settings['disabled_dict_labels'])
        hits = self.search_query.hits

        if hits == 0:
            self.results_page_input.setMinimum(0)
            self.results_page_input.setMaximum(0)
            self.results_first_page_btn.setEnabled(False)
            self.results_last_page_btn.setEnabled(False)

        elif hits <= self.page_len:
            self.results_page_input.setMinimum(1)
            self.results_page_input.setMaximum(1)
            self.results_first_page_btn.setEnabled(False)
            self.results_last_page_btn.setEnabled(False)

        else:
            pages = math.floor(hits / self.page_len) + 1
            self.results_page_input.setMinimum(1)
            self.results_page_input.setMaximum(pages)
            self.results_first_page_btn.setEnabled(True)
            self.results_last_page_btn.setEnabled(True)

        return results

    def _show_sutta_from_message(self, info):
        sutta: Optional[USutta] = None

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
        self.action_Sutta_Search.activate(QAction.Trigger) # type: ignore

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
            self.action_Sutta_Search.activate(QAction.Trigger) # type: ignore

    def _show_word_by_bword_url(self, url: QUrl):
        # FIXME encoding is wrong
        # araghaṭṭa
        # Show Word: xn--araghaa-jb4ca
        s = url.toString()
        query = s.replace("bword://", "")
        logger.info(f"Show Word: {query}")
        self._set_query(query)
        self._handle_query()
        self._handle_exact_query()

    def _show_word_by_uid(self, uid: str):
        results = self.queries.get_words_by_uid(uid)
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
            self.content_html.page().setDevToolsPage(self.dev_view.page())
        else:
            self.content_html.page().devToolsPage().deleteLater()
            self.dev_view.deleteLater()

    def _handle_open_content_new(self):
        if self._app_data.actions_manager is not None \
           and len(self._current_words) > 0:

            def _f(x: UDictWord):
                return (str(x.metadata.schema), int(x.id)) # type: ignore

            schemas_ids = list(map(_f, self._current_words))

            self._app_data.actions_manager.open_words_new(schemas_ids)
        else:
            logger.warn("Sutta is not set")

    def _setup_content_html_context_menu(self):
        self.content_html.setContextMenuPolicy(Qt.ActionsContextMenu) # type: ignore

        copyAction = QAction("Copy", self.content_html)
        # NOTE: don't bind Ctrl-C, will be ambiguous to the window menu action
        copyAction.triggered.connect(partial(self._handle_copy))

        self.content_html.addAction(copyAction)

        memoAction = QAction("Create Memo", self.content_html)
        memoAction.setShortcut(QKeySequence("Ctrl+M"))
        memoAction.triggered.connect(partial(self.handle_create_memo_for_dict_word))

        self.content_html.addAction(memoAction)

        lookupSelectionInSuttas = QAction("Lookup Selection in Suttas", self.content_html)
        lookupSelectionInSuttas.triggered.connect(partial(self._lookup_selection_in_suttas))

        self.content_html.addAction(lookupSelectionInSuttas)

        lookupSelectionInDictionary = QAction("Lookup Selection in Dictionary", self.content_html)
        lookupSelectionInDictionary.triggered.connect(partial(self._lookup_selection_in_dictionary))

        self.content_html.addAction(lookupSelectionInDictionary)

        if not IS_WINDOWS:
            # FIXME app not responding
            icon = QIcon()
            icon.addPixmap(QPixmap(":/new-window"))

            open_new_action = QAction("Open in New Window", self.content_html)
            open_new_action.setIcon(icon)
            open_new_action.triggered.connect(partial(self._handle_open_content_new))

            self.content_html.addAction(open_new_action)

        self.devToolsAction = QAction("Show Inspector", self.content_html)
        self.devToolsAction.setCheckable(True)
        self.devToolsAction.triggered.connect(partial(self._toggle_dev_tools_inspector))

        self.content_html.addAction(self.devToolsAction)

    def _handle_show_find_panel(self):
        self.find_toolbar.show()
        self._find_panel.search_input.setFocus()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textEdited.connect(partial(self._handle_query, min_length=4))
        self.search_input.completer().activated.connect(partial(self._handle_query, min_length=1))

        self.search_input.textEdited.connect(partial(self._handle_autocomplete_query, min_length=4))

        self.search_button.clicked.connect(partial(self._handle_exact_query, min_length=1))
        self.search_input.returnPressed.connect(partial(self._handle_exact_query, min_length=1))
        self.search_input.completer().activated.connect(partial(self._handle_exact_query, min_length=1))

        self.recent_list.itemSelectionChanged.connect(partial(self._handle_recent_select))

        self._find_panel.searched.connect(self.on_searched) # type: ignore
        self._find_panel.closed.connect(self.find_toolbar.hide)

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
            .triggered.connect(partial(show_search_info, self))

        self.action_Select_Dictionaries \
            .triggered.connect(partial(self._show_dict_select_dialog))

        self.action_Lookup_Clipboard_in_Suttas \
            .triggered.connect(partial(self._lookup_clipboard_in_suttas))

        self.action_Lookup_Clipboard_in_Dictionary \
            .triggered.connect(partial(self._lookup_clipboard_in_dictionary))
