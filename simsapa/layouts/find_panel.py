from functools import partial
from PyQt5 import QtCore, QtWidgets, QtWebEngineWidgets
from PyQt5.QtWebEngineWidgets import QWebEnginePage

from PyQt5.QtGui import QKeySequence
from PyQt5.QtWidgets import QHBoxLayout, QLineEdit, QPushButton, QShortcut

# Based on https://stackoverflow.com/a/54888872/195141

class FindPanel(QtWidgets.QWidget):
    searched = QtCore.pyqtSignal(str, QWebEnginePage.FindFlag)
    closed = QtCore.pyqtSignal()

    search_input: QLineEdit

    def __init__(self, parent=None):
        super(FindPanel, self).__init__(parent)

        lay = QHBoxLayout(self)

        done_button = QPushButton('&Done')
        self.case_button = QPushButton('Match &Case')
        self.case_button.setCheckable(True)
        next_button = QPushButton('&Next')
        prev_button = QPushButton('&Previous')

        self.search_input = QtWidgets.QLineEdit()
        self.search_input.setPlaceholderText("Find in Page")
        self.search_input.setClearButtonEnabled(True)
        self.setFocusProxy(self.search_input)

        done_button.clicked.connect(self.closed)
        next_button.clicked.connect(self.update_searching)
        prev_button.clicked.connect(self.on_prev_find)
        self.case_button.clicked.connect(self.update_searching)

        for btn in (self.search_input, self.case_button, next_button, prev_button, done_button, done_button):
            lay.addWidget(btn)
            if isinstance(btn, QtWidgets.QPushButton): btn.clicked.connect(self.setFocus)

        self.search_input.textChanged.connect(self.update_searching)
        self.search_input.returnPressed.connect(self.update_searching)
        self.closed.connect(self.search_input.clear)

        ac_next = QShortcut(QKeySequence.FindNext, self)
        ac_next.activated.connect(partial(next_button.animateClick))

        ac_prev = QShortcut(QKeySequence.FindPrevious, self)
        ac_prev.activated.connect(partial(prev_button.animateClick))

        ac_esc = QShortcut(QKeySequence(QtCore.Qt.Key.Key_Escape), self.search_input)
        ac_esc.activated.connect(self.closed)

    @QtCore.pyqtSlot()
    def on_prev_find(self):
        self.update_searching(QWebEnginePage.FindFlag.FindBackward) # type: ignore

    @QtCore.pyqtSlot()
    def update_searching(self, direction=QtWebEngineWidgets.QWebEnginePage.FindFlag()):
        flag = direction
        if self.case_button.isChecked():
            flag |= QtWebEngineWidgets.QWebEnginePage.FindFlag.FindCaseSensitively
        self.searched.emit(self.search_input.text(), flag)

    def showEvent(self, event):
        super(FindPanel, self).showEvent(event)
        self.setFocus()
