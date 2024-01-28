from PyQt6.QtWidgets import QHBoxLayout, QSpacerItem, QWidget, QVBoxLayout, QLabel, QSizePolicy
from simsapa import DbSchemaName, SearchResult

from simsapa.layouts.gui_types import QExpanding, QMinimum, SearchResultSizes

class SearchItemWidget(QWidget):
    def __init__(self, sizes: SearchResultSizes, parent=None):
        super(SearchItemWidget, self).__init__(parent)

        self.sizes = sizes

        font_style = f"font-family: {sizes['font_family']}; font-size: {sizes['font_size']}pt;"

        self.layout: QVBoxLayout = QVBoxLayout()
        self.layout.setContentsMargins(0, sizes['vertical_margin'], 0, sizes['vertical_margin'])

        self.setLayout(self.layout)

        self.top_info: QHBoxLayout = QHBoxLayout()
        self.top_info.setContentsMargins(0, 0, 0, 0)

        self.title = QLabel()
        self.title.setStyleSheet(font_style)
        self.title.setContentsMargins(0, 0, 0, 0)
        self.title.setMaximumHeight(self.sizes["header_height"])

        self.author = QLabel()
        self.author.setStyleSheet(font_style)
        self.author.setContentsMargins(0, 0, 0, 0)
        self.author.setMaximumHeight(self.sizes["header_height"])

        self.details = QLabel()
        self.details.setStyleSheet(font_style)
        self.details.setContentsMargins(0, 0, 0, 0)
        self.details.setMaximumHeight(self.sizes["header_height"])

        self.top_info.addWidget(self.title)
        self.top_info.addItem(QSpacerItem(0, 0, QExpanding, QMinimum))

        self.top_info.addWidget(self.author)
        self.top_info.addWidget(self.details)

        self.layout.addLayout(self.top_info)

        self.snippet = QLabel()
        self.snippet.setWordWrap(True)
        self.snippet.setStyleSheet(font_style)

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
        span.wrap { color: #000000; }
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
