from subprocess import Popen
import sys
import traceback
from typing import Optional
from PyQt6 import QtCore
import multiprocessing as mp
import threading

from PyQt6.QtCore import QUrl
from PyQt6.QtGui import QIcon, QAction
from PyQt6.QtWidgets import (QApplication, QSystemTrayIcon, QMenu)

from simsapa.app.api import start_server, find_available_port
from simsapa import logger
from simsapa import SERVER_QUEUE, APP_DB_PATH, IS_LINUX, IS_MAC, IS_WINDOWS
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.helpers import create_app_dirs, ensure_empty_graphs_cache
from .app.types import AppData, QueryType
from .app.windows import AppWindows
from .layouts.download_appdata import DownloadAppdataWindow
from .layouts.error_message import ErrorMessageWindow
from simsapa.layouts.create_search_index import CreateSearchIndexWindow

from simsapa.assets import icons_rc

def excepthook(exc_type, exc_value, exc_tb):
    logger.error("excepthook()")
    tb = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
    logger.error(tb)
    w = ErrorMessageWindow(user_message=None, debug_info=tb)
    w.show()


sys.excepthook = excepthook

create_app_dirs()


def start(port: Optional[int] = None, url: Optional[str] = None, splash_proc: Optional[Popen] = None):
    logger.info("gui::start()")

    if port is None:
        try:
            port = find_available_port()
        except Exception as e:
            logger.error(e)
            # FIXME show error to user
            port = 6789

    logger.info(f"Available port: {port}")

    if IS_MAC:
        # FIXME avoids multiprocessing error until Python 3.11.2 is released
        daemon = threading.Thread(name='daemon_server',
                                target=start_server,
                                args=(port, SERVER_QUEUE))
        daemon.setDaemon(True)
        daemon.start()

    else:
        server_proc = mp.Process(target=start_server, args=(port, SERVER_QUEUE), daemon=True)
        server_proc.start()

    ensure_empty_graphs_cache()

    if not APP_DB_PATH.exists():
        if splash_proc is not None:
            if splash_proc.poll() is None:
                splash_proc.kill()

        dl_app = QApplication(sys.argv)
        w = DownloadAppdataWindow()
        w.show()
        status = dl_app.exec()
        logger.info(f"gui::start() Exiting with status {status}.")
        sys.exit(status)

    app = QApplication(sys.argv)

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

    if len(app.screens()) > 0:
        app_data.screen_size = app.primaryScreen().size()

    if app_data.search_indexed.has_empty_index():
        w = CreateSearchIndexWindow()
        w.show()
        status = app.exec()
        logger.info(f"open_simsapa: {w.open_simsapa}")
        logger.info(f"app status: {status}")
        if not w.open_simsapa:
            logger.info("Exiting.")
            sys.exit(status)

    app_windows = AppWindows(app, app_data, hotkeys_manager)

    # === Create systray ===

    # Systray doesn't work on MAC
    if not IS_MAC:
        app.setQuitOnLastWindowClosed(True)

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

    ok = False
    if url:
        open_url = QUrl(url)
        if open_url.scheme() == 'ssp' and open_url.host() == QueryType.suttas:
            ok = app_windows._show_sutta_by_url_in_search(open_url)

        elif open_url.scheme() == 'ssp' and open_url.host() == QueryType.words:
            ok = app_windows._show_words_by_url(open_url)

    if not ok:
        app_windows.open_first_window()

    app_windows.show_startup_message()

    app_windows.check_updates()

    if splash_proc is not None:
        if splash_proc.poll() is None:
            splash_proc.kill()

    status = app.exec()

    if hotkeys_manager is not None:
        hotkeys_manager.unregister_all_hotkeys()

    logger.info(f"start() Exiting with status {status}.")
    sys.exit(status)
