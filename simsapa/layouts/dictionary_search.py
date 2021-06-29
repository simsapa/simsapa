from functools import partial
from typing import List
from markdown import markdown
from sqlalchemy.orm import joinedload  # type: ignore

from PyQt5.QtCore import Qt
from PyQt5.QtGui import QKeySequence
from PyQt5.QtWidgets import (QLabel, QMainWindow, QAction, QListWidgetItem,
                             QVBoxLayout, QHBoxLayout, QPushButton,
                             QSizePolicy)
from PyQt5.QtWebEngineWidgets import QWebEngineView

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, UDictWord  # type: ignore
from ..assets.ui.dictionary_search_window_ui import Ui_DictionarySearchWindow  # type: ignore
from .search_item import SearchItemWidget


class DictionarySearchWindow(QMainWindow, Ui_DictionarySearchWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data
        self._results: List[UDictWord] = []
        self._history: List[UDictWord] = []

        self._ui_setup()

        self._connect_signals()
        self._setup_content_html_context_menu()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("Word title")
        self.statusbar.addPermanentWidget(self.status_msg)

        self._setup_pali_buttons()
        self._setup_content_html()

        self.search_input.setFocus()

    def _setup_content_html(self):
        self.content_html = QWebEngineView()
        self.content_html.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.content_html.setHtml('')
        self.content_html.show()
        self.content_layout.addWidget(self.content_html)

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

    def _handle_query(self, min_length=4):
        self._highlight_query()
        query = self.search_input.text()
        if len(query) >= min_length:
            self._results = self._word_search_query(query)

            self.results_list.clear()

            for x in self._results:
                w = SearchItemWidget()
                w.setTitle(x.word)

                if x.definition_html:
                    w.setSnippet(x.definition_html[0:400].strip())

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

    def _handle_copy(self):
        text = self.content_html.selectedText()
        # U+2029 Paragraph Separator to blank line
        text = text.replace('\u2029', "\n\n")
        self._app_data.clipboard_setText(text)

    def _setup_content_html_context_menu(self):
        self.content_html.setContextMenuPolicy(Qt.ActionsContextMenu)

        copyAction = QAction("Copy", self.content_html)
        copyAction.setShortcut(QKeySequence("Ctrl+C"))
        copyAction.triggered.connect(partial(self._handle_copy))

        self.content_html.addActions([
            copyAction,
        ])

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textChanged.connect(partial(self._handle_query, min_length=4))
        # self.search_input.returnPressed.connect(partial(self._update_result))
        self.results_list.itemSelectionChanged.connect(partial(self._handle_result_select))
        self.history_list.itemSelectionChanged.connect(partial(self._handle_history_select))
