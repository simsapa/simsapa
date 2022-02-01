from subprocess import Popen
import sys
import os
import traceback
import logging as _logging
import logging.config
from typing import Optional
from PyQt5 import QtCore
import yaml
import threading

from PyQt5.QtGui import QIcon
from PyQt5.QtWidgets import (QApplication, QSystemTrayIcon, QMenu, QAction)

from simsapa import APP_DB_PATH, IS_LINUX, IS_MAC, IS_WINDOWS
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.helpers import write_log
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
    write_log("excepthook()")
    tb = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
    write_log(tb)
    logger.error("Error:\n", tb)
    w = ErrorMessageWindow(user_message=None, debug_info=tb)
    w.show()


sys.excepthook = excepthook


def start(splash_proc: Optional[Popen] = None):
    logger.info("start()")
    write_log("start()", start_new=True)

    create_app_dirs()

    if not APP_DB_PATH.exists():
        if splash_proc is not None:
            if splash_proc.poll() is None:
                splash_proc.kill()

        dl_app = QApplication(sys.argv)
        w = DownloadAppdataWindow()
        w.show()
        status = dl_app.exec_()
        logger.info(f"main() Exiting with status {status}.")
        sys.exit(status)

    app = QApplication(sys.argv)

    try:
        port = find_available_port()
        write_log(f"Available port: {port}")
        daemon = threading.Thread(name='daemon_server',
                                target=start_server,
                                args=(port,))
        daemon.setDaemon(True)
        daemon.start()
    except Exception as e:
        write_log(f"{e}")
        # FIXME show error to user
        port = 6789

    actions_manager = ActionsManager(port)

    # FIXME errors on MacOS
    hotkeys_manager = None

    if IS_LINUX:
        from .app.hotkeys_manager_linux import HotkeysManagerLinux
        hotkeys_manager = HotkeysManagerLinux(actions_manager)
    elif IS_WINDOWS:
        from .app.hotkeys_manager_windows_mac import HotkeysManagerWindowsMac
        hotkeys_manager = HotkeysManagerWindowsMac(actions_manager)

    app_data = AppData(actions_manager=actions_manager, app_clipboard=app.clipboard(), api_port=port)

    app_windows = AppWindows(app, app_data, hotkeys_manager)

    # === Create systray ===

    # Systray doesn't work on MAC
    if not IS_MAC:
        app.setQuitOnLastWindowClosed(False)

        tray = QSystemTrayIcon(QIcon(":simsapa-tray"))
        tray.setVisible(True)

        menu = QMenu()

        _translate = QtCore.QCoreApplication.translate

        ac1 = QAction(QIcon(":book"), "Lookup Clipboard in Suttas")
        ac1.setShortcut(_translate("Systray", "Ctrl+Shift+S"))
        menu.addAction(ac1)

        ac2 = QAction(QIcon(":dictionary"), "Lookup Clipboard in Dictionary")
        ac2.setShortcut(_translate("Systray", "Ctrl+Shift+G"))
        menu.addAction(ac2)

        if hotkeys_manager is not None:
            ac1.triggered.connect(hotkeys_manager.lookup_clipboard_in_suttas)
            ac2.triggered.connect(hotkeys_manager.lookup_clipboard_in_dictionary)

        ac3 = QAction(QIcon(":close"), "Quit")
        ac3.triggered.connect(app.quit)
        menu.addAction(ac3)

        tray.setContextMenu(menu)

    # === Create first window ===

    app_windows._new_sutta_search_window()

    app_windows.show_startup_message()

    app_windows.show_update_message()

    if splash_proc is not None:
        if splash_proc.poll() is None:
            splash_proc.kill()

    status = app.exec_()

    if hotkeys_manager is not None:
        hotkeys_manager.unregister_all_hotkeys()

    write_log(f"start() Exiting with status {status}.")
    sys.exit(status)
