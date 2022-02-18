import re
from typing import List, Union
from PyQt5.QtWidgets import QCheckBox, QDialog, QDialogButtonBox, QLabel, QScrollArea, QVBoxLayout, QWidget

from simsapa import DbSchemaName

from ..app.types import AppData, Labels
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

USutta = Union[Am.Sutta, Um.Sutta]

class SuttaSelectDialog(QDialog):

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)

        self.setWindowTitle("Select Sutta Authors")
        self.setFixedSize(400, 400)

        self._app_data = app_data

        self._checks = {
            DbSchemaName.AppData.value: [],
            DbSchemaName.UserData.value: [],
        }

        # Create scrolling layout

        base_layout = QVBoxLayout(self)
        self.setLayout(base_layout)

        scroll = QScrollArea(self)
        base_layout.addWidget(scroll)
        scroll.setWidgetResizable(True)
        scroll_content = QWidget(scroll)

        self._list_layout = QVBoxLayout(scroll_content)
        scroll_content.setLayout(self._list_layout)
        scroll.setWidget(scroll_content)

        self._list_layout.addWidget(QLabel("<b>Select Sutta Authors</b>"))

        for schema in [DbSchemaName.UserData.value,
                       DbSchemaName.AppData.value]:

            pli_checks = self._create_checks(schema, 'pli')
            en_checks = self._create_checks(schema, 'en')

            if len(pli_checks) + len(en_checks) > 0:
                self._list_layout.addWidget(QLabel(f"<b>{schema.capitalize()}</b>"))

                self._add_checks('Pali', pli_checks)
                self._add_checks('English', en_checks)

                self._checks[schema].extend(pli_checks)
                self._checks[schema].extend(en_checks)

        buttons = QDialogButtonBox.Ok | QDialogButtonBox.Cancel
        self.buttonBox = QDialogButtonBox(buttons)

        self.buttonBox.accepted.connect(self._ok_pressed)
        self.buttonBox.rejected.connect(self.reject)

        base_layout.addWidget(self.buttonBox)

    def _create_checks(self, schema: str, lang: str):
        if schema == DbSchemaName.AppData.value:
            a = self._app_data.db_session \
                            .query(Am.Sutta.uid) \
                            .filter(Am.Sutta.uid.like(f"%/{lang}/%")) \
                            .all()
        else:
            a = self._app_data.db_session \
                            .query(Um.Sutta.uid) \
                            .filter(Um.Sutta.uid.like(f"%/{lang}/%")) \
                            .all()

        def _uid_to_label(x):
            pat = f'.*/{lang}/(.*)'
            return re.sub(pat, r'\1', x['uid'])

        labels = sorted(set(map(_uid_to_label, a)))

        if schema == DbSchemaName.AppData.value:
            checks = list(map(self._appdata_label_to_check, labels))
        else:
            checks = list(map(self._userdata_label_to_check, labels))

        return checks

    def _add_checks(self, title: str, checks: List[QCheckBox]):
        if len(checks) > 0:
            self._list_layout.addWidget(QLabel(f"<b>{title}</b>"))

            for i in checks:
                self._list_layout.addWidget(i)

    def _ok_pressed(self):
        disabled_sutta_labels = Labels(
            userdata = [],
            appdata = [],
        )

        for schema in [DbSchemaName.AppData.value, DbSchemaName.UserData.value]:
            a = filter(lambda x: not x.isChecked(), self._checks[schema])
            disabled_sutta_labels[schema] = list(map(lambda x: x.property('sutta_label'), a))

        self._app_data.app_settings['disabled_sutta_labels'] = disabled_sutta_labels
        self._app_data._save_app_settings()

        self.accept()

    def _appdata_label_to_check(self, label: str):
        return self._label_to_check(label, DbSchemaName.AppData.value)

    def _userdata_label_to_check(self, label: str):
        return self._label_to_check(label, DbSchemaName.UserData.value)

    def _label_to_check(self, label: str, schema: str):
        chk = QCheckBox(label, self)
        chk.setProperty('sutta_label', label)

        if label in self._app_data.app_settings['disabled_sutta_labels'][schema]:
            chk.setChecked(False)
        else:
            chk.setChecked(True)

        return chk
