from functools import partial
from typing import Optional
import logging as _logging
from pathlib import Path
import queue
import json

from PyQt5.QtCore import Qt, QPoint, QRect, QUrl, QTimer
from PyQt5.QtGui import QImage, QPixmap, QCloseEvent
from PyQt5.QtWidgets import (QLabel, QMainWindow, QFileDialog, QInputDialog, QAction)

from simsapa import ASSETS_DIR, APP_QUEUES

from ..app.file_doc import FileDoc, PageImage
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import AppData, USutta
from ..assets.ui.document_reader_window_ui import Ui_DocumentReaderWindow
from .memos_sidebar import HasMemosSidebar
from .links_sidebar import HasLinksSidebar

logger = _logging.getLogger(__name__)


class DocumentReaderWindow(QMainWindow, Ui_DocumentReaderWindow, HasLinksSidebar, HasMemosSidebar):
    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self.features = []
        self._app_data: AppData = app_data

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.graph_path: Path = ASSETS_DIR.joinpath(f"{self.queue_id}.html")

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(300)

        self._ui_setup()
        self._connect_signals()

        self.init_memos_sidebar()
        self.init_links_sidebar()

        self.statusbar.showMessage("Ready", 3000)

    def closeEvent(self, event: QCloseEvent):
        if self.queue_id in APP_QUEUES.keys():
            del APP_QUEUES[self.queue_id]

        if self.graph_path.exists():
            self.graph_path.unlink()

        event.accept()

    def handle_messages(self):
        if self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                data = json.loads(s)
                if data['action'] == 'show_sutta':
                    self._show_sutta_from_message(data['arg'])

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _ui_setup(self):
        self.status_msg = QLabel("")
        self.statusbar.addPermanentWidget(self.status_msg)

        self.links_tab_idx = 0
        self.memos_tab_idx = 1

        self.file_doc: Optional[FileDoc] = None
        self.db_doc: Optional[Um.Document] = None
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

        if self.db_doc is not None and self.file_doc is not None:
            self.update_memos_list_for_document(self.file_doc, self.db_doc)

        self.file_doc = FileDoc(path)
        self.doc_go_to_page(1)

    def doc_show_current(self):
        if self.file_doc is not None:
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
            self.file_doc.go_to_page(page)

            page_img = self.file_doc.current_page_image
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
            self.update_memos_list_for_document(self.file_doc, self.db_doc)
            self.show_network_graph()

    def show_network_graph(self):
        if self.file_doc is None or self.db_doc is None:
            return

        self.generate_graph_for_document(self.file_doc, self.db_doc, self.queue_id, self.graph_path, self.messages_url)
        self.content_graph.load(QUrl('file://' + str(self.graph_path.absolute())))

    def _show_sutta_from_message(self, info):
        sutta: Optional[USutta] = None

        if info['table'] == 'appdata.suttas':
            sutta = self._app_data.db_session \
                .query(Am.Sutta) \
                .filter(Am.Sutta.id == info['id']) \
                .first()

        elif info['table'] == 'userdata.suttas':
            sutta = self._app_data.db_session \
                .query(Um.Sutta) \
                .filter(Um.Sutta.id == info['id']) \
                .first()

        self._app_data.sutta_to_open = sutta
        self.action_Sutta_Search.activate(QAction.ActionEvent.Trigger)

    def _select_start(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.selecting = True
            self.select_start_point = QPoint(event.pos())

    def _select_move(self, event):
        if not self.selecting:
            return
        if self.select_start_point is None:
            return
        self.select_end_point = QPoint(event.pos())
        self.select_rectangle = QRect(self.select_start_point, self.select_end_point).normalized()
        if self.file_doc and self.select_rectangle:
            self.selected_text = self.file_doc.get_selection_text(self.select_rectangle)
            self.file_doc.paint_selection_on_current(self.select_rectangle)
            self.doc_show_current()

    def _select_release(self, event):
        self._select_move(event)
        self.selecting = False

    def _previous_page(self):
        if not self.file_doc:
            return
        page_nr = self.file_doc.current_page_number() - 1
        if page_nr > 0:
            self.file_doc.go_to_page(page_nr)
            self._upd_current_page_input(page_nr)

    def _next_page(self):
        if not self.file_doc:
            return
        page_nr = self.file_doc.current_page_number() + 1
        if page_nr <= self.file_doc.number_of_pages():
            self.file_doc.go_to_page(page_nr)
            self._upd_current_page_input(page_nr)

    def _beginning(self):
        if self.file_doc and self.file_doc.current_page_number() != 1:
            self.file_doc.go_to_page(1)
            self._upd_current_page_input(1)

    def _end(self):
        if self.file_doc and self.file_doc.current_page_number() != self.file_doc.number_of_pages():
            self.file_doc.go_to_page(self.file_doc.number_of_pages())
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

    def _zoom_out(self):
        if self.file_doc is None:
            return
        if self.file_doc._zoom is None:
            return
        self.file_doc.set_zoom(self.file_doc._zoom - 0.1)
        self.doc_show_current()

    def _zoom_in(self):
        if self.file_doc is None:
            return
        if self.file_doc._zoom is None:
            return
        self.file_doc.set_zoom(self.file_doc._zoom + 0.1)
        self.doc_show_current()

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

        self.action_Zoom_Out \
            .triggered.connect(partial(self._zoom_out))
        self.action_Zoom_In \
            .triggered.connect(partial(self._zoom_in))

        self.current_page_input \
            .valueChanged.connect(partial(self._go_to_page_input))

        self.add_memo_button \
            .clicked.connect(partial(self.add_memo_for_document))
