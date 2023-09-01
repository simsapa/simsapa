from functools import partial

from PyQt6.QtCore import Qt
from PyQt6.QtGui import QAction
from PyQt6.QtWidgets import (QWidget, QVBoxLayout, QPushButton, QLabel)

from simsapa.app.app_data import AppData

from simsapa.layouts.asset_management import AssetManagement


class SuttaLanguagesWindow(AssetManagement):
    def __init__(self, app_data: AppData, parent = None) -> None:
        super().__init__(parent=parent)
        self.setWindowTitle("Sutta Languages")
        self.setFixedSize(500, 700)

        self._parent_window = parent
        self._app_data = app_data

        self._init_workers()
        self._setup_ui()

        def _quit_fn():
            if self._parent_window is not None:
                self._parent_window.action_Quit.activate(QAction.ActionEvent.Trigger)

        def _post_hook():
            self.remove_languages_frame.hide()

        self._quit_action = _quit_fn
        self._run_download_post_hook = _post_hook

        self.thread_pool.start(self.releases_worker)

    def _setup_ui(self):
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._central_widget.setLayout(self._layout)

        # Necessary to create it for ui functions, but we don't show it in this window
        self._close_button = QPushButton("Close")
        self._close_button.clicked.connect(partial(self._handle_quit))

        self._setup_animation()

        self._msg = QLabel("<p>Checking for available languages to download...<p>")
        self._msg.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._layout.addWidget(self._msg)

    def _setup_selection(self):
        self._msg.setText("")

        self._setup_progress_bar_frame()

        self._setup_add_languages_frame()
        self._setup_add_buttons()

        # spc3 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        # self._layout.addItem(spc3)

        # spacerItem = QtWidgets.QSpacerItem(20, 0, QSizeMinimum, QSizeExpanding)
        # self._layout.addItem(spacerItem)

        self._setup_remove_languages_frame()
        self._setup_remove_buttons()

    def _cancelled_cleanup_files(self):
        pass
