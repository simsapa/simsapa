import subprocess

from functools import partial
from urllib.parse import parse_qs
from datetime import datetime

from typing import List, Optional
from PyQt6.QtCore import QTimer, QUrl, Qt, pyqtSignal
from PyQt6.QtGui import QCursor, QEnterEvent
from PyQt6.QtWebEngineCore import QWebEngineSettings
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWidgets import QDialog, QVBoxLayout

from simsapa import logger, IS_MAC, IS_SWAY, SIMSAPA_PACKAGE_DIR, QueryType, SearchResult

from simsapa.app.helpers import bilara_content_json_to_html, bilara_text_to_segments, is_complete_sutta_uid
from simsapa.app.types import SearchArea, SearchMode, SearchParams, UDictWord, USutta
from simsapa.app.app_data import AppData
from simsapa.layouts.gui_queries import GuiSearchQueries

from simsapa.layouts.gui_types import QExpanding, LinkHoverData, sutta_quote_from_url
from simsapa.layouts.html_content import html_page
from simsapa.layouts.reader_web import ReaderWebEnginePage

TITLE_PRE = "Simsapa Preview"

PREVIEW_BG_COLOR = "#FCEFCF"
TITLE_BG_COLOR = "#FEF8E8"
DARK_BORDER_COLOR = "#D47400"
LIGHT_BORDER_COLOR = TITLE_BG_COLOR

PREVIEW_CSS_EXTRA = f"""
html, body {{ background-color: {PREVIEW_BG_COLOR}; }}
html {{ font-size: 16px; }}
body {{ padding: 0.5rem; max-width: 100%; }}
h1 {{ font-size: 20px; margin-top: 0pt; }}
"""

if IS_MAC:
    HOVER_DIST = 60
else:
    HOVER_DIST = 30

