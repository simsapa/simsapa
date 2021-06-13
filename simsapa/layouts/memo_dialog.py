from PyQt5.QtCore import pyqtSignal
from PyQt5.QtWidgets import (QDialog, QPushButton, QLineEdit, QFormLayout)


class MemoDialog(QDialog):

    accepted = pyqtSignal(dict)

    def __init__(self, text=''):
        super().__init__()
        self.front = QLineEdit()
        self.front.textEdited[str].connect(self.unlock)

        self.back = QLineEdit(text)
        self.back.textEdited[str].connect(self.unlock)

        self.btn = QPushButton('OK')
        self.btn.setDisabled(True)
        self.btn.clicked.connect(self.ok_pressed)

        form = QFormLayout(self)

        form.addRow('Front', self.front)
        form.addRow('Back', self.back)
        form.addRow(self.btn)

    def not_blanks(self) -> bool:
        return self.front.text().strip() != '' and self.back.text().strip() != ''

    def unlock(self):
        if self.not_blanks():
            self.btn.setEnabled(True)
        else:
            self.btn.setDisabled(True)

    def ok_pressed(self):
        values = {
            'Front': self.front.text(),
            'Back': self.back.text()
        }
        self.accepted.emit(values)
        self.accept()
