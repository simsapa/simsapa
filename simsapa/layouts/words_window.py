from typing import List
from PyQt6.QtCore import QUrl
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtWidgets import QSizePolicy, QVBoxLayout, QWidget

from simsapa import SIMSAPA_PACKAGE_DIR, DbSchemaName, logger
from simsapa.layouts.dictionary_queries import DictionaryQueries
from simsapa.layouts.reader_web import ReaderWebEnginePage
from ..app.types import AppData, AppWindowInterface, UDictWord

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

class WordsWindow(AppWindowInterface):
    def __init__(self, app_data: AppData, word_ids: List[tuple[str, int]], parent=None) -> None:
        super().__init__(parent)
        logger.info("WordsWindow()")

        self._app_data = app_data
        words = self._get_words_by_ids(word_ids)

        if len(words) == 0:
            self.close()
            return

        self.queries = DictionaryQueries(self._app_data)
        self.words = words
        self.setWindowTitle(str(words[0].word))
        self.resize(800, 800)

        self.qwe = self._new_webengine()

        self._setup_ui()
        self._connect_signals()

    def _get_words_by_ids(self, ids: List[tuple[str, int]]) -> List[UDictWord]:
        results: List[UDictWord] = []

        appdata_ids = list(map(lambda x: x[1], filter(lambda x: x[0] == DbSchemaName.AppData.value, ids)))
        userdata_ids = list(map(lambda x: x[1], filter(lambda x: x[0] == DbSchemaName.UserData.value, ids)))

        res = self._app_data.db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.id.in_(appdata_ids)) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.id.in_(userdata_ids)) \
            .all()
        results.extend(res)

        if len(results) == 0:
            logger.warn("No words found")

        return results

    def set_qwe_html(self, html: str):
        self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def render_words_content(self):
        html = self.queries.words_to_html_page(self.words)
        self.set_qwe_html(html)

    def _setup_ui(self):
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._central_widget.setLayout(self._layout)

        self._layout.addWidget(self.qwe, 100)

        self.render_words_content()

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
