from typing import List, Optional
from PyQt6.QtCore import QUrl
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtWidgets import QSizePolicy, QVBoxLayout, QWidget

from simsapa import SIMSAPA_PACKAGE_DIR, logger
from simsapa.layouts.html_content import html_page
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.app.types import USutta
from simsapa.app.app_data import AppData

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa.layouts.gui_types import AppWindowInterface

class SuttaWindow(AppWindowInterface):
    def __init__(self, app_data: AppData, sutta_uid: str, parent=None) -> None:
        super().__init__(parent)
        logger.info("SuttaWindow()")

        self._app_data = app_data
        sutta = self._get_sutta_by_uid(sutta_uid)

        if sutta is None:
            self.close()
            return

        self.sutta = sutta
        self.setWindowTitle(str(sutta.title))
        self.resize(800, 800)

        self.qwe = self._new_webengine()

        self._setup_ui()
        self._connect_signals()

    def _get_sutta_by_uid(self, uid: str) -> Optional[USutta]:
        results: List[USutta] = []

        res = self._app_data.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.uid == uid) \
            .all()
        results.extend(res)

        if len(results) == 0:
            logger.warn("No Sutta found with uid: %s" % uid)
            return None

        return results[0]

    def set_qwe_html(self, html: str):
        self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def render_sutta_content(self):
        if self.sutta.content_html is not None and self.sutta.content_html != '':
            content = str(self.sutta.content_html)
        elif self.sutta.content_plain is not None and self.sutta.content_plain != '':
            content = '<pre>' + str(self.sutta.content_plain) + '</pre>'
        else:
            content = 'No content.'

        html = html_page(content, self._app_data.api_url)

        self.set_qwe_html(html)

    def _setup_ui(self):
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._central_widget.setLayout(self._layout)

        self._layout.addWidget(self.qwe, 100)

        self.render_sutta_content()

    def _new_webengine(self) -> QWebEngineView:
        qwe = QWebEngineView()
        qwe.setPage(ReaderWebEnginePage(self))

        qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        # Enable dev tools
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

        return qwe

    def _connect_signals(self):
        pass
