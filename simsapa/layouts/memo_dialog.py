import json
from typing import Callable, List, Optional
from PyQt6 import QtWidgets

from PyQt6.QtCore import pyqtSignal, QItemSelectionModel
from PyQt6.QtGui import QKeySequence
from PyQt6.QtWidgets import (QHBoxLayout, QDialog, QListView, QPushButton, QPlainTextEdit, QFormLayout, QTabWidget)

from sqlalchemy.sql import func
# from simsapa.app.file_doc import FileDoc
from simsapa import logger

from simsapa.app.types import UDictWord
from simsapa.app.app_data import AppData
from simsapa.layouts.memos_sidebar import MemoPlainListModel
from simsapa.layouts.sutta_tab import SuttaTabWidget

# from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um


class MemoDialog(QDialog):

    accepted = pyqtSignal(dict) # type: ignore

    def __init__(self, front_text: Optional[str] = None, back_text: Optional[str] = None):
        super().__init__()
        self.setWindowTitle("Create Memo")

        if front_text is None:
            front_text = ''

        if back_text is None:
            back_text = ''

        self.front = QPlainTextEdit(front_text)
        self.front.setMinimumSize(400, 200)
        self.front.textChanged.connect(self.unlock)
        self.front.setTabChangesFocus(True)

        self.back = QPlainTextEdit(back_text)
        self.back.setMinimumSize(400, 200)
        self.back.textChanged.connect(self.unlock)
        self.back.setTabChangesFocus(True)

        self.add_btn = QPushButton('Add')
        self.add_btn.setDisabled(True)
        self.add_btn.setShortcut(QKeySequence("Ctrl+Return"))
        self.add_btn.setToolTip("Ctrl+Return")
        self.add_btn.clicked.connect(self.add_pressed)

        self.close_btn = QPushButton('Close')
        self.close_btn.clicked.connect(self.close_pressed)

        form = QFormLayout(self)

        form.addRow('Front', self.front)
        form.addRow('Back', self.back)

        self.buttons_layout = QHBoxLayout()
        self.buttons_layout.addWidget(self.add_btn)
        self.buttons_layout.addWidget(self.close_btn)

        form.addRow(self.buttons_layout)

    def not_blanks(self) -> bool:
        front = self.front.toPlainText()
        back = self.back.toPlainText()
        return front.strip() != '' and back.strip() != ''

    def unlock(self):
        if self.not_blanks():
            self.add_btn.setEnabled(True)
        else:
            self.add_btn.setDisabled(True)

    def add_pressed(self):
        values = {
            'Front': self.front.toPlainText(),
            'Back': self.back.toPlainText(),
        }
        self.accepted.emit(values)
        self.accept()

    def close_pressed(self):
        self.close()


class HasMemoDialog:
    _app_data: AppData
    front_input: QtWidgets.QPlainTextEdit
    back_input: QtWidgets.QPlainTextEdit
    front: QPlainTextEdit
    back: QPlainTextEdit
    features: List[str] = []
    memos_list: QListView
    _current_words: List[UDictWord]
    # file_doc: Optional[FileDoc]
    file_doc = None
    db_doc: Optional[Um.Document]
    clear_memo_button: QtWidgets.QPushButton
    remove_memo_button: QtWidgets.QPushButton
    rightside_tabs: QtWidgets.QTabWidget
    memos_tab_idx: int
    model: MemoPlainListModel
    update_memos_list: Callable
    sutta_tabs: QTabWidget
    _get_active_tab: Callable
    _get_selection: Callable
    sutta_tab: SuttaTabWidget
    _related_tabs: List[SuttaTabWidget]

    def init_memo_dialog(self):
        self.memo_dialog_fields = {}

    def set_memo_dialog_fields(self, values):
        self.memo_dialog_fields = values

    def handle_create_memo_for_sutta(self):
        tab = self._get_active_tab()

        if tab.sutta is None:
            logger.error("Sutta is not set")
            return

        front_text = f"...\n\n({tab.sutta.sutta_ref} {tab.sutta.title})"
        back_text = tab.qwe.selectedText()

        deck = self._app_data.db_session.query(Um.Deck).first()
        if deck is None:
            logger.error("Can't find the deck")
            return

        self.memo_dialog_fields = {
            'Front': '',
            'Back': '',
        }

        d = MemoDialog(front_text, back_text)
        d.accepted.connect(self.set_memo_dialog_fields)
        d.exec()

        if self.memo_dialog_fields['Front'] == '' or self.memo_dialog_fields['Back'] == '':
            return

        memo = Um.Memo(
            deck_id=deck.id,
            fields_json=json.dumps(self.memo_dialog_fields),
            created_at=func.now(),
        )

        try:
            self._app_data.db_session.add(memo)
            self._app_data.db_session.commit()

            schema = tab.sutta.metadata.schema

            if tab.sutta is not None:

                memo_assoc = Um.MemoAssociation(
                    memo_id=memo.id,
                    associated_table=f'{schema}.suttas',
                    associated_id=tab.sutta.id,
                )

                self._app_data.db_session.add(memo_assoc)
                self._app_data.db_session.commit()

        except Exception as e:
            logger.error(e)

        if 'memos_sidebar' in self.features:
            # Add to model
            if self.model.memos:
                self.model.memos.append(memo)
            else:
                self.model.memos = [memo]

            index = self.model.index(len(self.model.memos) - 1)
            self.memos_list.selectionModel().select(index, QItemSelectionModel.SelectionFlag.Select)

            self.update_memos_list()

    def handle_create_memo_for_dict_word(self):
        if len(self._current_words) == 0:
            logger.error("Word is not set")
            return

        text = self._get_selection()
        if text is None:
            text = ''

        deck = self._app_data.db_session.query(Um.Deck).first()
        if deck is None:
            logger.error("Can't find the deck")
            return

        self.memo_dialog_fields = {
            'Front': '',
            'Back': '',
        }

        d = MemoDialog(text)
        d.accepted.connect(self.set_memo_dialog_fields)
        d.exec()

        if self.memo_dialog_fields['Front'] == '' or self.memo_dialog_fields['Back'] == '':
            return

        memo = Um.Memo(
            deck_id=deck.id,
            fields_json=json.dumps(self.memo_dialog_fields),
            created_at=func.now(),
        )

        try:
            self._app_data.db_session.add(memo)
            self._app_data.db_session.commit()

            if len(self._current_words) > 0:

                schema = self._current_words[0].metadata.schema

                memo_assoc = Um.MemoAssociation(
                    memo_id=memo.id,
                    associated_table=f'{schema}.dict_words',
                    associated_id=self._current_words[0].id,
                )

                self._app_data.db_session.add(memo_assoc)
                self._app_data.db_session.commit()

        except Exception as e:
            logger.error(e)

        if 'memos_sidebar' in self.features:
            # Add to model
            if self.model.memos:
                self.model.memos.append(memo)
            else:
                self.model.memos = [memo]

            index = self.model.index(len(self.model.memos) - 1)
            self.memos_list.selectionModel().select(index, QItemSelectionModel.SelectionFlag.Select)

            self.update_memos_list()
