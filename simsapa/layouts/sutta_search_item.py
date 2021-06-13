from PyQt5.QtWidgets import QWidget, QVBoxLayout, QLabel


class SuttaSearchItemWidget(QWidget):
    def __init__(self, parent=None):
        super(SuttaSearchItemWidget, self).__init__(parent)

        self.layout = QVBoxLayout()
        self.setLayout(self.layout)

        self.title = QLabel()
        self.snippet = QLabel()
        self.snippet.setWordWrap(True)

        self.layout.addWidget(self.title)
        self.layout.addWidget(self.snippet)

    def setTitle(self, text):
        self.title.setText(text)

    def setSnippet(self, text):
        self.snippet.setText(text)
