import json
import os
import os.path
from pathlib import Path
from typing import List, Optional, TypedDict, Union

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.sql.functions import func

from PyQt5.QtGui import QClipboard

from simsapa import logger
from simsapa.app.actions_manager import ActionsManager

from .db.search import SearchIndexed

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import APP_DB_PATH, GRAPHS_DIR, USER_DB_PATH, SIMSAPA_DIR, ASSETS_DIR
from simsapa.app.helpers import find_or_create_db


USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]
UDeck = Union[Am.Deck, Um.Deck]
UMemo = Union[Am.Memo, Um.Memo]
UDocument = Union[Am.Document, Um.Document]


class Labels(TypedDict):
    appdata: List[str]
    userdata: List[str]


class AppSettings(TypedDict):
    disabled_sutta_labels: Labels
    disabled_dict_labels: Labels
    notify_about_updates: bool
    suttas_show_pali_buttons: bool
    dictionary_show_pali_buttons: bool

class AppMessage(TypedDict):
    kind: str
    text: str

class AppData:

    app_settings: AppSettings

    def __init__(self,
                 actions_manager: Optional[ActionsManager] = None,
                 app_clipboard: Optional[QClipboard] = None,
                 app_db_path: Optional[Path] = None,
                 user_db_path: Optional[Path] = None,
                 api_port: Optional[int] = None,
                 silent_index_if_empty: bool = True):

        self.clipboard: Optional[QClipboard] = app_clipboard

        self.actions_manager = actions_manager

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

        self._read_app_settings()
        self._ensure_user_memo_deck()

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
            logger.error(f"Can't connect to database: {e}")
            exit(1)

        return (db_conn, db_session)

    def _read_app_settings(self):
        x = self.db_session \
                .query(Um.AppSetting) \
                .filter(Um.AppSetting.key == 'app_settings') \
                .first()

        if x is not None:
            self.app_settings: AppSettings = json.loads(x.value)
        else:
            self.app_settings = AppSettings(
                disabled_sutta_labels = Labels(
                    appdata = [],
                    userdata = [],
                ),
                disabled_dict_labels = Labels(
                    appdata = [],
                    userdata = [],
                ),
                notify_about_updates = True,
                suttas_show_pali_buttons = True,
                dictionary_show_pali_buttons = True,
            )
            self._save_app_settings()

    def _save_app_settings(self):
        x = self.db_session \
                .query(Um.AppSetting) \
                .filter(Um.AppSetting.key == 'app_settings') \
                .first()

        try:
            if x is not None:
                x.value = json.dumps(self.app_settings)
                x.updated_at = func.now()
                self.db_session.commit()
            else:
                x = Um.AppSetting(
                    key = 'app_settings',
                    value = json.dumps(self.app_settings),
                    created_at = func.now(),
                )
                self.db_session.add(x)
                self.db_session.commit()
        except Exception as e:
            logger.error(e)

    def _ensure_user_memo_deck(self):
        deck = self.db_session.query(Um.Deck).first()
        if deck is None:
            deck = Um.Deck(name = "Simsapa")
            self.db_session.add(deck)
            self.db_session.commit()

    def clipboard_setText(self, text):
        if self.clipboard is not None:
            self.clipboard.clear()
            self.clipboard.setText(text)

    def clipboard_getText(self) -> Optional[str]:
        if self.clipboard is not None:
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

    if not GRAPHS_DIR.exists():
        GRAPHS_DIR.mkdir(parents=True, exist_ok=True)
