from pathlib import Path
import os
import os.path
import appdirs  # type: ignore
import logging as _logging

from sqlalchemy import create_engine, text  # type: ignore
from sqlalchemy_utils import database_exists, create_database  # type: ignore
from sqlalchemy.orm import sessionmaker  # type: ignore

import simsapa.app.db_models as db_models

logger = _logging.getLogger(__name__)

SIMSAPA_DIR = Path(appdirs.user_data_dir('simsapa'))
ASSETS_DIR = SIMSAPA_DIR.joinpath('assets')
APP_DB_PATH = ASSETS_DIR.joinpath('appdata.sqlite3')
USER_DB_PATH = ASSETS_DIR.joinpath('userdata.sqlite3')


class AppData:
    def __init__(self, app_db_path=None, user_db_path=None):

        if app_db_path is None:
            app_db_path = self._find_app_data_or_exit()

        if user_db_path is None:
            user_db_path = self._find_user_data_or_create()

        self.app_db_conn, self.app_db_session = self._connect_to_db(app_db_path)
        self.user_db_conn, self.user_db_session = self._connect_to_db(user_db_path)

    def _connect_to_db(self, db_path):
        if os.path.isfile(db_path):
            try:
                engine = create_engine(f"sqlite+pysqlite:///{db_path}", echo=False, future=True)
                db_conn = engine.connect()
                Session = sessionmaker(engine)
                Session.configure(bind=engine)
                db_session = Session()
            except Exception as e:
                logger.error(f"Can't connect to database: {db_path}")
                print(e)
                exit(1)
        else:
            logger.error(f"Database file doesn't exist: {db_path}")
            exit(1)

        return (db_conn, db_session)

    def _find_app_data_or_exit(self):
        if not APP_DB_PATH.exists():
            logger.error("Cannot find appdata.sqlite3")
            exit(1)
        else:
            return APP_DB_PATH

    def _find_user_data_or_create(self):
        if not USER_DB_PATH.exists():
            logger.info("Cannot find userdata.sqlite3, creating it")

            engine = create_engine(f"sqlite+pysqlite:///{USER_DB_PATH}",
                                   echo=False, future=True)
            if not database_exists(engine.url):
                create_database(engine.url)

            with engine.connect() as db_conn:
                for s in db_models.USERDATA_CREATE_SCHEMA_SQL.split(';'):
                    db_conn.execute(text(s))

        return USER_DB_PATH


class DictWord:
    def __init__(self, word: str):
        self.word = word
        self.definition_md = ''


class Sutta:
    def __init__(self, uid: str, title: str, content_html: str):
        self.uid = uid
        self.title = title
        self.content_html = content_html


def create_app_dirs():
    if not SIMSAPA_DIR.exists():
        os.mkdir(SIMSAPA_DIR)

    if not ASSETS_DIR.exists():
        os.mkdir(ASSETS_DIR)
