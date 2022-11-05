import os
import json
import time
from pathlib import Path
import queue

from functools import partial
from typing import Any, List, Optional, Tuple

from PyQt6.QtCore import QUrl, QTimer
from PyQt6.QtGui import QCloseEvent, QColor, QAction
from PyQt6.QtWidgets import (QLineEdit, QListWidgetItem,
                             QHBoxLayout, QPushButton, QSizePolicy, QMessageBox,
                             QComboBox, QSplitter, QVBoxLayout)
from PyQt6.QtWebEngineWidgets import QWebEngineView

from sqlalchemy import or_

from simsapa import LOADING_HTML, CLICK_GENERATE_HTML, logger, ApiAction, ApiMessage
from simsapa import APP_QUEUES, GRAPHS_DIR, TIMER_SPEED
from simsapa.app.helpers import compactRichText
from simsapa.layouts.links_sidebar import GraphGenerator
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.db.search import SearchResult
from ..app.types import AppData, AppWindowInterface, USutta, UDictWord, UDocument, default_search_result_sizes
from ..assets.ui.links_browser_window_ui import Ui_LinksBrowserWindow
from .search_item import SearchItemWidget

class LinksBrowserWindow(AppWindowInterface, Ui_LinksBrowserWindow):

    splitter: QSplitter
    tabs_layout: QVBoxLayout
    search_input: QLineEdit
    _last_graph_gen_timestamp: float
    selected_info: Any

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("LinksBrowserWindow()")

        self.link_table: QComboBox;

        self.features = []
        self._app_data: AppData = app_data
        self._results: List[SearchResult] = []

        self._current_from: Optional[SearchResult] = None
        self._current_to: Optional[SearchResult] = None

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.selected_info = {}

        self.graph_path: Path = GRAPHS_DIR.joinpath(f"{self.queue_id}.html")
        self._last_graph_gen_timestamp = 0.0

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self._ui_setup()
        self._connect_signals()

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
                msg: ApiMessage = json.loads(s)
                if msg['action'] == ApiAction.show_sutta:
                    info = json.loads(msg['data'])
                    self._show_sutta_from_message(info)

                elif msg['action'] == ApiAction.set_selected:
                    info = json.loads(msg['data'])
                    self.selected_info = info

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _ui_setup(self):
        self._setup_pali_buttons()

        self.setup_links_controls()
        self.setup_content_graph()

        # FIXME graph size is not known when opening the window
        # self.generate_and_show_graph()

        self.search_input.setFocus()

    def _setup_pali_buttons(self):
        lowercase_row = QHBoxLayout()

        lowercase = 'ā ī ū ṃ ṁ ṅ ñ ṭ ḍ ṇ ḷ'.split(' ')

        for i in lowercase:
            btn = QPushButton(i)
            btn.setFixedSize(30, 30)
            btn.clicked.connect(partial(self._append_to_query, i))
            lowercase_row.addWidget(btn)

        self.pali_buttons_layout.addLayout(lowercase_row)

    def setup_links_controls(self):
        self.min_links_input.setMinimum(1)
        self.min_links_input.setValue(3)

        def _show_graph(_: Optional[Any] = None):
            # ignore the argument value, will read params somewhere else
            self.show_network_graph()

        # "Sutta Ref.", "Ref. + Title", "No Labels"
        self.label_select.currentIndexChanged.connect(partial(_show_graph))

        self.min_links_input \
            .valueChanged.connect(partial(_show_graph))

        self.links_regenerate_button \
            .clicked.connect(partial(_show_graph))

        self.open_selected_link_button \
            .clicked.connect(partial(self._show_selected))

    def setup_content_graph(self):
        self.content_graph = QWebEngineView()
        self.content_graph.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.content_graph.setHtml(CLICK_GENERATE_HTML)
        self.content_graph.show()
        self.links_layout.addWidget(self.content_graph)

    def show_network_graph(self):
        self.generate_and_show_graph()

    def _graph_finished(self, result: Tuple[float, int, Path]):
        result_timestamp = result[0]
        graph_path = result[2]

        # Ignore this result if it is not the last which the user has initiated.
        if result_timestamp != self._last_graph_gen_timestamp:
            return

        self.content_graph.load(QUrl(str(graph_path.absolute().as_uri())))

    def generate_and_show_graph(self):
        # Remove worker threads which are in the queue and not yet started.
        self._app_data.graph_gen_pool.clear()

        min_links = self.min_links_input.value()

        n = self.label_select.currentIndex()
        labels = self.label_select.itemText(n)

        width = self.content_graph.frameGeometry().width() - 5
        height = self.content_graph.frameGeometry().height() - 5

        graph_gen_timestamp = time.time()
        self._last_graph_gen_timestamp = graph_gen_timestamp

        if self._app_data.api_url is None:
            return

        graph_gen = GraphGenerator(self._app_data.api_url, graph_gen_timestamp, None, None, self.queue_id, self.graph_path, self.messages_url, labels, 0, min_links, width, height)

        graph_gen.signals.result.connect(self._graph_finished)

        self.content_graph.setHtml(LOADING_HTML)

        self._app_data.graph_gen_pool.start(graph_gen)

    def _append_to_query(self, s: str):
        a = self.search_input.text()
        self.search_input.setText(a + s)
        self.search_input.setFocus()

    def _handle_table_changed(self):
        idx = self.link_table.currentIndex()
        link_table = self.link_table.itemText(idx)
        if link_table == 'Suttas' or link_table == 'DictWords':
            self.set_page_number.setEnabled(False)
            self.page_number.setEnabled(False)
        else:
            self.set_page_number.setEnabled(True)
            self.page_number.setEnabled(True)

        self._handle_query()

    def _handle_query(self, min_length=4):
        query = self.search_input.text()
        if len(query) < min_length:
            return

        idx = self.link_table.currentIndex()
        link_table = self.link_table.itemText(idx)

        if link_table == 'Suttas':
            self._results = self._sutta_search_query(query)
        elif link_table == 'DictWords':
            self._results = self._dict_words_search_query(query)
        else:
            self._results = self._documents_search_query(query)

        self.results_list.clear()

        # FIXME paginate results. Too many results hang the UI while rendering.
        self._results = self._results[0:100]

        def _compact_snippet(x: SearchResult) -> SearchResult:
            x['snippet'] = compactRichText(x['snippet'])
            return x

        self._results = list(map(_compact_snippet, self._results))

        colors = ["#ffffff", "#efefef"]

        sizes = self._app_data.app_settings.get('search_result_sizes', default_search_result_sizes())

        for idx, x in enumerate(self._results):
            w = SearchItemWidget(sizes)
            w.setFromResult(x)

            item = QListWidgetItem(self.results_list)
            item.setSizeHint(w.sizeHint())

            n = idx % len(colors)
            item.setBackground(QColor(colors[n]))

            self.results_list.addItem(item)
            self.results_list.setItemWidget(item, w)

    def _sutta_search_query(self, query: str) -> List[SearchResult]:
        db_results: List[USutta] = []

        r = self._app_data.db_session \
            .query(Am.Sutta) \
            .filter(or_(
                Am.Sutta.content_plain.like(f"%{query}%"),
                Am.Sutta.content_html.like(f"%{query}%"))) \
            .all()
        db_results.extend(r)

        r = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(or_(
                Um.Sutta.content_plain.like(f"%{query}%"),
                Um.Sutta.content_html.like(f"%{query}%"))) \
            .all()
        db_results.extend(r)

        def to_search_result(x: USutta):
            snippet = ''
            if x.content_html:
                snippet = x.content_html[0:400].strip()
            elif x.content_plain:
                snippet = x.content_plain[0:400].strip()

            if x.title is None:
                title = ''
            else:
                title = x.title.strip()

            return SearchResult(
                db_id=int(str(x.id)),
                schema_name=x.metadata.schema,
                table_name=f"{x.metadata.schema}.suttas",
                uid=str(x.uid),
                title=title,
                ref=str(x.sutta_ref),
                author=None,
                snippet=snippet,
                page_number=None,
            )

        return list(map(to_search_result, db_results))

    def _dict_words_search_query(self, query: str) -> List[SearchResult]:
        db_results: List[UDictWord] = []

        r = self._app_data.db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.word.like(f"%{query}%")) \
            .all()
        db_results.extend(r)

        r = self._app_data.db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.word.like(f"%{query}%")) \
            .all()
        db_results.extend(r)

        def to_search_result(x: UDictWord):
            snippet = ''
            if x.definition_html:
                snippet = compactRichText(str(x.definition_html))
                snippet = snippet[0:400]
            elif x.definition_plain:
                snippet = x.definition_plain[0:400].strip()

            return SearchResult(
                db_id=int(str(x.id)),
                schema_name=x.metadata.schema,
                table_name=f"{x.metadata.schema}.dict_words",
                uid=str(x.uid),
                title=str(x.word),
                ref=None,
                author=None,
                snippet=snippet,
                page_number=None,
            )

        return list(map(to_search_result, db_results))

    def _documents_search_query(self, query: str) -> List[SearchResult]:
        db_results: List[UDocument] = []

        r = self._app_data.db_session \
            .query(Am.Document) \
            .filter(or_(
                Am.Document.title.like(f"%{query}%"),
                Am.Document.filepath.like(f"%{query}%"))) \
            .all()
        db_results.extend(r)

        r = self._app_data.db_session \
            .query(Um.Document) \
            .filter(or_(
                Um.Document.title.like(f"%{query}%"),
                Um.Document.filepath.like(f"%{query}%"))) \
            .all()
        db_results.extend(r)

        def to_search_result(x: UDocument):
            title = x.title
            if x.author:
                title += ', ' + x.author

            return SearchResult(
                db_id=int(str(x.id)),
                schema_name=x.metadata.schema,
                table_name=f"{x.metadata.schema}.documents",
                uid=None,
                title=str(title).strip(),
                ref=None,
                author=None,
                snippet='',
                page_number=None,
            )

        return list(map(to_search_result, db_results))

    def _show_selected(self):
        self._show_sutta_from_message(self.selected_info)

    def _show_sutta_from_message(self, info):
        sutta: Optional[USutta] = None

        if not 'table' in info.keys() or not 'id' in info.keys():
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

    def _handle_set_from(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx == -1:
            return
        if selected_idx < len(self._results):
            self._current_from = self._results[selected_idx]
        else:
            return

        if self.set_page_number.isEnabled() and self.set_page_number.isChecked():
            self._current_from['page_number'] = self.page_number.value()

        text = self._current_from['title']
        if self._current_from['page_number'] is not None:
            text += f"\npage {self._current_from['page_number']}"

        self.from_view.setPlainText(text)

    def _handle_set_to(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx == -1:
            return
        if selected_idx < len(self._results):
            self._current_to = self._results[selected_idx]
        else:
            return

        if self.set_page_number.isEnabled() and self.set_page_number.isChecked():
            self._current_to['page_number'] = self.page_number.value()

        text = self._current_to['title']
        if self._current_to['page_number'] is not None:
            text += f"\npage {self._current_to['page_number']}"

        self.to_view.setPlainText(text)

    def _handle_create_link(self):
        if not self._check_from_and_to():
            return

        links = self._find_existing_links()

        if len(links) > 0:
            QMessageBox.information(self,
                                    "Link Exists",
                                    "This link already exists.",
                                    QMessageBox.StandardButton.Ok)
            return

        if self._current_from is None:
            return
        if self._current_to is None:
            return

        link = Um.Link(
            from_table=self._current_from['table_name'],
            from_id=self._current_from['db_id'],
            from_page_number=self._current_from['page_number'],
            to_table=self._current_to['table_name'],
            to_id=self._current_to['db_id'],
            to_page_number=self._current_to['page_number'],
        )

        try:
            self._app_data.db_session.add(link)
            self._app_data.db_session.commit()

            self.show_network_graph()
        except Exception as e:
            logger.error(e)

    def _handle_clear_link(self):
        self._current_from = None
        self._current_to = None
        self.from_view.setPlainText('')
        self.to_view.setPlainText('')

        self.show_network_graph()

    def _check_from_and_to(self) -> bool:
        if not self._current_from or not self._current_to:
            QMessageBox.information(self,
                                    "Missing Information",
                                    "'From' and 'To' cannot be empty.",
                                    QMessageBox.StandardButton.Ok)
            return False

        keys = ['table_name', 'db_id', 'page_number']
        is_same = True
        for k in keys:
            if self._current_from[k] != self._current_to[k]:
                is_same = False

        is_same = False

        if is_same:
            QMessageBox.information(self,
                                    "Self-Reference",
                                    "'From' and 'To' cannot be the same.",
                                    QMessageBox.StandardButton.Ok)
            return False

        return True

    def _find_existing_links(self):
        links = []

        if self._current_from is None:
            return []
        if self._current_to is None:
            return []

        r = self._app_data.db_session \
            .query(Am.Link) \
            .filter(Am.Link.from_table == self._current_from['table_name']) \
            .filter(Am.Link.from_id == self._current_from['db_id']) \
            .filter(Am.Link.from_page_number == self._current_from['page_number']) \
            .filter(Am.Link.to_table == self._current_to['table_name']) \
            .filter(Am.Link.to_id == self._current_to['db_id']) \
            .filter(Am.Link.to_page_number == self._current_to['page_number']) \
            .all()
        links.extend(r)

        r = self._app_data.db_session \
            .query(Am.Link) \
            .filter(Am.Link.to_table == self._current_from['table_name']) \
            .filter(Am.Link.to_id == self._current_from['db_id']) \
            .filter(Am.Link.to_page_number == self._current_from['page_number']) \
            .filter(Am.Link.from_table == self._current_to['table_name']) \
            .filter(Am.Link.from_id == self._current_to['db_id']) \
            .filter(Am.Link.from_page_number == self._current_to['page_number']) \
            .all()
        links.extend(r)

        r = self._app_data.db_session \
            .query(Um.Link) \
            .filter(Um.Link.from_table == self._current_from['table_name']) \
            .filter(Um.Link.from_id == self._current_from['db_id']) \
            .filter(Um.Link.from_page_number == self._current_from['page_number']) \
            .filter(Um.Link.to_table == self._current_to['table_name']) \
            .filter(Um.Link.to_id == self._current_to['db_id']) \
            .filter(Um.Link.to_page_number == self._current_to['page_number']) \
            .all()
        links.extend(r)

        r = self._app_data.db_session \
            .query(Um.Link) \
            .filter(Um.Link.to_table == self._current_from['table_name']) \
            .filter(Um.Link.to_id == self._current_from['db_id']) \
            .filter(Um.Link.to_page_number == self._current_from['page_number']) \
            .filter(Um.Link.from_table == self._current_to['table_name']) \
            .filter(Um.Link.from_id == self._current_to['db_id']) \
            .filter(Um.Link.from_page_number == self._current_to['page_number']) \
            .all()
        links.extend(r)

        return links

    def _handle_remove_link(self):
        if not self._check_from_and_to():
            return

        links = self._find_existing_links()

        if len(links) > 0:
            for x in links:
                try:
                    self._app_data.db_session.delete(x)
                    self._app_data.db_session.commit()

                except Exception as e:
                    logger.error(e)

            self.show_network_graph()
        else:
            QMessageBox.information(self,
                                    "No Link Found",
                                    "No link was found with these properties.",
                                    QMessageBox.StandardButton.Ok)

    def _select_prev_result(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx == -1:
            self.results_list.setCurrentRow(0)
        elif selected_idx == 0:
            return
        else:
            self.results_list.setCurrentRow(selected_idx - 1)

    def _select_next_result(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx == -1:
            self.results_list.setCurrentRow(0)
        elif selected_idx + 1 < len(self.results_list):
            self.results_list.setCurrentRow(selected_idx + 1)

    def _handle_toggle_links_panel(self):
        sizes = self.splitter.sizes()
        if sizes[1] == 0:
            self.splitter.setSizes([100,100])
        else:
            self.splitter.setSizes([100,0])

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textChanged.connect(partial(self._handle_query, min_length=4))
        self.link_table.currentIndexChanged.connect(partial(self._handle_table_changed))
        self.set_from_btn.clicked.connect(partial(self._handle_set_from))
        self.set_to_btn.clicked.connect(partial(self._handle_set_to))
        self.create_link_btn.clicked.connect(partial(self._handle_create_link))
        self.clear_link_btn.clicked.connect(partial(self._handle_clear_link))
        self.remove_link_btn.clicked.connect(partial(self._handle_remove_link))

        self.toggle_links_panel_button.clicked.connect(partial(self._handle_toggle_links_panel))

        self.action_Previous_Result \
            .triggered.connect(partial(self._select_prev_result))

        self.action_Next_Result \
            .triggered.connect(partial(self._select_next_result))

        s = os.getenv('ENABLE_WIP_FEATURES')
        if s is not None and s.lower() == 'true':
            pass
        else:
            # don't show "Documents" option
            self.link_table.removeItem(2)
            # hide setting page number
            self.page_number.setVisible(False)
            self.set_page_number.setVisible(False)
