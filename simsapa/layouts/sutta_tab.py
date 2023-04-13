from pathlib import Path
from typing import List, Optional

from PyQt6.QtCore import Qt, QUrl
from PyQt6.QtWidgets import QWidget, QVBoxLayout
from PyQt6.QtWebEngineWidgets import QWebEngineView

from simsapa import DbSchemaName, GRAPHS_DIR, SIMSAPA_PACKAGE_DIR, logger
from simsapa.app.db.search import SearchResult
from simsapa.app.export_helpers import render_sutta_content
from simsapa.layouts.simsapa_webengine import SimsapaWebEngine
from ..app.types import AppData, QueryType, SuttaQuote, USutta
from .html_content import html_page

class SuttaTabWidget(QWidget):

    def __init__(self,
                 app_data: AppData,
                 title: str,
                 tab_index: int,
                 qwe: SimsapaWebEngine,
                 sutta: Optional[USutta] = None) -> None:

        super().__init__()

        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose, True)
        self.setProperty('style_class', 'sutta_tab')

        self._app_data = app_data
        self.title = title
        self.tab_index = tab_index
        self.qwe = qwe
        self.sutta = sutta
        self.api_url = self._app_data.api_url
        self.current_html = ""
        self.devtools_open = False

        self._layout = QVBoxLayout()
        self._layout.setContentsMargins(0, 0, 0, 0)
        self.setLayout(self._layout)
        self._layout.addWidget(self.qwe, 100)

    def get_current_html(self) -> str:
        return self.current_html

    def set_qwe_html(self, html: str):
        # Around above this size, QWebEngineView doesn't display the HTML with .setHtml()
        size_limit = 0.8 * 1024 * 1024

        self.current_html = html

        if len(html) < size_limit:
            try:
                self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))
            except Exception as e:
                logger.error("set_qwe_html() : %s" % e)
        else:
            try:
                if self.sutta is not None:
                    sutta_id = self.sutta.id
                else:
                    sutta_id = 0

                html_path = GRAPHS_DIR.joinpath(f"sutta-{sutta_id}.html")
                with open(html_path, 'w', encoding='utf-8') as f:
                    f.write(html)

                self.qwe.load(QUrl(str(html_path.absolute().as_uri())))

                # TODO consider browser reload
                # - cache folder? empty on start / exit?
                # html_path.unlink()

            except Exception as e:
                logger.error("set_qwe_html() : %s" % e)

    def set_qwe_html_file(self, html_path: Path):
        self.qwe.load(QUrl(str(html_path.absolute().as_uri())))

    def render_sutta_content(self, sutta_quote: Optional[SuttaQuote] = None):
        if self.sutta is None:
            return
        logger.info(f"render_sutta_content(): {self.sutta.uid}, sutta_quote: {sutta_quote}")
        html = render_sutta_content(self._app_data, self.sutta, sutta_quote)
        self.set_qwe_html(html)

    def render_search_results(self, results: List[SearchResult]):
        content = ""
        colors = ["#fdf6e2", "#fae6b2"]

        for idx, r in enumerate(results):
            if r['ref'] is not None:
                title = f"{r['ref']} {r['title']}"
            else:
                title = r['title']

            if len(title) > 70:
                title = title[:70] + '...'

            details = ''
            href = ''

            if r['uid'] is not None:
                href = f"ssp://{QueryType.suttas.value}/{r['uid']}"
                details = r['uid']

            if r['schema_name'] == DbSchemaName.UserData.value:
                details += ' (u)'

            n = idx % len(colors)

            content += """
            <div style="display: flex; background-color: %s; padding: 0.2em 2em;">
              <div style="flex-grow: 1; text-align: left;">
                <a href="%s">%s</a>
              </div>
              <div style="flex-grow: 1; text-align: right; font-size: 0.9em;">
                %s
              </div>
            </div>
            """ % (colors[n], href, title, details)

        font_size = self._app_data.app_settings.get('sutta_font_size', 22) - 4
        max_width = self._app_data.app_settings.get('sutta_max_width', 75) * 2

        css_extra = f"html {{ font-size: {font_size}px; }} body {{ max-width: {max_width}ex; padding: 0; }}"

        html = html_page(content, self.api_url, css_extra)

        self.set_qwe_html(html)

    def _show_devtools(self):
        self.dev_view = QWebEngineView()
        self._layout.addWidget(self.dev_view, 100)
        self.qwe.page().setDevToolsPage(self.dev_view.page())
        self.devtools_open = True

    def _hide_devtools(self):
        self.qwe.page().devToolsPage().deleteLater()
        self.dev_view.deleteLater()
        self.devtools_open = False
