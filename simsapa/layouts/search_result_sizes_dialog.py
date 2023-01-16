from PyQt6.QtCore import pyqtSignal
from PyQt6.QtWidgets import QDialog, QHBoxLayout, QLabel, QPushButton, QSpinBox, QVBoxLayout

from ..app.types import AppData, SearchResultSizes, default_search_result_sizes


class SearchResultSizesDialog(QDialog):

    accepted = pyqtSignal() # type: ignore
    _layout: QVBoxLayout

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)

        self.setWindowTitle("Search Result Sizes")

        self._app_data = app_data

        self.sizes = self._app_data.app_settings.get('search_result_sizes', default_search_result_sizes())

        self._ui_setup()
        self._connect_signals()

    def _ui_setup(self):
        self._layout = QVBoxLayout()

        self.hbox1 = QHBoxLayout()

        self.header_height_label = QLabel("Header height:")
        self.hbox1.addWidget(self.header_height_label)

        self.header_height_input = QSpinBox(self)
        self.header_height_input.setMinimum(1)
        self.header_height_input.setValue(self.sizes['header_height'])
        self.hbox1.addWidget(self.header_height_input)

        self._layout.addLayout(self.hbox1)

        self.hbox2 = QHBoxLayout()

        self.snippet_length_label = QLabel("Snippet length:")
        self.hbox2.addWidget(self.snippet_length_label)

        self.snippet_length_input = QSpinBox(self)
        self.snippet_length_input.setMinimum(1)
        self.snippet_length_input.setMaximum(1000)
        self.snippet_length_input.setValue(self.sizes['snippet_length'])
        self.hbox2.addWidget(self.snippet_length_input)

        self._layout.addLayout(self.hbox2)

        self.hbox3 = QHBoxLayout()

        self.snippet_font_size_label = QLabel("Snippet font size:")
        self.hbox3.addWidget(self.snippet_font_size_label)

        self.snippet_font_size_input = QSpinBox(self)
        self.snippet_font_size_input.setMinimum(1)
        self.snippet_font_size_input.setValue(self.sizes['snippet_font_size'])
        self.hbox3.addWidget(self.snippet_font_size_input)

        self._layout.addLayout(self.hbox3)

        self.hbox4 = QHBoxLayout()

        self.snippet_min_height_label = QLabel("Snippet min height:")
        self.hbox4.addWidget(self.snippet_min_height_label)

        self.snippet_min_height_input = QSpinBox(self)
        self.snippet_min_height_input.setMinimum(1)
        self.snippet_min_height_input.setMaximum(200)
        self.snippet_min_height_input.setValue(self.sizes['snippet_min_height'])
        self.hbox4.addWidget(self.snippet_min_height_input)

        self._layout.addLayout(self.hbox4)

        self.hbox5 = QHBoxLayout()

        self.snippet_max_height_label = QLabel("Snippet max height:")
        self.hbox5.addWidget(self.snippet_max_height_label)

        self.snippet_max_height_input = QSpinBox(self)
        self.snippet_max_height_input.setMinimum(1)
        self.snippet_max_height_input.setMaximum(200)
        self.snippet_max_height_input.setValue(self.sizes['snippet_max_height'])
        self.hbox5.addWidget(self.snippet_max_height_input)

        self._layout.addLayout(self.hbox5)

        self.buttons_layout = QHBoxLayout()

        self.save_button = QPushButton("Save")
        self.buttons_layout.addWidget(self.save_button)

        self.cancel_button = QPushButton("Cancel")
        self.buttons_layout.addWidget(self.cancel_button)

        self._layout.addLayout(self.buttons_layout)

        self.setLayout(self._layout)

    def save_pressed(self):
        cur_sizes = SearchResultSizes(
            header_height = self.header_height_input.value(),
            snippet_length = self.snippet_length_input.value(),
            snippet_font_size = self.snippet_font_size_input.value(),
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
