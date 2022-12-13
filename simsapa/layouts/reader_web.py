import json
from functools import partial
from typing import TypedDict
from PyQt6.QtCore import QObject, QUrl, pyqtSignal, pyqtSlot
from PyQt6.QtWebEngineCore import QWebEnginePage
from PyQt6.QtGui import QDesktopServices
from PyQt6.QtWebChannel import QWebChannel

from simsapa import logger
from simsapa.app.types import QueryType


class LinkHoverData(TypedDict):
    href: str
    x: int
    y: int
    width: int
    height: int


class LinkHoverHelper(QObject):

    mouseover = pyqtSignal(dict)
    mouseleave = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)


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
    def link_mouseleave(self, msg: str):
        self.mouseleave.emit(msg)


class ReaderWebEnginePage(QWebEnginePage):
    """ Custom WebEnginePage to customize how we handle link navigation """

    link_hover_helper: LinkHoverHelper

    def __init__(self, parent=None):
        super(ReaderWebEnginePage, self).__init__(parent)
        self._parent_window = parent

        self.link_hover_helper = LinkHoverHelper()

        self.event_channel = QWebChannel()
        self.event_channel.registerObject("link_hover_helper", self.link_hover_helper)

        self.setWebChannel(self.event_channel)

        # self.loadFinished.connect(partial(self._init_channel))


    def acceptNavigationRequest(self, url: QUrl, _type: QWebEnginePage.NavigationType, isMainFrame):
        if _type == QWebEnginePage.NavigationType.NavigationTypeLinkClicked:

            # Don't follow relative URLs. It's usually from static content
            # linking to files (which are now not present) such as
            # '../pages/dhamma.html#anatta'
            if url.isRelative():
                logger.info("Not following relative links: %s" % url)

            elif url.scheme() == 'http' or \
               url.scheme() == 'https' or \
               url.scheme() == 'mailto':

                try:
                    QDesktopServices.openUrl(url)
                except Exception as e:
                    logger.error("Can't open %s : %s" % (url, e))

            # ssp://suttas/ud3.10/en/sujato?q=text
            elif url.scheme() == 'ssp' and url.host() == QueryType.suttas:

                if self._parent_window is None:
                    return

                if hasattr(self._parent_window, '_show_sutta_by_url'):
                    self._parent_window._show_sutta_by_url(url)

            elif url.scheme() == 'bword':

                if self._parent_window is not None and \
                   hasattr(self._parent_window, '_show_word_by_bword_url'):

                    self._parent_window._show_word_by_bword_url(url)

                else:
                    logger.warn("Can't handle: %s" % url)

            else:
                logger.info("Unrecognized sheme: %s" % url)

            return False

        return super().acceptNavigationRequest(url, _type, isMainFrame)


    # called when clicking a link with target="_blank" attribute
    def createWindow(self, _type):
        page = ReaderWebEnginePage(self)
        # this will be passed to acceptNavigationRequest, which will open the url
        return page
