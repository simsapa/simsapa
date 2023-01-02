from functools import partial
from urllib.parse import parse_qs

from typing import List, Optional
from PyQt6.QtCore import QTimer, QUrl, Qt
from PyQt6.QtGui import QEnterEvent
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWidgets import QDialog, QVBoxLayout

from simsapa import READING_BACKGROUND_COLOR, SIMSAPA_PACKAGE_DIR
from simsapa.app.helpers import bilara_content_json_to_html, bilara_text_to_segments
from simsapa.app.types import AppData, QExpanding, QueryType, UDictWord, USutta
from simsapa.layouts.dictionary_queries import DictionaryQueries
from simsapa.layouts.html_content import html_page
from simsapa.layouts.reader_web import LinkHoverData, ReaderWebEnginePage
from simsapa.layouts.sutta_queries import SuttaQueries


class PreviewWindow(QDialog):

    def __init__(self, app_data: AppData) -> None:
        super().__init__()

        self._app_data: AppData = app_data

        self._hover_data: Optional[LinkHoverData] = None
        self._link_mouseleave = False
        self._mouseover = False

        self.sutta_queries = SuttaQueries(self._app_data)
        self.dict_queries = DictionaryQueries(self._app_data)

        self._hide_timer = QTimer()
        self._hide_timer.timeout.connect(partial(self.hide))
        self._hide_timer.setSingleShot(True)

        self._ui_setup()
        self._connect_signals()


    def _do_hide(self):
        self._hover_data = None
        self._link_mouseleave = False
        self._mouseover = False
        self._hide_timer.stop()
        self.hide()

    def _ui_setup(self):
        self.wrap_layout = QVBoxLayout()
        self.wrap_layout.setContentsMargins(8, 8, 8, 8)
        self.setLayout(self.wrap_layout)

        self.setMinimumSize(50, 50)

        flags = Qt.WindowType.Dialog | \
            Qt.WindowType.CustomizeWindowHint | \
            Qt.WindowType.WindowStaysOnTopHint | \
            Qt.WindowType.FramelessWindowHint | \
            Qt.WindowType.BypassWindowManagerHint | \
            Qt.WindowType.X11BypassWindowManagerHint

        self.setWindowFlags(Qt.WindowType(flags))

        self.setObjectName("PreviewWindow")
        self.setStyleSheet("#PreviewWindow { background-color: %s; border: 1px solid #ababab; }" % READING_BACKGROUND_COLOR)

        self.qwe = self._new_webengine()

        self.wrap_layout.addWidget(self.qwe, 100)


    def set_qwe_html(self, html: str):
        self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))


    def enterEvent(self, e: QEnterEvent):
        self._mouseover = True
        self._hide_timer.stop()

        return super().enterEvent(e)


    def leaveEvent(self, e: QEnterEvent):
        self._mouseover = False

        if self._link_mouseleave:
            self._hide_timer.start(500)

        return super().leaveEvent(e)


    def link_mouseleave(self, href: str):
        if self._hover_data is not None \
           and self._hover_data['href'] == href:

            self._link_mouseleave = True
            if not self._mouseover:
                self._hide_timer.start(500)


    def link_mouseover(self, hover_data: LinkHoverData):
        if self._hover_data is not None and \
           self._hover_data['href'] == hover_data['href']:

            self._hide_timer.stop()

            if self.isHidden():
                self.show()
            else:
                return

        self._link_mouseleave = False
        self._hide_timer.stop()

        self._hover_data = hover_data

        url = QUrl(hover_data['href'])

        if url.host() == QueryType.suttas:

            sutta = self.sutta_queries.get_sutta_by_url(url)

            if sutta is None:
                return

            query = parse_qs(url.query())
            quote = None
            if 'q' in query.keys():
                quote = query['q'][0]

            self._render_sutta(sutta, quote)

        elif url.host() == QueryType.words:

            word_uid = url.path().strip("/")

            words = self.dict_queries.get_words_by_uid(word_uid)

            if len(words) == 0:
                return

            self._render_words(words)

        else:
            # It's not a sutta or dictionary word link.
            return

        self._move_window_from_hover()
        self.show()


    def _move_window_from_hover(self):
        preview_width = 500
        preview_height = 400

        self.setFixedSize(preview_width, preview_height)

        if self._app_data.screen_size:
            if self._hover_data:
                x_hover = self._hover_data['x']
                x_max = self._app_data.screen_size.width() - 10

                if x_hover + preview_width < x_max:
                    x = x_hover + 30
                else:
                    x = x_hover - preview_width + 30

                y_hover = self._hover_data['y'] + 130
                y_max = self._app_data.screen_size.height() - 10

                if y_hover + preview_height < y_max:
                    y = y_hover
                else:
                    y = y_hover - preview_height - 30

            else:
                x = self._app_data.screen_size.width() - preview_width - 10
                y = 10

            self.move(x, y)
        else:
            self.move(10, 10)


    def _render_words(self, words: List[UDictWord]):
        self.setWindowTitle("Simsapa Preview")

        css_extra = """
        html { font-size: 18px; }
        body { padding: 0.5rem; max-width: 100%; }
        h1 { font-size: 22px; margin-top: 0pt; }
        """

        js_extra = "const SHOW_BOOKMARKS = false;";

        html = self.dict_queries.words_to_html_page(words=words, css_extra=css_extra, js_extra=js_extra)

        self.set_qwe_html(html)


    def _render_sutta(self, sutta: USutta, highlight_text: Optional[str] = None):
        self.setWindowTitle(f"Simsapa Preview: {sutta.title}")

        if sutta.content_json is not None and sutta.content_json != '':
            segments_json = bilara_text_to_segments(str(sutta.content_json), str(sutta.content_json_tmpl))
            content = bilara_content_json_to_html(segments_json)

        elif sutta.content_html is not None and sutta.content_html != '':
            content = str(sutta.content_html)

        elif sutta.content_plain is not None and sutta.content_plain != '':
            content = '<pre>' + str(sutta.content_plain) + '</pre>'

        else:
            content = 'No content.'

        css_extra = """
        html { font-size: 18px; }
        body { padding: 0.5rem; max-width: 100%; }
        h1 { font-size: 22px; margin-top: 0pt; }
        """

        js_extra = f"const SUTTA_UID = '{sutta.uid}';";
        js_extra += "const SHOW_BOOKMARKS = false;";

        if highlight_text:
            text = highlight_text.replace('"', '\\"')
            js_extra += """document.addEventListener("DOMContentLoaded", function(event) { highlight_and_scroll_to("%s"); });""" % text

        html = html_page(content=content,
                         api_url=self._app_data.api_url,
                         css_extra=css_extra,
                         js_extra=js_extra)

        self.set_qwe_html(html)


    def _new_webengine(self) -> QWebEngineView:
        qwe = QWebEngineView()

        page = ReaderWebEnginePage(self)

        qwe.setPage(page)

        qwe.setSizePolicy(QExpanding, QExpanding)

        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ShowScrollBars, False)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.JavascriptEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.LocalContentCanAccessRemoteUrls, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.ErrorPageEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.WebAttribute.PluginsEnabled, True)

        return qwe


    def _connect_signals(self):
        pass
