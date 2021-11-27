import os.path
import logging as _logging
from typing import List, Optional, TypedDict

from pathlib import Path

from PyQt5.QtCore import Qt, pyqtSignal
from PyQt5.QtWidgets import QComboBox, QDialog, QFileDialog, QLabel, QLineEdit, QTabWidget

from sqlalchemy.sql import func
from sqlalchemy.dialects.sqlite import insert

from simsapa.app.types import AppData

from ..assets.ui.import_stardict_dialog_ui import Ui_ImportStarDictDialog

from ..app.db import userdata_models as Um

from ..app.stardict import (StarDictPaths, StarDictIfo, DictEntry,
                            parse_stardict_zip, parse_ifo, stardict_to_dict_entries)

logger = _logging.getLogger(__name__)

class DictData(TypedDict):
    id: int
    select_idx: int
    title: str
    label: str

class DbDictEntry(TypedDict):
    word: str
    definition_plain: str
    definition_html: str
    synonyms: str
    url_id: str
    dictionary_id: int

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

        # Add the titles to the selection, with the dictionary.id as item data.
        select_item_counter = 1
        for i in a:
            self.select_title.insertItem(
                select_item_counter,
                f"{i[1]} - {i[2]}",
                i[0])

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
        dictionary_id = self.select_title.itemData(idx)

        values = {
            'action': 'update_existing',
            'stardict_paths': self.stardict_paths,
            'dictionary_id': dictionary_id,
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
        # New Concise - NCPED
        title_label = self.select_title.itemText(idx)

        self.update_existing_btn.setEnabled(True)

    def do_import(self, values):
        paths = values['stardict_paths']
        action = values['action']

        if action == 'import_new':
            self.import_stardict_into_db_as_new(paths)
        elif action == 'update_existing':
            id = values['dictionary_id']
            self.import_stardict_into_db_update_existing(paths, id)

    def db_entries(self, x: DictEntry, dictionary_id: int) -> DbDictEntry:
        return DbDictEntry(
            # copy values
            word = x['word'],
            definition_plain = x['definition_plain'],
            definition_html = x['definition_html'],
            synonyms = ", ".join(x['synonyms']),
            # add missing data
            # TODO should we check for conflicting url_ids? generate with meaning count?
            url_id = x['word'],
            dictionary_id = dictionary_id,
        )

    def insert_db_words(self, db_words: List[DbDictEntry]):
        batch_size = 1000
        inserted = 0

        # TODO: The user can't see this message. Dialog doesn't update while the
        # import is blocking the GUI.
        self.msg.setText("Importing ...")

        while inserted <= len(db_words):
            b_start = inserted
            b_end = inserted + batch_size
            words_batch = db_words[b_start:b_end]

            try:
                stmt = insert(Um.DictWord).values(words_batch)

                # update the record if url_id already exists
                stmt = stmt.on_conflict_do_update(
                    index_elements = [Um.DictWord.url_id],
                    set_ = dict(
                        word = stmt.excluded.word,
                        word_nom_sg = stmt.excluded.word_nom_sg,
                        inflections = stmt.excluded.inflections,
                        phonetic = stmt.excluded.phonetic,
                        transliteration = stmt.excluded.transliteration,
                        # --- Meaning ---
                        meaning_order = stmt.excluded.meaning_order,
                        definition_plain = stmt.excluded.definition_plain,
                        definition_html = stmt.excluded.definition_html,
                        summary = stmt.excluded.summary,
                        # --- Associated words ---
                        synonyms = stmt.excluded.synonyms,
                        antonyms = stmt.excluded.antonyms,
                        homonyms = stmt.excluded.homonyms,
                        also_written_as = stmt.excluded.also_written_as,
                        see_also = stmt.excluded.see_also,
                    )
                )

                self._app_data.db_session.execute(stmt)
                self._app_data.db_session.commit()
            except Exception as e:
                print(e)
                logger.error(e)

            inserted += batch_size
            self.msg.setText(f"Imported {inserted} ...")

    def import_stardict_into_db_update_existing(self, paths: StarDictPaths, dictionary_id: int):
        words: List[DictEntry] = stardict_to_dict_entries(paths)
        db_words: List[DbDictEntry] = list(map(lambda x: self.db_entries(x, dictionary_id), words))
        self.insert_db_words(db_words)

    def import_stardict_into_db_as_new(self, paths: StarDictPaths):
        # upsert recommended by docs instead of bulk_insert_mappings
        # Using PostgreSQL ON CONFLICT with RETURNING to return upserted ORM objects
        # https://docs.sqlalchemy.org/en/14/orm/persistence_techniques.html#using-postgresql-on-conflict-with-returning-to-return-upserted-orm-objects

        words: List[DictEntry] = stardict_to_dict_entries(paths)
        ifo = parse_ifo(paths)
        dict_title = ifo['bookname']

        # create a dictionary, commit to get its ID
        dictionary = Um.Dictionary(
            title = dict_title,
            label = dict_title,
            created_at = func.now(),
        )

        try:
            self._app_data.db_session.add(dictionary)
            self._app_data.db_session.commit()
        except Exception as e:
            logger.error(e)

        d_id: int = dictionary.id # type: ignore
        db_words: List[DbDictEntry] = list(map(lambda x: self.db_entries(x, d_id), words))

        self.insert_db_words(db_words)

class HasImportStarDictDialog():
    _app_data: AppData

    def init_stardict_import_dialog(self):
        pass

    def show_import_from_stardict_dialog(self):
        d = ImportStarDictDialog(self._app_data)
        d.accepted.connect(d.do_import)
        d.exec_()
