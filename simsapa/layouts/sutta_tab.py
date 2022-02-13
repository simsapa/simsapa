from functools import partial
from typing import Optional

from PyQt5.QtCore import Qt, QUrl
from PyQt5.QtWidgets import QWidget, QAction, QVBoxLayout
from PyQt5.QtWebEngineWidgets import QWebEngineView

from simsapa import SIMSAPA_PACKAGE_DIR
from ..app.types import USutta
from .html_content import html_page

class SuttaTabWidget(QWidget):
    def __init__(self,
                 title: str,
                 tab_index: int,
                 qwe: QWebEngineView,
                 api_url: Optional[str] = None,
                 sutta: Optional[USutta] = None) -> None:

        super().__init__()

        self.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose, True)
        self.setProperty('style_class', 'sutta_tab')

        self.title = title
        self.tab_index = tab_index
        self.qwe = qwe
        self.api_url = api_url
        self.sutta = sutta

        self._layout = QVBoxLayout()
        self._layout.setContentsMargins(0, 0, 0, 0)
        self.setLayout(self._layout)
        self._layout.addWidget(self.qwe, 100)

        self.devToolsAction = QAction("Show Inspector", qwe)
        self.devToolsAction.setCheckable(True)
        self.devToolsAction.triggered.connect(partial(self._toggle_dev_tools_inspector))

        self.qwe.addAction(self.devToolsAction)

    def set_content_html(self, html: str):
        self.qwe.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def render_sutta_content(self):
        if self.sutta is None:
            return

        if self.sutta.content_html is not None and self.sutta.content_html != '':
            content = str(self.sutta.content_html)
        elif self.sutta.content_plain is not None and self.sutta.content_plain != '':
            content = '<pre>' + str(self.sutta.content_plain) + '</pre>'
        else:
            content = 'No content.'

        html = html_page(content, self.api_url)

        self.set_content_html(html)

    def _toggle_dev_tools_inspector(self):
        if self.devToolsAction.isChecked():
            self.dev_view = QWebEngineView()
            self._layout.addWidget(self.dev_view, 100)
            self.qwe.page().setDevToolsPage(self.dev_view.page())
        else:
            self.qwe.page().devToolsPage().deleteLater()
            self.dev_view.deleteLater()
