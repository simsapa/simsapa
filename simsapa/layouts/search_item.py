from PyQt5.QtWidgets import QFrame, QHBoxLayout, QSpacerItem, QWidget, QVBoxLayout, QLabel, QSizePolicy

from simsapa.app.db.search import SearchResult

class SearchItemWidget(QWidget):
    def __init__(self, parent=None):
        super(SearchItemWidget, self).__init__(parent)

        self.layout: QVBoxLayout = QVBoxLayout()
        self.setLayout(self.layout)

        self.top_info: QHBoxLayout = QHBoxLayout()

        self.title = QLabel()
        self.author = QLabel()
        self.details = QLabel()

        self.top_spacer = QSpacerItem(20, 0, QSizePolicy.Expanding, QSizePolicy.Minimum)

        self.top_info.addWidget(self.title)

        self.top_info.addItem(self.top_spacer)

        self.top_info.addWidget(self.author)
        self.top_info.addWidget(self.details)

        self.layout.addLayout(self.top_info)

        self.snippet = QLabel()
        self.snippet.setWordWrap(True)

        self.snippet.setFixedHeight(35)
        self.snippet.setContentsMargins(0, 0, 0, 5)

        self.layout.addWidget(self.snippet)

        # self.bottom_border = QFrame()
        # self.bottom_border.setFrameShape(QFrame.Shape.HLine)
        # self.bottom_border.setFrameShadow(QFrame.Shadow.Sunken)

        # self.layout.addWidget(self.bottom_border)

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
