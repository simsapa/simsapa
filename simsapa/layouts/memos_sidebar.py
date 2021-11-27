from functools import partial
from typing import List, Optional
import logging as _logging
import json
from PyQt5 import QtWidgets

from PyQt5.QtCore import Qt, QAbstractListModel, QItemSelectionModel
from PyQt5.QtWidgets import QListView, QListWidget, QMessageBox, QPlainTextEdit

from sqlalchemy.sql import func

from ..app.file_doc import FileDoc
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import AppData, USutta, UDictWord, UMemo

logger = _logging.getLogger(__name__)


class MemoPlainListModel(QAbstractListModel):
    def __init__(self, *args, memos=None, **kwargs):
        super(MemoPlainListModel, self).__init__(*args, **kwargs)
        self.memos = memos or []

    def data(self, index, role):
        if role == Qt.ItemDataRole.DisplayRole:
            fields = json.loads(self.memos[index.row()].fields_json)
            text = "\n".join(fields.values())
            return text

    def rowCount(self, index):
        if self.memos:
            return len(self.memos)
        else:
            return 0


class HasMemosSidebar:
    _app_data: AppData
    front_input: QtWidgets.QPlainTextEdit
    back_input: QtWidgets.QPlainTextEdit
    front: QPlainTextEdit
    back: QPlainTextEdit
    features: List[str] = []
    memos_list: QListView
    _current_sutta: Optional[USutta]
    _current_word: Optional[UDictWord]
    file_doc: Optional[FileDoc]
    db_doc: Optional[Um.Document]
    clear_memo_button: QtWidgets.QPushButton
    remove_memo_button: QtWidgets.QPushButton
    rightside_tabs: QtWidgets.QTabWidget
    memos_tab_idx: int

    def init_memos_sidebar(self):
        self.features.append('memos_sidebar')

        self.model = MemoPlainListModel()
        self.memos_list.setModel(self.model)
        self.sel_model = self.memos_list.selectionModel()

        palette = self.memos_list.palette()

        style = """
QListView::item { border-bottom: 1px solid %s; }
QListView::item:selected { background-color: %s; color: %s; }
        """ % (palette.midlight().color().name(),
               palette.highlight().color().name(),
               palette.highlightedText().color().name())

        self.memos_list.setStyleSheet(style)

        self.show_memo_clear()

        self.connect_memos_sidebar_signals()

    def show_memo_clear(self):
        self.front_input.clear()
        self.back_input.clear()

    def get_memos_for_sutta(self, sutta: USutta) -> List[UMemo]:
        am_assoc = []
        um_assoc = []

        schema = sutta.metadata.schema

        if schema == 'appdata':

            res = self._app_data.db_session \
                                .query(Am.MemoAssociation) \
                                .filter(
                                    Am.MemoAssociation.associated_table == 'appdata.suttas',
                                    Am.MemoAssociation.associated_id == sutta.id) \
                                .all()
            am_assoc.extend(res)

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'appdata.suttas',
                                    Um.MemoAssociation.associated_id == sutta.id) \
                                .all()
            um_assoc.extend(res)

        else:

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'userdata.suttas',
                                    Um.MemoAssociation.associated_id == sutta.id) \
                                .all()
            um_assoc.extend(res)

        memos: List[UMemo] = []

        ids = list(map(lambda x: x.memo_id, am_assoc))

        res = self._app_data.db_session \
                            .query(Am.Memo) \
                            .filter(Am.Memo.id.in_(ids)) \
                            .all()
        memos.extend(res)

        ids = list(map(lambda x: x.memo_id, um_assoc))

        res = self._app_data.db_session \
                            .query(Um.Memo) \
                            .filter(Um.Memo.id.in_(ids)) \
                            .all()
        memos.extend(res)

        return memos

    def get_memos_for_dict_word(self, word: UDictWord) -> List[UMemo]:
        am_assoc = []
        um_assoc = []

        schema = word.metadata.schema

        if schema == 'appdata':

            res = self._app_data.db_session \
                                .query(Am.MemoAssociation) \
                                .filter(
                                    Am.MemoAssociation.associated_table == 'appdata.dict_words',
                                    Am.MemoAssociation.associated_id == word.id) \
                                .all()
            am_assoc.extend(res)

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'appdata.dict_words',
                                    Um.MemoAssociation.associated_id == word.id) \
                                .all()
            um_assoc.extend(res)

        else:

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'userdata.dict_words',
                                    Um.MemoAssociation.associated_id == word.id) \
                                .all()
            um_assoc.extend(res)

        memos: List[UMemo] = []

        ids = list(map(lambda x: x.memo_id, am_assoc))

        res = self._app_data.db_session \
                            .query(Am.Memo) \
                            .filter(Am.Memo.id.in_(ids)) \
                            .all()
        memos.extend(res)

        ids = list(map(lambda x: x.memo_id, um_assoc))

        res = self._app_data.db_session \
                            .query(Um.Memo) \
                            .filter(Um.Memo.id.in_(ids)) \
                            .all()
        memos.extend(res)

        return memos

    def update_memos_list_for_sutta(self, sutta: USutta):
        memos = self.get_memos_for_sutta(sutta)
        if memos:
            self.model.memos = memos
        else:
            self.model.memos = []

        self.update_memos_list()

    def update_memos_list_for_dict_word(self, word: UDictWord):
        memos = self.get_memos_for_dict_word(word)
        if memos:
            self.model.memos = memos
        else:
            self.model.memos = []

        self.update_memos_list()

    def update_memos_list_for_document(self, file_doc: FileDoc, db_doc: Um.Document):
        memos = self.get_memos_for_document_page(file_doc, db_doc)
        if memos:
            self.model.memos = memos
        else:
            self.model.memos = []

        self.update_memos_list()

    def update_memos_list(self):
        self.memos_list.clearSelection()
        self.show_memo_clear()

        hits = len(self.model.memos)
        if hits > 0:
            self.rightside_tabs.setTabText(self.memos_tab_idx, f"Memos ({hits})")
        else:
            self.rightside_tabs.setTabText(self.memos_tab_idx, "Memos")

        self.model.layoutChanged.emit()

    def get_memos_for_document_page(self, file_doc: FileDoc, db_doc: Um.Document) -> List[UMemo]:
        am_assoc = []
        um_assoc = []

        doc_schema = db_doc.metadata.schema

        if doc_schema == 'appdata':

            res = self._app_data.db_session \
                                .query(Am.MemoAssociation) \
                                .filter(
                                    Am.MemoAssociation.associated_table == 'appdata.documents',
                                    Am.MemoAssociation.associated_id == db_doc.id,
                                    Am.MemoAssociation.page_number == file_doc.current_page_number()) \
                                .all()
            am_assoc.extend(res)

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'appdata.documents',
                                    Um.MemoAssociation.associated_id == db_doc.id,
                                    Um.MemoAssociation.page_number == file_doc.current_page_number()) \
                                .all()
            um_assoc.extend(res)

        else:

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'userdata.documents',
                                    Um.MemoAssociation.associated_id == db_doc.id,
                                    Um.MemoAssociation.page_number == file_doc.current_page_number()) \
                                .all()
            um_assoc.extend(res)

        memos: List[UMemo] = []

        ids = list(map(lambda x: x.memo_id, am_assoc))

        res = self._app_data.db_session \
                            .query(Am.Memo) \
                            .filter(Am.Memo.id.in_(ids)) \
                            .all()
        memos.extend(res)

        ids = list(map(lambda x: x.memo_id, um_assoc))

        res = self._app_data.db_session \
                            .query(Um.Memo) \
                            .filter(Um.Memo.id.in_(ids)) \
                            .all()
        memos.extend(res)

        return memos

    def get_selected_memo(self) -> Optional[UMemo]:
        a = self.memos_list.selectedIndexes()
        if not a:
            return None

        item = a[0]
        return self.model.memos[item.row()]

    def remove_selected_memo(self):
        a = self.memos_list.selectedIndexes()
        if not a:
            return None

        # Remove from model
        item = a[0]
        memo = self.model.memos[item.row()]
        memo_id = memo.id
        schema = memo.metadata.schema

        del self.model.memos[item.row()]
        self.model.layoutChanged.emit()
        self.memos_list.clearSelection()
        self.show_memo_clear()

        # Remove memo from database

        if schema == 'appdata':
            db_item = self._app_data.db_session \
                .query(Am.Memo) \
                .filter(Am.Memo.id == memo_id) \
                .first()

            assoc_item = self._app_data.db_session \
                .query(Am.MemoAssociation) \
                .filter(Am.MemoAssociation.memo_id == memo_id) \
                .first()
        else:
            db_item = self._app_data.db_session \
                .query(Um.Memo) \
                .filter(Um.Memo.id == memo_id) \
                .first()

            assoc_item = self._app_data.db_session \
                .query(Um.MemoAssociation) \
                .filter(Um.MemoAssociation.memo_id == memo_id) \
                .first()

        self._app_data.db_session.delete(db_item)
        self._app_data.db_session.delete(assoc_item)
        self._app_data.db_session.commit()

    def _handle_memo_select(self):
        memo = self.get_selected_memo()
        if memo:
            self._show_memo(memo)

    def _show_memo(self, memo: UMemo):
        fields = json.loads(memo.fields_json) # type: ignore
        self.front_input.setPlainText(fields['Front'])
        self.back_input.setPlainText(fields['Back'])

    def clear_memo(self):
        self.sel_model.clearSelection()
        self.front_input.clear()
        self.back_input.clear()

    def add_memo_for_sutta(self):
        if self._current_sutta is None:
            return

        self.sel_model.clearSelection()

        front = self.front_input.toPlainText()
        back = self.back_input.toPlainText()

        if len(front) == 0 and len(back) == 0:
            logger.info("Empty content, cancel adding.")
            return

        # Insert database record

        logger.info("Adding new memo")

        deck = self._app_data.db_session.query(Um.Deck).first()
        if deck is None:
            logger.error("Can't find the deck")
            return

        memo_fields = {
            'Front': front,
            'Back': back
        }

        memo = Um.Memo(
            deck_id=deck.id,
            fields_json=json.dumps(memo_fields),
            created_at=func.now(),
        )

        try:
            self._app_data.db_session.add(memo)
            self._app_data.db_session.commit()

            memo_assoc = Um.MemoAssociation(
                memo_id=memo.id,
                associated_table='appdata.suttas',
                associated_id=self._current_sutta.id,
            )

            self._app_data.db_session.add(memo_assoc)
            self._app_data.db_session.commit()

            # Add to model
            if self.model.memos:
                self.model.memos.append(memo)
            else:
                self.model.memos = [memo]

        except Exception as e:
            logger.error(e)

        index = self.model.index(len(self.model.memos) - 1)
        self.memos_list.selectionModel().select(index, QItemSelectionModel.SelectionFlag.Select)

        self.update_memos_list()

    def add_memo_for_dict_word(self):
        if self._current_word is None:
            return

        self.sel_model.clearSelection()

        front = self.front_input.toPlainText()
        back = self.back_input.toPlainText()

        if len(front) == 0 and len(back) == 0:
            logger.info("Empty content, cancel adding.")
            return

        # Insert database record

        logger.info("Adding new memo")

        deck = self._app_data.db_session.query(Um.Deck).first()
        if deck is None:
            logger.error("Can't find the deck")
            return

        memo_fields = {
            'Front': front,
            'Back': back
        }

        memo = Um.Memo(
            deck_id=deck.id,
            fields_json=json.dumps(memo_fields),
            created_at=func.now(),
        )

        try:
            self._app_data.db_session.add(memo)
            self._app_data.db_session.commit()

            schema = self._current_word.metadata.schema

            memo_assoc = Um.MemoAssociation(
                memo_id=memo.id,
                associated_table=f"{schema}.dict_words",
                associated_id=self._current_word.id,
            )

            self._app_data.db_session.add(memo_assoc)
            self._app_data.db_session.commit()

            # Add to model
            if self.model.memos:
                self.model.memos.append(memo)
            else:
                self.model.memos = [memo]

        except Exception as e:
            logger.error(e)

        index = self.model.index(len(self.model.memos) - 1)
        self.memos_list.selectionModel().select(index, QItemSelectionModel.SelectionFlag.Select)

        self.update_memos_list()

    def add_memo_for_document(self):
        if self.file_doc is None:
            return

        self.sel_model.clearSelection()

        front = self.front_input.toPlainText()
        back = self.back_input.toPlainText()

        if len(front) == 0 and len(back) == 0:
            logger.info("Empty content, cancel adding.")
            return

        # Insert database record

        logger.info("Adding new memo")

        deck = self._app_data.db_session.query(Um.Deck).first()
        if deck is None:
            logger.error("Can't find the deck")
            return

        memo_fields = {
            'Front': front,
            'Back': back
        }

        memo = Um.Memo(
            deck_id=deck.id,
            fields_json=json.dumps(memo_fields),
            created_at=func.now(),
        )

        try:
            self._app_data.db_session.add(memo)
            self._app_data.db_session.commit()

            if self.db_doc is not None:

                memo_assoc = Um.MemoAssociation(
                    memo_id=memo.id,
                    associated_table='userdata.documents',
                    associated_id=self.db_doc.id,
                    page_number=self.file_doc.current_page_number(),
                )

                self._app_data.db_session.add(memo_assoc)
                self._app_data.db_session.commit()

            # Add to model
            if self.model.memos:
                self.model.memos.append(memo)
            else:
                self.model.memos = [memo]

        except Exception as e:
            logger.error(e)

        index = self.model.index(len(self.model.memos) - 1)
        self.memos_list.selectionModel().select(index, QItemSelectionModel.SelectionFlag.Select)

        self.update_memos_list()

    def update_selected_memo_fields(self):
        memo = self.get_selected_memo()
        if memo is None:
            return

        fields = {
            'Front': self.front_input.toPlainText(),
            'Back': self.back_input.toPlainText()
        }

        memo.fields_json = json.dumps(fields) # type: ignore

        self._app_data.db_session.commit()
        self.model.layoutChanged.emit()

    def remove_memo_dialog(self):
        memo = self.get_selected_memo()
        if not memo:
            return

        reply = QMessageBox.question(self, # type: ignore
                                     'Remove Memo...',
                                     'Remove this item?',
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)
        if reply == QMessageBox.Yes:
            self.remove_selected_memo()
            self.update_memos_list()

    def connect_memos_sidebar_signals(self):
        self.sel_model.selectionChanged.connect(partial(self._handle_memo_select))

        self.clear_memo_button \
            .clicked.connect(partial(self.clear_memo))

        self.remove_memo_button \
            .clicked.connect(partial(self.remove_memo_dialog))

        self.front_input \
            .textChanged.connect(partial(self.update_selected_memo_fields))

        self.back_input \
            .textChanged.connect(partial(self.update_selected_memo_fields))
