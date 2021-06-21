import os
import os.path
import logging as _logging

from typing import Union

from sqlalchemy import create_engine, text  # type: ignore
from sqlalchemy_utils import database_exists, create_database  # type: ignore
from sqlalchemy.orm import sessionmaker  # type: ignore

from PyQt5.QtGui import QClipboard

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import APP_DB_PATH, USER_DB_PATH, SIMSAPA_DIR, ASSETS_DIR, SIMSAPA_MIGRATIONS_DIR

logger = _logging.getLogger(__name__)

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]
UDeck = Union[Am.Deck, Um.Deck]
UMemo = Union[Am.Memo, Um.Memo]
UDocument = Union[Am.Document, Um.Document]


class AppData:
    def __init__(self,  app_clipboard: QClipboard, app_db_path=None, user_db_path=None):

        self.clipboard: QClipboard = app_clipboard

        if app_db_path is None:
            app_db_path = self._find_app_data_or_exit()

        if user_db_path is None:
            user_db_path = self._find_user_data_or_create()

        self.db_conn, self.db_session = self._connect_to_db(app_db_path, user_db_path)

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

    def clipboard_getText(self) -> str:
        if self.clipboard is not None:
            self.clipboard.clear()
            return self.clipboard.text()

    def create_schema_sql() -> str:
        try:
            with open(SIMSAPA_MIGRATIONS_DIR.joinpath('create_schema.sql'), 'r') as f:
                data = f.read()
        except Exception as e:
            logger.error(e)
            return ""
        return data

    def _find_app_data_or_exit(self):
        if not APP_DB_PATH.exists():
            logger.error("Cannot find appdata.sqlite3")
            exit(1)
        else:
            return APP_DB_PATH

    def _find_user_data_or_create(self):
        if not USER_DB_PATH.exists():
            logger.info("Cannot find userdata.sqlite3, creating it")

            engine = create_engine(f"sqlite+pysqlite:///{USER_DB_PATH}", echo=False)

            if not database_exists(engine.url):
                create_database(engine.url)

            with engine.connect() as db_conn:
                sql = self.create_schema_sql()
                for s in sql.split(';'):
                    db_conn.execute(text(s))

        return USER_DB_PATH


def create_app_dirs():
    if not SIMSAPA_DIR.exists():
        SIMSAPA_DIR.mkdir(parents=True, exist_ok=True)

    if not ASSETS_DIR.exists():
        ASSETS_DIR.mkdir(parents=True, exist_ok=True)
