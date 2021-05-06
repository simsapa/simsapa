from functools import partial
from typing import List

from PyQt5.QtCore import Qt  # type: ignore
from PyQt5.QtWidgets import (QHBoxLayout, QLabel, QLineEdit,  # type: ignore
                             QMainWindow, QPushButton, QTextBrowser, QListWidget,
                             QVBoxLayout, QWidget)

from ..app.db_models import RootText as DbSutta  # type: ignore
from ..app.types import AppData, Sutta  # type: ignore


class SuttaSearchWindow(QMainWindow):
    def __init__(self, app_data: AppData) -> None:
        super().__init__()
        self.setWindowTitle('Simsapa - Sutta Search')

        self._app_data: AppData = app_data
        self._sutta_results: List[Sutta] = []
        self._sutta_history: List[Sutta] = []

        self.layout = QVBoxLayout()
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._central_widget.setLayout(self.layout)

        self._create_window_layout()

    def _create_window_layout(self):
        self._create_search_bar()
        self._create_results()

    def _create_search_bar(self):
        layout = QHBoxLayout()

        w = QLineEdit()
        w.setFixedHeight(35)
        w.setAlignment(Qt.AlignLeft)
        w.setFocus()

        self.search_input = w
        layout.addWidget(self.search_input)

        w = QPushButton('Search')
        w.setFixedSize(100, 40)

        self.search_button = w
        layout.addWidget(self.search_button)

        self.layout.addLayout(layout)

    def _create_results(self):
        layout = QHBoxLayout()

        w = QTextBrowser()

        self.html_frame = w
        layout.addWidget(self.html_frame)

        results_history_layout = QVBoxLayout()

        w = QLabel('Results')
        results_history_layout.addWidget(w)

        self.sutta_results = QListWidget()
        results_history_layout.addWidget(self.sutta_results)

        w = QLabel('History')
        results_history_layout.addWidget(w)

        self.sutta_history = QListWidget()
        results_history_layout.addWidget(self.sutta_history)

        layout.addLayout(results_history_layout)

        self.layout.addLayout(layout)

    def set_html_content(self, html):
        self.html_frame.setText(html)


class SuttaSearchCtrl:
    def __init__(self, view):
        self._view = view
        self._connect_signals()

    def _handle_query(self):
        query = self._view.search_input.text()
        if len(query) > 3:
            self._view._sutta_results = self._sutta_search_query(query)
            titles = list(map(lambda s: s.title, self._view._sutta_results))
            self._view.sutta_results.clear()
            self._view.sutta_results.addItems(titles)

    def _handle_result_select(self):
        selected_idx = self._view.sutta_results.currentRow()
        sutta: Sutta = self._view._sutta_results[selected_idx]
        self._show_sutta(sutta)

        self._view._sutta_history.insert(0, sutta)
        self._view.sutta_history.insertItem(0, sutta.title)

    def _handle_history_select(self):
        selected_idx = self._view.sutta_history.currentRow()
        sutta: Sutta = self._view._sutta_history[selected_idx]
        self._show_sutta(sutta)

    def _show_sutta(self, sutta: Sutta):
        html = """
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <style>%s</style>
  </head>
  <body>
  %s
  </body>
</html>
""" % ('', sutta.content_html)

        self._view.set_html_content(html)

    def _sutta_search_query(self, query: str):
        results = self._view._app_data.db_session \
                               .query(DbSutta) \
                               .filter(DbSutta.content_html.like(f"%{query}%")) \
                               .all()
        return results

    def _connect_signals(self):
        self._view.search_button.clicked.connect(partial(self._handle_query))
        self._view.search_input.textChanged.connect(partial(self._handle_query))
        # self._view.search_input.returnPressed.connect(partial(self._update_result))
        self._view.sutta_results.itemSelectionChanged.connect(partial(self._handle_result_select))
        self._view.sutta_history.itemSelectionChanged.connect(partial(self._handle_history_select))
