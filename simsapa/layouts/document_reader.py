from functools import partial
from typing import List, Optional
import logging as _logging

from PyQt5.QtCore import QAbstractListModel, Qt
from PyQt5.QtGui import QImage, QPixmap
from PyQt5.QtWidgets import (QLabel, QMainWindow, QFileDialog, QInputDialog,
                             QMessageBox)  # type: ignore

from ..app.file_doc import FileDoc  # type: ignore
from ..app.db_models import Document, Note  # type: ignore

from ..app.types import AppData  # type: ignore
from ..assets.ui.document_reader_window_ui import Ui_DocumentReaderWindow  # type: ignore

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
        if self.notes:
            return len(self.notes)
        else:
            return 0


class DocumentReaderWindow(QMainWindow, Ui_DocumentReaderWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.model = NoteListModel()
        self.notes_list.setModel(self.model)
        self.sel_model = self.notes_list.selectionModel()

        self._ui_setup()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)
        self._show_note_clear()

        self.file_doc = None
        self.db_doc = None

    def open_file_dialog(self):
        file_path, _ = QFileDialog.getOpenFileName(
            None,
            "Open File...",
            "",
            "PDF or Epub Files (*.pdf *.epub)")

        if len(file_path) != 0:
            self.open_doc(file_path)

    def open_doc(self, path):
        self.db_doc = self._app_data.user_db_session \
                                .query(Document) \
                                .filter(Document.filepath == path) \
                                .first()

        notes = self._get_notes_for_this_page()
        if notes:
            self.model.notes = notes
        else:
            self.model.notes = []

        self.model.layoutChanged.emit()

        self.file_doc = FileDoc(path)
        self.doc_go_to_page(1)

    def doc_show_current(self):
        self.doc_go_to_page(self.file_doc.current_page_number())

    def doc_go_to_page(self, page: int):
        if not self.file_doc:
            return

        logger.info(f"doc_go_to_page({page})")

        page_count = self.file_doc.number_of_pages()

        if self.file_doc is None or page < 1 or page > page_count:
            return

        self.page_current_of_total.setText(f"{page} of {page_count}")

        if self.file_doc:
            self.file_doc.set_page_number(page)

            page_img = self.file_doc.current_page_image()
            if page_img:
                img = QImage(page_img.image_bytes,
                             page_img.width,
                             page_img.height,
                             page_img.stride,
                             QImage.Format.Format_RGB888)
                self.content_page.setPixmap(QPixmap.fromImage(img))

        if self.db_doc:
            self.notes_list.clearSelection()
            self._show_note_clear()
            self.model.notes = self._get_notes_for_this_page()
            self.model.layoutChanged.emit()

    def _previous_page(self):
        page_nr = self.file_doc.current_page_number() - 1
        if self.file_doc and page_nr > 0:
            self.file_doc.set_page_number(page_nr)
            self._upd_current_page_input(page_nr)

    def _next_page(self):
        page_nr = self.file_doc.current_page_number() + 1
        if self.file_doc and page_nr <= self.file_doc.number_of_pages():
            self.file_doc.set_page_number(page_nr)
            self._upd_current_page_input(page_nr)

    def _beginning(self):
        if self.file_doc and self.file_doc.current_page_number() != 1:
            self.file_doc.set_page_number(1)
            self._upd_current_page_input(1)

    def _end(self):
        if self.file_doc and self.file_doc.current_page_number() != self.file_doc.number_of_pages():
            self.file_doc.set_page_number(self.file_doc.number_of_pages())
            self._upd_current_page_input(self.file_doc.number_of_pages())

    def _upd_current_page_input(self, n):
        self.current_page_input.setValue(n)
        self.current_page_input.clearFocus()

    def _go_to_page_dialog(self):
        n, ok = QInputDialog.getInt(self, "Go to Page...", "Page:", 1, 1, self.file_doc.number_of_pages(), 1)
        if ok:
            self._upd_current_page_input(n)

    def _go_to_page_input(self):
        n = self.current_page_input.value()
        self.doc_go_to_page(n)

    def _get_notes_for_this_page(self) -> List[Note]:
        if self.db_doc is None or self.file_doc is None:
            return

        notes = self._app_data.user_db_session \
                              .query(Note) \
                              .filter(
                                  Note.document_id == self.db_doc.id,
                                  Note.doc_page_number == self.file_doc.current_page_number()) \
                              .all()

        return notes

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
        self.front_input.clear()
        self.back_input.clear()

    def _show_note(self, note: Note):
        self.front_input.setPlainText(note.front)
        self.back_input.setPlainText(note.back)

    def add_note(self):

        front = self.front_input.toPlainText()
        back = self.back_input.toPlainText()

        if len(front) == 0 and len(back) == 0:
            logger.info("Empty content, cancel adding.")
            return

        # Insert database record

        logger.info("Adding new note")

        if self.db_doc is None:
            db_note = Note(
                front=front,
                back=back
            )
        else:
            db_note = Note(
                front=front,
                back=back,
                document_id=self.db_doc.id,
                doc_page_number=self.file_doc.current_page_number(),
            )

        try:
            self._app_data.user_db_session.add(db_note)
            self._app_data.user_db_session.commit()

            # Add to model
            if self.model.notes:
                self.model.notes.append(db_note)
            else:
                self.model.notes = [db_note]

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


class DocumentReaderCtrl:
    def __init__(self, view):
        self._view = view
        self._connect_signals()

    def _connect_signals(self):
        self._view.action_Close_Window \
            .triggered.connect(partial(self._view.close))

        self._view.action_Previous_Page \
            .triggered.connect(partial(self._view._previous_page))
        self._view.action_Next_Page \
            .triggered.connect(partial(self._view._next_page))

        self._view.action_Beginning \
            .triggered.connect(partial(self._view._beginning))
        self._view.action_End \
            .triggered.connect(partial(self._view._end))

        self._view.action_Go_to_Page \
            .triggered.connect(partial(self._view._go_to_page_dialog))

        self._view.current_page_input \
            .valueChanged.connect(partial(self._view._go_to_page_input))

        self._view.sel_model.selectionChanged.connect(partial(self._view._handle_note_select))

        self._view.add_note_button \
                  .clicked.connect(partial(self._view.add_note))

        self._view.remove_note_button \
                  .clicked.connect(partial(self._view.remove_note_dialog))

        self._view.front_input \
                  .textChanged.connect(partial(self._view.update_selected_note_front))

        self._view.back_input \
                  .textChanged.connect(partial(self._view.update_selected_note_back))
