from functools import partial
from typing import List
import logging as _logging

from PyQt5.QtGui import QImage, QPixmap
from PyQt5.QtWidgets import (QLabel, QMainWindow)  # type: ignore
import fitz  # type: ignore

from ..app.types import AppData  # type: ignore
from ..assets.ui.document_reader_window_ui import Ui_DocumentReaderWindow  # type: ignore

logger = _logging.getLogger(__name__)


class DocumentReaderWindow(QMainWindow, Ui_DocumentReaderWindow):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data
        self._current_idx: int = 0

        self._ui_setup()

        self.statusbar.showMessage("Ready", 3000)

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)

        self._doc = None

        self._zoom = 1.5
        self._matrix = fitz.Matrix(self._zoom, self._zoom)

    def open_doc(self, path):
        self._doc = fitz.open(path)
        self.doc_go_to_page(1)

    def doc_show_current(self):
        self.doc_go_to_page(self._current_idx + 1)

    def doc_go_to_page(self, page: int):
        logger.info(f"doc_go_to_page({page})")

        if self._doc is None or page < 1 or page > len(self._doc):
            return

        self.page_current_of_total.setText(f"{page} of {len(self._doc)}")

        self._current_idx = page - 1

        pix = self._doc[self._current_idx].get_pixmap(matrix=self._matrix, alpha=False)

        img = QImage(pix.tobytes("ppm"), pix.width, pix.height, pix.stride, QImage.Format.Format_RGB888)
        self.content_page.setPixmap(QPixmap.fromImage(img))

    def _previous_page(self):
        if self._doc and self._current_idx > 0:
            self._current_idx += -1
            self.current_page_input.setValue(self._current_idx + 1)

    def _next_page(self):
        if self._doc and self._current_idx < len(self._doc) - 1:
            self._current_idx += 1
            self.current_page_input.setValue(self._current_idx + 1)

    def _beginning(self):
        if self._doc and self._current_idx != 0:
            self._current_idx = 0
            self.current_page_input.setValue(self._current_idx + 1)

    def _end(self):
        if self._doc and self._current_idx != len(self._doc):
            self._current_idx = len(self._doc) - 1
            self.current_page_input.setValue(self._current_idx + 1)

    def _go_to_page_dialog(self):
        pass

    def _go_to_page_input(self):
        n = self.current_page_input.value()
        self.doc_go_to_page(n)

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
