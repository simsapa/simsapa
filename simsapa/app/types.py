import os
import os.path
import logging as _logging
from pathlib import Path
from typing import Optional, Union

from sqlalchemy import create_engine, text
from sqlalchemy.engine import Engine
from sqlalchemy_utils import database_exists, create_database
from sqlalchemy.orm import sessionmaker
from alembic import command
from alembic.config import Config
from alembic.script import ScriptDirectory
from alembic.runtime.migration import MigrationContext

from PyQt5.QtGui import QClipboard

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import APP_DB_PATH, USER_DB_PATH, SIMSAPA_DIR, ASSETS_DIR, ALEMBIC_INI, ALEMBIC_DIR

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
                 api_port: Optional[int] = None):

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

    def _find_user_data_or_create(self):
        # Create an in-memory database
        engine = create_engine("sqlite+pysqlite://", echo=False)

        if isinstance(engine, Engine):
            db_conn = engine.connect()
            user_db_url = f"sqlite+pysqlite:///{USER_DB_PATH}"

            alembic_cfg = Config(f"{ALEMBIC_INI}")
            alembic_cfg.set_main_option('script_location', f"{ALEMBIC_DIR}")
            alembic_cfg.set_main_option('sqlalchemy.url', user_db_url)

            if not database_exists(user_db_url):
                logger.info("Cannot find userdata.sqlite3, creating it")
                # On a new install, create database and all tables with the recent schema.
                create_database(user_db_url)
                db_conn.execute(f"ATTACH DATABASE '{USER_DB_PATH}' AS userdata;")
                Um.metadata.create_all(bind=engine)

                # generate the Alembic version table, "stamping" it with the most recent rev:
                command.stamp(alembic_cfg, "head")

            elif not self._is_db_revision_at_head(alembic_cfg, engine):
                logger.info("userdata.sqlite3 is stale, running migrations")

                if db_conn is not None:
                    # db_conn.execute(f"ATTACH DATABASE '{USER_DB_PATH}' AS userdata;")
                    alembic_cfg.attributes['connection'] = db_conn
                    try:
                        command.upgrade(alembic_cfg, "head")
                    except Exception as e:
                        # NOTE: logger.error() is not printed for some reason.
                        print("ERROR - Failed to run migrations.")
                        print(e)
                        exit(1)
        else:
            logger.error("Can't create in-memory database")

        return USER_DB_PATH

    def _is_db_revision_at_head(self, alembic_cfg: Config, e: Engine) -> bool:
        directory = ScriptDirectory.from_config(alembic_cfg)
        with e.begin() as db_conn:
            context = MigrationContext.configure(db_conn)
            return set(context.get_current_heads()) == set(directory.get_heads())

def create_app_dirs():
    if not SIMSAPA_DIR.exists():
        SIMSAPA_DIR.mkdir(parents=True, exist_ok=True)

    if not ASSETS_DIR.exists():
        ASSETS_DIR.mkdir(parents=True, exist_ok=True)