class PreviewWindow(QDialog):

    open_new = pyqtSignal(str)
    make_windowed = pyqtSignal()

    _queries: GuiSearchQueries

    show_words_by_url = pyqtSignal(QUrl)

    def __init__(self, app_data: AppData,
                 hover_data: Optional[LinkHoverData] = None,
                 frameless: bool = True) -> None:
        super().__init__()

        self._app_data: AppData = app_data

        self._queries = GuiSearchQueries(self._app_data.db_session,
                                         None,
                                         self._app_data.get_search_indexes,
                                         self._app_data.api_url)

        self.title = ''

        self._hover_data = hover_data
        self._frameless = frameless
        self._link_mouseleave = False
        self._mouseover = False

        self._hide_timer = QTimer()
        self._hide_timer.timeout.connect(partial(self.hide))
        self._hide_timer.setSingleShot(True)

        self._setup_ui()
        self._connect_signals()

    def _set_title(self):
        if self.title == '':
            self.setWindowTitle(TITLE_PRE)
        else:
            self.setWindowTitle(f"{TITLE_PRE}: {self.title}")

    def _do_show(self, check_settings: bool = True):
        if check_settings and not self._app_data.app_settings.get('link_preview', True):
            return

        self._set_title()
        self.show()
        self._move_window_from_hover()

    def _do_hide(self):
        self._hover_data = None
        self._link_mouseleave = False
        self._mouseover = False
        self._hide_timer.stop()
        self.hide()

    def _setup_ui(self):
        self.wrap_layout = QVBoxLayout()
        self.wrap_layout.setContentsMargins(1, 1, 1, 8)
        self.setLayout(self.wrap_layout)
        self._set_title()

        self.setMinimumSize(50, 50)

        if self._frameless:
            flags = Qt.WindowType.Dialog | \
                Qt.WindowType.CustomizeWindowHint | \
                Qt.WindowType.WindowStaysOnTopHint | \
                Qt.WindowType.FramelessWindowHint | \
                Qt.WindowType.BypassWindowManagerHint | \
                Qt.WindowType.X11BypassWindowManagerHint

        else:
            flags = Qt.WindowType.Dialog | \
                Qt.WindowType.WindowStaysOnTopHint

        self.setWindowFlags(Qt.WindowType(flags))

        self.setObjectName("PreviewWindow")
        self.setStyleSheet("#PreviewWindow { background-color: %s; border: 1px solid %s; }" % (PREVIEW_BG_COLOR, LIGHT_BORDER_COLOR))

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

        if self._link_mouseleave and self._frameless:
            self._hide_timer.start(500)

        return super().leaveEvent(e)


    def link_mouseleave(self, href: str):
        if self._hover_data is not None \
           and self._hover_data['href'] == href:

            self._link_mouseleave = True
            if not self._mouseover:
                self._hide_timer.start(500)


    def link_mouseover(self, hover_data: LinkHoverData):
        if self.isVisible() and \
           self._hover_data is not None and \
           self._hover_data['href'] == hover_data['href']:
            return

        if self.render_hover_data(hover_data):
            self._do_show()

    def graph_link_mouseover(self, hover_data: LinkHoverData):
        if self.isVisible() and \
           self._hover_data is not None and \
           self._hover_data['href'] == hover_data['href']:
            return

        if self.render_hover_data(hover_data):
            self._do_show()

            self._link_mouseleave = True
            self._mouseover = False

            self._hide_timer.start(2000)

    def _sutta_search_query_finished(self, __query_started_time__: Optional[datetime] = None):
        if not self._queries.all_finished():
            return
        self._query_results = self._queries.results_page(0)

        sutta: Optional[USutta] = None

        assert(self._hover_data is not None)
        url = QUrl(self._hover_data['href'])

        if len(self._query_results) > 0:
            r = self._query_results[0]
            logger.info(f"Rank: {r['rank']}")

            if r['uid'] is not None:
                sutta = self._queries.sutta_queries.get_sutta_by_uid(r['uid'])

        if sutta is not None:
            quote = sutta_quote_from_url(url)
            if quote:
                self._render_sutta(sutta, quote['quote'])
            else:
                self._render_sutta(sutta)

        else:
            self._render_not_found(url)

    def render_hover_data(self, hover_data: Optional[LinkHoverData] = None) -> bool:
        if self._hover_data is not None and \
           hover_data is not None and \
           self._hover_data['href'] == hover_data['href']:

            self._hide_timer.stop()

            return self.isHidden()

        self._link_mouseleave = False
        self._hide_timer.stop()

        if hover_data:
            self._hover_data = hover_data

        if self._hover_data is None:
            return False

        url = QUrl(self._hover_data['href'])

        if url.host() == QueryType.suttas:

            sutta = self._queries.sutta_queries.get_sutta_by_url(url)
            quote = sutta_quote_from_url(url)

            if sutta is None and quote is not None:

                uid = url.path().strip("/")
                if is_complete_sutta_uid(uid):
                    # dn22/pli/ms
                    _, lang, _ = uid.split("/")
                else:
                    lang = "pli"

                params = SearchParams(
                    mode = SearchMode.FulltextMatch,
                    page_len = 10,
                    lang = lang,
                    lang_include = True,
                    source = None,
                    source_include = True,
                    enable_regex = False,
                    fuzzy_distance = 0,
                )

                self._query_results: List[SearchResult] = []

                self._last_query_time = datetime.now()

                self._queries.start_search_query_workers(
                    quote['quote'],
                    SearchArea.Suttas,
                    self._last_query_time,
                    self._sutta_search_query_finished,
                    params,
                )

            else:

                if sutta is None:
                    self._render_not_found(url)
                    return True

                query = parse_qs(url.query())
                quote = None
                if 'q' in query.keys():
                    quote = query['q'][0]

                self._render_sutta(sutta, quote)

            return True

        if url.host() == QueryType.words:

            word_uid = url.path().strip("/")

            words = self._queries.dictionary_queries.get_words_by_uid(word_uid)

            if len(words) == 0:
                self._render_not_found(url)
                return True

            self._render_words(words)

            return True

        # It's not a sutta or dictionary word link.
        return False

    def _move_window_from_hover(self):
        preview_width = 500
        preview_height = 400

        if IS_SWAY:
            self.setFixedSize(preview_width, preview_height)
        elif self._frameless:
            self.setFixedSize(preview_width, preview_height)
        else:
            self.resize(preview_width, preview_height)

        cursor_pos = QCursor.pos()

        if self._app_data.screen_size is not None:
            if self._hover_data:
                # x_hover = self._hover_data['x']
                x_hover = cursor_pos.x()
                x_max = self._app_data.screen_size.width() - 10

                if x_hover + preview_width < x_max:
                    x = x_hover + HOVER_DIST
                else:
                    x = x_hover - preview_width + HOVER_DIST

                # y_hover = self._hover_data['y'] + 130
                y_hover = cursor_pos.y()
                y_max = self._app_data.screen_size.height() - 10

                if y_hover + preview_height < y_max:
                    y = y_hover
                else:
                    y = y_hover - preview_height - HOVER_DIST

            else:
                x = self._app_data.screen_size.width() - preview_width - 10
                y = 10

            if IS_SWAY:
                self._sway_move(x, y)
            else:
                self.move(x, y)
        else:

            if IS_SWAY:
                self._sway_move(10, 10)
            else:
                self.move(10, 10)


    def _sway_move(self, x: int, y: int):
        cmd = f"""swaymsg 'for_window [title="Simsapa Preview.*"] floating enable' && swaymsg 'for_window [title="Simsapa Preview.*"] move position {x} {y}'"""
        subprocess.Popen(cmd, shell=True)


    def _render_not_found(self, url: QUrl):
        self.title = "Content Not Found"

        content = f"""
        <h1>Content Not Found</h1>
        <p>No content found for URL: {url.toString()}</p>
        <p>
        <i>Host:</i> {url.host()}<br>
        <i>Path:</i> {url.path()}<br>
        <i>Query:</i> {parse_qs(url.query())}<br>
        </p>
        """

        css_extra = PREVIEW_CSS_EXTRA

        html = html_page(content, self._app_data.api_url, css_extra)

        self.set_qwe_html(html)


    def _render_words(self, words: List[UDictWord]):
        self.title = ''

        css_extra = PREVIEW_CSS_EXTRA
        js_extra = "const SHOW_BOOKMARKS = false;"

        if self._frameless:
            js_extra += """
            document.addEventListener("DOMContentLoaded", function(event) {
                let el = document.querySelector('#preview-close');
                el.addEventListener("click", function() {
                    document.qt_channel.objects.helper.emit_do_close();
                });
            });
            """

        html_title = None
        if self._frameless:
            html_title = """
            <div class="btn pull-right" id="preview-close">
                <a href="#"><svg class="icon icon-close"><use xlink:href="#icon-circle-xmark-solid"></use></svg></a>
            </div>
            """

            css_extra += """
            #preview-close { font-size: 14.5px; line-height: 1; position: fixed; top: 6px; right: 6px; }
            """

        html = self._queries.dictionary_queries.words_to_html_page(
            words=words,
            css_extra=css_extra,
            js_extra=js_extra,
            html_title=html_title)

        self.set_qwe_html(html)


    def _window_title_bar_html(self) -> str:
        html = f"""<div id="window-title-bar">
        <div class="flex-container">
            <div id="left-btn-box">
                <div class="btn" id="preview-open-new">
                    <a href="#"><svg class="icon icon-open-new"><use xlink:href="#icon-square-up-right-solid"></use></svg></a>
                </div>
                <div class="btn" id="preview-make-windowed">
                    <a href="#"><svg class="icon icon-make-windowed"><use xlink:href="#icon-copy-solid"></use></svg></a>
                </div>
            </div>
            <div id="preview-title-text">{self.title}</div>
            <div id="right-btn-box">
                <div class="btn pull-right" id="preview-close">
                    <a href="#"><svg class="icon icon-close"><use xlink:href="#icon-circle-xmark-solid"></use></svg></a>
                </div>
            </div>
        </div>
        </div>"""

        return html


    def _render_sutta(self, sutta: USutta, highlight_text: Optional[str] = None):
        if sutta.title_trans is not None and sutta.title_trans != '':
            s = sutta.title_trans
        else:
            s = sutta.title
            self.title = f"{sutta.sutta_ref} {s} ({sutta.uid})"

        if sutta.content_json is not None and sutta.content_json != '':
            segments_json = bilara_text_to_segments(str(sutta.content_json), str(sutta.content_json_tmpl))
            content = bilara_content_json_to_html(segments_json)

        elif sutta.content_html is not None and sutta.content_html != '':
            content = str(sutta.content_html)

        elif sutta.content_plain is not None and sutta.content_plain != '':
            content = '<pre>' + str(sutta.content_plain) + '</pre>'

        else:
            content = 'No content.'

        if self._frameless:
            content = self._window_title_bar_html() + content

        css_extra = f"""
        html, body {{ background-color: {PREVIEW_BG_COLOR}; }}
        html {{ font-size: 16px; }}
        body {{ padding: 0; margin: 2rem 1rem 1rem 1rem; max-width: 100%; }}
        h1 {{ font-size: 20px; margin-top: 0pt; }}
        """

        js_extra = f"const SUTTA_UID = '{sutta.uid}';"
        js_extra += "const SHOW_BOOKMARKS = false;"

        if highlight_text:
            # NOTE highlight_and_scroll_to() replaces #ssp_main.innerHTML and
            # loses eventlisteners. Add document.addEventListener() after this,
            # or use onclick HTML attributes.
            text = highlight_text.replace('"', '\\"')
            js_extra += """document.addEventListener("DOMContentLoaded", function(event) { highlight_and_scroll_to("%s"); });""" % text

        if self._frameless:
            js_extra += """
            document.addEventListener("DOMContentLoaded", function(event) {
                let el = document.getElementById('preview-open-new');
                el.addEventListener("click", function() {
                    document.qt_channel.objects.helper.emit_open_new();
                });

                el = document.getElementById('preview-make-windowed');
                el.addEventListener("click", function() {
                    document.qt_channel.objects.helper.emit_make_windowed();
                });

                el = document.querySelector('#preview-close');
                el.addEventListener("click", function() {
                    document.qt_channel.objects.helper.emit_do_close();
                });
            });
            """

        html = html_page(content=content,
                         api_url=self._app_data.api_url,
                         css_extra=css_extra,
                         js_extra=js_extra)

        self.set_qwe_html(html)

    def _show_url(self, url: QUrl):
        if url.host() == QueryType.suttas:
            self._show_sutta_by_url(url)

        elif url.host() == QueryType.words:
            self._show_words_by_url(url)

    def _show_words_by_url(self, url: QUrl):
        if url.host() != QueryType.words:
            return

        self.show_words_by_url.emit(url)

    def _show_sutta_by_url(self, url: QUrl):
        self.open_new.emit(url.toString())


    def _new_webengine(self) -> QWebEngineView:
        qwe = QWebEngineView()

        page = ReaderWebEnginePage(self)

        page.helper.do_close.connect(partial(self._do_hide))
        page.helper.make_windowed.connect(partial(self.make_windowed.emit))

        def _open_new():
            if self._hover_data:
                self.open_new.emit(self._hover_data['href'])

        page.helper.open_new.connect(partial(_open_new))

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
