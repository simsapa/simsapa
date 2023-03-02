from typing import Callable

from PyQt6.QtWidgets import QMenu, QSizePolicy
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebEngineCore import QWebEngineSettings

from simsapa.layouts.reader_web import ReaderWebEnginePage

class SimsapaWebEngine(QWebEngineView):
    menu: QMenu

    def __init__(self,
                 page: ReaderWebEnginePage,
                 create_qwe_context_menu: Callable,
                 *args,
                 **kwargs):
        super().__init__(*args, **kwargs)

        self.create_qwe_context_menu = create_qwe_context_menu

        self.setPage(page)

        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        # Enable dev tools
        self.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        self.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        self.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        self.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

    def contextMenuEvent(self, event):
        self.menu = QMenu(self)

        self.create_qwe_context_menu(self.menu)

        self.menu.popup(event.globalPos())
