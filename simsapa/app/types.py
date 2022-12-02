from enum import Enum
from functools import partial
import re
import json
import os
import os.path
from pathlib import Path
from typing import Callable, List, Optional, TypedDict, Union
from PyQt6.QtCore import QObject, QRunnable, QThreadPool, pyqtSignal, pyqtSlot

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.sql.functions import func

from PyQt6 import QtWidgets
from PyQt6.QtGui import QAction, QClipboard
from PyQt6.QtWidgets import QFrame, QLineEdit, QMainWindow, QTabWidget, QToolBar

from simsapa import IS_MAC, DbSchemaName, ShowLabels, logger, APP_DB_PATH, USER_DB_PATH
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.db_helpers import get_db_engine_connection_session, upgrade_db

from .db.search import SearchIndexed

from .db import appdata_models as Am
from .db import userdata_models as Um

QSizeMinimum = QtWidgets.QSizePolicy.Policy.Minimum
QSizeExpanding = QtWidgets.QSizePolicy.Policy.Expanding

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]
ULink = Union[Am.Link, Um.Link]

UDeck = Union[Am.Deck, Um.Deck]
UMemo = Union[Am.Memo, Um.Memo]

UBookmark = Union[Am.Bookmark, Um.Bookmark]
UDocument = Union[Am.Document, Um.Document]

UChallengeCourse = Union[Am.ChallengeCourse, Um.ChallengeCourse]
UChallengeGroup = Union[Am.ChallengeGroup, Um.ChallengeGroup]
UChallenge = Union[Am.Challenge, Um.Challenge]


class Labels(TypedDict):
    appdata: List[str]
    userdata: List[str]

class WindowType(int, Enum):
    SuttaSearch = 0
    SuttaStudy = 1
    DictionarySearch = 2
    Memos = 3
    Links = 4

class WindowPosSize(TypedDict):
    x: int
    y: int
    width: int
    height: int

WindowNameToType = {
    "Sutta Search": WindowType.SuttaSearch,
    "Sutta Study": WindowType.SuttaStudy,
    "Dictionary Search": WindowType.DictionarySearch,
    "Memos": WindowType.Memos,
    "Links": WindowType.Links,
}

class SearchMode(int, Enum):
    FulltextMatch = 0
    ExactMatch = 1
    HeadwordMatch = 2
    TitleMatch = 3

SuttaSearchModeNameToType = {
    "Fulltext Match": SearchMode.FulltextMatch,
    "Exact Match": SearchMode.ExactMatch,
    "Title Match": SearchMode.TitleMatch,
}

DictionarySearchModeNameToType = {
    "Fulltext Match": SearchMode.FulltextMatch,
    "Exact Match": SearchMode.ExactMatch,
    "Headword Match": SearchMode.HeadwordMatch,
}

class SearchResultSizes(TypedDict):
    header_height: int
    snippet_length: int
    snippet_font_size: int
    snippet_min_height: int
    snippet_max_height: int

def default_search_result_sizes() -> SearchResultSizes:
    return SearchResultSizes(
        header_height = 20,
        snippet_length = 500,
        snippet_font_size = 10 if IS_MAC else 9,
        snippet_min_height = 25,
        snippet_max_height = 60,
    )

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
    show_translation_and_pali_line_by_line: bool
    show_all_variant_readings: bool
    sutta_font_size: int
    sutta_max_width: int
    dictionary_font_size: int
    search_result_sizes: SearchResultSizes
    search_as_you_type: bool
    search_completion: bool
    sutta_search_mode: SearchMode
    dictionary_search_mode: SearchMode
    word_scan_search_mode: SearchMode
    sutta_language_filter_idx: int
    sutta_source_filter_idx: int
    dict_filter_idx: int
    word_scan_dict_filter_idx: int
    audio_volume: float

def default_app_settings() -> AppSettings:
    return AppSettings(
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
        show_translation_and_pali_line_by_line = False,
        show_all_variant_readings = True,
        sutta_font_size = 22,
        sutta_max_width = 75,
        dictionary_font_size = 18,
        search_result_sizes = default_search_result_sizes(),
        search_as_you_type = True,
        search_completion = True,
        sutta_search_mode = SearchMode.FulltextMatch,
        dictionary_search_mode = SearchMode.FulltextMatch,
        word_scan_search_mode = SearchMode.FulltextMatch,
        sutta_language_filter_idx = 0,
        sutta_source_filter_idx = 0,
        dict_filter_idx = 0,
        word_scan_dict_filter_idx = 0,
        audio_volume = 1.0,
    )

class CompletionCache(TypedDict):
    sutta_titles: List[str]
    dict_words: List[str]

# Message to show to the user.
class AppMessage(TypedDict):
    kind: str
    text: str

