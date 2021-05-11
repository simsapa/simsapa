from functools import partial
from typing import List

from PyQt5.QtWidgets import (QLabel, QMainWindow)  # type: ignore

from ..app.db_models import RootText as DbSutta  # type: ignore
from ..app.types import (AppData, Sutta)  # type: ignore
from ..assets.ui.sutta_search_window_ui import Ui_SuttaSearchWindow  # type: ignore


class SuttaSearchWindow(QMainWindow, Ui_SuttaSearchWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data
        self._results: List[Sutta] = []
        self._history: List[Sutta] = []

        self._ui_setup()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("Sutta title")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.search_input.setFocus()


class SuttaSearchCtrl:
    def __init__(self, view):
        self._view = view
        self._connect_signals()

    def _handle_query(self):
        query = self._view.search_input.text()
        if len(query) > 3:
            self._view._results = self._sutta_search_query(query)
            titles = list(map(lambda s: s.title, self._view._results))
            self._view.results_list.clear()
            self._view.results_list.addItems(titles)

    def _set_content_html(self, html):
        self._view.content_html.setText(html)

    def _handle_result_select(self):
        selected_idx = self._view.results_list.currentRow()
        sutta: Sutta = self._view._results[selected_idx]
        self._show_sutta(sutta)

        self._view._history.insert(0, sutta)
        self._view.history_list.insertItem(0, sutta.title)

    def _handle_history_select(self):
        selected_idx = self._view.history_list.currentRow()
        sutta: Sutta = self._view._history[selected_idx]
        self._show_sutta(sutta)

    def _show_sutta(self, sutta: Sutta):
        self._view.status_msg.setText(sutta.title)

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

        self._set_content_html(html)

    def _sutta_search_query(self, query: str):
        results = self._view._app_data.app_db_session \
                               .query(DbSutta) \
                               .filter(DbSutta.content_html.like(f"%{query}%")) \
                               .all()
        return results

    def _connect_signals(self):
        self._view.action_Close_Window \
            .triggered.connect(partial(self._view.close))

        self._view.search_button.clicked.connect(partial(self._handle_query))
        self._view.search_input.textChanged.connect(partial(self._handle_query))
        # self._view.search_input.returnPressed.connect(partial(self._update_result))
        self._view.results_list.itemSelectionChanged.connect(partial(self._handle_result_select))
        self._view.history_list.itemSelectionChanged.connect(partial(self._handle_history_select))
