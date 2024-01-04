from typing import List

from PyQt6 import QtWidgets
from PyQt6.QtGui import QColor
from PyQt6.QtWidgets import QFrame, QLabel, QListWidget, QListWidgetItem, QVBoxLayout, QWidget
from simsapa import IS_MAC

from simsapa.app.app_data import AppData
from simsapa.app.search.helpers import dpd_deconstructor_query
from simsapa.layouts.gui_types import QExpanding, QFixed, QMinimum, SearchResultSizes, default_search_result_sizes

class ResultWidget(QWidget):
    def __init__(self, sizes: SearchResultSizes, label_content: str, parent=None):
        super(ResultWidget, self).__init__(parent)

        self.layout: QVBoxLayout = QVBoxLayout()
        self.layout.setContentsMargins(0, 0, 0, 0)
        self.setLayout(self.layout)

        self.label = QLabel(label_content)
        self.label.setWordWrap(False)
        self.label.setSizePolicy(QExpanding, QFixed)
        self.label.setFixedHeight(sizes['header_height'])
        self.label.setContentsMargins(0, 0, 0, 0)

        if IS_MAC:
            self.label.setStyleSheet(f"font-family: Helvetica; font-size: {sizes['snippet_font_size']};")
        else:
            self.label.setStyleSheet(f"font-family: DejaVu Sans; font-size: {sizes['snippet_font_size']};")

        self.layout.addWidget(self.label)

class HasDeconstructorList:
    _app_data: AppData
    features: List[str]
    deconstructor_frame: QFrame
    deconstructor_layout: QVBoxLayout
    deconstructor_list: QListWidget

    def init_deconstructor_list(self):
        self.features.append('deconstructor_list')

        self._setup_deconstructor_ui()

    def _setup_deconstructor_ui(self):
        self.deconstructor_layout = QVBoxLayout()
        self.deconstructor_frame.setLayout(self.deconstructor_layout)
        self.deconstructor_frame.setSizePolicy(QExpanding, QMinimum)
        self.deconstructor_frame.setVisible(False)

        self.deconstructor_layout.addWidget(QLabel("<b>Deconstructor Results:</b>"))

        self.deconstructor_list = QListWidget()
        self.deconstructor_list.setUniformItemSizes(True)
        self.deconstructor_list.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        self.deconstructor_list.setSizePolicy(QExpanding, QMinimum)

        self.deconstructor_layout.addWidget(self.deconstructor_list)

    def render_deconstructor_list_for_query(self, query_text: str):
        self.deconstructor_list.clear()

        r = dpd_deconstructor_query(self._app_data.db_session, query_text)

        if r is None:
            self.deconstructor_frame.setVisible(False)
            return

        self.deconstructor_frame.setVisible(True)

        sizes = self._app_data.app_settings.get('search_result_sizes', default_search_result_sizes())

        result_wigets = []

        for variation in r.headwords:
            content = "<b>" + " + ".join(variation) + "</b>"
            result_wigets.append(ResultWidget(sizes, content))

            for word in variation:
                result_wigets.append(ResultWidget(sizes, word))

        for w in result_wigets:
            item = QListWidgetItem(self.deconstructor_list)
            item.setSizeHint(w.sizeHint())
            item.setBackground(QColor("#ffffff"))

            self.deconstructor_list.addItem(item)
            self.deconstructor_list.setItemWidget(item, w)
