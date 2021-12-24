from PyQt5.QtWidgets import QHBoxLayout, QWidget, QVBoxLayout, QLabel

from simsapa.app.db.search import SearchResult

class SearchItemWidget(QWidget):
    def __init__(self, parent=None):
        super(SearchItemWidget, self).__init__(parent)

        self.layout: QVBoxLayout = QVBoxLayout()
        self.setLayout(self.layout)

        self.top_info: QHBoxLayout = QHBoxLayout()

        self.title = QLabel()
        self.title.setWordWrap(True)
        self.author = QLabel()
        self.author.setWordWrap(True)

        self.top_info.addWidget(self.title)
        self.top_info.addWidget(self.author)

        self.layout.addLayout(self.top_info)

        self.snippet = QLabel()
        self.snippet.setWordWrap(True)

        self.snippet.setFixedHeight(30)

        self.layout.addWidget(self.snippet)

        self.details = QLabel()
        self.details.setWordWrap(True)

        self.layout.addWidget(self.details)

    def setFromResult(self, r: SearchResult):
        if r['ref'] is not None:
            self.setTitle(f"{r['ref']} {r['title']}")
        else:
            self.setTitle(r['title'])

        if r['author'] is not None:
            self.setAuthor(r['author'])

        self.setSnippet(r['snippet'])

        if r['uid'] is not None:
            self.setDetails(r['uid'])

    def setTitle(self, text: str):
        if len(text.strip()) == 0:
            text = "(Untitled)"
        text = f"<b>{text}</b>"
        self.title.setText(text)

    def setAuthor(self, text: str):
        if len(text.strip()) == 0:
            self.author.clear()
            return
        text = f"<b>{text}</b>"
        self.author.setText(text)

    def setSnippet(self, text):
        self.snippet.setText(text)

    def setDetails(self, text: str):
        if len(text.strip()) == 0:
            self.details.clear()
            return
        text = f"<i>{text}</i>"
        self.details.setText(text)
