from PyQt5.QtWidgets import QHBoxLayout, QSpacerItem, QWidget, QVBoxLayout, QLabel, QSizePolicy
from simsapa import IS_MAC, DbSchemaName

from simsapa.app.db.search import SearchResult

class SearchItemWidget(QWidget):
    def __init__(self, parent=None):
        super(SearchItemWidget, self).__init__(parent)

        self.layout: QVBoxLayout = QVBoxLayout()
        self.setLayout(self.layout)

        self.top_info: QHBoxLayout = QHBoxLayout()

        self.title = QLabel()
        self.title.setMaximumHeight(20)

        self.author = QLabel()
        self.author.setMaximumHeight(20)

        self.details = QLabel()
        self.details.setMaximumHeight(20)

        self.top_spacer = QSpacerItem(20, 0, QSizePolicy.Expanding, QSizePolicy.Minimum)

        self.top_info.addWidget(self.title)

        self.top_info.addItem(self.top_spacer)

        self.top_info.addWidget(self.author)
        self.top_info.addWidget(self.details)

        self.layout.addLayout(self.top_info)

        self.snippet = QLabel()
        self.snippet.setWordWrap(True)

        if IS_MAC:
            self.snippet.setStyleSheet("font-family: DejaVu Sans; font-size: 10pt;")
        else:
            self.snippet.setStyleSheet("font-family: DejaVu Sans; font-size: 9pt;")

        self.snippet.setMinimumHeight(25)
        self.snippet.setMaximumHeight(60)
        self.snippet.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Minimum)

        self.snippet.setContentsMargins(0, 0, 0, 0)

        self.layout.addWidget(self.snippet)

        # self.bottom_border = QFrame()
        # self.bottom_border.setFrameShape(QFrame.Shape.HLine)
        # self.bottom_border.setFrameShadow(QFrame.Shadow.Sunken)

        # self.layout.addWidget(self.bottom_border)

    def setFromResult(self, r: SearchResult):
        style = """<style>
        span.wrap { color: black; }
        span.match { background-color: yellow; }
        </style>"""

        if r['ref'] is not None:
            title = f"{r['ref']} {r['title']}"
        else:
            title = r['title']

        self.setTitle(f"{style}<span class='wrap'>{title}</span>")

        if r['author'] is not None:
            self.setAuthor(f"{style}<span class='wrap'>{r['author']}</span>")

        snippet = r['snippet'].strip() \
                              .replace("\n", " ") \
                              .replace("  ", " ")
        snippet = snippet[0:500]
        self.setSnippet(f"{style}<span class='wrap'>{snippet}</span>")

        details = ''

        if r['uid'] is not None:
            details = r['uid']

        if r['schema_name'] == DbSchemaName.UserData.value:
            details += ' (u)'

        self.setDetails(f"{style}<span class='wrap'>{details}</span>")

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
