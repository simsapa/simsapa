from pathlib import Path
import os.path
import logging as _logging
from sqlalchemy import create_engine, text  # type: ignore
from sqlalchemy_utils import database_exists, create_database  # type: ignore
from sqlalchemy.orm import sessionmaker  # type: ignore

import simsapa.app.db_models as db_models

logger = _logging.getLogger(__name__)


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
        paths = [
            Path.cwd().joinpath("appdata.sqlite3"),
            Path.home().joinpath(".config/simsapa/assets/appdata.sqlite3"),
        ]

        db_path = None

        for p in paths:
            if p.is_file():
                db_path = p

        if db_path is None:
            logger.error("Cannot find appdata.sqlite3")
            exit(1)
        else:
            return db_path

    def _find_user_data_or_create(self):
        homedata = Path.home().joinpath(".config/simsapa/assets/userdata.sqlite3")

        paths = [
            Path.cwd().joinpath("userdata.sqlite3"),
            homedata
        ]

        db_path = None

        for p in paths:
            if p.is_file():
                db_path = p

        if db_path is None:
            logger.info("Cannot find userdata.sqlite3, creating it")

            db_path = homedata
            engine = create_engine(f"sqlite+pysqlite:///{db_path}",
                                   echo=False, future=True)
            if not database_exists(engine.url):
                create_database(engine.url)

            with engine.connect() as db_conn:
                for s in db_models.USERDATA_CREATE_SCHEMA_SQL.split(';'):
                    db_conn.execute(text(s))

        return db_path


class DictWord:
    def __init__(self, word: str):
        self.word = word
        self.definition_md = ''


class Sutta:
    def __init__(self, uid: str, title: str, content_html: str):
        self.uid = uid
        self.title = title
        self.content_html = content_html
