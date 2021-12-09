import sys
import os
import traceback
import logging as _logging
import logging.config
import yaml
import threading

from PyQt5.QtGui import QIcon
from PyQt5.QtWidgets import (QApplication, QSystemTrayIcon, QMenu, QAction)

from simsapa import APP_DB_PATH
from .app.types import AppData, create_app_dirs
from .app.windows import AppWindows
from .app.api import start_server, find_available_port
from .layouts.download_appdata import DownloadAppdataWindow
from .layouts.error_message import ErrorMessageWindow

from simsapa.assets import icons_rc  # noqa: F401

logger = _logging.getLogger(__name__)

if os.path.exists("logging.yaml"):
    with open("logging.yaml", 'r') as f:
        config = yaml.safe_load(f.read())
        _logging.config.dictConfig(config) # type: ignore


def excepthook(exc_type, exc_value, exc_tb):
    tb = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
    logger.error("Error:\n", tb)
    w = ErrorMessageWindow(user_message=None, debug_info=tb)
    w.show()


sys.excepthook = excepthook


def start():
    logger.info("start()")

    create_app_dirs()

    if not APP_DB_PATH.exists():
        dl_app = QApplication(sys.argv)
        w = DownloadAppdataWindow()
        w.show()
        status = dl_app.exec_()
        logger.info(f"main() Exiting with status {status}.")
        sys.exit(status)

    app = QApplication(sys.argv)

    port = find_available_port()
    daemon = threading.Thread(name='daemon_server',
                              target=start_server,
                              args=(port,))
    daemon.setDaemon(True)
    daemon.start()

    app_data = AppData(app_clipboard=app.clipboard(), api_port=port)

    # === Create systray ===

    app.setQuitOnLastWindowClosed(False)

    tray = QSystemTrayIcon(QIcon(":simsapa-tray"))
    tray.setVisible(True)

    menu = QMenu()

    ac = QAction(QIcon(":close"), "Quit")
    ac.triggered.connect(app.quit)
    menu.addAction(ac)

    tray.setContextMenu(menu)

    # === Create first window ===

    app_windows = AppWindows(app, app_data)
    app_windows._new_sutta_search_window()

    status = app.exec_()
    logger.info(f"main() Exiting with status {status}.")
    sys.exit(status)
