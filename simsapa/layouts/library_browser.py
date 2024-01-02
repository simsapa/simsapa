import os.path
from functools import partial
from typing import List, Optional
from sqlalchemy.sql import func

from PyQt6.QtCore import QAbstractListModel, Qt
from PyQt6.QtGui import QIcon, QImage, QPixmap
from PyQt6.QtWidgets import (QFileDialog, QMainWindow,
                             QMessageBox)

from simsapa import DbSchemaName, logger
from ..app.file_doc import FileDoc
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import UDocument
from ..app.app_data import AppData
from ..assets.ui.library_browser_window_ui import Ui_LibraryBrowserWindow


class DocumentListModel(QAbstractListModel):
    def __init__(self, *args, docs=None, **kwargs):
        super(DocumentListModel, self).__init__(*args, **kwargs)
        self.docs = docs or []

    def data(self, index, role):
        if role == Qt.ItemDataRole.DisplayRole:
            filepath = self.docs[index.row()].filepath
            name = os.path.basename(filepath)
            return name

        if role == Qt.ItemDataRole.DecorationRole:
            filepath = self.docs[index.row()].filepath
            if not os.path.exists(filepath):
                return QIcon(":close")

    def rowCount(self, __index__):
        return len(self.docs)


class LibraryBrowserWindow(QMainWindow, Ui_LibraryBrowserWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.model = DocumentListModel(docs=self._get_all_documents())

        self.documents_list.setModel(self.model)
        self.sel_model = self.documents_list.selectionModel()

        self._setup_ui()
        self._connect_signals()

    def _setup_ui(self):
        self.search_input.setFocus()
        self._show_document_clear()

    def _get_all_documents(self) -> List[UDocument]:
        results = []

        res = self._app_data.db_session.query(Am.Document).all()
        results.extend(res)
        res = self._app_data.db_session.query(Um.Document).all()
        results.extend(res)

        return results

    def _handle_query(self):
        pass

    def get_selected_document(self) -> Optional[UDocument]:
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
        doc = self.model.docs[item.row()]
        doc_id = doc.id
        schema = doc.metadata.schema

        del self.model.docs[item.row()]
        self.model.layoutChanged.emit()
        self.documents_list.clearSelection()
        self._show_document_clear()

        # Remove from database

        if schema == DbSchemaName.AppData.value:
            db_item = self._app_data.db_session \
                                    .query(Am.Document) \
                                    .filter(Am.Document.id == doc_id) \
                                    .first()

        elif schema == DbSchemaName.UserData.value:
            db_item = self._app_data.db_session \
                                    .query(Um.Document) \
                                    .filter(Um.Document.id == doc_id) \
                                    .first()

        else:
            raise Exception("Only appdata and userdata schema are allowed.")

        self._app_data.db_session.delete(db_item)
        self._app_data.db_session.commit()

    def _handle_document_select(self):
        doc = self.get_selected_document()
        if doc:
            self._show_document(doc)

    def _show_document_clear(self):
        self.doc_title.clear()
        self.doc_author.clear()
        self.doc_cover.clear()

    def _show_document(self, doc: UDocument):
        self.doc_title.setText(doc.title)
        self.doc_author.setText(doc.author)

        if doc.cover_data:
            img = QImage(
                doc.cover_data,
                doc.cover_width,
                doc.cover_height,
                doc.cover_stride,
                QImage.Format.Format_RGB888
            )
            self.doc_cover.setPixmap(QPixmap.fromImage(img))

    def _documents_search_query(self, query: str):
        results = []

        res = self._app_data.db_session \
                            .query(Am.Document) \
                            .filter(Am.Document.filepath.like(f"%{query}%")) \
                            .all()
        results.extend(res)

        res = self._app_data.db_session \
                            .query(Um.Document) \
                            .filter(Um.Document.filepath.like(f"%{query}%")) \
                            .all()
        results.extend(res)

        return results

    def add_document(self, filepath):
        if not os.path.exists(filepath):
            logger.error(f"File doesn't exist: {filepath}")
            return

        file_doc = FileDoc(filepath)

        title = file_doc.title or "Unknown"

        author = file_doc.author or "Unknown"

        page_image = file_doc.page_image(0, 0.7)

        # Insert or update database record

        item = self._app_data.db_session \
                             .query(Um.Document) \
                             .filter(Um.Document.filepath == filepath) \
                             .first()
        if item is None:
            logger.info(f"Add new: {filepath}")

            if page_image:
                db_doc = Um.Document(
                    filepath=filepath,
                    title=title,
                    author=author,
                    cover_data=page_image.image_bytes,
                    cover_width=page_image.width,
                    cover_height=page_image.height,
                    cover_stride=page_image.stride,
                    created_at=func.now(),
                )
            else:
                db_doc = Um.Document(
                    filepath=filepath,
                    title=title,
                    author=author,
                    created_at=func.now(),
                )

            try:
                self._app_data.db_session.add(db_doc)
                self._app_data.db_session.commit()

                # Add to model
                self.model.docs.append(db_doc)

            except Exception as e:
                logger.error(e)

        else:
            logger.info(f"Update: {item.filepath}")

            if page_image:
                values = {
                    'filepath': filepath,
                    'title': title,
                    'author': author,
                    'cover_data': page_image.image_bytes,
                    'cover_width': page_image.width,
                    'cover_height': page_image.height,
                    'cover_stride': page_image.stride,
                    'updated_at': func.now(),
                }
            else:
                values = {
                    'filepath': filepath,
                    'title': title,
                    'author': author,
                    'updated_at': func.now(),
                }

            try:
                self._app_data.db_session \
                              .query(Um.Document) \
                              .filter(Um.Document.id == item.id) \
                              .update(values)

                self._app_data.db_session.commit()

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
                                     QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                                     QMessageBox.StandardButton.No)
        if reply == QMessageBox.StandardButton.Yes:
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
