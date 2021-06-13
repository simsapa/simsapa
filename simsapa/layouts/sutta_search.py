import logging as _logging
import json

from functools import partial
from typing import List, Optional

from PyQt5.QtCore import Qt
from PyQt5.QtGui import QKeySequence, QTextCursor, QTextCharFormat, QPalette
from PyQt5.QtWidgets import (QLabel, QMainWindow, QAction, QTextBrowser, QListWidgetItem)  # type: ignore

from sqlalchemy import or_
from sqlalchemy.sql import func  # type: ignore

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

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("Sutta title")
        self.statusbar.addPermanentWidget(self.status_msg)

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
                    w.setSnippet(x.content_html[0:200])

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

    def _show_sutta(self, sutta: USutta):
        self._current_sutta = sutta
        self.status_msg.setText(sutta.title)

        if sutta.content_html is not None and sutta.content_html != '':
            content = sutta.content_html
        elif sutta.content_plain is not None and sutta.content_plain != '':
            content = '<pre>' + sutta.content_plain + '</pre>'
        else:
            content = 'No content.'

        html = """
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

        self._set_content_html(html)

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
