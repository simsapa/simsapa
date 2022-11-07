from PyQt6.QtWidgets import QHBoxLayout, QSpacerItem, QWidget, QVBoxLayout, QLabel, QSizePolicy
from simsapa import IS_MAC, DbSchemaName

from simsapa.app.db.search import SearchResult
from simsapa.app.types import SearchResultSizes

class SearchItemWidget(QWidget):
    def __init__(self, sizes: SearchResultSizes, parent=None):
        super(SearchItemWidget, self).__init__(parent)

        self.sizes = sizes

        self.layout: QVBoxLayout = QVBoxLayout()
        self.setLayout(self.layout)

        self.top_info: QHBoxLayout = QHBoxLayout()

        self.title = QLabel()
        self.title.setMaximumHeight(self.sizes["header_height"])

        self.author = QLabel()
        self.author.setMaximumHeight(self.sizes["header_height"])

        self.details = QLabel()
        self.details.setMaximumHeight(self.sizes["header_height"])

        self.top_spacer = QSpacerItem(self.sizes["header_height"], 0, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

        self.top_info.addWidget(self.title)

        self.top_info.addItem(self.top_spacer)

        self.top_info.addWidget(self.author)
        self.top_info.addWidget(self.details)

        self.layout.addLayout(self.top_info)

        self.snippet = QLabel()
        self.snippet.setWordWrap(True)

        if IS_MAC:
            self.snippet.setStyleSheet(f"font-family: Helvetica; font-size: {sizes['snippet_font_size']}pt;")
        else:
            self.snippet.setStyleSheet(f"font-family: DejaVu Sans; font-size: {sizes['snippet_font_size']}pt;")

        self.snippet.setMinimumHeight(self.sizes["snippet_min_height"])
        self.snippet.setMaximumHeight(self.sizes["snippet_max_height"])
        self.snippet.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

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

        if len(title) > 70:
            title = title[:70] + '...'

        self.setTitle(f"{style}<span class='wrap'>{title}</span>")

        if r['author'] is not None:
            self.setAuthor(f"{style}<span class='wrap'>{r['author']}</span>")

        snippet = r['snippet'].strip() \
                              .replace("\n", " ") \
                              .replace("  ", " ")
        n = self.sizes['snippet_length']
        snippet = snippet[0:n]
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
