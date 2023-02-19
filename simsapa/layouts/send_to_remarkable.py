from PyQt6 import QtWidgets

from PyQt6.QtWidgets import (QMainWindow, QPushButton, QVBoxLayout)

from simsapa import logger
# from ..app.db import appdata_models as Am
# from ..app.db import userdata_models as Um

from ..app.types import AppData

class SendToRemarkableWindow(QMainWindow):

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        logger.info("SendToRemarkableWindow()")

        self._app_data: AppData = app_data

        self._ui_setup()
        self._connect_signals()

    def _ui_setup(self):
        self.setWindowTitle("Send to reMarkable")
        self.setFixedSize(850, 650)

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        self.close_button = QPushButton("Close")
        self._layout.addWidget(self.close_button)

    def _connect_signals(self):
        self.close_button.clicked.connect(self.close)
