import logging as _logging
import json
from pathlib import Path
import queue

from functools import partial
from typing import List, Optional

from PyQt5.QtCore import Qt, QUrl, QTimer
from PyQt5.QtGui import QKeySequence, QTextCursor, QTextCharFormat, QPalette, QCloseEvent
from PyQt5.QtWidgets import (QLabel, QMainWindow, QAction, QTextBrowser, QListWidgetItem)  # type: ignore
from PyQt5.QtWebEngineWidgets import QWebEngineView

from sqlalchemy import or_
from sqlalchemy.sql import func  # type: ignore

import networkx as nx
from bokeh.io import output_file, save, curdoc
from bokeh.document import Document
from bokeh.models import (Button, Plot, Circle, MultiLine, Range1d, ColumnDataSource,
                          LabelSet, PanTool, WheelZoomTool, TapTool, ResetTool,
                          NodesAndLinkedEdges, EdgesAndLinkedNodes)
from bokeh.palettes import Spectral4
from bokeh.plotting import from_networkx
from bokeh.layouts import column
from bokeh.models import CustomJS
from bokeh import events

from simsapa import ASSETS_DIR, APP_QUEUES
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, USutta  # type: ignore
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
                if data['action'] == 'show_sutta_by_uid':
                    self._show_sutta_by_uid(data['arg'])

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _ui_setup(self):
        self.status_msg = QLabel("Sutta title")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.content_graph = QWebEngineView()
        self.content_graph.setHtml('')
        self.content_graph.show()
        self.results_layout.addWidget(self.content_graph)

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
        self.content_html.setText(html)
        self._highlight_query()

    def _highlight_query(self):
        find_text = self.search_input.text()

        # get default selection format from view's palette
        palette = self.palette()
        text_format = QTextCharFormat()
        text_format.setBackground(palette.brush(QPalette.Normal, QPalette.Highlight))
        text_format.setForeground(palette.brush(QPalette.Normal, QPalette.HighlightedText))

        # find all occurrences of the text
        doc = self.content_html.document()
        cur = QTextCursor()
        selections = []
        while True:
            cur = doc.find(find_text, cur)
            if cur.isNull():
                break
            sel = QTextBrowser.ExtraSelection()
            sel.cursor = cur
            sel.format = text_format
            selections.append(sel)

        doc = self.content_html.setExtraSelections(selections)

    def _handle_result_select(self):
        selected_idx = self.results_list.currentRow()
        sutta: USutta = self._results[selected_idx]
        self._show_sutta(sutta)

        self._history.insert(0, sutta)
        self.history_list.insertItem(0, sutta.title)

    def _handle_history_select(self):
        selected_idx = self.history_list.currentRow()
        sutta: USutta = self._history[selected_idx]
        self._show_sutta(sutta)

    def _generate_network_bokeh(self, sutta: USutta):
        G = nx.Graph()

        nodes = [
            (1, {'sutta_ref': 'DN 1', 'uid': 'dn1'}),
            (2, {'sutta_ref': 'DN 2', 'uid': 'dn2'}),
            (3, {'sutta_ref': 'MN 1', 'uid': 'mn1'}),
            (4, {'sutta_ref': 'MN 2', 'uid': 'mn2'}),
        ]
        G.add_nodes_from(nodes)

        edges = [
            (1, 2),
            (1, 3),
            (1, 4),
            (3, 4),
        ]
        G.add_edges_from(edges)

        plot = Plot(
            plot_width=700,
            plot_height=400,
            x_range=Range1d(-1.1, 1.1),
            y_range=Range1d(-1.1, 1.1),
        )

        wheel_zoom = WheelZoomTool()
        plot.add_tools(
            PanTool(),
            TapTool(),
            wheel_zoom,
            ResetTool()
        )
        plot.toolbar.active_scroll = wheel_zoom

        network_graph = from_networkx(G, nx.spring_layout, scale=0.8, center=(0, 0), seed=100)

        network_graph.node_renderer.glyph = Circle(size=15, fill_color=Spectral4[0])
        network_graph.node_renderer.selection_glyph = Circle(size=15, fill_color=Spectral4[2])
        network_graph.node_renderer.nonselection_glyph = Circle(size=15, fill_color=Spectral4[1])

        network_graph.edge_renderer.glyph = MultiLine(line_color="black", line_alpha=0.8, line_width=1)
        network_graph.edge_renderer.selection_glyph = MultiLine(line_color=Spectral4[2], line_width=2)

        network_graph.selection_policy = NodesAndLinkedEdges()
        network_graph.inspection_policy = EdgesAndLinkedNodes()

        plot.renderers.append(network_graph)

        x, y = zip(*network_graph.layout_provider.graph_layout.values())
        source = ColumnDataSource({
            'x': x,
            'y': y,
            'sutta_ref': [nodes[i][1]['sutta_ref'] for i in range(len(x))],
            'uid': [nodes[i][1]['uid'] for i in range(len(x))],
        })

        labels = LabelSet(
            x='x',
            y='y',
            text='sutta_ref',
            source=source,
            background_fill_color='white',
            text_font_size='15px',
            background_fill_alpha=.7)

        plot.renderers.append(labels)

        selected = []

        for idx, n in enumerate(nodes):
            if n[1]['uid'] == sutta.uid:
                selected.append(idx)

        network_graph.node_renderer.data_source.selected.indices = selected

        network_graph.node_renderer.data_source.selected.js_on_change('indices', CustomJS(args=dict(source=source), code="""
window.selected_uids = [];
var inds = cb_obj.indices;
var d1 = source.data;
for (var i=0; i<inds.length; i++) {
  var x = d1['uid'][inds[i]];
  window.selected_uids.push(x);
}
"""))

        button = Button(label='Open Sutta')

        url = f'http://localhost:8000/queues/{self.queue_id}'
        js_code = """
if (typeof window.selected_uids === 'undefined') {
    window.selected_uids = [];
}
if (window.selected_uids.length > 0) {
    const params = {
        action: 'show_sutta_by_uid',
        arg: window.selected_uids[0],
    };
    const options = {
        method: 'POST',
        headers: {
          'Accept': 'application/json',
          'Content-Type': 'application/json'
        },
        body: JSON.stringify(params),
    };
    fetch('%s', options);
}
""" % (url,)

        button.js_on_event(events.ButtonClick, CustomJS(code=js_code, args={}))

        layout = column(button, plot)

        doc: Document = curdoc()
        doc.clear()
        doc.add_root(layout)

        output_file(filename=str(self.graph_path), mode='absolute')
        save(doc)

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
        text = self.content_html.textCursor().selectedText()
        # U+2029 Paragraph Separator to blank line
        text = text.replace('\u2029', "\n\n")
        self._app_data.clipboard_setText(text)

    def _set_memo_fields(self, values):
        self.memo_fields = values

    def _handle_create_memo(self):
        text = self.content_html.textCursor().selectedText()

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

    def _handle_select_all(self):
        self.content_html.selectAll()

    def _setup_content_html_context_menu(self):
        self.content_html.setContextMenuPolicy(Qt.ActionsContextMenu)

        copyAction = QAction("Copy", self.content_html)
        copyAction.setShortcut(QKeySequence("Ctrl+C"))
        copyAction.triggered.connect(partial(self._handle_copy))

        memoAction = QAction("Create Memo", self.content_html)
        memoAction.setShortcut(QKeySequence("Ctrl+M"))
        memoAction.triggered.connect(partial(self._handle_create_memo))

        selectAllAction = QAction("Select All", self.content_html)
        selectAllAction.setShortcut(QKeySequence("Ctrl+A"))
        selectAllAction.triggered.connect(partial(self._handle_select_all))

        self.content_html.addActions([
            copyAction,
            memoAction,
            selectAllAction,
        ])

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query))
        self.search_input.textChanged.connect(partial(self._handle_query))
        # self.search_input.returnPressed.connect(partial(self._update_result))
        self.results_list.itemSelectionChanged.connect(partial(self._handle_result_select))
        self.history_list.itemSelectionChanged.connect(partial(self._handle_history_select))
