from typing import Union
from PyQt5.QtWidgets import QCheckBox, QDialog, QDialogButtonBox, QLabel, QVBoxLayout

from ..app.types import AppData, Labels
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

UDictionary = Union[Am.Dictionary, Um.Dictionary]

class DictionarySelectDialog(QDialog):

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)

        self.setWindowTitle("Select Dictionaries")

        self._app_data = app_data

        self._layout = QVBoxLayout()

        self._layout.addWidget(QLabel("<b>Select Dictionaries</b>"))

        a = self._app_data.db_session.query(Um.Dictionary).all()
        self.userdata_checks = list(map(self._dict_to_check, a))

        if len(self.userdata_checks) > 0:
            self._layout.addWidget(QLabel("<b>Userdata</b>"))

            for i in self.userdata_checks:
                self._layout.addWidget(i)

        a = self._app_data.db_session.query(Am.Dictionary).all()
        self.appdata_checks = list(map(self._dict_to_check, a))

        if len(self.appdata_checks) > 0:
            self._layout.addWidget(QLabel("<b>Appdata</b>"))

            for i in self.appdata_checks:
                self._layout.addWidget(i)

        buttons = QDialogButtonBox.Ok | QDialogButtonBox.Cancel
        self.buttonBox = QDialogButtonBox(buttons)

        self.buttonBox.accepted.connect(self._ok_pressed)
        self.buttonBox.rejected.connect(self.reject)

        self._layout.addWidget(self.buttonBox)
        self.setLayout(self._layout)

    def _ok_pressed(self):
        a = filter(lambda x: not x.isChecked(), self.userdata_checks)
        disabled_dict_labels = Labels(
            userdata = [],
            appdata = [],
        )
        disabled_dict_labels['userdata'] = list(map(lambda x: x.property('dict_label'), a))

        a = filter(lambda x: not x.isChecked(), self.appdata_checks)
        disabled_dict_labels['appdata'] = list(map(lambda x: x.property('dict_label'), a))

        self._app_data.app_settings['disabled_dict_labels'] = disabled_dict_labels
        self._app_data._save_app_settings()

        self.accept()

    def _dict_to_check(self, d: UDictionary):
        chk = QCheckBox(f"{d.label} - {d.title}", self)
        chk.setProperty('dict_label', d.label)

        if d.label in self._app_data.app_settings['disabled_dict_labels'][d.metadata.schema]:
            chk.setChecked(False)
        else:
            chk.setChecked(True)

        return chk
