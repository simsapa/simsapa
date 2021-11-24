import sys
import logging as _logging
from functools import partial
import pyperclip

from PyQt5.QtCore import Qt
from PyQt5.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QTextBrowser,
                             QPushButton, QLabel, QMainWindow, QSizePolicy)

logger = _logging.getLogger(__name__)


class ErrorMessageWindow(QMainWindow):
    def __init__(self, user_message=None, debug_info=None, status=None) -> None:
        super().__init__()
        self.setWindowTitle("Application Error")
        self.setFixedSize(500, 500)
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)

        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        self._msg = QLabel()
        self._msg.setWordWrap(True)

        if user_message:
            self._msg.setText(user_message)
        else:
            self._msg.setText("<p>The application encountered and error.</p>")

        self._msg.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self._layout.addWidget(self._msg)

        if debug_info:
            debug_info = f"```\n{debug_info}\n```"
            self._info_help = QLabel()
            self._info_help.setText("""
<p>
  You can submit an issue report at:<br>
  <a href="https://github.com/simsapa/simsapa/issues">https://github.com/simsapa/simsapa/issues</a>
</p>
<p>
  Or send an email to <a href="mailto:profound.labs@gmail.com">profound.labs@gmail.com</a>
</p>
<p>
  Please copy the error mesage in your bug report:
</p>""")

            self._info_help.setAlignment(Qt.AlignmentFlag.AlignCenter)
            self._info_help.setOpenExternalLinks(True)
            self._layout.addWidget(self._info_help)

            self._copy_button = QPushButton("Copy")
            self._copy_button.setFixedSize(100, 30)

            self._copy_button.clicked.connect(partial(self._copy, debug_info))

            layout = QHBoxLayout()
            layout.addWidget(self._copy_button)

            self._layout.addLayout(layout)

            self._debug_info = QTextBrowser()
            self._debug_info.setText(debug_info)
            self._debug_info.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
            self._layout.addWidget(self._debug_info)

        buttons_layout = QHBoxLayout()
        buttons_layout.setContentsMargins(0, 20, 0, 10)

        self._continue_button = QPushButton("Continue")
        self._continue_button.setFixedSize(100, 30)
        self._quit_button = QPushButton("Quit")
        self._quit_button.setFixedSize(100, 30)

        self._continue_button.clicked.connect(partial(self.close))
        self._quit_button.clicked.connect(partial(self._close_and_quit))

        buttons_layout.addWidget(self._continue_button)
        buttons_layout.addWidget(self._quit_button)

        self._layout.addLayout(buttons_layout)

    def _copy(self, text):
        pyperclip.copy(text)

    def _close_and_quit(self):
        self.close()
        status = 1
        logger.error(f"Exiting with status {status}.")
        sys.exit(status)
