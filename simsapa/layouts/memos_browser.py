import logging as _logging
import json
import requests
from functools import partial
from typing import List, Optional

from PyQt5.QtCore import QAbstractListModel, Qt, QItemSelectionModel
from PyQt5.QtWidgets import (QLabel, QMainWindow,  QMessageBox)
from sqlalchemy.sql import func

from simsapa.assets import icons_rc  # noqa: F401

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import AppData, UMemo
from ..assets.ui.memos_browser_window_ui import Ui_MemosBrowserWindow

logger = _logging.getLogger(__name__)


class MemoListModel(QAbstractListModel):
    def __init__(self, *args, memos=None, **kwargs):
        super(MemoListModel, self).__init__(*args, **kwargs)
        self.memos = memos or []

    def data(self, index, role):
        if role == Qt.ItemDataRole.DisplayRole:
            fields = json.loads(self.memos[index.row()].fields_json)
            text = " ".join(fields.values())
            return text

    def rowCount(self, index):
        if self.memos:
            return len(self.memos)
        else:
            return 0


class MemosBrowserWindow(QMainWindow, Ui_MemosBrowserWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.model = MemoListModel(memos=self._get_all_memos())

        self.memos_list.setModel(self.model)
        self.sel_model = self.memos_list.selectionModel()

        self._ui_setup()
        self._connect_signals()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.front_input.setFocus()
        self._show_memo_clear()

    def _get_all_memos(self) -> List[UMemo]:
        results = []
        res = self._app_data.db_session.query(Am.Memo).all()
        results.extend(res)
        res = self._app_data.db_session.query(Um.Memo).all()
        results.extend(res)

        return results

    def _handle_query(self):
        pass

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
        self._show_memo_clear()

        # Remove from database

        if schema == 'appdata':
            db_item = self._app_data.db_session \
                                    .query(Am.Memo) \
                                    .filter(Am.Memo.id == memo_id) \
                                    .first()
        else:
            db_item = self._app_data.db_session \
                                    .query(Um.Memo) \
                                    .filter(Um.Memo.id == memo_id) \
                                    .first()

        self._app_data.db_session.delete(db_item)
        self._app_data.db_session.commit()

    def _handle_memo_select(self):
        memo = self.get_selected_memo()
        if memo:
            self._show_memo(memo)

    def _show_memo_clear(self):
        self.status_msg.clear()
        self.front_input.clear()
        self.back_input.clear()

    def _show_memo(self, memo: UMemo):
        fields = json.loads(memo.fields_json) # type: ignore
        self.front_input.setPlainText(fields['Front'])
        self.back_input.setPlainText(fields['Back'])

    def clear_memo(self):
        self.sel_model.clearSelection()
        self.front_input.clear()
        self.back_input.clear()

    def _memos_search_query(self, query: str):
        return []

    def add_memo(self):
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
            logger.error("Can't find Deck")
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

            # Add to model
            if self.model.memos:
                self.model.memos.append(memo)
            else:
                self.model.memos = [memo]

        except Exception as e:
            logger.error(e)

        index = self.model.index(len(self.model.memos) - 1)
        self.memos_list.selectionModel().select(index, QItemSelectionModel.SelectionFlag.Select)

        self.model.layoutChanged.emit()

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

        reply = QMessageBox.question(self,
                                     'Remove Memo...',
                                     'Remove this item?',
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)
        if reply == QMessageBox.Yes:
            self.remove_selected_memo()

    def test_anki_live_or_notify(self) -> bool:
        if not is_anki_live():
            QMessageBox.information(self,
                                    "Anki is not connected",
                                    "Anki must be running with the AnkiConnect plugin.",
                                    QMessageBox.Ok)
            return False
        return True

    def sync_to_anki(self):
        if not self.test_anki_live_or_notify():
            return

        res = anki_invoke('deckNames')
        if 'Simsapa' not in res:
            anki_invoke('createDeck', deck='Simsapa')

        for memo in self.model.memos:

            fields = json.loads(memo.fields_json)

            if memo.anki_note_id:
                logger.info("Updating Anki note...")

                anki_invoke(
                    'updateNoteFields',
                    note={
                        'id': memo.anki_note_id,
                        'fields': {
                            'Front': fields['Front'],
                            'Back': fields['Back'],
                        },
                    }
                )

            else:
                logger.info("Creating Anki note...")

                res = anki_invoke(
                    'addNote',
                    note={
                        'deckName': 'Simsapa',
                        'modelName': 'Basic',
                        'fields': {
                            'Front': fields['Front'],
                            'Back': fields['Back'],
                        },
                        'options': {
                            'allowDuplicate': True,
                            'duplicateScope': 'deck',
                            'duplicateScopeOptions': {
                                'deckName': 'Simsapa',
                                'checkChildren': False
                            }
                        },
                    }
                )

                memo.anki_note_id = res
                self._app_data.db_session.commit()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Add \
            .triggered.connect(partial(self.add_memo))

        self.action_Clear \
            .triggered.connect(partial(self.clear_memo))

        self.action_Remove \
            .triggered.connect(partial(self.remove_memo_dialog))

        self.action_Sync_to_Anki \
            .triggered.connect(partial(self.sync_to_anki))

        self.search_button.clicked.connect(partial(self._handle_query))
        self.search_input.textChanged.connect(partial(self._handle_query))

        self.front_input.textChanged.connect(partial(self.update_selected_memo_fields))
        self.back_input.textChanged.connect(partial(self.update_selected_memo_fields))

        self.sel_model.selectionChanged.connect(partial(self._handle_memo_select))


def is_anki_live() -> bool:
    try:
        res = requests.get('http://localhost:8765')
    except Exception:
        return False

    return res.status_code == 200


def anki_request(action, **params):
    return {'action': action, 'params': params, 'version': 6}


def anki_invoke(action, **params):
    res = requests.get(url='http://localhost:8765', json=anki_request(action, **params))
    response = res.json()

    if len(response) != 2:
        raise Exception('response has an unexpected number of fields')

    if 'error' not in response:
        raise Exception('response is missing required error field')

    if 'result' not in response:
        raise Exception('response is missing required result field')

    if response['error'] is not None:
        raise Exception(response['error'])

    return response['result']
