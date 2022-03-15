from subprocess import Popen
import sys
import traceback
from typing import Optional
from PyQt5 import QtCore
import threading

from PyQt5.QtGui import QIcon
from PyQt5.QtWidgets import (QApplication, QSystemTrayIcon, QMenu, QAction)

from simsapa import logger
from simsapa import APP_DB_PATH, IS_LINUX, IS_MAC, IS_WINDOWS
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.helpers import create_app_dirs, ensure_empty_graphs_cache
from .app.types import AppData
from .app.windows import AppWindows
from .app.api import start_server, find_available_port
from .layouts.download_appdata import DownloadAppdataWindow
from .layouts.error_message import ErrorMessageWindow
from simsapa.layouts.create_search_index import CreateSearchIndexWindow

from simsapa.assets import icons_rc  # noqa: F401

def excepthook(exc_type, exc_value, exc_tb):
    logger.error("excepthook()")
    tb = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
    logger.error(tb)
    w = ErrorMessageWindow(user_message=None, debug_info=tb)
    w.show()


sys.excepthook = excepthook

create_app_dirs()

def start(splash_proc: Optional[Popen] = None):
    logger.info("start()", start_new=True)

    ensure_empty_graphs_cache()

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
        logger.info(f"Available port: {port}")
        daemon = threading.Thread(name='daemon_server',
                                target=start_server,
                                args=(port,))
        daemon.setDaemon(True)
        daemon.start()
    except Exception as e:
        logger.error(e)
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

    if app_data.search_indexed.has_empty_index():
        w = CreateSearchIndexWindow()
        w.show()
        status = app.exec_()
        logger.info(f"open_simsapa: {w.open_simsapa}")
        logger.info(f"app status: {status}")
        if not w.open_simsapa:
            logger.info("Exiting.")
            sys.exit(status)

    app_windows = AppWindows(app, app_data, hotkeys_manager)

    # === Create systray ===

    # Systray doesn't work on MAC
    if not IS_MAC:
        app.setQuitOnLastWindowClosed(False)

        tray = QSystemTrayIcon(QIcon(":simsapa-tray"))
        tray.setVisible(True)

        menu = QMenu()

        _translate = QtCore.QCoreApplication.translate

        ac0 = QAction("Show Word Scan Popup")
        ac0.setShortcut(_translate("Systray", "Ctrl+Shift+F6"))
        menu.addAction(ac0)

        ac1 = QAction(QIcon(":book"), "Lookup Clipboard in Suttas")
        ac1.setShortcut(_translate("Systray", "Ctrl+Shift+S"))
        menu.addAction(ac1)

        ac2 = QAction(QIcon(":dictionary"), "Lookup Clipboard in Dictionary")
        ac2.setShortcut(_translate("Systray", "Ctrl+Shift+G"))
        menu.addAction(ac2)

        if hotkeys_manager is not None:
            ac0.triggered.connect(hotkeys_manager.show_word_scan_popup)
            ac1.triggered.connect(hotkeys_manager.lookup_clipboard_in_suttas)
            ac2.triggered.connect(hotkeys_manager.lookup_clipboard_in_dictionary)

        ac3 = QAction(QIcon(":close"), "Quit")
        ac3.triggered.connect(app.quit)
        menu.addAction(ac3)

        tray.setContextMenu(menu)

    # === Create first window ===

    app_windows.open_first_window()

    app_windows.show_startup_message()

    app_windows.show_update_message()

    if splash_proc is not None:
        if splash_proc.poll() is None:
            splash_proc.kill()

    status = app.exec_()

    if hotkeys_manager is not None:
        hotkeys_manager.unregister_all_hotkeys()

    logger.info(f"start() Exiting with status {status}.")
    sys.exit(status)
