#!/usr/bin/env python3

from PyQt5.QtGui import QPixmap
import sys

from PyQt5.QtCore import Qt
from PyQt5.QtWidgets import QApplication, QLabel, QMainWindow, QVBoxLayout, QWidget

from simsapa import ICONS_DIR

class SplashWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("")

        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.FramelessWindowHint) # type: ignore

        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        self._image = QLabel()
        pixmap = QPixmap(str(ICONS_DIR.joinpath('simsapa-logo-horizontal-gray-w600.png')))
        self._image.setPixmap(pixmap)

        self._layout.addWidget(self._image)

        self.setFixedSize(pixmap.width()+20, pixmap.height()+20)

def run_splash():
    app = QApplication(sys.argv)
    w = SplashWindow()
    w.show()
    app.exec()
    # Don't exit, wait for parent process to kill.

if __name__ == "__main__":
    run_splash()
