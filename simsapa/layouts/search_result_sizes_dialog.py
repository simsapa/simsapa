from PyQt6.QtCore import pyqtSignal
from PyQt6.QtWidgets import QDialog, QHBoxLayout, QLabel, QLineEdit, QPushButton, QSpinBox, QVBoxLayout

from simsapa.app.app_data import AppData
from simsapa.layouts.gui_types import SearchResultSizes, default_search_result_sizes


class SearchResultSizesDialog(QDialog):

    accepted = pyqtSignal() # type: ignore
    _layout: QVBoxLayout

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)

        self.setWindowTitle("Search Result Sizes")

        self._app_data = app_data

        self.sizes = self._app_data.app_settings.get('search_result_sizes', default_search_result_sizes())

        self._setup_ui()
        self._connect_signals()

    def _setup_ui(self):
        self._layout = QVBoxLayout()

        # === Font family ===

        box = QHBoxLayout()
        box.addWidget(QLabel("Font family:"))

        self.font_family_input = QLineEdit()
        self.font_family_input.setText(self.sizes['font_family'])
        box.addWidget(self.font_family_input)

        self._layout.addLayout(box)

        # === Font size ===

        box = QHBoxLayout()
        box.addWidget(QLabel("Font size:"))

        self.font_size_input = QSpinBox(self)
        self.font_size_input.setMinimum(1)
        self.font_size_input.setValue(self.sizes['font_size'])
        box.addWidget(self.font_size_input)

        self._layout.addLayout(box)

        # === Vertical margin ===

        box = QHBoxLayout()
        box.addWidget(QLabel("Vertical margin:"))

        self.vertical_margin_input = QSpinBox(self)
        self.vertical_margin_input.setMinimum(0)
        self.vertical_margin_input.setValue(self.sizes['vertical_margin'])
        box.addWidget(self.vertical_margin_input)

        self._layout.addLayout(box)

        # === Header height ===

        box = QHBoxLayout()
        box.addWidget(QLabel("Header height:"))

        self.header_height_input = QSpinBox(self)
        self.header_height_input.setMinimum(1)
        self.header_height_input.setValue(self.sizes['header_height'])
        box.addWidget(self.header_height_input)

        self._layout.addLayout(box)

        # === Snippet length ===

        box = QHBoxLayout()
        box.addWidget(QLabel("Snippet length:"))

        self.snippet_length_input = QSpinBox(self)
        self.snippet_length_input.setMinimum(1)
        self.snippet_length_input.setMaximum(1000)
        self.snippet_length_input.setValue(self.sizes['snippet_length'])
        box.addWidget(self.snippet_length_input)

        self._layout.addLayout(box)

        # === Snippet min height ===

        box = QHBoxLayout()
        box.addWidget(QLabel("Snippet min height:"))

        self.snippet_min_height_input = QSpinBox(self)
        self.snippet_min_height_input.setMinimum(1)
        self.snippet_min_height_input.setMaximum(200)
        self.snippet_min_height_input.setValue(self.sizes['snippet_min_height'])
        box.addWidget(self.snippet_min_height_input)

        self._layout.addLayout(box)

        # === Snippet max height ===

        box = QHBoxLayout()
        box.addWidget(QLabel("Snippet max height:"))

        self.snippet_max_height_input = QSpinBox(self)
        self.snippet_max_height_input.setMinimum(1)
        self.snippet_max_height_input.setMaximum(200)
        self.snippet_max_height_input.setValue(self.sizes['snippet_max_height'])
        box.addWidget(self.snippet_max_height_input)

        self._layout.addLayout(box)

        # === Buttons ===

        self.buttons_layout = QHBoxLayout()

        self.save_button = QPushButton("Save")
        self.buttons_layout.addWidget(self.save_button)

        self.cancel_button = QPushButton("Cancel")
        self.buttons_layout.addWidget(self.cancel_button)

        self._layout.addLayout(self.buttons_layout)

        self.setLayout(self._layout)

    def save_pressed(self):
        cur_sizes = SearchResultSizes(
            font_family = self.font_family_input.text(),
            font_size = self.font_size_input.value(),
            vertical_margin = self.vertical_margin_input.value(),
            header_height = self.header_height_input.value(),
            snippet_length = self.snippet_length_input.value(),
            snippet_min_height = self.snippet_min_height_input.value(),
            snippet_max_height = self.snippet_max_height_input.value(),
        )
        self._app_data.app_settings['search_result_sizes'] = cur_sizes
        self._app_data._save_app_settings()
        self.accept()

    def cancel_pressed(self):
        self.reject()

    def _connect_signals(self):
        self.save_button.clicked.connect(self.save_pressed)

        self.cancel_button.clicked.connect(self.cancel_pressed)
