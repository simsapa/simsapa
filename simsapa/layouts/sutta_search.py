import logging as _logging
import json
from pathlib import Path
import queue

from functools import partial
from typing import List, Optional

from PyQt5.QtCore import Qt, QUrl, QTimer
from PyQt5.QtGui import QKeySequence, QCloseEvent
from PyQt5.QtWidgets import (QLabel, QMainWindow, QAction, QListWidgetItem,
                             QVBoxLayout, QHBoxLayout, QPushButton,
                             QSizePolicy)
from PyQt5.QtWebEngineWidgets import QWebEngineView

from sqlalchemy import or_
from sqlalchemy.sql import func  # type: ignore

from simsapa import ASSETS_DIR, APP_QUEUES
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, USutta  # type: ignore
from ..app.graph import generate_graph, sutta_nodes_and_edges, sutta_graph_id
from ..assets.ui.sutta_search_window_ui import Ui_SuttaSearchWindow  # type: ignore
from .memo_dialog import MemoDialog
from .sutta_search_item import SuttaSearchItemWidget

logger = _logging.getLogger(__name__)


class SuttaSearchWindow(QMainWindow, Ui_SuttaSearchWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data
        self._results: List[USutta] = []
        self._history: List[USutta] = []

        self._current_sutta: Optional[USutta] = None

        self._ui_setup()

        self._connect_signals()
        self._setup_content_html_context_menu()

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()

        self.graph_path: Path = ASSETS_DIR.joinpath(f"{self.queue_id}.html")

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(300)

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

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _ui_setup(self):
        self.status_msg = QLabel("Sutta title")
        self.statusbar.addPermanentWidget(self.status_msg)

        self._setup_pali_buttons()
        self._setup_content_html()
        self._setup_content_graph()

        self.search_input.setFocus()

    def _setup_content_html(self):
        self.content_html = QWebEngineView()
        self.content_html.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_html.setHtml('')
        self.content_html.show()
        self.content_layout.addWidget(self.content_html)

    def _setup_content_graph(self):
        self.content_graph = QWebEngineView()
        self.content_graph.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_graph.setHtml('')
        self.content_graph.show()
        self.results_layout.addWidget(self.content_graph)

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

    def _handle_query(self):
        self._highlight_query()
        query = self.search_input.text()
        if len(query) > 3:
            self._results = self._sutta_search_query(query)

            self.results_list.clear()

            for x in self._results:
                w = SuttaSearchItemWidget()
                w.setTitle(x.title)

                if x.content_html:
                    w.setSnippet(x.content_html[0:400].strip())

                item = QListWidgetItem(self.results_list)
                item.setSizeHint(w.sizeHint())

                self.results_list.addItem(item)
                self.results_list.setItemWidget(item, w)

    def _set_content_html(self, html):
        self.content_html.setHtml(html)
        self._highlight_query()

    def _highlight_query(self):
        query = self.search_input.text()
        self.content_html.findText(query)

    def _handle_result_select(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx < len(self._results):
            sutta: USutta = self._results[selected_idx]
            self._show_sutta(sutta)

            self._history.insert(0, sutta)
            self.history_list.insertItem(0, sutta.title)

    def _handle_history_select(self):
        selected_idx = self.history_list.currentRow()
        sutta: USutta = self._history[selected_idx]
        self._show_sutta(sutta)

    def _generate_network_bokeh(self, sutta: USutta):
        (nodes, edges) = sutta_nodes_and_edges(app_data=self._app_data, sutta=sutta, distance=3)

        selected = []

        for idx, n in enumerate(nodes):
            if n[0] == sutta_graph_id(sutta):
                selected.append(idx)

        generate_graph(nodes, edges, selected, self.queue_id, self.graph_path)

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

    def _show_sutta(self, sutta: USutta):
        self._current_sutta = sutta
        self.status_msg.setText(sutta.title)

        if sutta.content_html is not None and sutta.content_html != '':
            content = sutta.content_html
        elif sutta.content_plain is not None and sutta.content_plain != '':
            content = '<pre>' + sutta.content_plain + '</pre>'
        else:
            content = 'No content.'

        content_html = """
<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <style>
        pre { white-space: pre-wrap; }
    </style>
    <style>%s</style>
</head>
<body>
%s
</body>
</html>
""" % ('', content)

        # show the network graph in a browser
        self._generate_network_bokeh(sutta)
        self.content_graph.load(QUrl('file://' + str(self.graph_path.absolute())))

        # show the sutta content
        self._set_content_html(content_html)

    def _sutta_search_query(self, query: str) -> List[USutta]:
        results: List[USutta] = []

        res = self._app_data.db_session \
                            .query(Am.Sutta) \
                            .filter(or_(
                                Am.Sutta.content_plain.like(f"%{query}%"),
                                Am.Sutta.content_html.like(f"%{query}%"))) \
                            .all()
        results.extend(res)

        res = self._app_data.db_session \
                            .query(Um.Sutta) \
                            .filter(or_(
                                Um.Sutta.content_plain.like(f"%{query}%"),
                                Um.Sutta.content_html.like(f"%{query}%"))) \
                            .all()
        results.extend(res)

        return results

    def _handle_copy(self):
        text = self.content_html.selectedText()
        # U+2029 Paragraph Separator to blank line
        text = text.replace('\u2029', "\n\n")
        self._app_data.clipboard_setText(text)

    def _set_memo_fields(self, values):
        self.memo_fields = values

    def _handle_create_memo(self):
        text = self.content_html.selectedText()

        deck = self._app_data.db_session.query(Um.Deck).first()

        self.memo_fields = {}

        d = MemoDialog(text)
        d.accepted.connect(self._set_memo_fields)
        d.exec_()

        memo = Um.Memo(
            deck_id=deck.id,
            fields_json=json.dumps(self.memo_fields),
            created_at=func.now(),
        )

        try:
            self._app_data.db_session.add(memo)
            self._app_data.db_session.commit()

            if self._current_sutta is not None:

                memo_assoc = Um.MemoAssociation(
                    memo_id=memo.id,
                    associated_table='userdata.suttas',
                    associated_id=self._current_sutta.id,
                )

                self._app_data.db_session.add(memo_assoc)
                self._app_data.db_session.commit()

        except Exception as e:
            logger.error(e)

    def _setup_content_html_context_menu(self):
        self.content_html.setContextMenuPolicy(Qt.ActionsContextMenu)

        copyAction = QAction("Copy", self.content_html)
        copyAction.setShortcut(QKeySequence("Ctrl+C"))
        copyAction.triggered.connect(partial(self._handle_copy))

        memoAction = QAction("Create Memo", self.content_html)
        memoAction.setShortcut(QKeySequence("Ctrl+M"))
        memoAction.triggered.connect(partial(self._handle_create_memo))

        self.content_html.addActions([
            copyAction,
            memoAction,
        ])

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query))
        self.search_input.textChanged.connect(partial(self._handle_query))
        # self.search_input.returnPressed.connect(partial(self._update_result))
        self.results_list.itemSelectionChanged.connect(partial(self._handle_result_select))
        self.history_list.itemSelectionChanged.connect(partial(self._handle_history_select))
