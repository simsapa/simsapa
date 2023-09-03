import subprocess
from functools import partial
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtWebEngineWidgets import QWebEngineView

from PyQt6 import QtWidgets
from PyQt6.QtCore import QUrl, pyqtSignal
from PyQt6.QtGui import QAction, QCloseEvent
from PyQt6.QtWidgets import QMenu, QMenuBar, QSizePolicy, QVBoxLayout

from simsapa import HTML_RESOURCES_APPDATA_DIR, IS_SWAY, SIMSAPA_PACKAGE_DIR, logger
from simsapa.layouts.reader_web import LinkHoverData, ReaderWebEnginePage
from simsapa.layouts.preview_window import PreviewWindow

from simsapa.app.html_resources_server import HtmlResourcesServer
from simsapa.app.types import AppWindowInterface, QueryType
from simsapa.app.app_data import AppData

class SuttaIndexWindow(AppWindowInterface):

    show_sutta_by_url = pyqtSignal(QUrl)

    link_mouseover = pyqtSignal(dict)
    link_mouseleave = pyqtSignal(str)
    hide_preview = pyqtSignal()

    def __init__(self, app_data: AppData, parent = None) -> None:
        super().__init__(parent)
        logger.info("SuttaIndexWindow()")

        self._app_data: AppData = app_data

        self.server = HtmlResourcesServer(HTML_RESOURCES_APPDATA_DIR.joinpath('sutta-index-khemaratana/'))
        self.server.start_server()

        self.server_url = QUrl(f"http://127.0.0.1:{self.server.port}/index.html")

        self._setup_ui()
        self._connect_signals()

    def _setup_ui(self):
        self.setWindowTitle("Sutta Index")
        self.resize(850, 650)

        if IS_SWAY:
            cmd = """swaymsg 'for_window [title="Sutta Index"] floating enable'"""
            subprocess.Popen(cmd, shell=True)

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._central_widget.setLayout(self._layout)

        self.content_layout = QVBoxLayout()
        self.content_layout.setContentsMargins(0, 0, 0, 0)
        self._layout.addLayout(self.content_layout)

        self._setup_menubar()
        self._setup_qwe()

    def _setup_menubar(self):
        self.menubar = QMenuBar()
        self.setMenuBar(self.menubar)

        self.menu_file = QMenu("&File")
        self.menubar.addMenu(self.menu_file)

        self.action_close_window = QAction("&Close Window")
        self.menu_file.addAction(self.action_close_window)

    def _link_mouseover(self, hover_data: LinkHoverData):
        self.link_mouseover.emit(hover_data)

    def _link_mouseleave(self, href: str):
        self.link_mouseleave.emit(href)

    def _emit_hide_preview(self):
        self.hide_preview.emit()

    def _setup_qwe(self):
        self.qwe = QWebEngineView()

        page = ReaderWebEnginePage(self)
        page.helper.mouseover.connect(partial(self._link_mouseover))
        page.helper.mouseleave.connect(partial(self._link_mouseleave))
        page.helper.hide_preview.connect(partial(self._emit_hide_preview))

        page.show_url_signal.connect(partial(self._show_url_text))

        self.qwe.setPage(page)

        self.qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        self.qwe.setUrl(self.server_url)

        self.content_layout.addWidget(self.qwe, 100)

        self.qwe.show()

        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        self.qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

    def set_qwe_html(self, html: str):
        # Around above this size, QWebEngineView doesn't display the HTML with .setHtml()
        size_limit = 0.8 * 1024 * 1024

        if len(html) < size_limit:
            try:
                self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))
            except Exception as e:
                logger.error("set_qwe_html() : %s" % e)
        else:
            logger.error("HTML too long")

    def _show_url_text(self, url: str):
        self._show_url(QUrl(url))

    def _show_url(self, url: QUrl):
        if url.host() == QueryType.suttas:
            self._show_sutta_by_url(url)

    def _show_sutta_by_url(self, url: QUrl):
        if url.host() != QueryType.suttas:
            return

        self.show_sutta_by_url.emit(url)

    def _handle_close(self):
        self.close()

    def _handle_hide(self):
        self.close()

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.link_mouseover.connect(partial(preview_window.link_mouseover))
        self.link_mouseleave.connect(partial(preview_window.link_mouseleave))
        self.hide_preview.connect(partial(preview_window._do_hide))

    def closeEvent(self, event: QCloseEvent):
        event.accept()

    def _connect_signals(self):
        # Hide the window instead of closing, so the window can be re-shown and
        # server and web page doesn't have to be loaded again.
        self.action_close_window.triggered.connect(partial(self._handle_hide))
