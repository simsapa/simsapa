import re
from typing import Callable, Optional
from PyQt6 import QtCore

from PyQt6.QtCore import pyqtSignal
from PyQt6.QtGui import QKeySequence
from PyQt6.QtWidgets import (QHBoxLayout, QDialog, QLineEdit, QPushButton, QPlainTextEdit, QFormLayout)

from sqlalchemy.sql import func
# from simsapa.app.file_doc import FileDoc
from simsapa import DbSchemaName, logger

from simsapa.app.types import AppData
from simsapa.layouts.sutta_tab import SuttaTabWidget

# from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

class BookmarkDialog(QDialog):

    accepted = pyqtSignal(dict) # type: ignore

    def __init__(self, quote: str = '', show_quote: bool = True):
        super().__init__()
        self.setWindowTitle("Create Bookmark")

        self.name_input = QLineEdit()
        self.name_input.setMinimumSize(QtCore.QSize(250, 35))
        self.name_input.setClearButtonEnabled(True)

        self.name_input.textChanged.connect(self.unlock)

        self.quote_input = QPlainTextEdit(quote)
        self.quote_input.setMinimumSize(400, 200)

        self.add_btn = QPushButton('Add')
        self.add_btn.setDisabled(True)
        self.add_btn.setShortcut(QKeySequence("Ctrl+Return"))
        self.add_btn.setToolTip("Ctrl+Return")
        self.add_btn.clicked.connect(self.add_pressed)

        self.close_btn = QPushButton('Close')
        self.close_btn.clicked.connect(self.close_pressed)

        form = QFormLayout(self)

        form.addRow('Name', self.name_input)
        if show_quote:
            form.addRow('Quote', self.quote_input)

        self.buttons_layout = QHBoxLayout()
        self.buttons_layout.addWidget(self.add_btn)
        self.buttons_layout.addWidget(self.close_btn)

        form.addRow(self.buttons_layout)

    def not_blanks(self) -> bool:
        name = self.name_input.text()
        return name.strip() != ''

    def unlock(self):
        if self.not_blanks():
            self.add_btn.setEnabled(True)
        else:
            self.add_btn.setDisabled(True)

    def add_pressed(self):
        # strip space around the separator /
        path = re.sub(r' */ *', '/', self.name_input.text())

        values = {
            'name': path,
            'quote': self.quote_input.toPlainText(),
        }
        self.accepted.emit(values)
        self.accept()

    def close_pressed(self):
        self.close()

class HasBookmarkDialog:
    _app_data: AppData
    _get_active_tab: Callable
    _get_selection: Callable
    sutta_tab: SuttaTabWidget

    bookmark_created = pyqtSignal()

    def init_bookmark_dialog(self):
        self.new_bookmark_values = {}

    def set_new_bookmark(self, values: dict):
        self.new_bookmark_values = values

    def _create_bookmark(self,
                         bookmark_name: str,
                         bookmark_quote: Optional[str] = None,
                         sutta_id: Optional[int] = None,
                         sutta_uid: Optional[str] = None,
                         sutta_schema: Optional[str] = None,
                         sutta_ref: Optional[str] = None,
                         sutta_title: Optional[str] = None):

        # ensure no trailing / before split
        bookmark_name = re.sub(r'/+$', '', bookmark_name)

        # create empty Bookmark db parent entries if they don't exist
        parts = bookmark_name.split("/")
        parents = []
        for n in range(1, len(parts)):
            parents.append("/".join(parts[0:n]))

        for name in parents:
            # append trailing / for db value
            name += "/"
            r = self._app_data.db_session \
                              .query(Um.Bookmark) \
                              .filter(Um.Bookmark.name == name) \
                              .first()
            if not r:
                b = Um.Bookmark(name = name, created_at = func.now())
                self._app_data.db_session.add(b)
                self._app_data.db_session.commit()

        # create the new Bookmark

        # append trailing / for db value
        bookmark_name += "/"

        bookmark = Um.Bookmark(
            name = bookmark_name,
            quote = bookmark_quote,
            sutta_id = sutta_id,
            sutta_uid = sutta_uid,
            sutta_schema = sutta_schema,
            sutta_ref = sutta_ref,
            sutta_title = sutta_title,
            created_at = func.now(),
        )

        try:
            self._app_data.db_session.add(bookmark)
            self._app_data.db_session.commit()

            self.bookmark_created.emit() # type: ignore

        except Exception as e:
            logger.error(e)

    def handle_create_bookmark_for_sutta(self):
        tab = self._get_active_tab()

        if tab.sutta is None:
            logger.error("Sutta is not set")
            return

        quote = str(tab.qwe.selectedText()).replace("\n", " ").strip()

        d = BookmarkDialog(quote)
        d.accepted.connect(self.set_new_bookmark)
        d.exec()

        if self.new_bookmark_values['name'] == '':
            return

        if tab.sutta.metadata.schema == DbSchemaName.AppData.value:
            sutta_id = None
        else:
            sutta_id = tab.sutta.id

        try:
            self._create_bookmark(
                bookmark_name = self.new_bookmark_values['name'],
                bookmark_quote = self.new_bookmark_values['quote'],
                sutta_id = sutta_id,
                sutta_uid = tab.sutta.uid,
                sutta_schema = tab.sutta.metadata.schema,
                sutta_ref = tab.sutta.sutta_ref,
                sutta_title = tab.sutta.title,
            )

            self.bookmark_created.emit() # type: ignore

        except Exception as e:
            logger.error(e)
