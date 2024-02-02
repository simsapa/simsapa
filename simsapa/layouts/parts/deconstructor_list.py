from typing import Callable, List, Optional
from functools import partial
from PyQt6 import QtCore

from PyQt6.QtCore import Qt
from PyQt6 import QtWidgets
from PyQt6.QtGui import QAction, QColor
from PyQt6.QtWidgets import QBoxLayout, QCheckBox, QFrame, QLabel, QListWidget, QListWidgetItem, QSplitter, QTabWidget, QVBoxLayout, QWidget

from simsapa.app.app_data import AppData
from simsapa.app.search.helpers import dpd_deconstructor_query
from simsapa.layouts.gui_types import QExpanding, QMinimum, SearchResultSizes, default_search_result_sizes

class ResultWidget(QWidget):
    def __init__(self, sizes: SearchResultSizes, label_content: str, copy_clipboard_fn: Callable[[str], None], parent=None):
        super(ResultWidget, self).__init__(parent)

        self.layout: QVBoxLayout = QVBoxLayout()
        self.layout.setContentsMargins(8, int(sizes['vertical_margin']/2), 8, int(sizes['vertical_margin']/2))
        self.setLayout(self.layout)

        self.label = QLabel(label_content)
        self.label.setStyleSheet(f"color: #000000; background-color: #ffffff; font-family: {sizes['font_family']}; font-size: {sizes['font_size']};")
        self.label.setContentsMargins(0, 0, 0, 0)
        self.label.setWordWrap(False)

        self.setContextMenuPolicy(Qt.ContextMenuPolicy.ActionsContextMenu)
        copy = QAction("Copy", self)
        copy.triggered.connect(partial(copy_clipboard_fn, self.label.text()))
        self.addAction(copy)

        self.layout.addWidget(self.label)

class HasDeconstructorList:
    _app_data: AppData
    features: List[str]
    tabs: QTabWidget
    deconstructor_above_words: bool
    deconstructor_tab_idx: Optional[int]
    show_deconstructor: QCheckBox
    deconstructor_frame: QFrame
    deconstructor_wrap_layout: QBoxLayout
    deconstructor_layout: QVBoxLayout
    deconstructor_list: QListWidget

    def init_deconstructor_list(self, deconstructor_tab_idx: int):
        self.features.append('deconstructor_list')

        if not self.deconstructor_above_words:
            self._setup_deconstructor_tab()
            self.deconstructor_tab_idx = deconstructor_tab_idx
        else:
            self.deconstructor_tab_idx = None

        self._setup_deconstructor_ui()

    def setup_deconstructor_layout(self, central_widget: QWidget, wrap_layout: QBoxLayout):
        self.deconstructor_layout = QVBoxLayout()
        self.deconstructor_layout.setContentsMargins(0, 0, 0, 0)

        if self.deconstructor_above_words:
            self.vert_splitter = QSplitter(central_widget)
            wrap_layout.addWidget(self.vert_splitter)

            self.vert_splitter.setHandleWidth(10)
            self.vert_splitter.setMinimumHeight(200)
            self.vert_splitter.setOrientation(QtCore.Qt.Orientation.Vertical)

            self.deconstructor_wrap_widget = QWidget(self.vert_splitter)
            self.deconstructor_wrap_layout = QVBoxLayout(self.deconstructor_wrap_widget)

        else:
            self.deconstructor_wrap_widget = QWidget()
            self.deconstructor_wrap_layout = QVBoxLayout()

        self.show_deconstructor = QCheckBox("Deconstructor Results (0)")
        self.show_deconstructor.setChecked(True)

        if self.deconstructor_above_words:
            self.deconstructor_wrap_layout.addWidget(self.show_deconstructor)

        self.deconstructor_frame = QFrame(self.deconstructor_wrap_widget)
        self.deconstructor_frame.setFrameShape(QFrame.Shape.NoFrame)
        self.deconstructor_frame.setFrameShadow(QFrame.Shadow.Raised)
        self.deconstructor_frame.setContentsMargins(0, 0, 0, 0)
        self.deconstructor_frame.setLineWidth(0)
        self.deconstructor_frame.setMinimumHeight(100)
        self.deconstructor_frame.setObjectName("DeconstructorFrame")

        if self.deconstructor_above_words:
            self.deconstructor_wrap_layout.addWidget(self.deconstructor_frame)
        else:
            self.deconstructor_wrap_layout.addLayout(self.deconstructor_layout)

    def _setup_deconstructor_tab(self):
        self.tab_deconstructor = QWidget()
        self.tab_deconstructor.setObjectName("DeconstructorTab")

        self.tabs.addTab(self.tab_deconstructor, "Deconstructor")

        self.tab_deconstructor.setLayout(self.deconstructor_wrap_layout)

    def _toggle_deconstructor(self):
        is_on = self.show_deconstructor.isChecked()
        self.deconstructor_frame.setVisible(is_on)

    def _setup_deconstructor_ui(self):
        self.show_deconstructor.setChecked(True)
        self.show_deconstructor.stateChanged.connect(self._toggle_deconstructor)

        if self.deconstructor_above_words:
            self.deconstructor_frame.setLayout(self.deconstructor_layout)
            self.deconstructor_frame.setSizePolicy(QExpanding, QMinimum)
            self.deconstructor_frame.setVisible(False)

        self.deconstructor_list = QListWidget()
        self.deconstructor_list.setStyleSheet("color: #000000; background-color: #ffffff;")
        self.deconstructor_list.setUniformItemSizes(True)
        self.deconstructor_list.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        self.deconstructor_list.setSizePolicy(QExpanding, QMinimum)

        self.deconstructor_layout.addWidget(self.deconstructor_list)

    def render_deconstructor_list_for_query(self, query_text: str):
        self.deconstructor_list.clear()

        r = dpd_deconstructor_query(self._app_data.db_session, query_text)

        if r is None:
            if self.deconstructor_above_words:
                self.deconstructor_frame.setVisible(False)
                self.show_deconstructor.setText("Deconstructor Results (0)")

            elif self.deconstructor_tab_idx is not None:
                self.tabs.setTabText(self.deconstructor_tab_idx, "Deconstructor (0)")

            return

        if self.deconstructor_above_words:
            self.show_deconstructor.setText(f"Deconstructor Results ({len(r.headwords)})")
            is_on = self.show_deconstructor.isChecked()
            if is_on:
                self.deconstructor_frame.setVisible(True)
            else:
                return

        elif self.deconstructor_tab_idx is not None:
            self.tabs.setTabText(self.deconstructor_tab_idx, f"Deconstructor ({len(r.headwords)})")

        sizes = self._app_data.app_settings.get('search_result_sizes', default_search_result_sizes())

        result_wigets = []

        for variation in r.headwords:
            content = " + ".join(variation)
            result_wigets.append(ResultWidget(sizes, content, self._app_data.clipboard_setText))

        for w in result_wigets:
            item = QListWidgetItem(self.deconstructor_list)
            item.setSizeHint(w.sizeHint())
            item.setForeground(QColor("#000000"))
            item.setBackground(QColor("#ffffff"))

            self.deconstructor_list.addItem(item)
            self.deconstructor_list.setItemWidget(item, w)
