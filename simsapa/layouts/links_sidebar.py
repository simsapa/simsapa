import time
from functools import partial
from pathlib import Path
from typing import Any, Callable, Optional, Tuple

from PyQt5.QtWidgets import QComboBox, QPushButton, QSizePolicy, QSpinBox, QTabWidget, QVBoxLayout
from PyQt5.QtWebEngineWidgets import QWebEngineView
from PyQt5.QtCore import QObject, QRunnable, QUrl, pyqtSignal, pyqtSlot

from ..app.graph import (generate_graph, sutta_nodes_and_edges,
                         dict_word_nodes_and_edges,
                         document_page_nodes_and_edges, sutta_graph_id)

from ..app.file_doc import FileDoc
from ..app.db import userdata_models as Um

from ..app.types import AppData, USutta, UDictWord

from simsapa import LOADING_HTML, ShowLabels
from simsapa.app.helpers import get_db_engine_connection_session


class GraphGenSignals(QObject):
    finished = pyqtSignal()
    # Result is a Tuple[float, int, Path]:
    # - timestamp
    # - number of hits (links)
    # - path of the graph html
    result = pyqtSignal(tuple)


class GraphGenerator(QRunnable):
    def __init__(self,
                 graph_gen_timestamp: float,
                 sutta: Optional[USutta],
                 dict_word: Optional[UDictWord],
                 queue_id: str,
                 graph_path: Path,
                 messages_url: str,
                 labels: str,
                 distance: int,
                 min_links: int,
                 width: int,
                 height: int):


        super(GraphGenerator, self).__init__()

        # Allow removing the thread when another graph is started and this is
        # still in the queue.
        self.setAutoDelete(True)

        self.signals = GraphGenSignals()

        self.graph_gen_timestamp = graph_gen_timestamp
        self.sutta = sutta
        self.dict_word = dict_word
        self.queue_id = queue_id
        self.graph_path = graph_path
        self.messages_url = messages_url
        self.width = width
        self.height = height

        self.labels = ShowLabels(labels)

        self.distance = distance
        self.min_links = min_links

    @pyqtSlot()
    def run(self):
        try:
            _, _, self._db_session = get_db_engine_connection_session()

            if self.sutta is not None:

                (nodes, edges) = sutta_nodes_and_edges(self._db_session, self.sutta, distance=self.distance)

                selected = []
                for idx, n in enumerate(nodes):
                    if n[0] == sutta_graph_id(self.sutta):
                        selected.append(idx)

            elif self.dict_word is not None:
                (nodes, edges) = dict_word_nodes_and_edges(self._db_session, self.dict_word, distance=self.distance)

                selected = []
                for idx, n in enumerate(nodes):
                    if n[0] == sutta_graph_id(self.dict_word):
                        selected.append(idx)

            else:
                return

            generate_graph(nodes,
                           edges,
                           selected,
                           self.queue_id,
                           self.graph_path,
                           self.messages_url,
                           self.labels,
                           self.min_links,
                           (self.width, self.height))

            hits = len(nodes) - 1

            result = (self.graph_gen_timestamp, hits, self.graph_path)

        except Exception as e:
            print("ERROR: %s" % e)

        else:
            self.signals.result.emit(result)

        finally:
            self.signals.finished.emit()


class HasLinksSidebar:
    _app_data: AppData
    links_layout: QVBoxLayout
    rightside_tabs: QTabWidget
    links_tab_idx: int
    content_graph: QWebEngineView
    label_select: QComboBox
    distance_input: QSpinBox
    min_links_input: QSpinBox
    links_regenerate_button: QPushButton
    open_selected_link_button: QPushButton
    open_links_new_window_button: QPushButton
    show_network_graph: Callable
    _show_selected: Callable
    _last_graph_gen_timestamp: float

    def init_links_sidebar(self):
        self._last_graph_gen_timestamp = 0.0

        self.setup_links_controls()
        self.setup_content_graph()

    def setup_links_controls(self):
        self.distance_input.setMinimum(1)
        self.distance_input.setValue(2)

        self.min_links_input.setMinimum(1)
        self.min_links_input.setValue(1)

        def _show_graph(arg: Optional[Any] = None):
            # ignore the argument value, will read params somewhere else
            self.show_network_graph()

        # "Sutta Ref.", "Ref. + Title", "No Labels"
        self.label_select.currentIndexChanged.connect(partial(_show_graph))

        self.distance_input \
            .valueChanged.connect(partial(_show_graph))

        self.min_links_input \
            .valueChanged.connect(partial(_show_graph))

        self.links_regenerate_button \
            .clicked.connect(partial(_show_graph))

        self.open_selected_link_button \
            .clicked.connect(partial(self._show_selected))

    def setup_content_graph(self):
        self.content_graph = QWebEngineView()
        self.content_graph.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_graph.setHtml('')
        self.content_graph.show()
        self.links_layout.addWidget(self.content_graph)

    def _graph_finished(self, result: Tuple[float, int, Path]):
        result_timestamp = result[0]
        hits = result[1]
        graph_path = result[2]

        # Ignore this result if it is not the last which the user has initiated.
        if result_timestamp != self._last_graph_gen_timestamp:
            return

        if hits > 0:
            self.rightside_tabs.setTabText(self.links_tab_idx, f"Links ({hits})")
        else:
            self.rightside_tabs.setTabText(self.links_tab_idx, "Links")

        self.content_graph.load(QUrl(str(graph_path.absolute().as_uri())))

    def generate_and_show_graph(self,
                                sutta: Optional[USutta],
                                dict_word: Optional[UDictWord],
                                queue_id: str,
                                graph_path: Path,
                                messages_url: str):

        # Remove worker threads which are in the queue and not yet started.
        self._app_data.graph_gen_pool.clear()

        distance = self.distance_input.value()
        min_links = self.min_links_input.value()

        n = self.label_select.currentIndex()
        labels = self.label_select.itemText(n)

        width = self.rightside_tabs.frameGeometry().width() - 20
        height = self.rightside_tabs.frameGeometry().height() - 80

        graph_gen_timestamp = time.time()
        self._last_graph_gen_timestamp = graph_gen_timestamp
        graph_gen = GraphGenerator(graph_gen_timestamp, sutta, dict_word, queue_id, graph_path, messages_url, labels, distance, min_links, width, height)

        graph_gen.signals.result.connect(self._graph_finished)

        # Only update text when it already has a links number,
        # so that empty results don't cause jumping layout
        if self.rightside_tabs.tabText(self.links_tab_idx) != "Links":
            self.rightside_tabs.setTabText(self.links_tab_idx, "Links (...)")

        self.content_graph.setHtml(LOADING_HTML)

        self._app_data.graph_gen_pool.start(graph_gen)

    def generate_graph_for_document(self,
                                    file_doc: FileDoc,
                                    db_doc: Um.Document,
                                    queue_id: str,
                                    graph_path: Path,
                                    messages_url: str):

        (nodes, edges) = document_page_nodes_and_edges(
            db_session=self._app_data.db_session,
            db_doc=db_doc,
            page_number=file_doc._current_idx + 1,
            distance=3
        )

        hits = len(nodes) - 1
        if hits > 0:
            self.rightside_tabs.setTabText(self.links_tab_idx, f"Links ({hits})")
        else:
            self.rightside_tabs.setTabText(self.links_tab_idx, "Links")

        # central node was appended last
        selected = [len(nodes) - 1]

        generate_graph(nodes, edges, selected, queue_id, graph_path, messages_url)
