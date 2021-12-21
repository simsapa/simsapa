from functools import partial
import math
from typing import List, Optional
from markdown import markdown
# from sqlalchemy.orm import joinedload
from pathlib import Path
import json
import queue

from PyQt5.QtCore import Qt, QUrl, QTimer
from PyQt5.QtGui import QKeySequence, QCloseEvent
from PyQt5.QtWidgets import (QLabel, QListWidget, QMainWindow, QAction,
                             QVBoxLayout, QHBoxLayout, QPushButton,
                             QSizePolicy)
from PyQt5.QtWebEngineWidgets import QWebEngineView

from simsapa import ASSETS_DIR, APP_QUEUES
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.db.search import SearchQuery, SearchResult, dict_word_hit_to_search_result
from ..app.types import AppData, USutta, UDictWord
from ..assets.ui.dictionary_search_window_ui import Ui_DictionarySearchWindow
from .memo_dialog import HasMemoDialog
from .memos_sidebar import HasMemosSidebar
from .links_sidebar import HasLinksSidebar
from .results_list import HasResultsList
from .html_content import html_page
from .import_stardict_dialog import HasImportStarDictDialog


class DictionarySearchWindow(QMainWindow, Ui_DictionarySearchWindow,
                             HasMemoDialog, HasLinksSidebar, HasMemosSidebar,
                             HasResultsList, HasImportStarDictDialog):

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self.results_list: QListWidget
        self.recent_list: QListWidget

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._results: List[SearchResult] = []
        self._recent: List[UDictWord] = []

        self._current_word: Optional[UDictWord] = None

        self.page_len = 20
        self.search_query = SearchQuery(
            self._app_data.search_indexed.dict_words_index,
            self.page_len,
            dict_word_hit_to_search_result,
        )

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.graph_path: Path = ASSETS_DIR.joinpath(f"{self.queue_id}.html")

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(300)

        self._ui_setup()
        self._connect_signals()

        self.init_results_list()
        self.init_memo_dialog()
        self.init_memos_sidebar()
        self.init_links_sidebar()
        self.init_stardict_import_dialog()

        self._setup_content_html_context_menu()

        self.statusbar.showMessage("Ready", 3000)

    def closeEvent(self, event: QCloseEvent):
        if self.queue_id in APP_QUEUES.keys():
            del APP_QUEUES[self.queue_id]

        if self.graph_path.exists():
            self.graph_path.unlink()

        event.accept()

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

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _ui_setup(self):
        self.status_msg = QLabel("Word title")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.links_tab_idx = 1
        self.memos_tab_idx = 2

        self._setup_pali_buttons()
        self._setup_content_html()

        self.search_input.setFocus()

    def _setup_content_html(self):
        self.content_html = QWebEngineView()
        self.content_html.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_html.setHtml('')
        self.content_html.show()
        self.content_layout.addWidget(self.content_html)

    def _setup_pali_buttons(self):
        self.pali_buttons_layout = QVBoxLayout()
        self.searchbar_layout.addLayout(self.pali_buttons_layout)

        lowercase = 'ā ī ū ṃ ṁ ṅ ñ ṭ ḍ ṇ ḷ'.split(' ')
        uppercase = "Ā Ī Ū Ṃ Ṁ Ṅ Ñ Ṭ Ḍ Ṇ Ḷ".split(' ')

        lowercase_row = QHBoxLayout()

        for i in lowercase:
            btn = QPushButton(i)
            btn.setFixedSize(30, 30)
            btn.clicked.connect(partial(self._append_to_query, i))
            lowercase_row.addWidget(btn)

        uppercase_row = QHBoxLayout()

        for i in uppercase:
            btn = QPushButton(i)
            btn.setFixedSize(30, 30)
            btn.clicked.connect(partial(self._append_to_query, i))
            uppercase_row.addWidget(btn)

        self.pali_buttons_layout.addLayout(lowercase_row)
        self.pali_buttons_layout.addLayout(uppercase_row)

    def _append_to_query(self, s: str):
        a = self.search_input.text()
        self.search_input.setText(a + s)
        self.search_input.setFocus()

    def _handle_query(self, min_length=4):
        self._highlight_query()
        query = self.search_input.text()

        if len(query) >= min_length:
            self._results = self._word_search_query(query)

            if self.search_query.hits > 0:
                self.rightside_tabs.setTabText(0, f"Results ({self.search_query.hits})")
            else:
                self.rightside_tabs.setTabText(0, "Results")

            self.render_results_page()

    def _set_content_html(self, html):
        self.content_html.setHtml(html)
        self._highlight_query()

    def _highlight_query(self):
        query = self.search_input.text()
        self.content_html.findText(query)

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

    def _dict_word_from_result(self, x: SearchResult) -> Optional[UDictWord]:
        if x['schema_name'] == 'appdata':
            word = self._app_data.db_session \
                                 .query(Am.DictWord) \
                                 .filter(Am.DictWord.id == x['db_id']) \
                                 .first()
        else:
            word = self._app_data.db_session \
                                 .query(Um.DictWord) \
                                 .filter(Um.DictWord.id == x['db_id']) \
                                 .first()
        return word

    def _handle_result_select(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx < len(self._results):
            word = self._dict_word_from_result(self._results[selected_idx])
            if word is not None:
                self._show_word(word)
                self._add_recent(word)

    def _handle_recent_select(self):
        selected_idx = self.recent_list.currentRow()
        word: UDictWord = self._recent[selected_idx]
        self._show_word(word)

    def _show_word(self, word: UDictWord):
        self._current_word = word
        self.status_msg.setText(word.word) # type: ignore

        self.update_memos_list_for_dict_word(word)
        self.show_network_graph(word)

        def example_format(example):
            return "<div>" + example.text_html + "</div><div>" + example.translation_html + "</div>"

        examples = "".join(list(map(example_format, word.examples))) # type: ignore

        if word.definition_html is not None and word.definition_html != '':
            definition = word.definition_html
        elif word.definition_plain is not None and word.definition_plain != '':
            definition = markdown(word.definition_plain) # type: ignore
        else:
            definition = '<p>No definition.</p>'

        messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'
        content = "<div>%s</div><div>%s</div>" % (definition, examples)
        html = html_page(content, messages_url)

        # show the word content
        self._set_content_html(html)

    def show_network_graph(self, word: UDictWord):
        self.generate_graph_for_dict_word(word, self.queue_id, self.graph_path, self.messages_url)
        self.content_graph.load(QUrl('file://' + str(self.graph_path.absolute())))

    def _word_search_query(self, query: str) -> List[SearchResult]:
        results = self.search_query.new_query(query)
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

    def _show_word_by_uid(self, uid: str):
        results: List[UDictWord] = []

        res = self._app_data.db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.uid == uid) \
            .all()
        results.extend(res)

        if len(results) > 0:
            self._show_word(results[0])

    def _handle_copy(self):
        text = self.content_html.selectedText()
        # U+2029 Paragraph Separator to blank line
        text = text.replace('\u2029', "\n\n")
        self._app_data.clipboard_setText(text)

    def _setup_content_html_context_menu(self):
        self.content_html.setContextMenuPolicy(Qt.ActionsContextMenu) # type: ignore

        copyAction = QAction("Copy", self.content_html)
        copyAction.setShortcut(QKeySequence("Ctrl+C"))
        copyAction.triggered.connect(partial(self._handle_copy))

        self.content_html.addAction(copyAction)

        memoAction = QAction("Create Memo", self.content_html)
        memoAction.setShortcut(QKeySequence("Ctrl+M"))
        memoAction.triggered.connect(partial(self.handle_create_memo_for_dict_word))

        self.content_html.addAction(memoAction)

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textChanged.connect(partial(self._handle_query, min_length=4))
        # self.search_input.returnPressed.connect(partial(self._update_result))

        self.recent_list.itemSelectionChanged.connect(partial(self._handle_recent_select))

        self.add_memo_button \
            .clicked.connect(partial(self.add_memo_for_dict_word))

        self.action_Import_from_StarDict \
            .triggered.connect(partial(self.show_import_from_stardict_dialog))
