from functools import partial
from typing import List
from markdown import markdown
from sqlalchemy.orm import joinedload  # type: ignore

from PyQt5.QtWidgets import (QLabel, QMainWindow)  # type: ignore

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, UDictWord  # type: ignore
from ..assets.ui.dictionary_search_window_ui import Ui_DictionarySearchWindow  # type: ignore


class DictionarySearchWindow(QMainWindow, Ui_DictionarySearchWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data
        self._results: List[UDictWord] = []
        self._history: List[UDictWord] = []

        self._ui_setup()

        self._connect_signals()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("Word title")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.search_input.setFocus()

    def _handle_query(self):
        query = self.search_input.text()
        if len(query) > 3:
            self._results = self._word_search_query(query)
            titles = list(map(lambda s: s.word, self._results))
            self.results_list.clear()
            self.results_list.addItems(titles)

    def _set_content_html(self, html):
        self.content_html.setText(html)

    def _handle_result_select(self):
        selected_idx = self.results_list.currentRow()
        word: UDictWord = self._results[selected_idx]
        self._show_word(word)

        self._history.insert(0, word)
        self.history_list.insertItem(0, word.word)

    def _handle_history_select(self):
        selected_idx = self.history_list.currentRow()
        word: UDictWord = self._history[selected_idx]
        self._show_word(word)

    def _show_word(self, word: UDictWord):
        self.status_msg.setText(word.word)

        def example_format(example):
            return "<div>" + example.text_html + "</div><div>" + example.translation_html + "</div>"

        examples = "".join(list(map(example_format, word.examples)))

        if word.definition_html is not None and word.definition_html != '':
            content = word.definition_html
        elif word.definition_plain is not None and word.definition_plain != '':
            content = markdown(word.definition_plain)
        else:
            content = '<p>No content.</p>'

        html = """
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <style>%s</style>
  </head>
  <body>
    <div> %s </div>
    <div> %s </div>
  </body>
</html>
""" % ('', content, examples)

        self._set_content_html(html)

    def _word_search_query(self, query: str) -> List[UDictWord]:
        results: List[UDictWord] = []

        res = self._app_data.db_session \
                                  .query(Am.DictWord) \
                                  .options(joinedload(Am.DictWord.examples)) \
                                  .filter(Am.DictWord.word.like(f"%{query}%")) \
                                  .all()
        results.extend(res)

        res = self._app_data.db_session \
                                  .query(Um.DictWord) \
                                  .options(joinedload(Um.DictWord.examples)) \
                                  .filter(Um.DictWord.word.like(f"%{query}%")) \
                                  .all()
        results.extend(res)

        return results

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query))
        self.search_input.textChanged.connect(partial(self._handle_query))
        # self.search_input.returnPressed.connect(partial(self._update_result))
        self.results_list.itemSelectionChanged.connect(partial(self._handle_result_select))
        self.history_list.itemSelectionChanged.connect(partial(self._handle_history_select))
