from subprocess import Popen
import sys
import traceback
from typing import Optional
from PyQt6 import QtCore
import threading
from functools import partial

from PyQt6.QtCore import QUrl
from PyQt6.QtGui import QIcon, QAction
from PyQt6.QtWidgets import (QApplication, QSystemTrayIcon, QMenu)
from PyQt6.QtWebEngineWidgets import QWebEngineView

from simsapa import DESKTOP_FILE_PATH, SIMSAPA_API_PORT_PATH, START_LOW_MEM, set_is_gui, logger, SERVER_QUEUE, APP_DB_PATH

from simsapa.app.actions_manager import ActionsManager
from simsapa.app.helpers import find_available_port
from simsapa.app.dir_helpers import create_app_dirs, check_delete_files, ensure_empty_graphs_cache
from simsapa.app.types import QueryType
from simsapa.app.app_data import AppData
from simsapa.app.windows import AppWindows

from simsapa.layouts.error_message import ErrorMessageWindow

# NOTE: Importing icons_rc is necessary once, in order for icon assets,
# animation gifs, etc. to be loaded.
from simsapa.assets import icons_rc
from simsapa.layouts.gui_helpers import get_app_version


def excepthook(exc_type, exc_value, exc_tb):
    logger.error("excepthook()")
    tb = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
    logger.error(tb)
    w = ErrorMessageWindow(user_message=None, debug_info=tb)
    w.show()


sys.excepthook = excepthook

check_delete_files()
create_app_dirs()