class AppData:

    app_settings: AppSettings
    completion_cache: CompletionCache

    def __init__(self,
                 actions_manager: Optional[ActionsManager] = None,
                 app_clipboard: Optional[QClipboard] = None,
                 app_db_path: Optional[Path] = None,
                 user_db_path: Optional[Path] = None,
                 api_port: Optional[int] = None):

        self.clipboard: Optional[QClipboard] = app_clipboard

        self.actions_manager = actions_manager

        if app_db_path is None:
            app_db_path = self._find_app_data_or_exit()

        user_db_path = USER_DB_PATH

        self.completion_cache = CompletionCache(
            sutta_titles=[],
            dict_words=[],
        )

        self.thread_pool = QThreadPool()

        self.completion_cache_worker = CompletionCacheWorker()
        self.completion_cache_worker.signals.finished.connect(partial(self._set_completion_cache))
        self.thread_pool.start(self.completion_cache_worker)

        self.graph_gen_pool = QThreadPool()

        self.api_url: Optional[str] = None

        if api_port:
            self.api_url = f'http://localhost:{api_port}'

        self.sutta_to_open: Optional[USutta] = None
        self.dict_word_to_open: Optional[UDictWord] = None

        self._connect_to_db(app_db_path, user_db_path)

        self.search_indexed = SearchIndexed()

        self._read_app_settings()
        self._ensure_user_memo_deck()

    def _set_completion_cache(self, values: CompletionCache):
        self.completion_cache = values

    def _connect_to_db(self, app_db_path, user_db_path):
        if not os.path.isfile(app_db_path):
            logger.error(f"Database file doesn't exist: {app_db_path}")
            exit(1)

        upgrade_db(app_db_path, DbSchemaName.AppData.value)

        if not os.path.isfile(user_db_path):
            logger.error(f"Database file doesn't exist: {user_db_path}")
            exit(1)

        upgrade_db(user_db_path, DbSchemaName.UserData.value)

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
            self.app_settings = default_app_settings()
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


class AppWindowInterface(QMainWindow):
    action_Notify_About_Updates: QAction
    action_Show_Toolbar: QAction
    action_Show_Word_Scan_Popup: QAction
    action_Search_As_You_Type: QAction
    action_Search_Completion: QAction
    action_Re_index_database: QAction
    action_Re_download_database: QAction
    action_Focus_Search_Input: QAction
    action_Quit: QAction
    action_Sutta_Search: QAction
    action_Sutta_Study: QAction
    action_Dictionary_Search: QAction
    action_Bookmarks: QAction
    action_Pali_Courses: QAction
    action_Memos: QAction
    action_Links: QAction
    action_First_Window_on_Startup: QAction
    action_Website: QAction
    action_About: QAction
    action_Open: QAction
    action_Dictionaries_Manager: QAction
    action_Document_Reader: QAction
    action_Library: QAction

    toolBar: QToolBar
    search_input: QLineEdit
    start_loading_animation: Callable
    stop_loading_animation: Callable

    _focus_search_input: Callable

class SuttaSearchWindowInterface(AppWindowInterface):
    addToolBar: Callable
    _update_sidebar_fulltext: Callable
    _set_recent_list: Callable
    show_network_graph: Callable
    update_memos_list_for_sutta: Callable
    _lookup_selection_in_suttas: Callable
    _lookup_selection_in_dictionary: Callable
    _select_next_recent: Callable
    _select_prev_recent: Callable

    rightside_tabs: QTabWidget
    palibuttons_frame: QFrame
    action_Dictionary_Search: QAction
    action_Show_Related_Suttas: QAction
    action_Show_Translation_and_Pali_Line_by_Line: QAction
    action_Show_All_Variant_Readings: QAction
    action_Find_in_Page: QAction

class DictionarySearchWindowInterface(AppWindowInterface):
    rightside_tabs: QTabWidget
    palibuttons_frame: QFrame


class GraphRequest(TypedDict):
    sutta_uid: Optional[str]
    dict_word_uid: Optional[str]
    distance: int
    queue_id: str
    graph_gen_timestamp: float
    graph_path: str
    messages_url: str
    labels: Optional[ShowLabels]
    min_links: Optional[int]
    width: int
    height: int

class CompletionCacheWorkerSignals(QObject):
    finished = pyqtSignal(dict)

class CompletionCacheWorker(QRunnable):
    signals: CompletionCacheWorkerSignals

    def __init__(self):
        super().__init__()
        self.signals = CompletionCacheWorkerSignals()

    @pyqtSlot()
    def run(self):
        try:
            _, _, db_session = get_db_engine_connection_session()

            res = []
            r = db_session.query(Am.Sutta.title).all()
            res.extend(r)

            r = db_session.query(Um.Sutta.title).all()
            res.extend(r)

            a: List[str] = list(map(lambda x: x[0] or 'none', res))
            b = list(map(lambda x: re.sub(r' *\d+$', '', x.lower()), a))
            b.sort()
            titles = list(set(b))

            res = []
            r = db_session.query(Am.DictWord.word).all()
            res.extend(r)

            r = db_session.query(Um.DictWord.word).all()
            res.extend(r)

            a: List[str] = list(map(lambda x: x[0] or 'none', res))
            b = list(map(lambda x: re.sub(r' *\d+$', '', x.lower()), a))
            b.sort()
            words = list(set(b))

            db_session.close()

            self.signals.finished.emit(CompletionCache(
                sutta_titles=titles,
                dict_words=words,
            ))

        except Exception as e:
            logger.error(e)


QExpanding = QtWidgets.QSizePolicy.Policy.Expanding
QMinimum = QtWidgets.QSizePolicy.Policy.Minimum


class PaliItem(TypedDict):
    text: str
    audio: Optional[str]
    gfx: Optional[str]
    uuid: Optional[str]


class PaliCourseGroup(TypedDict):
    db_schema: str
    db_id: int


class PaliListModel(str, Enum):
    ChallengeCourse = "ChallengeCourse"
    ChallengeGroup = "ChallengeGroup"


class PaliListItem(TypedDict):
    db_model: PaliListModel
    db_schema: DbSchemaName
    db_id: int


class PaliChallengeType(str, Enum):
    Explanation = 'Explanation'
    Vocabulary = 'Vocabulary'
    TranslateFromEnglish = 'Translate from English'
    TranslateFromPali = 'Translate from Pali'
