import logging as _logging
import requests
from functools import partial
from typing import List, Optional

from PyQt5.QtCore import QAbstractListModel, Qt
from PyQt5.QtWidgets import (QLabel, QMainWindow,  QMessageBox)  # type: ignore
from sqlalchemy.sql import func  # type: ignore

from simsapa.assets import icons_rc

from ..app.db_models import Note  # type: ignore
from ..app.types import AppData  # type: ignore
from ..assets.ui.notes_browser_window_ui import Ui_NotesBrowserWindow  # type: ignore

logger = _logging.getLogger(__name__)


class NoteListModel(QAbstractListModel):
    def __init__(self, *args, notes=None, **kwargs):
        super(NoteListModel, self).__init__(*args, **kwargs)
        self.notes = notes or []

    def data(self, index, role):
        if role == Qt.DisplayRole:
            text = self.notes[index.row()].front + " " + self.notes[index.row()].back
            return text

    def rowCount(self, index):
        return len(self.notes)


class NotesBrowserWindow(QMainWindow, Ui_NotesBrowserWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.model = NoteListModel(notes=self._get_all_notes())

        self.notes_list.setModel(self.model)
        self.sel_model = self.notes_list.selectionModel()

        self._ui_setup()
        self._connect_signals()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.front_input.setFocus()
        self._show_note_clear()

    def _get_all_notes(self) -> List[Note]:
        return self._app_data.user_db_session.query(Note).all()

    def _handle_query(self):
        pass

    def get_selected_note(self) -> Optional[Note]:
        a = self.notes_list.selectedIndexes()
        if not a:
            return None

        item = a[0]
        return self.model.notes[item.row()]

    def remove_selected_note(self):
        a = self.notes_list.selectedIndexes()
        if not a:
            return None

        # Remove from model
        item = a[0]
        note_id = self.model.notes[item.row()].id

        del self.model.notes[item.row()]
        self.model.layoutChanged.emit()
        self.notes_list.clearSelection()
        self._show_note_clear()

        # Remove from database

        db_item = self._app_data.user_db_session \
                                .query(Note) \
                                .filter(Note.id == note_id) \
                                .first()
        self._app_data.user_db_session.delete(db_item)
        self._app_data.user_db_session.commit()

    def _handle_note_select(self):
        note = self.get_selected_note()
        if note:
            self._show_note(note)

    def _show_note_clear(self):
        self.status_msg.clear()
        self.front_input.clear()
        self.back_input.clear()

    def _show_note(self, note: Note):
        self.front_input.setPlainText(note.front)
        self.back_input.setPlainText(note.back)

    def _notes_search_query(self, query: str):
        return []

    def add_note(self):

        front = self.front_input.toPlainText()
        back = self.back_input.toPlainText()

        if len(front) == 0 and len(back) == 0:
            logger.info("Empty content, cancel adding.")
            return

        # Insert database record

        logger.info("Adding new note")

        db_note = Note(
            front=front,
            back=back,
            created_at=func.now(),
        )

        try:
            self._app_data.user_db_session.add(db_note)
            self._app_data.user_db_session.commit()

            # Add to model
            self.model.notes.append(db_note)

        except Exception as e:
            logger.error(e)

        self.model.layoutChanged.emit()

    def update_selected_note_front(self):
        note = self.get_selected_note()
        if note is None:
            return

        note.front = self.front_input.toPlainText()

        self._app_data.user_db_session.commit()
        self.model.layoutChanged.emit()

    def update_selected_note_back(self):
        note = self.get_selected_note()
        if note is None:
            return

        note.back = self.back_input.toPlainText()

        self._app_data.user_db_session.commit()
        self.model.layoutChanged.emit()

    def remove_note_dialog(self):
        note = self.get_selected_note()
        if not note:
            return

        reply = QMessageBox.question(self,
                                     'Remove Note...',
                                     'Remove this item?',
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)
        if reply == QMessageBox.Yes:
            self.remove_selected_note()

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

        for note in self.model.notes:

            if note.anki_note_id:
                logger.info("Updating Anki note...")

                anki_invoke(
                    'updateNoteFields',
                    note={
                        'id': note.anki_note_id,
                        'fields': {
                            'Front': note.front,
                            'Back': note.back,
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
                            'Front': note.front,
                            'Back': note.back,
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

                note.anki_note_id = res
                self._app_data.user_db_session.commit()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Add \
            .triggered.connect(partial(self.add_note))

        self.action_Remove \
            .triggered.connect(partial(self.remove_note_dialog))

        self.action_Sync_to_Anki \
            .triggered.connect(partial(self.sync_to_anki))

        self.search_button.clicked.connect(partial(self._handle_query))
        self.search_input.textChanged.connect(partial(self._handle_query))

        self.front_input.textChanged.connect(partial(self.update_selected_note_front))
        self.back_input.textChanged.connect(partial(self.update_selected_note_back))

        self.sel_model.selectionChanged.connect(partial(self._handle_note_select))


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
