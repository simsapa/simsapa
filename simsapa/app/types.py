from enum import Enum
import json
import os
import os.path
from pathlib import Path
from typing import List, Optional, TypedDict, Union

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.sql.functions import func

from PyQt5.QtGui import QClipboard

from simsapa import DbSchemaName, logger
from simsapa.app.actions_manager import ActionsManager

from .db.search import SearchIndexed

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import APP_DB_PATH, USER_DB_PATH
from simsapa.app.helpers import find_or_create_db


USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]
UDeck = Union[Am.Deck, Um.Deck]
UMemo = Union[Am.Memo, Um.Memo]
UDocument = Union[Am.Document, Um.Document]

class Labels(TypedDict):
    appdata: List[str]
    userdata: List[str]

class WindowType(int, Enum):
    SuttaSearch = 0
    DictionarySearch = 1
    Memos = 2
    Links = 3

class WindowPosSize(TypedDict):
    x: int
    y: int
    width: int
    height: int

WindowNameToType = {
    "Sutta Search": WindowType.SuttaSearch,
    "Dictionary Search": WindowType.DictionarySearch,
    "Memos": WindowType.Memos,
    "Links": WindowType.Links,
}

class AppSettings(TypedDict):
    disabled_sutta_labels: Labels
    disabled_dict_labels: Labels
    notify_about_updates: bool
    suttas_show_pali_buttons: bool
    dictionary_show_pali_buttons: bool
    show_toolbar: bool
    first_window_on_startup: WindowType
    word_scan_popup_pos: WindowPosSize
    show_related_suttas: bool

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
                 silent_index_if_empty: bool = False):

        self.clipboard: Optional[QClipboard] = app_clipboard

        self.actions_manager = actions_manager

        if app_db_path is None:
            app_db_path = self._find_app_data_or_exit()

        if user_db_path is None:
            user_db_path = self._find_user_data_or_create()

        self.silent_index_if_empty = silent_index_if_empty

        self.api_url: Optional[str] = None

        if api_port:
            self.api_url = f'http://localhost:{api_port}'

        self.sutta_to_open: Optional[USutta] = None
        self.dict_word_to_open: Optional[UDictWord] = None

        self._connect_to_db(app_db_path, user_db_path)

        self.search_indexed = SearchIndexed()

        self._read_app_settings()
        self._ensure_user_memo_deck()

    def _connect_to_db(self, app_db_path, user_db_path):
        if not os.path.isfile(app_db_path):
            logger.error(f"Database file doesn't exist: {app_db_path}")
            exit(1)

        if not os.path.isfile(user_db_path):
            logger.error(f"Database file doesn't exist: {user_db_path}")
            exit(1)

        try:
            # Create an in-memory database
            self.db_eng = create_engine("sqlite+pysqlite://", echo=False)

            self.db_conn = self.db_eng.connect()

            # Attach appdata and userdata
            self.db_conn.execute(f"ATTACH DATABASE '{app_db_path}' AS appdata;")
            self.db_conn.execute(f"ATTACH DATABASE '{user_db_path}' AS userdata;")

            Session = sessionmaker(self.db_eng)
            Session.configure(bind=self.db_eng)
            self.db_session = Session()

        except Exception as e:
            logger.error(f"Can't connect to database: {e}")
            exit(1)

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
                show_toolbar = False,
                first_window_on_startup = WindowType.SuttaSearch,
                word_scan_popup_pos = WindowPosSize(
                    x = 100,
                    y = 100,
                    width = 400,
                    height = 500,
                ),
                show_related_suttas = True,
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
        find_or_create_db(USER_DB_PATH, DbSchemaName.UserData.value)
        return USER_DB_PATH

