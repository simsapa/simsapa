import time
import requests
from functools import partial
from pathlib import Path
from typing import Any, Callable, Optional, Tuple

from PyQt6.QtWidgets import QComboBox, QPushButton, QSizePolicy, QSpinBox, QTabWidget, QVBoxLayout
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtCore import QObject, QRunnable, QUrl, pyqtSignal, pyqtSlot

from simsapa.layouts.reader_web import LinkHoverData, ReaderWebEnginePage

# from ..app.file_doc import FileDoc
# from ..app.db import userdata_models as Um

from simsapa.app.types import GraphRequest, QueryType, USutta, UDictWord
from simsapa.app.app_data import AppData

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa import IS_LINUX, LOADING_HTML, ShowLabels, logger


class GraphGenSignals(QObject):
    finished = pyqtSignal()
    # Result is a Tuple[float, int, Path]:
    # - timestamp
    # - number of hits (links)
    # - path of the graph html
    result = pyqtSignal(tuple)


class GraphGenerator(QRunnable):
    def __init__(self,
                 api_url: str,
                 graph_gen_timestamp: float,
                 sutta_uid: Optional[str],
                 dict_word_uid: Optional[str],
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

        self.api_url = api_url
        self.graph_gen_timestamp = graph_gen_timestamp
        self.sutta_uid = sutta_uid
        self.dict_word_uid = dict_word_uid
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
            p = GraphRequest(
                sutta_uid=self.sutta_uid,
                dict_word_uid=self.dict_word_uid,
                distance=self.distance,
                queue_id=self.queue_id,
                graph_gen_timestamp=self.graph_gen_timestamp,
                graph_path=str(self.graph_path),
                messages_url=self.messages_url,
                labels=self.labels,
                min_links=self.min_links,
                width=self.width,
                height=self.height,
            )

            res = requests.post(self.api_url + "/generate_graph", json=p)

        except Exception as e:
            logger.error("%s" % e)

        else:
            if res.ok:
                j = res.json()
                # [1667675976.411231, 100, '/home/yume/.local/share/simsapa/assets/graphs/window_1.html']
                r = (j[0], j[1], Path(j[2]))
                self.signals.result.emit(r)
            else:
                logger.error("%s" % res)

        finally:
            self.signals.finished.emit()


class HasLinksSidebar(QObject):
    _app_data: AppData
    links_layout: QVBoxLayout
    rightside_tabs: QTabWidget
    links_tab_idx: int
    qwe: QWebEngineView
    label_select: QComboBox
    distance_input: QSpinBox
    min_links_input: QSpinBox
    links_regenerate_button: QPushButton
    open_selected_link_button: QPushButton
    open_links_new_window_button: QPushButton
    show_network_graph: Callable
    _show_selected: Callable
    _last_graph_gen_timestamp: float

    graph_link_mouseover: pyqtSignal

    def init_links_sidebar(self):
        self._last_graph_gen_timestamp = 0.0

        self.setup_links_controls()
        self.setup_qwe()

    def setup_links_controls(self):
        self.distance_input.setMinimum(1)
        self.distance_input.setValue(2)

        self.min_links_input.setMinimum(1)
        self.min_links_input.setValue(1)

        def _show_graph(_: Optional[Any] = None):
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

    def _link_mouseover_graph_node(self, data: dict):
        # {'table': 'appdata.suttas', 'id': 19707}

        if data['table'].startswith('appdata'):
            r = self._app_data.db_session.query(Am.Sutta) \
                                         .filter(Am.Sutta.id == int(data['id'])) \
                                         .first()
        else:
            r = self._app_data.db_session.query(Um.Sutta) \
                                         .filter(Um.Sutta.id == int(data['id'])) \
                                         .first()

        if r is None:
            return

        hover_data = LinkHoverData(
            href = f"ssp://{QueryType.suttas.value}/{r.uid}",
            # Coords not necessary. The preview is rendered next to the mouse cursor's position.
            x = 0, y = 0, width = 0, height = 0,
        )

        self.graph_link_mouseover.emit(hover_data)

    def setup_qwe(self):
        self.links_graph_qwe = QWebEngineView()

        page = ReaderWebEnginePage(self)
        page.helper.mouseover_graph_node.connect(partial(self._link_mouseover_graph_node))

        self.links_graph_qwe.setPage(page)

        self.links_graph_qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        self.links_graph_qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        self.links_graph_qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        self.links_graph_qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

        self.links_graph_qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.links_graph_qwe.setHtml('')
        self.links_graph_qwe.show()
        self.links_layout.addWidget(self.links_graph_qwe)

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

        self.links_graph_qwe.load(QUrl(str(graph_path.absolute().as_uri())))

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

        if IS_LINUX:
            width = self.rightside_tabs.frameGeometry().width() - 20
        else:
            width = self.rightside_tabs.frameGeometry().width() - 40
        height = self.rightside_tabs.frameGeometry().height() - 80

        graph_gen_timestamp = time.time()
        self._last_graph_gen_timestamp = graph_gen_timestamp

        if self._app_data.api_url is None:
            return

        if sutta is None:
            sutta_uid = None
        else:
            sutta_uid = str(sutta.uid)

        if dict_word is None:
            dict_word_uid = None
        else:
            dict_word_uid = str(dict_word.uid)

        graph_gen = GraphGenerator(self._app_data.api_url, graph_gen_timestamp, sutta_uid, dict_word_uid, queue_id, graph_path, messages_url, labels, distance, min_links, width, height)

        graph_gen.signals.result.connect(self._graph_finished)

        # Only update text when it already has a links number,
        # so that empty results don't cause jumping layout
        if self.rightside_tabs.tabText(self.links_tab_idx) != "Links":
            self.rightside_tabs.setTabText(self.links_tab_idx, "Links (...)")

        self.links_graph_qwe.setHtml(LOADING_HTML)

        self._app_data.graph_gen_pool.start(graph_gen)

    # def generate_graph_for_document(self,
    #                                 file_doc: FileDoc,
    #                                 db_doc: Um.Document,
    #                                 queue_id: str,
    #                                 graph_path: Path,
    #                                 messages_url: str):

    #     (nodes, edges) = document_page_nodes_and_edges(
    #         db_doc=db_doc,
    #         page_number=file_doc._current_idx + 1,
    #         distance=3
    #     )

    #     hits = len(nodes) - 1
    #     if hits > 0:
    #         self.rightside_tabs.setTabText(self.links_tab_idx, f"Links ({hits})")
    #     else:
    #         self.rightside_tabs.setTabText(self.links_tab_idx, "Links")

    #     # central node was appended last
    #     selected = [len(nodes) - 1]

    #     generate_graph(nodes, edges, selected, queue_id, graph_path, messages_url)
