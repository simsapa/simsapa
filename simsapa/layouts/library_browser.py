import logging as _logging
import os.path
from functools import partial
from typing import List, Optional
from sqlalchemy.sql import func  # type: ignore

import fitz  # type: ignore
from PyQt5.QtCore import QAbstractListModel, Qt
from PyQt5.QtGui import QIcon, QImage, QPixmap
from PyQt5.QtWidgets import (QFileDialog, QLabel, QMainWindow,  # type: ignore
                             QMessageBox)

from simsapa.assets import icons_rc

from ..app.db_models import Document  # type: ignore
from ..app.types import AppData  # type: ignore
from ..assets.ui.library_browser_window_ui import \
    Ui_LibraryBrowserWindow  # type: ignore

logger = _logging.getLogger(__name__)


class DocumentListModel(QAbstractListModel):
    def __init__(self, *args, docs=None, **kwargs):
        super(DocumentListModel, self).__init__(*args, **kwargs)
        self.docs = docs or []

    def data(self, index, role):
        if role == Qt.DisplayRole:
            filepath = self.docs[index.row()].filepath
            name = os.path.basename(filepath)
            return name

        if role == Qt.DecorationRole:
            filepath = self.docs[index.row()].filepath
            if not os.path.exists(filepath):
                return QIcon(":close")

    def rowCount(self, index):
        return len(self.docs)


class LibraryBrowserWindow(QMainWindow, Ui_LibraryBrowserWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.model = DocumentListModel(docs=self._get_all_documents())

        self.documents_list.setModel(self.model)
        self.sel_model = self.documents_list.selectionModel()

        self._ui_setup()
        self._connect_signals()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.search_input.setFocus()
        self._show_document_clear()

    def _get_all_documents(self) -> List[Document]:
        return self._app_data.user_db_session.query(Document).all()

    def _handle_query(self):
        pass

    def get_selected_document(self) -> Optional[Document]:
        a = self.documents_list.selectedIndexes()
        if not a:
            return None

        item = a[0]
        return self.model.docs[item.row()]

    def remove_selected_document(self):
        a = self.documents_list.selectedIndexes()
        if not a:
            return None

        # Remove from model
        item = a[0]
        doc_id = self.model.docs[item.row()].id

        del self.model.docs[item.row()]
        self.model.layoutChanged.emit()
        self.documents_list.clearSelection()
        self._show_document_clear()

        # Remove from database

        db_item = self._app_data.user_db_session \
                                .query(Document) \
                                .filter(Document.id == doc_id) \
                                .first()
        self._app_data.user_db_session.delete(db_item)
        self._app_data.user_db_session.commit()

    def _handle_document_select(self):
        doc = self.get_selected_document()
        if doc:
            self._show_document(doc)

    def _show_document_clear(self):
        self.status_msg.clear()
        self.doc_title.clear()
        self.doc_author.clear()
        self.doc_cover.clear()

    def _show_document(self, doc: Document):
        self.status_msg.setText(doc.filepath)
        self.doc_title.setText(doc.title)
        self.doc_author.setText(doc.author)

        img = QImage(doc.cover_data, doc.cover_width, doc.cover_height, doc.cover_stride, QImage.Format.Format_RGB888)
        self.doc_cover.setPixmap(QPixmap.fromImage(img))

    def _documents_search_query(self, query: str):
        results = self._app_data.user_db_session \
                               .query(Document) \
                               .filter(Document.filepath.like(f"%{query}%")) \
                               .all()
        return results

    def add_document(self, filepath):
        if not os.path.exists(filepath):
            logger.error(f"File doesn't exist: {filepath}")
            return

        # Get properties with pymupdf
        file_doc = fitz.open(filepath)

        title = file_doc.metadata['title']
        if len(title) == 0:
            title = "Unknown"

        author = file_doc.metadata['author']
        if len(author) == 0:
            author = "Unknown"

        pix = file_doc[0].get_pixmap(matrix=fitz.Matrix(0.7, 0.7), alpha=False)
        cover_data = pix.tobytes("ppm")

        # Insert or update database record

        item = self._app_data.user_db_session \
                             .query(Document) \
                             .filter(Document.filepath == filepath) \
                             .first()
        if item is None:
            logger.info(f"Add new: {filepath}")

            db_doc = Document(
                filepath=filepath,
                title=title,
                author=author,
                cover_data=cover_data,
                cover_width=pix.width,
                cover_height=pix.height,
                cover_stride=pix.stride,
                created_at=func.now(),
            )

            try:
                self._app_data.user_db_session.add(db_doc)
                self._app_data.user_db_session.commit()

                # Add to model
                self.model.docs.append(db_doc)

            except Exception as e:
                logger.error(e)

        else:
            logger.info(f"Update: {item.filepath}")

            values = {
                'filepath': filepath,
                'title': title,
                'author': author,
                'cover_data': cover_data,
                'cover_width': pix.width,
                'cover_height': pix.height,
                'cover_stride': pix.stride,
                'updated_at': func.now(),
            }

            try:
                self._app_data.user_db_session \
                              .query(Document) \
                              .filter(Document.id == item.id) \
                              .update(values)

                self._app_data.user_db_session.commit()

            except Exception as e:
                logger.error(e)

        self.model.layoutChanged.emit()

    def _add_document_dialog(self):
        file_paths, _ = QFileDialog.getOpenFileNames(
            None,
            "Add Documents to the Library...",
            "",
            "PDF or Epub Files (*.pdf *.epub)")

        for p in file_paths:
            self.add_document(p)

    def _remove_document_dialog(self):
        doc = self.get_selected_document()
        if not doc:
            return

        reply = QMessageBox.question(self,
                                     'Remove Document...',
                                     'Remove this item from the Library? (Files are NOT deleted.)',
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)
        if reply == QMessageBox.Yes:
            self.remove_selected_document()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Add \
            .triggered.connect(partial(self._add_document_dialog))

        self.action_Remove \
            .triggered.connect(partial(self._remove_document_dialog))

        self.search_button.clicked.connect(partial(self._handle_query))
        self.search_input.textChanged.connect(partial(self._handle_query))

        self.sel_model.selectionChanged.connect(partial(self._handle_document_select))
