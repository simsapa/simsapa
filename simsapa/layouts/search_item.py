import re
from typing import Optional, TypedDict

import bleach

from PyQt5.QtWidgets import QWidget, QVBoxLayout, QLabel


class SearchResult(TypedDict):
    title: str
    snippet: str
    table: str
    id: int
    page_number: Optional[int]


class SearchItemWidget(QWidget):
    def __init__(self, parent=None):
        super(SearchItemWidget, self).__init__(parent)

        self.layout: QVBoxLayout = QVBoxLayout()
        self.setLayout(self.layout)

        self.title = QLabel()

        self.snippet = QLabel()
        self.snippet.setWordWrap(True)

        self.snippet.setFixedHeight(30)

        self.layout.addWidget(self.title)
        self.layout.addWidget(self.snippet)

    def setTitle(self, text):
        self.title.setText(text)

    def setSnippet(self, text):
        self.snippet.setText(self.compactRichText(text))

    def compactRichText(self, text: str) -> str:
        text = bleach.clean(text, strip=True)

        # clean up whitespace so that all text is one line
        text = text.replace("\n", ' ')
        text = re.sub(r"  +", ' ', text)

        return text
