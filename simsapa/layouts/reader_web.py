from PyQt5.QtCore import QUrl
from PyQt5.QtWebEngineWidgets import QWebEnginePage
from PyQt5.QtGui import QDesktopServices

from simsapa import logger

class ReaderWebEnginePage(QWebEnginePage):
    """ Custom WebEnginePage to customize how we handle link navigation """

    def __init__(self, parent=None):
        super(ReaderWebEnginePage, self).__init__(parent)
        self._parent_window = parent

    def acceptNavigationRequest(self, url: QUrl, _type: QWebEnginePage.NavigationType, isMainFrame):
        if _type == QWebEnginePage.NavigationType.NavigationTypeLinkClicked:
            if url.scheme() == 'http' or \
               url.scheme() == 'https' or \
               url.scheme() == 'mailto':

                try:
                    QDesktopServices.openUrl(url)
                except Exception as e:
                    logger.error("Can't open %s : %s" % (url, e))

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
