import json
import math
from pathlib import Path
import queue

from functools import partial
from typing import List, Optional
from PyQt5.QtCore import Qt, QUrl, QTimer
from PyQt5.QtGui import QIcon, QKeySequence, QCloseEvent, QPixmap, QStandardItem, QStandardItemModel
from PyQt5.QtWidgets import (QCompleter, QFrame, QLabel, QLineEdit, QMainWindow, QAction,
                             QHBoxLayout, QPushButton,
                             QSizePolicy, QListWidget)
from PyQt5.QtWebEngineWidgets import QWebEngineView

from simsapa import logger
from simsapa import APP_QUEUES, GRAPHS_DIR, TIMER_SPEED
from ..app.db.search import SearchResult, SearchQuery, sutta_hit_to_search_result
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, USutta, UDictWord
from ..assets.ui.sutta_search_window_ui import Ui_SuttaSearchWindow
from .memo_dialog import HasMemoDialog
from .memos_sidebar import HasMemosSidebar
from .links_sidebar import HasLinksSidebar
from .results_list import HasResultsList
from .html_content import html_page
from .help_info import show_search_info, setup_info_button
from .sutta_select_dialog import SuttaSelectDialog


class SuttaSearchWindow(QMainWindow, Ui_SuttaSearchWindow, HasMemoDialog,
                        HasLinksSidebar, HasMemosSidebar, HasResultsList):

    searchbar_layout: QHBoxLayout
    search_extras: QHBoxLayout
    palibuttons_frame: QFrame
    search_input: QLineEdit
    toggle_pali_btn: QPushButton

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("SuttaSearchWindow()")

        self.results_list: QListWidget
        self.recent_list: QListWidget

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._results: List[SearchResult] = []
        self._recent: List[USutta] = []

        self._current_sutta: Optional[USutta] = None

        self.page_len = 20
        self.search_query = SearchQuery(
            self._app_data.search_indexed.suttas_index,
            self.page_len,
            sutta_hit_to_search_result,
        )

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

        self._setup_content_html_context_menu()

        self.statusbar.showMessage("Ready", 3000)

    def _lookup_clipboard_in_suttas(self):
        self.activateWindow()
        s = self._app_data.clipboard_getText()
        if s is not None:
            self._set_query(s)
            self._handle_query()

    def _lookup_clipboard_in_dictionary(self):
        text = self._app_data.clipboard_getText()
        if text is not None and self._app_data.actions_manager is not None:
            self._app_data.actions_manager.lookup_in_dictionary(text)

    def _lookup_selection_in_suttas(self):
        self.activateWindow()
        text = self._get_selection()
        if text is not None:
            self._set_query(text)
            self._handle_query()

    def _lookup_selection_in_dictionary(self):
        text = self._get_selection()
        if text is not None and self._app_data.actions_manager is not None:
            self._app_data.actions_manager.lookup_in_dictionary(text)

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

                elif data['action'] == 'lookup_clipboard_in_suttas':
                    self._lookup_clipboard_in_suttas()

                elif data['action'] == 'lookup_in_suttas':
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
QWidget:focus { border: 1px solid blue; }
        """

        self.search_input.setStyleSheet(style)

        completer = QCompleter(self._autocomplete_model, self)
        completer.setMaxVisibleItems(20)
        completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        completer.setModelSorting(QCompleter.ModelSorting.CaseInsensitivelySortedModel)

        self.search_input.setCompleter(completer)

        self._setup_sutta_select_button()
        self._setup_toggle_pali_button()
        setup_info_button(self.search_extras, self)
        self._setup_pali_buttons()
        self._setup_content_html()

        self.search_input.setFocus()

    def _setup_content_html(self):
        self.content_html = QWebEngineView()
        self.content_html.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_html.setHtml('')
        self.content_html.show()
        self.content_layout.addWidget(self.content_html)

    def _toggle_pali_buttons(self):
        show = self.toggle_pali_btn.isChecked()
        self.palibuttons_frame.setVisible(show)

        self._app_data.app_settings['suttas_show_pali_buttons'] = show
        self._app_data._save_app_settings()

    def _setup_toggle_pali_button(self):
        icon = QIcon()
        icon.addPixmap(QPixmap(":/keyboard"))

        btn = QPushButton()
        btn.setFixedSize(40, 40)
        btn.setToolTip("Toggle Pali Buttons")
        btn.clicked.connect(partial(self._toggle_pali_buttons))
        btn.setIcon(icon)

        show = self._app_data.app_settings.get('suttas_show_pali_buttons', False)
        btn.setCheckable(True)
        btn.setChecked(show)

        self.toggle_pali_btn = btn
        self.search_extras.addWidget(self.toggle_pali_btn)

    def _setup_pali_buttons(self):
        palibuttons_layout = QHBoxLayout()
        self.palibuttons_frame.setLayout(palibuttons_layout)

        lowercase = 'ā ī ū ṃ ṁ ṅ ñ ṭ ḍ ṇ ḷ ṛ ṣ ś'.split(' ')

        for i in lowercase:
            btn = QPushButton(i)
            btn.setFixedSize(35, 35)
            btn.clicked.connect(partial(self._append_to_query, i))
            palibuttons_layout.addWidget(btn)

        show = self._app_data.app_settings.get('suttas_show_pali_buttons', False)
        self.palibuttons_frame.setVisible(show)

    def _setup_sutta_select_button(self):
        icon = QIcon()
        icon.addPixmap(QPixmap(":/book"))

        btn = QPushButton()
        btn.setFixedSize(40, 40)
        btn.setToolTip("Select Sutta Sources")
        btn.clicked.connect(partial(self._show_sutta_select_dialog))
        btn.setIcon(icon)

        self.sutta_select_btn = btn
        self.search_extras.addWidget(self.sutta_select_btn)

    def _show_sutta_select_dialog(self):
        d = SuttaSelectDialog(self._app_data, self)

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
        self._highlight_query()
        query = self.search_input.text()

        if len(query) >= min_length:
            self._autocomplete_search(query)
            self._results = self._sutta_search_query(query)

            if self.search_query.hits > 0:
                self.rightside_tabs.setTabText(0, f"Results ({self.search_query.hits})")
            else:
                self.rightside_tabs.setTabText(0, "Results")

            self.render_results_page()

    def _autocomplete_search(self, text: str):
        self._autocomplete_model.clear()

        if text:
            res: List[USutta] = []
            r = self._app_data.db_session \
                              .query(Am.Sutta.title) \
                              .filter(Am.Sutta.title.like(f"%{text}%")) \
                              .all()
            res.extend(r)

            r = self._app_data.db_session \
                              .query(Um.Sutta.title) \
                              .filter(Um.Sutta.title.like(f"%{text}%")) \
                              .all()
            res.extend(r)

            a = set(map(lambda x: x[0], res))

            for i in set(a):
                self._autocomplete_model.appendRow(QStandardItem(i))

            self._autocomplete_model.sort(0)

    def _set_content_html(self, html):
        self.content_html.setHtml(html)
        self._highlight_query()

    def _highlight_query(self):
        query = self.search_input.text()
        self.content_html.findText(query)

    def _add_recent(self, sutta: USutta):
        # de-duplicate: if item already exists, remove it
        if sutta in self._recent:
            self._recent.remove(sutta)
        # insert new item on top
        self._recent.insert(0, sutta)

        # Rebuild Qt recents list
        self.recent_list.clear()
        titles = list(map(lambda x: x.title, self._recent))
        self.recent_list.insertItems(0, titles) # type: ignore

    def _sutta_from_result(self, x: SearchResult) -> Optional[USutta]:
        if x['schema_name'] == 'appdata':
            sutta = self._app_data.db_session \
                                  .query(Am.Sutta) \
                                  .filter(Am.Sutta.id == x['db_id']) \
                                  .first()
        else:
            sutta = self._app_data.db_session \
                                  .query(Um.Sutta) \
                                  .filter(Um.Sutta.id == x['db_id']) \
                                  .first()
        return sutta

    def _handle_result_select(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx < len(self._results):
            sutta = self._sutta_from_result(self._results[selected_idx])
            if sutta is not None:
                self._show_sutta(sutta)
                self._add_recent(sutta)

    def _handle_recent_select(self):
        selected_idx = self.recent_list.currentRow()
        sutta: USutta = self._recent[selected_idx]
        self._show_sutta(sutta)

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

        if sutta:
            self._show_sutta(sutta)

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
            self._show_sutta(results[0])

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
            self._app_data.dict_word_to_open = results[0]
            self.action_Dictionary_Search.activate(QAction.ActionEvent.Trigger)

    def _show_sutta(self, sutta: USutta):
        self._current_sutta = sutta
        self.status_msg.setText(sutta.title) # type: ignore

        self.update_memos_list_for_sutta(sutta)
        self.show_network_graph(sutta)

        if sutta.content_html is not None and sutta.content_html != '':
            # Hide SuttaCentral ref link text
            style = '<style>a.ref { display: none; }</style>'
            content = style + sutta.content_html
        elif sutta.content_plain is not None and sutta.content_plain != '':
            style = '<style>pre { font-family: serif; }</style>'
            content = style + '<pre>' + sutta.content_plain + '</pre>'
        else:
            content = 'No content.'

        html = html_page(content, self.messages_url) # type: ignore

        # show the sutta content
        self._set_content_html(html)

    def show_network_graph(self, sutta: USutta):
        self.generate_graph_for_sutta(sutta, self.queue_id, self.graph_path, self.messages_url)
        self.content_graph.load(QUrl(str(self.graph_path.absolute().as_uri())))

    def _sutta_search_query(self, query: str) -> List[SearchResult]:
        results = self.search_query.new_query(query, self._app_data.app_settings['disabled_sutta_labels'])
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

    def _handle_copy(self):
        text = self._get_selection()
        if text is not None:
            self._app_data.clipboard_setText(text)

    def _handle_paste(self):
        s = self._app_data.clipboard_getText()
        if s is not None:
            self._append_to_query(s)
            self._handle_query()

    def _setup_content_html_context_menu(self):
        self.content_html.setContextMenuPolicy(Qt.ContextMenuPolicy.ActionsContextMenu)

        copyAction = QAction("Copy", self.content_html)
        # NOTE: don't bind Ctrl-C, will be ambiguous to the window menu action
        copyAction.triggered.connect(partial(self._handle_copy))

        self.content_html.addAction(copyAction)

        memoAction = QAction("Create Memo", self.content_html)
        memoAction.setShortcut(QKeySequence("Ctrl+M"))
        memoAction.triggered.connect(partial(self.handle_create_memo_for_sutta))

        self.content_html.addAction(memoAction)

        lookupSelectionInSuttas = QAction("Lookup Selection in Suttas", self.content_html)
        lookupSelectionInSuttas.triggered.connect(partial(self._lookup_selection_in_suttas))

        self.content_html.addAction(lookupSelectionInSuttas)

        lookupSelectionInDictionary = QAction("Lookup Selection in Dictionary", self.content_html)
        lookupSelectionInDictionary.triggered.connect(partial(self._lookup_selection_in_dictionary))

        self.content_html.addAction(lookupSelectionInDictionary)

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textChanged.connect(partial(self._handle_query, min_length=4))
        self.search_input.returnPressed.connect(partial(self._handle_query, min_length=1))

        self.recent_list.itemSelectionChanged.connect(partial(self._handle_recent_select))

        self.add_memo_button \
            .clicked.connect(partial(self.add_memo_for_sutta))

        self.action_Copy \
            .triggered.connect(partial(self._handle_copy))

        self.action_Paste \
            .triggered.connect(partial(self._handle_paste))

        self.action_Search_Query_Terms \
            .triggered.connect(partial(show_search_info, self))

        self.action_Select_Sutta_Authors \
            .triggered.connect(partial(self._show_sutta_select_dialog))

        self.action_Lookup_Clipboard_in_Suttas \
            .triggered.connect(partial(self._lookup_clipboard_in_suttas))

        self.action_Lookup_Clipboard_in_Dictionary \
            .triggered.connect(partial(self._lookup_clipboard_in_dictionary))