def start(port: Optional[int] = None, url: Optional[str] = None, splash_proc: Optional[Popen] = None):
    logger.profile("gui::start()")
    set_is_gui(True)

    if START_LOW_MEM:
        import psutil
        logger.info(f"START_LOW_MEM: {START_LOW_MEM}")
        mem = psutil.virtual_memory()
        logger.info(f"Memory: {mem}")
        cpu = psutil.cpu_freq()
        logger.info(f"CPU Freq: {cpu}")
        cores = psutil.cpu_count(logical=True)
        logger.info(f"CPU Cores: {cores}")

    if SIMSAPA_API_PORT_PATH.exists():
        # If there is a running Simsapa app, tell the api server to open a
        # window. If the request succeeded, quit. If it failed, continue
        # starting this app.
        try:
            with open(SIMSAPA_API_PORT_PATH, mode='r', encoding='utf-8') as f:
                api_port = int(f.read())
            import requests
            r = requests.get(f"http://localhost:{api_port}/open_window")
            if r.ok:
                logger.info(f"Running Simsapa instance detected, sent an api request to open a window. Response status: {r.status_code}. Exiting.")
                sys.exit(0)

        except Exception as e:
            logger.error(f"Can't read file or port not an integer: {e}")

    ensure_empty_graphs_cache()

    if not APP_DB_PATH.exists():
        if splash_proc is not None:
            if splash_proc.poll() is None:
                splash_proc.kill()

        from simsapa.layouts.download_appdata import DownloadAppdataWindow

        dl_app = QApplication(sys.argv)
        w = DownloadAppdataWindow()
        w.show()
        status = dl_app.exec()
        logger.info(f"gui::start() Exiting with status {status}.")
        sys.exit(status)

    # Start the API server after checking for APP_DB. If this is the first run,
    # the server would create the userdata db, and we can't use it to test in
    # DownloadAppdataWindow() if this is the first ever start.
    if port is None:
        try:
            port = find_available_port()
        except Exception as e:
            logger.error(e)
            # FIXME show error to user
            port = 6789

    logger.info(f"Available port: {port}")

    app = QApplication(sys.argv)

    # Initialize a QWebEngineView(). Otherwise the app errors:
    #
    # QtWebEngineWidgets must be imported or Qt.AA_ShareOpenGLContexts must be
    # set before a QCoreApplication instance is created
    _ = QWebEngineView()

    actions_manager = ActionsManager(port)

    # FIXME errors on MacOS
    hotkeys_manager = None

    # FIXME 'keyboard' lib imports 'requests' which delays app.exec()

    # if IS_LINUX:
    #     from .app.hotkeys_manager_linux import HotkeysManagerLinux
    #     hotkeys_manager = HotkeysManagerLinux(actions_manager)
    # elif IS_WINDOWS:
    #     from .app.hotkeys_manager_windows_mac import HotkeysManagerWindowsMac
    #     hotkeys_manager = HotkeysManagerWindowsMac(actions_manager)

    app_data = AppData(actions_manager=actions_manager, app_clipboard=app.clipboard(), api_port=port)

    if len(app.screens()) > 0:
        app_data.screen_size = app.primaryScreen().size()
        logger.info(f"Screen size: {app_data.screen_size}")
        logger.info(f"Device pixel ratio: {app.primaryScreen().devicePixelRatio()}")

    app_windows = AppWindows(app, app_data, hotkeys_manager)

    def emit_open_window_signal_fn(window_type: str = ''):
        app_windows.signals.open_window_signal.emit(window_type)

    def _start_daemon_server():
        # This way the import happens in the thread, and doesn't delay app.exec()
        from simsapa.app.api import start_server

        with open(SIMSAPA_API_PORT_PATH, mode='w', encoding='utf-8') as f:
            f.write(str(port))

        start_server(port, SERVER_QUEUE, emit_open_window_signal_fn)

    daemon = threading.Thread(name='daemon_server', target=_start_daemon_server)
    daemon.setDaemon(True)
    daemon.start()


    app.setApplicationName("Simsapa Dhamma Reader")
    app.setWindowIcon(QIcon(":simsapa-appicon"))

    if DESKTOP_FILE_PATH is not None:
        app.setDesktopFileName(str(DESKTOP_FILE_PATH.with_suffix("")))

    app.setApplicationVersion(get_app_version() or "v0.1.0")

    keep_running = app_data.app_settings.get('keep_running_in_background', True)
    app.setQuitOnLastWindowClosed((not keep_running))

    # === Create systray ===

    logger.profile("Create systray: start")

    tray = QSystemTrayIcon(QIcon(":simsapa-tray"))
    tray.setVisible(True)

    def _system_tray_clicked():
        app_windows.handle_system_tray_clicked()

    tray.activated.connect(partial(_system_tray_clicked))

    menu = QMenu()

    _translate = QtCore.QCoreApplication.translate

    ac0 = QAction("Show Word Lookup")
    ac0.setShortcut(_translate("Systray", "Ctrl+Shift+F6"))
    menu.addAction(ac0)

    ac1 = QAction(QIcon(":book"), "Lookup Clipboard in Suttas")
    ac1.setShortcut(_translate("Systray", "Ctrl+Shift+S"))
    menu.addAction(ac1)

    ac2 = QAction(QIcon(":dictionary"), "Lookup Clipboard in Dictionary")
    ac2.setShortcut(_translate("Systray", "Ctrl+Shift+G"))
    menu.addAction(ac2)

    if hotkeys_manager is not None:
        ac0.triggered.connect(hotkeys_manager.show_word_lookup)
        ac1.triggered.connect(hotkeys_manager.lookup_clipboard_in_suttas)
        ac2.triggered.connect(hotkeys_manager.lookup_clipboard_in_dictionary)

    ac3 = QAction(QIcon(":close"), "Quit")
    ac3.triggered.connect(app.quit)
    menu.addAction(ac3)

    tray.setContextMenu(menu)

    logger.profile("Create systray: end")

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

    if splash_proc is not None:
        if splash_proc.poll() is None:
            splash_proc.kill()

    logger.profile("app.exec()")
    status = app.exec()

    if hotkeys_manager is not None:
        hotkeys_manager.unregister_all_hotkeys()

    # This avoids the unused import warning.
    logger.info(icons_rc.rcc_version)

    if SIMSAPA_API_PORT_PATH.exists():
        SIMSAPA_API_PORT_PATH.unlink()

    logger.info(f"start() Exiting with status {status}.")
    sys.exit(status)
