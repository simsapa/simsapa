import json

from PyQt6.QtCore import QObject, QUrl, pyqtSignal, pyqtSlot
from PyQt6.QtWebEngineCore import QWebEnginePage
from PyQt6.QtGui import QDesktopServices
from PyQt6.QtWebChannel import QWebChannel

from simsapa import logger

from simsapa.layouts.gui_types import LinkHoverData

class Helper(QObject):

    mouseover = pyqtSignal(dict)
    mouseover_graph_node = pyqtSignal(dict)
    mouseleave = pyqtSignal(str)
    dblclick = pyqtSignal()
    copy_clipboard_text = pyqtSignal(str)
    copy_clipboard_html = pyqtSignal(str)
    copy_gloss = pyqtSignal(str, int, str)
    copy_meaning = pyqtSignal(str, int)

    hide_preview = pyqtSignal()
    bookmark_edit = pyqtSignal(str)

    do_close = pyqtSignal()
    open_new = pyqtSignal()
    make_windowed = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)


    @pyqtSlot()
    def emit_do_close(self):
        self.do_close.emit()


    @pyqtSlot()
    def emit_open_new(self):
        self.open_new.emit()


    @pyqtSlot()
    def emit_make_windowed(self):
        self.make_windowed.emit()


    @pyqtSlot(str)
    def link_mouseover(self, msg: str):
        d: dict = json.loads(msg)

        hover_data = LinkHoverData(
            href = d['href'],
            x = int(d['x']),
            y = int(d['y']),
            width = int(d['width']),
            height = int(d['height']),
        )

        self.mouseover.emit(hover_data)

    @pyqtSlot(str)
    def link_mouseover_graph_node(self, msg: str):
        d: dict = json.loads(msg)
        self.mouseover_graph_node.emit(d)

    @pyqtSlot(str)
    def link_mouseleave(self, msg: str):
        self.mouseleave.emit(msg)

    @pyqtSlot()
    def page_dblclick(self):
        self.dblclick.emit()

    @pyqtSlot(str)
    def send_bookmark_edit(self, schema_and_id: str):
        self.bookmark_edit.emit(schema_and_id)

    @pyqtSlot(str)
    def emit_copy_clipboard_text(self, text: str):
        self.copy_clipboard_text.emit(text)

    @pyqtSlot(str)
    def emit_copy_clipboard_html(self, html: str):
        self.copy_clipboard_html.emit(html)

    @pyqtSlot(str, int, str)
    def emit_copy_gloss(self, db_schema: str, db_id: int, gloss_keys: str):
        self.copy_gloss.emit(db_schema, db_id, gloss_keys)

    @pyqtSlot(str, int)
    def emit_copy_meaning(self, db_schema: str, db_id: int):
        self.copy_meaning.emit(db_schema, db_id)


class ReaderWebEnginePage(QWebEnginePage):
    """ Custom WebEnginePage to customize how we handle link navigation """

    helper: Helper

    show_url_signal = pyqtSignal(str)

    def __init__(self, parent=None):
        super(ReaderWebEnginePage, self).__init__(parent)
        self._parent_window = parent

        self.helper = Helper()

        self.event_channel = QWebChannel()
        self.event_channel.registerObject("helper", self.helper)

        self.setWebChannel(self.event_channel)

    def acceptNavigationRequest(self, url: QUrl, _type: QWebEnginePage.NavigationType, isMainFrame):
        if _type == QWebEnginePage.NavigationType.NavigationTypeLinkClicked:

            self.helper.hide_preview.emit()

            # Allow following relative URLs. Ebooks have HTML Contents pages and
            # cross-links between chapter files.
            #
            # The path sometimes fails when it is from static content linking to
            # files which are now not present such as
            # '../pages/dhamma.html#anatta'

            if url.isRelative() or url.scheme() == 'file':
                logger.info("Following relative link: %s" % url)
                return super().acceptNavigationRequest(url, _type, isMainFrame)

            elif url.scheme() == 'http' or \
               url.scheme() == 'https' or \
               url.scheme() == 'mailto':

                try:
                    QDesktopServices.openUrl(url)
                except Exception as e:
                    logger.error("Can't open %s : %s" % (url, e))

            # ssp://suttas/ud3.10/en/sujato?q=text
            # ssp://words/dhammacakkhu
            elif url.scheme() == 'ssp':

                if self._parent_window is None:
                    return

                if hasattr(self._parent_window, '_show_url'):
                    logger.info(f"self._parent_window._show_url(): {url}")
                    self._parent_window._show_url(url)

                elif hasattr(self._parent_window, 'show_url_signal'):
                    logger.info(f"self._parent_window.show_url_signal.emit(): {url}")
                    self._parent_window.show_url_signal.emit(url.toString())

                else:
                    logger.info(f"No handler found on parent window: {self._parent_window} for url: {url}")
                    return False

            elif url.scheme() == 'bword':

                if self._parent_window is not None and \
                   hasattr(self._parent_window, '_show_word_by_url'):

                    self._parent_window._show_word_by_url(url)

                else:
                    logger.warn("Can't handle: %s" % url)

            else:
                logger.info("Unrecognized scheme: %s" % url)

            return False

        return super().acceptNavigationRequest(url, _type, isMainFrame)


    # called when clicking a link with target="_blank" attribute
    def createWindow(self, _):
        page = ReaderWebEnginePage(self)
        # this will be passed to acceptNavigationRequest, which will open the url
        return page
