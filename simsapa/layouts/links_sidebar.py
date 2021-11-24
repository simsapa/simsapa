from pathlib import Path
import logging as _logging

from PyQt5.QtWidgets import QSizePolicy
from PyQt5.QtWebEngineWidgets import QWebEngineView

from ..app.graph import (generate_graph, sutta_nodes_and_edges,
                         dict_word_nodes_and_edges,
                         document_page_nodes_and_edges, sutta_graph_id)

from ..app.file_doc import FileDoc
from ..app.db import userdata_models as Um

from ..app.types import USutta, UDictWord

logger = _logging.getLogger(__name__)


class HasLinksSidebar:
    def init_links_sidebar(self):
        self.setup_content_graph()

    def setup_content_graph(self):
        self.content_graph = QWebEngineView()
        self.content_graph.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_graph.setHtml('')
        self.content_graph.show()
        self.links_layout.addWidget(self.content_graph)

    def generate_graph_for_sutta(self,
                                 sutta: USutta,
                                 queue_id: str,
                                 graph_path: Path,
                                 messages_url: str):

        (nodes, edges) = sutta_nodes_and_edges(app_data=self._app_data, sutta=sutta, distance=3)

        hits = len(nodes) - 1
        if hits > 0:
            self.rightside_tabs.setTabText(self.links_tab_idx, f"Links ({hits})")
        else:
            self.rightside_tabs.setTabText(self.links_tab_idx, "Links")

        selected = []

        for idx, n in enumerate(nodes):
            if n[0] == sutta_graph_id(sutta):
                selected.append(idx)

        generate_graph(nodes, edges, selected, queue_id, graph_path, messages_url)

    def generate_graph_for_dict_word(self,
                                     dict_word: UDictWord,
                                     queue_id: str,
                                     graph_path: Path,
                                     messages_url: str):

        (nodes, edges) = dict_word_nodes_and_edges(app_data=self._app_data, dict_word=dict_word, distance=3)

        hits = len(nodes) - 1
        if hits > 0:
            self.rightside_tabs.setTabText(self.links_tab_idx, f"Links ({hits})")
        else:
            self.rightside_tabs.setTabText(self.links_tab_idx, "Links")

        # central node was appended last
        selected = [len(nodes) - 1]

        generate_graph(nodes, edges, selected, queue_id, graph_path, messages_url)

    def generate_graph_for_document(self,
                                    file_doc: FileDoc,
                                    db_doc: Um.Document,
                                    queue_id: str,
                                    graph_path: Path,
                                    messages_url: str):

        (nodes, edges) = document_page_nodes_and_edges(
            app_data=self._app_data,
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
