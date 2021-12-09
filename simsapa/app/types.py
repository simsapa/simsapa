import os
import os.path
import logging as _logging
from pathlib import Path
from typing import Optional, Union

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker

from PyQt5.QtGui import QClipboard

from .db.search import SearchIndexed

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import APP_DB_PATH, USER_DB_PATH, SIMSAPA_DIR, ASSETS_DIR
from simsapa.app.helpers import find_or_create_db

logger = _logging.getLogger(__name__)

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]
UDeck = Union[Am.Deck, Um.Deck]
UMemo = Union[Am.Memo, Um.Memo]
UDocument = Union[Am.Document, Um.Document]


class AppData:
    def __init__(self,
                 app_clipboard: Optional[QClipboard] = None,
                 app_db_path: Optional[Path] = None,
                 user_db_path: Optional[Path] = None,
                 api_port: Optional[int] = None,
                 silent_index_if_empty: bool = True):

        self.clipboard: Optional[QClipboard] = app_clipboard

        if app_db_path is None:
            app_db_path = self._find_app_data_or_exit()

        if user_db_path is None:
            user_db_path = self._find_user_data_or_create()

        self.api_url: Optional[str] = None

        if api_port:
            self.api_url = f'http://localhost:{api_port}'

        self.sutta_to_open: Optional[USutta] = None
        self.dict_word_to_open: Optional[UDictWord] = None

        self.db_conn, self.db_session = self._connect_to_db(app_db_path, user_db_path)

        self.search_indexed = SearchIndexed()

        if silent_index_if_empty:
            self.search_indexed.index_all(self.db_session, only_if_empty=True)

    def _connect_to_db(self, app_db_path, user_db_path):
        if not os.path.isfile(app_db_path):
            logger.error(f"Database file doesn't exist: {app_db_path}")
            exit(1)

        if not os.path.isfile(user_db_path):
            logger.error(f"Database file doesn't exist: {user_db_path}")
            exit(1)

        try:
            # Create an in-memory database
            engine = create_engine("sqlite+pysqlite://", echo=False)

            db_conn = engine.connect()

            # Attach appdata and userdata
            db_conn.execute(f"ATTACH DATABASE '{app_db_path}' AS appdata;")
            db_conn.execute(f"ATTACH DATABASE '{user_db_path}' AS userdata;")

            Session = sessionmaker(engine)
            Session.configure(bind=engine)
            db_session = Session()
        except Exception as e:
            logger.error("Can't connect to database.")
            print(e)
            exit(1)

        return (db_conn, db_session)

    def clipboard_setText(self, text):
        if self.clipboard is not None:
            self.clipboard.clear()
            self.clipboard.setText(text)

    def clipboard_getText(self) -> Optional[str]:
        if self.clipboard is not None:
            self.clipboard.clear()
            return self.clipboard.text()
        else:
            return None

    def _find_app_data_or_exit(self):
        if not APP_DB_PATH.exists():
            logger.error("Cannot find appdata.sqlite3")
            exit(1)
        else:
            return APP_DB_PATH

    def _find_user_data_or_create(self) -> Path:
        find_or_create_db(USER_DB_PATH, 'userdata')
        return USER_DB_PATH

def create_app_dirs():
    if not SIMSAPA_DIR.exists():
        SIMSAPA_DIR.mkdir(parents=True, exist_ok=True)

    if not ASSETS_DIR.exists():
        ASSETS_DIR.mkdir(parents=True, exist_ok=True)
