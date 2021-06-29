import re

from PyQt5.QtWidgets import QWidget, QVBoxLayout, QLabel


class SearchItemWidget(QWidget):
    def __init__(self, parent=None):
        super(SearchItemWidget, self).__init__(parent)

        self.layout = QVBoxLayout()
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

    def compactRichText(self, text) -> str:
        # remove div, p, br tags, but leave the contents
        text = re.sub(r"<div[^>]*>", '', text)
        text = text.replace('</div>', '')
        text = re.sub(r"<p[^>]*>", '', text)
        text = text.replace('</p>', '')
        text = re.sub(r"<br[^>]*>", '', text)

        # clean up whitespace so that all text is one line
        text = text.replace("\n", ' ')
        text = re.sub(r"  +", ' ', text)

        return text
