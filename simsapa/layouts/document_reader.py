from functools import partial
from typing import List, Optional
import logging as _logging
import json

from PyQt5.QtCore import QAbstractListModel, Qt, QItemSelectionModel, QPoint, QRect
from PyQt5.QtGui import QImage, QPixmap
from PyQt5.QtWidgets import (QLabel, QMainWindow, QFileDialog, QInputDialog,
                             QMessageBox)  # type: ignore

from sqlalchemy.sql import func  # type: ignore

from ..app.file_doc import FileDoc, PageImage  # type: ignore
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import (AppData, UMemo)  # type: ignore
from ..assets.ui.document_reader_window_ui import Ui_DocumentReaderWindow  # type: ignore

logger = _logging.getLogger(__name__)


class MemoListModel(QAbstractListModel):
    def __init__(self, *args, memos=None, **kwargs):
        super(MemoListModel, self).__init__(*args, **kwargs)
        self.memos = memos or []

    def data(self, index, role):
        if role == Qt.DisplayRole:
            fields = json.loads(self.memos[index.row()].fields_json)
            text = " ".join(fields.values())
            return text

    def rowCount(self, index):
        if self.memos:
            return len(self.memos)
        else:
            return 0


class DocumentReaderWindow(QMainWindow, Ui_DocumentReaderWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.model = MemoListModel()
        self.memos_list.setModel(self.model)
        self.sel_model = self.memos_list.selectionModel()

        self._ui_setup()

        self._connect_signals()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)
        self._show_memo_clear()

        self.file_doc = None
        self.db_doc = None
        self.content_page_image: Optional[PageImage] = None

        self.selecting = False
        self.selected_text = None
        self.select_start_point: Optional[QPoint] = None
        self.select_end_point: Optional[QPoint] = None
        self.select_rectangle: Optional[QRect] = None

    def open_file_dialog(self):
        file_path, _ = QFileDialog.getOpenFileName(
            None,
            "Open File...",
            "",
            "PDF or Epub Files (*.pdf *.epub)")

        if len(file_path) != 0:
            self.open_doc(file_path)

    def open_doc(self, path):
        self.db_doc = self._app_data.db_session \
                                    .query(Um.Document) \
                                    .filter(Um.Document.filepath == path) \
                                    .first()

        memos = self._get_memos_for_this_page()
        if memos:
            self.model.memos = memos
        else:
            self.model.memos = []

        self.model.layoutChanged.emit()

        self.file_doc = FileDoc(path)
        self.doc_go_to_page(1)

    def doc_show_current(self):
        self.doc_go_to_page(self.file_doc.current_page_number())

    def doc_go_to_page(self, page: int):
        if not self.file_doc:
            return

        # logger.info(f"doc_go_to_page({page})")

        page_count = self.file_doc.number_of_pages()

        if self.file_doc is None or page < 1 or page > page_count:
            return

        self.page_current_of_total.setText(f"{page} of {page_count}")

        if self.file_doc:
            self.file_doc.set_page_number(page)

            page_img = self.file_doc.current_page_image()
            self.content_page_image = page_img

            if page_img:
                img = QImage(page_img.image_bytes,
                             page_img.width,
                             page_img.height,
                             page_img.stride,
                             QImage.Format.Format_RGB888)
                self.content_page.setPixmap(QPixmap.fromImage(img))

                self.content_page.mousePressEvent = self._select_start
                self.content_page.mouseMoveEvent = self._select_move
                self.content_page.mouseReleaseEvent = self._select_release

        if self.db_doc:
            self.memos_list.clearSelection()
            self._show_memo_clear()
            self.model.memos = self._get_memos_for_this_page()
            self.model.layoutChanged.emit()

    def _select_start(self, event):
        if event.button() == Qt.LeftButton:
            self.selecting = True
            self.select_start_point = QPoint(event.pos())

    def _select_move(self, event):
        if not self.selecting:
            return
        self.select_end_point = QPoint(event.pos())
        self.select_rectangle = QRect(self.select_start_point, self.select_end_point).normalized()
        if self.file_doc and self.select_rectangle:
            self.selected_text = self.file_doc.select_highlight_text(self.content_page_image, self.select_rectangle)
            self.doc_show_current()

    def _select_release(self, event):
        self._select_move(event)
        self.selecting = False

    def _previous_page(self):
        if not self.file_doc:
            return
        page_nr = self.file_doc.current_page_number() - 1
        if page_nr > 0:
            self.file_doc.set_page_number(page_nr)
            self._upd_current_page_input(page_nr)

    def _next_page(self):
        if not self.file_doc:
            return
        page_nr = self.file_doc.current_page_number() + 1
        if page_nr <= self.file_doc.number_of_pages():
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
        if not self.file_doc:
            return

        n, ok = QInputDialog.getInt(self, "Go to Page...", "Page:", 1, 1, self.file_doc.number_of_pages(), 1)
        if ok:
            self._upd_current_page_input(n)

    def _go_to_page_input(self):
        n = self.current_page_input.value()
        self.doc_go_to_page(n)

    def _get_memos_for_this_page(self) -> List[UMemo]:
        if self.db_doc is None or self.file_doc is None:
            return

        am_assoc = []
        um_assoc = []

        doc_schema = self.db_doc.metadata.schema

        if doc_schema == 'appdata':

            res = self._app_data.db_session \
                                .query(Am.MemoAssociation) \
                                .filter(
                                    Am.MemoAssociation.associated_table == 'appdata.documents',
                                    Am.MemoAssociation.associated_id == self.db_doc.id,
                                    Am.MemoAssociation.page_number == self.file_doc.current_page_number()) \
                                .all()
            am_assoc.extend(res)

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'appdata.documents',
                                    Um.MemoAssociation.associated_id == self.db_doc.id,
                                    Um.MemoAssociation.page_number == self.file_doc.current_page_number()) \
                                .all()
            um_assoc.extend(res)

        else:

            res = self._app_data.db_session \
                                .query(Um.MemoAssociation) \
                                .filter(
                                    Um.MemoAssociation.associated_table == 'userdata.documents',
                                    Um.MemoAssociation.associated_id == self.db_doc.id,
                                    Um.MemoAssociation.page_number == self.file_doc.current_page_number()) \
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
        self.front_input.clear()
        self.back_input.clear()

    def _show_memo(self, memo: UMemo):
        fields = json.loads(memo.fields_json)
        self.front_input.setPlainText(fields['Front'])
        self.back_input.setPlainText(fields['Back'])

    def clear_memo(self):
        self.sel_model.clearSelection()
        self.front_input.clear()
        self.back_input.clear()

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
        self.memos_list.selectionModel().select(index, QItemSelectionModel.Select)

        self.model.layoutChanged.emit()

    def update_selected_memo_fields(self):
        memo = self.get_selected_memo()
        if memo is None:
            return

        fields = {
            'Front': self.front_input.toPlainText(),
            'Back': self.back_input.toPlainText()
        }

        memo.fields_json = json.dumps(fields)

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

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Previous_Page \
            .triggered.connect(partial(self._previous_page))
        self.action_Next_Page \
            .triggered.connect(partial(self._next_page))

        self.action_Beginning \
            .triggered.connect(partial(self._beginning))
        self.action_End \
            .triggered.connect(partial(self._end))

        self.action_Go_to_Page \
            .triggered.connect(partial(self._go_to_page_dialog))

        self.current_page_input \
            .valueChanged.connect(partial(self._go_to_page_input))

        self.sel_model.selectionChanged.connect(partial(self._handle_memo_select))

        self.add_memo_button \
            .clicked.connect(partial(self.add_memo))

        self.clear_memo_button \
            .clicked.connect(partial(self.clear_memo))

        self.remove_memo_button \
            .clicked.connect(partial(self.remove_memo_dialog))

        self.front_input \
            .textChanged.connect(partial(self.update_selected_memo_fields))

        self.back_input \
            .textChanged.connect(partial(self.update_selected_memo_fields))
