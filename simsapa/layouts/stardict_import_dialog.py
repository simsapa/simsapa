import logging as _logging
from typing import List, Optional, TypedDict

from pathlib import Path

from PyQt5.QtCore import pyqtSignal
from PyQt5.QtWidgets import (QHBoxLayout, QDialog, QLabel, QPushButton, QPlainTextEdit,
                             QFormLayout, QFileDialog)

from sqlalchemy.sql import func
from sqlalchemy.dialects.sqlite import insert

from simsapa.app.types import AppData

from ..app.db import userdata_models as Um

from ..app.stardict import (StarDictPaths, StarDictIfo, DictEntry,
                            parse_stardict_zip, parse_ifo, stardict_to_dict_entries)

logger = _logging.getLogger(__name__)

class StarDictImportDialog(QDialog):

    accepted = pyqtSignal(dict) # type: ignore
    stardict_paths: Optional[StarDictPaths] = None
    stardict_ifo: Optional[StarDictIfo] = None

    def __init__(self):
        super().__init__()
        self.setWindowTitle("Import a StarDict Dictionary")
        self.build_layout()

    def build_layout(self):
        self.import_btn = QPushButton('Import')
        self.import_btn.setDisabled(True)
        self.import_btn.clicked.connect(self.import_pressed)

        self.close_btn = QPushButton('Close')
        self.close_btn.clicked.connect(self.close_pressed)

        self.open_btn = QPushButton('Select StarDict ZIP...')
        self.open_btn.clicked.connect(self.open_file_dialog)

        form = QFormLayout(self)

        self.open_layout = QHBoxLayout()
        self.open_layout.addWidget(self.open_btn)

        form.addRow(self.open_layout)

        self.info_layout = QHBoxLayout()
        self.info = QLabel()
        self.info_layout.addWidget(self.info)

        form.addRow(self.info_layout)

        self.msg_layout = QHBoxLayout()
        self.msg = QLabel()
        self.msg_layout.addWidget(self.msg)

        form.addRow(self.msg_layout)

        self.buttons_layout = QHBoxLayout()
        self.buttons_layout.addWidget(self.import_btn)
        self.buttons_layout.addWidget(self.close_btn)

        form.addRow(self.buttons_layout)

    def import_pressed(self):
        values = {
            'stardict_paths': self.stardict_paths
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
                   [self.stardict_ifo['bookname'],
                    self.stardict_ifo['description'],
                    self.stardict_ifo['wordcount'] + " words",
                    self.stardict_ifo['date'],
                    self.stardict_ifo['author'],
                   ]))

        self.info.setText("\n".join(a))

    def unlock(self):
        if self.stardict_ifo is None:
            self.import_btn.setDisabled(True)
            return

        # No errors in the message label
        if len(self.msg.text()) == 0:
            self.import_btn.setEnabled(True)

class DbDictEntry(TypedDict):
    word: str
    definition_plain: str
    definition_html: str
    synonyms: str
    url_id: str
    dictionary_id: int

class HasStarDictImportDialog():
    _app_data: AppData

    def init_stardict_import_dialog(self):
        pass

    def parse_stardict_into_db(self, values):
        paths = values['stardict_paths']

        words: List[DictEntry] = stardict_to_dict_entries(paths)
        ifo = parse_ifo(paths)
        dict_title = ifo['bookname']

        a = self._app_data.db_session \
                          .query(Um.Dictionary) \
                          .filter(Um.Dictionary.title == dict_title) \
                          .first()
        if a is None:
            # create a dictionary, commit to get its ID
            dictionary = Um.Dictionary(
                title = dict_title,
                label = dict_title,
                created_at = func.now(),
            )
        else:
            dictionary = a

        try:
            self._app_data.db_session.add(dictionary)
            self._app_data.db_session.commit()
        except Exception as e:
            logger.error(e)

        print(f"Dictionary id: {dictionary.id}")

        # result is in tuples [("hey"), ("ho")]
        a = self._app_data.db_session \
                          .query(Um.DictWord.url_id) \
                          .all()
        url_ids: List[str] = list(map(lambda x: x[0], a))

        def assign_url_id(word: str, counter = 0) -> str:
            if word not in url_ids:
                url_ids.append(word)
                return word
            else:
                c = counter + 1
                w = f"{word}-{counter}"
                return assign_url_id(w, c)

        def db_entries(x: DictEntry) -> DbDictEntry:
            return DbDictEntry(
                # copy values
                word = x['word'],
                definition_plain=x['definition_plain'],
                definition_html=x['definition_html'],
                synonyms=", ".join(x['synonyms']),
                # add missing data
                url_id=assign_url_id(x['word']),
                dictionary_id=dictionary.id, # type: ignore
            )

        db_words: List[DbDictEntry] = list(map(db_entries, words))

        # upsert recommended by docs instead of bulk_insert_mappings
        # Using PostgreSQL ON CONFLICT with RETURNING to return upserted ORM objects
        # https://docs.sqlalchemy.org/en/14/orm/persistence_techniques.html#using-postgresql-on-conflict-with-returning-to-return-upserted-orm-objects

        try:
            stmt = insert(Um.DictWord).values(db_words)

            # FIXME As it is now, always new url_ids are generated. Generate
            # url_ids in a way that updates will apply.
            #
            # FIXME update the other fields of the dict word

            # update the record if url_id already exists
            stmt = stmt.on_conflict_do_update(
                index_elements = [Um.DictWord.url_id],
                set_ = dict(definition_html = stmt.excluded.definition_html)
            )

            self._app_data.db_session.execute(stmt)
            self._app_data.db_session.commit()
        except Exception as e:
            print(e)
            logger.error(e)

    def import_from_stardict_dialog(self):
        d = StarDictImportDialog()
        d.accepted.connect(self.parse_stardict_into_db)
        d.exec_()
