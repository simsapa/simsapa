from pathlib import Path
from functools import partial
from typing import Optional

from PyQt6 import QtWidgets, QtCore, QtGui
from PyQt6.QtCore import QItemSelection, QItemSelectionModel, QModelIndex, QObject, QRunnable, QSize, QThreadPool, pyqtSignal, pyqtSlot
from PyQt6.QtGui import QAction, QStandardItem, QStandardItemModel
from PyQt6.QtWidgets import QHBoxLayout, QMenu, QMenuBar, QPushButton, QSpacerItem, QSplitter, QTabWidget, QTreeView, QVBoxLayout, QWidget

from simsapa import logger
from simsapa.layouts.html_content import html_page
from simsapa.layouts.sutta_search_window_state import SuttaSearchWindowState

from ..app.types import AppData, QExpanding, QMinimum, SuttaSearchWindowInterface

class EbookReaderWindow(SuttaSearchWindowInterface):

    def __init__(self, app_data: AppData, parent = None) -> None:
        super().__init__(parent)
        logger.info("EbookReaderWindow()")

        self._app_data: AppData = app_data

        self.extract_epub_worker: Optional[ExtractEbookWorker] = None

        self.thread_pool = QThreadPool()

        self.toc_panel_visible = True
        self.sutta_panel_visible = True

        self._setup_ui()
        self._connect_signals()
        self._update_vert_splitter_widths()

    def _setup_ui(self):
        self.setWindowTitle("Ebook Reader")
        self.resize(1068, 625)
        self.setBaseSize(QtCore.QSize(800, 600))

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        # top buttons

        self._top_buttons_box = QHBoxLayout()
        self._layout.addLayout(self._top_buttons_box)

        self.toggle_toc_panel_btn = QPushButton()

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/angles-left"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.toggle_toc_panel_btn.setIcon(icon)
        self.toggle_toc_panel_btn.setMinimumSize(QSize(40, 40))
        self.toggle_toc_panel_btn.setToolTip("Toggle Table of Contents")

        self._top_buttons_box.addWidget(self.toggle_toc_panel_btn)

        self._top_buttons_box.addItem(QSpacerItem(0, 0, QExpanding, QMinimum))

        self.toggle_sutta_panel_btn = QPushButton()

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/angles-right"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.toggle_sutta_panel_btn.setIcon(icon)
        self.toggle_sutta_panel_btn.setMinimumSize(QSize(40, 40))
        self.toggle_sutta_panel_btn.setToolTip("Toggle Sutta Panel")

        self._top_buttons_box.addWidget(self.toggle_sutta_panel_btn)

        # horizontal splitter

        self.vert_splitter = QSplitter(self._central_widget)
        self.vert_splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)

        self._layout.addWidget(self.vert_splitter)

        self.toc_panel_widget = QWidget(self.vert_splitter)
        self.toc_panel_layout = QVBoxLayout(self.toc_panel_widget)
        self.toc_panel_layout.setContentsMargins(0, 0, 0, 0)

        self.reading_panel_widget = QWidget(self.vert_splitter)
        self.reading_panel_layout = QVBoxLayout(self.reading_panel_widget)
        self.reading_panel_layout.setContentsMargins(0, 0, 0, 0)

        self.sutta_panel_widget = QWidget(self.vert_splitter)
        self.sutta_panel_layout = QVBoxLayout(self.sutta_panel_widget)
        self.sutta_panel_layout.setContentsMargins(0, 0, 0, 0)

        # TODO Sutta tabs

        self._setup_menubar()
        self._setup_sutta_panel()
        self._setup_reading_panel()
        self._setup_toc_panel()

    def _setup_menubar(self):
        self.menubar = QMenuBar()
        self.setMenuBar(self.menubar)

        self.menu_File = QMenu(self.menubar)
        self.menu_File.setTitle("&File")

        self.menubar.addAction(self.menu_File.menuAction())

        self.action_Close_Window = QAction("&Close Window")
        self.menu_File.addAction(self.action_Close_Window)

    def _setup_toc_panel(self):
        self.toc_tabs = QTabWidget()
        self.toc_panel_layout.addWidget(self.toc_tabs)

        self.toc_tab_widget = QWidget()
        self.toc_tab_layout = QVBoxLayout()
        self.toc_tab_widget.setLayout(self.toc_tab_layout)

        self.toc_tabs.addTab(self.toc_tab_widget, "Contents")

        self._init_toc_tree()

    def _init_toc_tree(self):
        self.toc_tree_view = QTreeView()
        self.toc_tab_layout.addWidget(self.toc_tree_view)

        self.toc_tree_model = QStandardItemModel(0, 1, self)

        self.toc_tree_view.setHeaderHidden(True)
        self.toc_tree_view.setRootIsDecorated(True)

        self.toc_tree_view.setModel(self.toc_tree_model)

        self._create_toc_tree_items(self.toc_tree_model)

        # Select the first item when opening the window.
        idx = self.toc_tree_model.index(0, 0)
        self.toc_tree_view.selectionModel() \
                          .select(idx,
                                  QItemSelectionModel.SelectionFlag.ClearAndSelect | \
                                  QItemSelectionModel.SelectionFlag.Rows)

        self._handle_toc_tree_clicked(idx)

        self.toc_tree_view.expandAll()

    def _create_toc_tree_items(self, model):
        root_node = model.invisibleRootItem()

        toc = [ "Titlepage", "Preface", "Chapter One", "Chapter Two" ]

        for i in toc:
            item = QStandardItem(i)
            root_node.appendRow(item)

        self.toc_tree_view.resizeColumnToContents(0)

    def _handle_toc_tree_clicked(self, val: QModelIndex):
        item = self.toc_tree_model.itemFromIndex(val) # type: ignore
        if item is not None:
            content = f"<h1>{item.text()}</h1>"
            html = html_page(content, self._app_data.api_url)
            self.reading_state.sutta_tab.set_qwe_html(html)

    def _setup_reading_panel(self):
        self.reading_state = SuttaSearchWindowState(
            app_data = self._app_data,
            parent_window = self,
            searchbar_layout = None,
            sutta_tabs_layout = self.reading_panel_layout,
            tabs_layout = None,
            focus_input = False,
            enable_language_filter = False,
            enable_search_extras = False,
            enable_sidebar = False,
            enable_find_panel = False,
            show_query_results_in_active_tab = False)

    def _setup_sutta_panel(self):
        self.sutta_state = SuttaSearchWindowState(
            app_data = self._app_data,
            parent_window = self,
            searchbar_layout = None,
            sutta_tabs_layout = self.sutta_panel_layout,
            tabs_layout = None,
            focus_input = False,
            enable_language_filter = False,
            enable_search_extras = False,
            enable_sidebar = False,
            enable_find_panel = False,
            show_query_results_in_active_tab = False)

    def start_loading_animation(self):
        pass

    def stop_loading_animation(self):
        pass

    def _update_vert_splitter_widths(self):
        left_toc = 2000 if self.toc_panel_visible else 0
        middle_reading = 2000
        right_suttas = 2000 if self.sutta_panel_visible else 0

        self.vert_splitter.setSizes([left_toc, middle_reading, right_suttas])

    def _toggle_toc_panel(self):
        self.toc_panel_visible = not self.toc_panel_visible
        self._update_vert_splitter_widths()

    def _toggle_sutta_panel(self):
        self.sutta_panel_visible = not self.sutta_panel_visible
        self._update_vert_splitter_widths()

    def _handle_selection_changed(self, selected: QItemSelection, _: QItemSelection):
        indexes = selected.indexes()
        if len(indexes) > 0:
            self._handle_toc_tree_clicked(indexes[0])

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self._handle_close))

        self.toggle_toc_panel_btn.clicked.connect(partial(self._toggle_toc_panel))
        self.toggle_sutta_panel_btn.clicked.connect(partial(self._toggle_sutta_panel))

        self.toc_tree_view.selectionModel().selectionChanged.connect(partial(self._handle_selection_changed))

class ExtractEbookWorkerSignals(QObject):
    error = pyqtSignal(str)
    finished = pyqtSignal()

class ExtractEbookWorker(QRunnable):
    signals: ExtractEbookWorkerSignals

    def __init__(self, ebook_path: Path):
        super().__init__()

        self.signals = ExtractEbookWorkerSignals()

        self.ebook_path = ebook_path

        self.will_emit_finished = True

    @pyqtSlot()
    def run(self):
        try:
            print("extract epub/mobi")

            if self.will_emit_finished:
                self.signals.finished.emit()

        except Exception as e:
            logger.error(e)
            self.signals.error.emit(f"<p>Ebook extract error:</p><p>{e}</p>")
