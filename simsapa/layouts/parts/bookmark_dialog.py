import re
import json
from typing import Callable, Optional, TypedDict
from PyQt6 import QtCore

from PyQt6.QtCore import QAbstractListModel, Qt, pyqtSignal
from PyQt6.QtGui import QKeySequence
from PyQt6.QtWidgets import (QHBoxLayout, QDialog, QLabel, QLineEdit, QListView, QPushButton, QPlainTextEdit, QFormLayout)

from sqlalchemy.sql import func
# from simsapa.app.file_doc import FileDoc
from simsapa import IS_SWAY, DbSchemaName, logger
from simsapa.app.helpers import compact_plain_text

from simsapa.app.app_data import AppData
from simsapa.layouts.sutta_tab import SuttaTabWidget

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um


class SelectionData(TypedDict):
    sel_range: str
    sel_text: str
    anchor_text: str
    nth_in_anchor: int


class SuggestModel(QAbstractListModel):
    def __init__(self):
        super().__init__()
        self.items = []

    def data(self, index, role):
        if role == Qt.ItemDataRole.DisplayRole:
            return self.items[index.row()]

    def rowCount(self, _):
        if self.items:
            return len(self.items)
        else:
            return 0


class BookmarkDialog(QDialog):

    accepted = pyqtSignal(dict) # type: ignore

    def __init__(self,
                 app_data: AppData,
                 name: str = '',
                 quote: str = '',
                 selection_range: str = '',
                 comment: str = '',
                 show_quote: bool = True,
                 show_comment: bool = True,
                 db_id: Optional[int] = None,
                 creating_new: bool = True):

        super().__init__()

        if IS_SWAY:
            self.setFixedSize(400, 800)

        self.creating_new = creating_new
        self.init_name = name
        self.db_id = db_id

        self.selection_range = selection_range

        if self.creating_new:
            self.setWindowTitle("Create Bookmark")
        else:
            self.setWindowTitle("Edit Bookmark")

        self._app_data = app_data

        self.name_input = QLineEdit(self.init_name)
        self.name_input.setPlaceholderText("e.g. bhavana/solitude/delights in")
        self.name_input.setMinimumSize(QtCore.QSize(250, 35))
        self.name_input.setClearButtonEnabled(True)

        self.name_input.textChanged.connect(self.unlock)
        self.name_input.textChanged.connect(self._suggest_names)

        self.quote_input = QPlainTextEdit(quote)
        self.quote_input.setPlaceholderText("quote from the text")
        self.quote_input.setMinimumSize(400, 100)

        self.comment_input = QPlainTextEdit(comment)
        self.comment_input.setPlaceholderText("comment notes")
        self.comment_input.setMinimumSize(400, 100)

        if self.creating_new:
            self.add_btn = QPushButton('Add')
        else:
            self.add_btn = QPushButton('Save')

        self.add_btn.setDisabled(True)
        self.add_btn.setShortcut(QKeySequence("Ctrl+Return"))
        self.add_btn.setToolTip("Ctrl+Return")
        self.add_btn.clicked.connect(self.add_pressed)

        self.close_btn = QPushButton('Close')
        self.close_btn.clicked.connect(self.close_pressed)

        form = QFormLayout(self)

        form.addRow(QLabel("Bookmark name:"))
        form.addRow(self.name_input)

        self.suggest_list = QListView(self)
        self.suggest_list.setMinimumHeight(100)
        self.suggest_model = SuggestModel()
        self.suggest_list.setModel(self.suggest_model)

        self.suggest_sel = self.suggest_list.selectionModel()
        if self.suggest_sel is not None:
            self.suggest_sel.selectionChanged.connect(self._handle_suggest_select)

        form.addRow(self.suggest_list)

        if show_quote:
            form.addRow(QLabel("Quote:"))
            form.addRow(self.quote_input)

        if show_comment:
            form.addRow(QLabel("Comment:"))
            form.addRow(self.comment_input)

        self.buttons_layout = QHBoxLayout()
        self.buttons_layout.addWidget(self.add_btn)
        self.buttons_layout.addWidget(self.close_btn)

        form.addRow(self.buttons_layout)

        # Check if add_btn can be already unlocked (such as editing existing bookmarks)
        self.unlock()

    def _handle_suggest_select(self):
        a = self.suggest_list.selectedIndexes()
        if not a:
            return

        idx = a[0]
        name = self.suggest_model.items[idx.row()]

        self.name_input.setText(name)

    def _suggest_names(self):
        query = self.name_input.text()

        res = self._app_data.db_session \
            .query(Um.Bookmark.name) \
            .filter(Um.Bookmark.name.like(f"%{query}%")) \
            .all()

        names = list(set(map(lambda x: x[0], res)))
        names.sort()

        self.suggest_model.items = names

        self.suggest_model.layoutChanged.emit()

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
        name = re.sub(r' */ *', '/', self.name_input.text())

        if self.creating_new:
            values = {
                'name': name,
                'quote': self.quote_input.toPlainText(),
                'selection_range': self.selection_range,
                'comment_text': self.comment_input.toPlainText(),
            }

        else:
            values = {
                'old_name': self.init_name,
                'new_name': name,
                'db_id': self.db_id,
                'quote': self.quote_input.toPlainText(),
                'selection_range': self.selection_range,
                'comment_text': self.comment_input.toPlainText(),
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
    _active_tab: SuttaTabWidget

    bookmark_created = pyqtSignal()
    bookmark_updated = pyqtSignal()

    def init_bookmark_dialog(self):
        self.new_bookmark_values = {'name': '', 'quote': '', 'selection_range': '', 'comment_text': ''}

    def set_new_bookmark(self, values: dict):
        self.new_bookmark_values = values

    def _create_or_update_bookmark(self,
                                   bookmark_name: str,
                                   bookmark_id: Optional[int] = None,
                                   bookmark_quote: Optional[str] = None,
                                   bookmark_nth: Optional[int] = None,
                                   bookmark_selection_range: Optional[str] = None,
                                   bookmark_comment_text: Optional[str] = None,
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

        # create or update the new Bookmark

        # append trailing / for db value
        bookmark_name += "/"

        if bookmark_id:

            bookmark = self._app_data.db_session \
                .query(Um.Bookmark) \
                .filter(Um.Bookmark.id == bookmark_id) \
                .first()

            if not bookmark:
                logger.error(f"Bookmark not found: {bookmark_id}")
                return

            bookmark.name = bookmark_name

            if bookmark_quote:
                bookmark.quote = bookmark_quote
                bookmark.nth = bookmark_nth
            if bookmark_selection_range:
                bookmark.selection_range = bookmark_selection_range
            if bookmark_comment_text:
                bookmark.comment_text = bookmark_comment_text

            try:
                self._app_data.db_session.commit()

                self.bookmark_updated.emit() # type: ignore

            except Exception as e:
                logger.error(e)

        else:

            bookmark = Um.Bookmark(
                name = bookmark_name,
                quote = bookmark_quote,
                nth = bookmark_nth,
                selection_range = bookmark_selection_range,
                comment_text = bookmark_comment_text,
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


    def _count_occurrence_before(self, quote: str, before: str, sutta_schema: str, sutta_id: int) -> int:
        r = None
        if sutta_schema == DbSchemaName.AppData.value:
            r = self._app_data \
                    .db_session.query(Am.Sutta) \
                    .filter(Am.Sutta.id == sutta_id) \
                    .first()


        elif sutta_schema == DbSchemaName.UserData.value:
            r = self._app_data \
                    .db_session.query(Um.Sutta) \
                    .filter(Um.Sutta.id == sutta_id) \
                    .first()

        else:
            raise Exception("Only appdata and userdata schema are allowed.")

        if r is None or r.content_plain is None or r.content_plain == '':
            return 0

        content = str(r.content_plain)

        a = content.split(compact_plain_text(before))

        # if it was not split, len is 1
        if len(a) > 1:
            text = a[0]
            count = len(text.split(quote)) - 1
            return count
        else:
            return 0


    def _bookmark_with_range(self, sel_data_json: str):
        if sel_data_json == '':
            return

        sel_data: SelectionData = json.loads(sel_data_json)
        logger.info(f"_bookmark_with_range(): {sel_data}")

        if self._active_tab.sutta is None:
            logger.error("Sutta is not set")
            return

        quote = sel_data['sel_text'].replace("\n", " ").strip()

        sutta_uid = self._active_tab.sutta.uid
        name = "/".join([sutta_uid.replace("/", "_"), quote[0:20]])

        d = BookmarkDialog(self._app_data, name=name, quote=quote, selection_range=sel_data['sel_range'])
        d.accepted.connect(self.set_new_bookmark)
        d.exec()

        if 'name' not in self.new_bookmark_values.keys() or self.new_bookmark_values['name'] == '':
            return

        sutta_schema = self._active_tab.sutta.metadata.schema
        sutta_id = int(str(self._active_tab.sutta.id))

        n_before = self._count_occurrence_before(
            quote = sel_data['sel_text'],
            before = sel_data['anchor_text'],
            sutta_schema = sutta_schema,
            sutta_id = sutta_id,
        )

        bookmark_nth = n_before + sel_data['nth_in_anchor']

        try:
            self._create_or_update_bookmark(
                bookmark_name = self.new_bookmark_values['name'],
                bookmark_quote = self.new_bookmark_values['quote'],
                bookmark_nth = bookmark_nth,
                bookmark_selection_range = self.new_bookmark_values['selection_range'],
                bookmark_comment_text = self.new_bookmark_values['comment_text'],
                sutta_id = sutta_id,
                sutta_uid = str(self._active_tab.sutta.uid),
                sutta_schema = sutta_schema,
                sutta_ref = str(self._active_tab.sutta.sutta_ref),
                sutta_title = str(self._active_tab.sutta.title),
            )

        except Exception as e:
            logger.error(e)

    def handle_create_bookmark_for_sutta(self):
        self._active_tab = self._get_active_tab()

        if self._active_tab.sutta is None:
            logger.error("Sutta is not set")
            return

        self._active_tab.qwe.page().runJavaScript("get_selection_data()", self._bookmark_with_range)

    def handle_edit_bookmark(self, schema_and_id: str):
        _, db_id = schema_and_id.split("-")

        item = self._app_data.db_session \
                            .query(Um.Bookmark) \
                            .filter(Um.Bookmark.id == int(db_id)) \
                            .first()

        if item is None:
            return

        d = BookmarkDialog(self._app_data,
                           name=str(item.name),
                           quote=str(item.quote) if item.quote is not None else '',
                           selection_range=str(item.selection_range) if item.selection_range is not None else '',
                           comment=str(item.comment_text) if item.comment_text is not None else '',
                           db_id=int(db_id),
                           creating_new=False)

        d.accepted.connect(self.set_new_bookmark)
        d.exec()

        if 'new_name' not in self.new_bookmark_values.keys() or self.new_bookmark_values['new_name'] == '':
            return

        try:
            self._create_or_update_bookmark(
                bookmark_name = self.new_bookmark_values['new_name'],
                bookmark_id = int(self.new_bookmark_values['db_id']),
                bookmark_quote = self.new_bookmark_values['quote'],
                bookmark_selection_range = self.new_bookmark_values['selection_range'],
                bookmark_comment_text = self.new_bookmark_values['comment_text'],
            )

        except Exception as e:
            logger.error(e)
