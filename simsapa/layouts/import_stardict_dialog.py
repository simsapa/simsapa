import os.path
import logging as _logging
import shutil
from typing import List, Optional, TypedDict

from pathlib import Path

from PyQt5.QtCore import Qt, pyqtSignal
from PyQt5.QtWidgets import QComboBox, QDialog, QFileDialog, QLabel, QLineEdit, QTabWidget

from simsapa.app.types import AppData

from ..assets.ui.import_stardict_dialog_ui import Ui_ImportStarDictDialog

from ..app.db import userdata_models as Um

from ..app.stardict import StarDictPaths, StarDictIfo, parse_stardict_zip, parse_ifo
from ..app.db.stardict import import_stardict_into_db_as_new, import_stardict_into_db_update_existing

logger = _logging.getLogger(__name__)

class DictData(TypedDict):
    id: int
    select_idx: int
    title: str
    label: str

class ImportStarDictDialog(QDialog, Ui_ImportStarDictDialog):

    accepted = pyqtSignal(dict) # type: ignore
    stardict_paths: Optional[StarDictPaths] = None
    stardict_ifo: Optional[StarDictIfo] = None

    msg: QLabel
    title: QLineEdit
    label: QLineEdit
    select_title: QComboBox
    tabWidget: QTabWidget

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)

        self._app_data: AppData = app_data

        self.dict_select_data: List[DictData] = []

        self._ui_setup()
        self._connect_signals()

    def _ui_setup(self):
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)

        self.title.setFixedSize(500, 40)
        self.label.setFixedSize(500, 40)

        self.info.setText("Select a StarDict .zip file to open.")
        self.msg.clear()
        self.selected_info.clear()

        # fill select_title with title - label
        self.select_title.insertItem(0, 'Select dictionary')

        # TODO also consider appdata
        a = self._app_data.db_session \
                          .query(Um.Dictionary.id,
                                 Um.Dictionary.title,
                                 Um.Dictionary.label) \
                          .all()

        # Add the titles to the selection, with the (dictionary.id, dictionary.label) as item data.
        select_item_counter = 1
        for i in a:
            self.select_title.insertItem(
                select_item_counter,
                f"{i[1]} - {i[2]}",
                (i[0], i[2]))

            self.dict_select_data.append(DictData(
                id = i[0],
                select_idx = select_item_counter,
                title = i[1],
                label = i[2],
            ))

            select_item_counter += 1

    def _connect_signals(self):
        self.import_new_btn.clicked.connect(self.import_new_pressed)
        self.update_existing_btn.clicked.connect(self.update_existing_pressed)
        self.close_btn.clicked.connect(self.close_pressed)
        self.open_btn.clicked.connect(self.open_file_dialog)

        self.title.textChanged.connect(self.unlock)
        self.label.textChanged.connect(self.unlock)
        self.select_title.currentIndexChanged.connect(self.unlock)

    def import_new_pressed(self):
        values = {
            'action': 'import_new',
            'stardict_paths': self.stardict_paths,
            'title': self.title.text(),
            'label': self.label.text(),
        }
        self.accepted.emit(values)
        self.accept()

    def update_existing_pressed(self):
        idx = self.select_title.currentIndex()
        if idx == 0:
            return
        dictionary_id, label = self.select_title.itemData(idx)

        values = {
            'action': 'update_existing',
            'stardict_paths': self.stardict_paths,
            'dictionary_id': dictionary_id,
            'label': label,
        }
        self.accepted.emit(values)
        self.accept()

    def close_pressed(self):
        self.close()

    def open_file_dialog(self):
        file_path, _ = QFileDialog.getOpenFileName(
            None,
            "Open File...",
            "",
            "ZIP Files (*.zip)")

        if len(file_path) != 0:
            self.open_stardict_zip(file_path)
            self.unlock()

    def open_stardict_zip(self, zip_path: str):
        self.msg.clear()

        try:
            self.stardict_paths = parse_stardict_zip(Path(zip_path))
            self.stardict_ifo = parse_ifo(self.stardict_paths)
        except Exception as e:
            logger.error(e)
            self.msg.setText(f"{e}")
            return

        a = list(filter(lambda x: len(x) > 0,
                   [os.path.basename(self.stardict_paths['zip_path']),
                    self.stardict_ifo['bookname'],
                    self.stardict_ifo['description'],
                    self.stardict_ifo['wordcount'] + " words",
                    self.stardict_ifo['date'],
                    self.stardict_ifo['author'],
                   ]))

        self.info.setText("\n".join(a))

        title = self.stardict_ifo['bookname']

        self.title.setText(title)

        a = list(filter(lambda x: x['title'] == title, self.dict_select_data))

        # If a dictionary with this title exists, select the update tab
        # with the existing dictionary selected.
        if len(a) > 0:
            self.tabWidget.setCurrentIndex(1)
            self.select_title.setCurrentIndex(a[0]['select_idx'])


    def unlock(self):
        if self.stardict_ifo is None:
            return

        self.unlock_import_new()
        self.unlock_update_existing()

    def unlock_import_new(self):
        self.import_new_btn.setDisabled(True)

        if len(self.title.text()) < 3:
            self.msg.setText("Title must be at least 3 characters long.")
            return

        if len(self.label.text()) < 3:
            self.msg.setText("Label must be at least 3 characters long.")
            # TODO label must be unique
            return

        self.msg.clear()
        self.import_new_btn.setEnabled(True)

    def unlock_update_existing(self):
        self.update_existing_btn.setDisabled(True)

        idx = self.select_title.currentIndex()
        if idx == 0:
            return

        self.update_existing_btn.setEnabled(True)

    def do_import(self, values):
        paths = values['stardict_paths']
        action = values['action']
        label = values['label']

        if action == 'import_new':
            import_stardict_into_db_as_new(self._app_data.db_session, 'userdata', paths, label)
        elif action == 'update_existing':
            id = values['dictionary_id']
            import_stardict_into_db_update_existing(self._app_data.db_session, 'userdata', paths, id, label)

        # remove zip extract dir
        if paths['unzipped_dir'].exists():
            shutil.rmtree(paths['unzipped_dir'])

class HasImportStarDictDialog():
    _app_data: AppData

    def init_stardict_import_dialog(self):
        pass

    def show_import_from_stardict_dialog(self):
        d = ImportStarDictDialog(self._app_data)
        d.accepted.connect(d.do_import)
        d.exec_()
