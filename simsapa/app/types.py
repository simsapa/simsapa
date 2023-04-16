from enum import Enum
import csv, re, json, os, shutil
import os.path
from pathlib import Path
from typing import Callable, Dict, List, Optional, TypedDict, Union
from urllib.parse import parse_qs

from deepmerge import always_merger
import tomlkit
from tomlkit.toml_document import TOMLDocument

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.sql import func, text
from sqlalchemy import and_

from PyQt6 import QtWidgets
from PyQt6.QtCore import QSize, QThreadPool, QUrl, pyqtSignal
from PyQt6.QtGui import QAction, QClipboard
from PyQt6.QtWidgets import QFrame, QLineEdit, QMainWindow, QTabWidget, QToolBar

from simsapa import COURSES_DIR, IS_MAC, DbSchemaName, ShowLabels, logger, APP_DB_PATH, USER_DB_PATH, ASSETS_DIR, INDEX_DIR
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.db_helpers import find_or_create_db, get_db_session_with_schema, upgrade_db
from simsapa.app.helpers import bilara_text_to_segments

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

UMultiRef = Union[Am.MultiRef, Um.MultiRef]

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
    RegexMatch = 4

SuttaSearchModeNameToType = {
    "Fulltext Match": SearchMode.FulltextMatch,
    "Exact Match": SearchMode.ExactMatch,
    "Title Match": SearchMode.TitleMatch,
    "Regex Match": SearchMode.RegexMatch,
}

DictionarySearchModeNameToType = {
    "Fulltext Match": SearchMode.FulltextMatch,
    "Exact Match": SearchMode.ExactMatch,
    "Headword Match": SearchMode.HeadwordMatch,
    "Regex Match": SearchMode.RegexMatch,
}

class SearchResultSizes(TypedDict):
    header_height: int
    snippet_length: int
    snippet_font_size: int
    snippet_min_height: int
    snippet_max_height: int

class PaliListModel(str, Enum):
    ChallengeCourse = "ChallengeCourse"
    ChallengeGroup = "ChallengeGroup"

class PaliGroupStats(TypedDict):
    completed: int
    total: int

class TomlCourseChallenge(TypedDict):
    challenge_type: str
    explanation_md: str
    question: str
    answer: str
    audio: str
    gfx: str

class TomlCourseGroup(TypedDict):
    name: str
    description: str
    sort_index: int
    challenges: List[TomlCourseChallenge]

def default_search_result_sizes() -> SearchResultSizes:
    return SearchResultSizes(
        header_height = 20,
        snippet_length = 500,
        snippet_font_size = 10 if IS_MAC else 9,
        snippet_min_height = 25,
        snippet_max_height = 60,
    )

class SmtpServicePreset(str, Enum):
    NoPreset = "No preset"
    GoogleMail = "Google Mail"
    FastMail = "Fast Mail"
    ProtonMail = "Proton Mail"

SmtpServicePresetToEnum = {
    "Google Mail": SmtpServicePreset.GoogleMail,
    "Fast Mail": SmtpServicePreset.FastMail,
    "Proton Mail": SmtpServicePreset.ProtonMail,
    "No preset": SmtpServicePreset.NoPreset,
}

class SmtpLoginData(TypedDict):
    host: str
    port_tls: int
    user: str
    password: str

SmtpLoginDataPreset: Dict[SmtpServicePreset, SmtpLoginData] = dict()

SmtpLoginDataPreset[SmtpServicePreset.GoogleMail] = SmtpLoginData(
    host = "smtp.gmail.com",
    port_tls = 587,
    user = "e.g. account@gmail.com",
    password = "",
)

class ExportFileFormat(str, Enum):
    EPUB = "EPUB"
    MOBI = "MOBI"
    HTML = "HTML"
    TXT = "TXT"

ExportFileFormatToEnum = {
    "EPUB": ExportFileFormat.EPUB,
    "MOBI": ExportFileFormat.MOBI,
    "HTML": ExportFileFormat.HTML,
    "TXT": ExportFileFormat.TXT,
}

class KindleFileFormat(str, Enum):
    EPUB = "EPUB"
    MOBI = "MOBI"
    HTML = "HTML"
    TXT = "TXT"

KindleFileFormatToEnum = {
    "EPUB": KindleFileFormat.EPUB,
    "MOBI": KindleFileFormat.MOBI,
    "HTML": KindleFileFormat.HTML,
    "TXT": KindleFileFormat.TXT,
}

class KindleAction(str, Enum):
    SaveViaUSB = "Save to Kindle via USB"
    SendToEmail = "Send to Kindle Email"

KindleActionToEnum = {
    "Save to Kindle via USB": KindleAction.SaveViaUSB,
    "Send to Kindle Email": KindleAction.SendToEmail,
}

class SendToKindleSettings(TypedDict):
    format: KindleFileFormat
    kindle_email: Optional[str]

def default_send_to_kindle_settings() -> SendToKindleSettings:
    return SendToKindleSettings(
        format = KindleFileFormat.EPUB,
        kindle_email = None,
    )

class RemarkableFileFormat(str, Enum):
    EPUB = "EPUB"
    HTML = "HTML"
    TXT = "TXT"

RemarkableFileFormatToEnum = {
    "EPUB": RemarkableFileFormat.EPUB,
    "HTML": RemarkableFileFormat.HTML,
    "TXT": RemarkableFileFormat.TXT,
}

class RemarkableAction(str, Enum):
    SaveWithCurl = "Save to reMarkable with curl"
    SaveWithScp = "Save to reMarkable with scp"

RemarkableActionToEnum = {
    "Save to reMarkable with curl": RemarkableAction.SaveWithCurl,
    "Save to reMarkable with scp": RemarkableAction.SaveWithScp,
}

class SendToRemarkableSettings(TypedDict):
    format: RemarkableFileFormat
    rmk_web_ip: str
    rmk_ssh_ip: str
    rmk_folder_to_scp: str
    user_ssh_pubkey_path: str

def default_send_to_remarkable_settings() -> SendToRemarkableSettings:
    return SendToRemarkableSettings(
        format = RemarkableFileFormat.EPUB,
        rmk_web_ip = "10.11.99.1",
        rmk_ssh_ip = "10.11.99.1",
        rmk_folder_to_scp = "/home/root",
        user_ssh_pubkey_path = "~/.ssh/id_rsa",
    )

class OpenAIModel(str, Enum):
    Gpt3_5_Turbo = "GPT 3.5 Turbo"
    Gpt4_8k = "GPT-4 (8k)"
    Gpt4_32k = "GPT-4 (32k)"

def model_max_tokens(model: OpenAIModel) -> int:
    if model == OpenAIModel.Gpt4_8k:
        return 8192

    if model == OpenAIModel.Gpt4_32k:
        return 32768

    else:
        return 4096

OpenAIModelToEnum = {
    "GPT 3.5 Turbo": OpenAIModel.Gpt3_5_Turbo,
    "GPT-4 (8k)": OpenAIModel.Gpt4_8k,
    "GPT-4 (32k)": OpenAIModel.Gpt4_32k,
}

OpenAIModelLatest = {
    "GPT 3.5 Turbo": "gpt-3.5-turbo",
    "GPT-4 (8k)": "gpt-4",
    "GPT-4 (32k)": "gpt-4-32k",
}

"""
https://platform.openai.com/docs/api-reference/chat

Chat Completion Response:

{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677652288,
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": "\n\nHello there, how may I assist you today?",
    },
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 9,
    "completion_tokens": 12,
    "total_tokens": 21
  }
}
"""

class ChatRole(str, Enum):
    System = 'system'
    User = 'user'

class ChatMessage(TypedDict):
    role: str
    content: str

class ChatChoice(TypedDict):
    message: ChatMessage
    finish_reason: str
    index: int

class ChatResponse(TypedDict):
    id: str
    object: str
    created: int
    model: str
    usage: dict
    choices: List[ChatChoice]

class OpenAISettings(TypedDict):
    api_key: Optional[str]
    model: OpenAIModel
    temperature: float
    max_tokens: int
    auto_max_tokens: bool
    n_completions: int
    join_short_lines: int
    append_mode: bool

def default_openai_settings() -> OpenAISettings:
    return OpenAISettings(
        api_key = None,
        model = OpenAIModel.Gpt3_5_Turbo,
        temperature = 0.7,
        max_tokens = 256,
        auto_max_tokens = False,
        n_completions = 1,
        join_short_lines = 80,
        append_mode = False,
    )

class AppSettings(TypedDict):
    disabled_sutta_labels: Labels
    disabled_dict_labels: Labels
    notify_about_updates: bool
    suttas_show_pali_buttons: bool
    dictionary_show_pali_buttons: bool
    show_toolbar: bool
    link_preview: bool
    first_window_on_startup: WindowType
    word_scan_popup_pos: WindowPosSize
    show_sutta_sidebar: bool
    show_dictionary_sidebar: bool
    show_related_suttas: bool
    show_translation_and_pali_line_by_line: bool
    show_all_variant_readings: bool
    show_bookmarks: bool
    sutta_font_size: int
    sutta_max_width: int
    dictionary_font_size: int
    search_result_sizes: SearchResultSizes
    search_as_you_type: bool
    search_completion: bool
    double_click_dict_lookup: bool
    clipboard_monitoring_for_dict: bool
    sutta_search_mode: SearchMode
    dictionary_search_mode: SearchMode
    word_scan_search_mode: SearchMode
    sutta_language_filter_idx: int
    sutta_source_filter_idx: int
    dict_filter_idx: int
    word_scan_dict_filter_idx: int
    audio_volume: float
    audio_device_desc: str
    path_to_curl: Optional[str]
    path_to_scp: Optional[str]
    path_to_ebook_convert: Optional[str]
    smtp_sender_email: Optional[str]
    smtp_preset: SmtpServicePreset
    smtp_login_data: Optional[SmtpLoginData]
    export_format: ExportFileFormat
    send_to_kindle: SendToKindleSettings
    send_to_remarkable: SendToRemarkableSettings
    openai: OpenAISettings

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
        link_preview = True,
        first_window_on_startup = WindowType.SuttaSearch,
        word_scan_popup_pos = WindowPosSize(
            x = 100,
            y = 100,
            width = 400,
            height = 500,
        ),
        show_sutta_sidebar = True,
        show_dictionary_sidebar = True,
        show_related_suttas = True,
        show_translation_and_pali_line_by_line = False,
        show_all_variant_readings = False,
        show_bookmarks = True,
        sutta_font_size = 22,
        sutta_max_width = 75,
        dictionary_font_size = 18,
        search_result_sizes = default_search_result_sizes(),
        search_as_you_type = True,
        search_completion = True,
        double_click_dict_lookup = True,
        clipboard_monitoring_for_dict = False,
        sutta_search_mode = SearchMode.FulltextMatch,
        dictionary_search_mode = SearchMode.FulltextMatch,
        word_scan_search_mode = SearchMode.FulltextMatch,
        sutta_language_filter_idx = 0,
        sutta_source_filter_idx = 0,
        dict_filter_idx = 0,
        word_scan_dict_filter_idx = 0,
        audio_volume = 0.9,
        audio_device_desc = '',
        path_to_curl = None,
        path_to_scp = None,
        path_to_ebook_convert = None,
        smtp_sender_email = None,
        smtp_preset = SmtpServicePreset.NoPreset,
        smtp_login_data = SmtpLoginDataPreset[SmtpServicePreset.GoogleMail],
        export_format = ExportFileFormat.EPUB,
        send_to_kindle = default_send_to_kindle_settings(),
        send_to_remarkable = default_send_to_remarkable_settings(),
        openai = default_openai_settings(),
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
    screen_size: Optional[QSize] = None
    completion_cache: CompletionCache
    # Keys are db schema, and course group id
    pali_groups_stats: Dict[DbSchemaName, Dict[int, PaliGroupStats]] = dict()

    def __init__(self,
                 actions_manager: Optional[ActionsManager] = None,
                 app_clipboard: Optional[QClipboard] = None,
                 app_db_path: Optional[Path] = None,
                 user_db_path: Optional[Path] = None,
                 api_port: Optional[int] = None):

        self.clipboard: Optional[QClipboard] = app_clipboard

        self.actions_manager = actions_manager

        # Remove indexes marked to be deleted in a previous session. Can't
        # safely clear and remove them after the application has opened them.
        self._remove_marked_indexes()

        if app_db_path is None:
            app_db_path = self._find_app_data_or_exit()

        if user_db_path is None:
            user_db_path = USER_DB_PATH
        else:
            user_db_path = user_db_path

        find_or_create_db(user_db_path, DbSchemaName.UserData.value)

        self.completion_cache = CompletionCache(
            sutta_titles=[],
            dict_words=[],
        )

        self.graph_gen_pool = QThreadPool()

        self.api_url: Optional[str] = None

        if api_port:
            self.api_url = f'http://localhost:{api_port}'

        self.sutta_to_open: Optional[USutta] = None
        self.dict_word_to_open: Optional[UDictWord] = None

        self._connect_to_db(app_db_path, user_db_path)

        self.search_indexed = SearchIndexed()

        self._read_app_settings()
        self._find_cli_paths()
        self._read_pali_groups_stats()
        self._ensure_user_memo_deck()

    def _remove_marked_indexes(self):
        p = ASSETS_DIR.joinpath('indexes_to_remove.txt')
        if not p.exists():
            return

        with open(p, 'r') as f:
            s = f.read()
        p.unlink()

        if s == "":
            return

        langs = s.split(',')
        for lang in langs:
            p = INDEX_DIR.joinpath(f'suttas_lang_{lang}_WRITELOCK')
            p.unlink()

            for p in INDEX_DIR.glob(f'suttas_lang_{lang}_*.seg'):
                p.unlink()

            for p in INDEX_DIR.glob(f'_suttas_lang_{lang}_*.toc'):
                p.unlink()

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
            self.db_conn.execute(text(f"ATTACH DATABASE '{app_db_path}' AS appdata;"))
            self.db_conn.execute(text(f"ATTACH DATABASE '{user_db_path}' AS userdata;"))

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

        if x is not None and x.value is not None:
            default_settings = default_app_settings()
            stored_settings = json.loads(x.value)

            # Ensure that keys are not missing by updating the default value.
            s = always_merger.merge(default_settings, stored_settings)
            self.app_settings = s

        else:
            self.app_settings = default_app_settings()

        # Always save it back, default keys might have been updated.
        self._save_app_settings()

    def _save_app_settings(self):
        x = self.db_session \
                .query(Um.AppSetting) \
                .filter(Um.AppSetting.key == 'app_settings') \
                .first()

        try:
            if x is not None:
                x.value = json.dumps(self.app_settings)
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

    def _find_cli_paths(self):
        s = self.app_settings

        if not s['path_to_curl']:
            p = shutil.which('curl')
            if p:
                s['path_to_curl'] = str(p)

        if not s['path_to_scp']:
            p = shutil.which('scp')
            if p:
                s['path_to_scp'] = str(p)

        if not s['path_to_ebook_convert']:
            p = shutil.which('ebook-convert')
            if p:
                s['path_to_ebook_convert'] = str(p)

        self.app_settings = s
        self._save_app_settings()

    def _read_pali_groups_stats(self):
        schemas = [DbSchemaName.AppData, DbSchemaName.UserData]

        for sc in schemas:
            key = f"{sc}_pali_groups_stats"
            r = self.db_session \
                    .query(Um.AppSetting) \
                    .filter(Um.AppSetting.key == key) \
                    .first()

            if r is not None and r.value is not None:
                self.pali_groups_stats[sc] = json.loads(r.value)
            else:
                self.pali_groups_stats[sc] = dict()
                self._save_pali_groups_stats(sc)

    def _save_pali_groups_stats(self, schema: DbSchemaName):
        key = f"{schema}_pali_groups_stats"
        r = self.db_session \
                .query(Um.AppSetting) \
                .filter(Um.AppSetting.key == key) \
                .first()

        try:
            if r is not None:
                r.value = json.dumps(self.pali_groups_stats[schema])
                self.db_session.commit()
            else:
                r = Um.AppSetting(
                    key = key,
                    value = json.dumps(self.pali_groups_stats[schema]),
                    created_at = func.now(),
                )
                self.db_session.add(r)
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

    def import_bookmarks(self, file_path: str) -> int:
        p = Path(file_path)
        if not p.exists():
            return 0

        rows = []

        with open(file_path) as f:
            reader = csv.DictReader(f)
            for row in reader:
                rows.append(row)

        def _to_bookmark(x: Dict[str, str]) -> UBookmark:
            return Um.Bookmark(
                name          = x['name']          if x['name']          != 'None' else None,
                quote         = x['quote']         if x['quote']         != 'None' else None,
                selection_range = x['selection_range'] if x['selection_range'] != 'None' else None,
                sutta_id      = int(x['sutta_id']) if x['sutta_id']      != 'None' else None,
                sutta_uid     = x['sutta_uid']     if x['sutta_uid']     != 'None' else None,
                sutta_schema  = x['sutta_schema']  if x['sutta_schema']  != 'None' else None,
                sutta_ref     = x['sutta_ref']     if x['sutta_ref']     != 'None' else None,
                sutta_title   = x['sutta_title']   if x['sutta_title']   != 'None' else None,
                comment_text  = x['comment_text']  if x['comment_text']  != 'None' else None,
                comment_attr_json = x['comment_attr_json'] if x['comment_attr_json'] != 'None' else None,
                read_only     = x['read_only']     if x['read_only']     != 'None' else None,
            )

        bookmarks = list(map(_to_bookmark, rows))

        try:
            for i in bookmarks:
                self.db_session.add(i)
            self.db_session.commit()
        except Exception as e:
            logger.error(e)
            return 0

        return len(bookmarks)

    def import_suttas_to_userdata(self, db_path: str) -> int:
        import_db_eng, import_db_conn, import_db_session = get_db_session_with_schema(Path(db_path), DbSchemaName.UserData)

        import_suttas = import_db_session.query(Um.Sutta).all()

        if len(import_suttas) == 0:
            import_db_conn.close()
            import_db_session.close()
            import_db_eng.dispose()
            return 0

        for i in import_suttas:
            sutta = Um.Sutta(
                uid = i.uid,
                group_path = i.group_path,
                group_index = i.group_index,
                sutta_ref = i.sutta_ref,
                language = i.language,
                order_index = i.order_index,

                sutta_range_group = i.sutta_range_group,
                sutta_range_start = i.sutta_range_start,
                sutta_range_end = i.sutta_range_end,

                title = i.title,
                title_pali = i.title_pali,
                title_trans = i.title_trans,
                description = i.description,
                content_plain = i.content_plain,
                content_html = i.content_html,
                content_json = i.content_json,
                content_json_tmpl = i.content_json_tmpl,

                source_uid = i.source_uid,
                source_info = i.source_info,
                source_language = i.source_language,
                message = i.message,
                copyright = i.copyright,
                license = i.license,
            )

            author_uid = i.source_uid

            author = self.db_session \
                         .query(Um.Author) \
                         .filter(Um.Author.uid == author_uid) \
                         .first()

            if author is None:
                author = Um.Author(uid = author_uid)

            self.db_session.add(author)
            sutta.author = author

            self.db_session.add(sutta)

        self.db_session.commit()

        n = len(import_suttas)

        import_db_conn.close()
        import_db_session.close()
        import_db_eng.dispose()

        return n

    def export_bookmarks(self, file_path: str) -> int:
        if not file_path.endswith(".csv"):
            file_path = f"{file_path}.csv"

        res = self.db_session \
                  .query(Um.Bookmark) \
                  .filter(Um.Bookmark.sutta_uid != '') \
                  .all()

        if not res:
            return 0

        def _to_row(x: UBookmark) -> Dict[str, str]:
            return {
                "name": str(x.name),
                "quote": str(x.quote),
                "selection_range": str(x.selection_range),
                "sutta_id": str(x.sutta_id),
                "sutta_uid": str(x.sutta_uid),
                "sutta_schema": str(x.sutta_schema),
                "sutta_ref": str(x.sutta_ref),
                "sutta_title": str(x.sutta_title),
                "comment_text": str(x.comment_text),
                "comment_attr_json": str(x.comment_attr_json),
                "read_only": str(x.read_only),
            }

        a = list(map(_to_row, res))
        rows = sorted(a, key=lambda x: x['name'])

        try:
            with open(file_path, 'w') as f:
                w = csv.DictWriter(f, fieldnames=rows[0].keys())
                w.writeheader()
                for r in rows:
                    w.writerow(r)
        except Exception as e:
            logger.error(e)
            return 0

        return len(rows)

    def import_prompts(self, file_path: str) -> int:
        p = Path(file_path)
        if not p.exists():
            return 0

        rows = []

        with open(file_path) as f:
            reader = csv.DictReader(f)
            for row in reader:
                rows.append(row)

        def _to_prompt(x: Dict[str, str]) -> Um.GptPrompt:
            return Um.GptPrompt(
                name_path       = x['name_path']       if x['name_path']       != 'None' else None,
                messages_json   = x['messages_json']   if x['messages_json']   != 'None' else None,
                show_in_context = True                 if x['show_in_context'] == 'True' else False,
            )

        prompts = list(map(_to_prompt, rows))

        try:
            for i in prompts:
                # GPT Prompt name_path is unique, unlike the bookmark name.
                r = self.db_session.query(Um.GptPrompt).filter(Um.GptPrompt.name_path == i.name_path).first()
                if r is not None:
                    self.db_session.delete(r)
                    self.db_session.commit()

                self.db_session.add(i)

            self.db_session.commit()
        except Exception as e:
            logger.error(e)
            return 0

        return len(prompts)

    def export_prompts(self, file_path: str) -> int:
        if not file_path.endswith(".csv"):
            file_path = f"{file_path}.csv"

        res = self.db_session.query(Um.GptPrompt).all()

        if not res:
            return 0

        def _to_row(x: Um.GptPrompt) -> Dict[str, str]:
            return {
                "name_path": str(x.name_path),
                "messages_json": str(x.messages_json),
                "show_in_context": str(x.show_in_context),
            }

        a = list(map(_to_row, res))
        rows = sorted(a, key=lambda x: x['name_path'])

        try:
            with open(file_path, 'w') as f:
                w = csv.DictWriter(f, fieldnames=rows[0].keys())
                w.writeheader()
                for r in rows:
                    w.writerow(r)
        except Exception as e:
            logger.error(e)
            return 0

        return len(rows)

    def export_app_settings(self, file_path: str):
        if not file_path.endswith(".json"):
            file_path = f"{file_path}.json"

        try:
            with open(file_path, 'w') as f:
                s = json.dumps(self.app_settings)
                f.write(s)
        except Exception as e:
            logger.error(e)
            return 0

    def import_app_settings(self, file_path: str):
        p = Path(file_path)
        if not p.exists():
            return

        with open(p, 'r') as f:
            value = json.loads(f.read())

            d = default_app_settings()
            d.update(value)
            self.app_settings = d

            self._save_app_settings()

    def parse_toml(self, path: Path) -> Optional[TOMLDocument]:
        with open(path) as f:
            s = f.read()

        t = None
        try:
            t = tomlkit.parse(s)
        except Exception as e:
            msg = f"Can't parse TOML: {path}\n\n{e}"
            logger.error(msg)
            raise Exception(msg)

        return t


    def _course_base_from_name(self, course_name: str) -> Path:
        p = COURSES_DIR.joinpath(re.sub(r'[^0-9A-Za-z]', '_', course_name))
        return p


    def _copy_to_courses(self, toml_path: Path, asset_rel_path: Path, course_base: Path):
        toml_dir = toml_path.parent

        from_path = toml_dir.joinpath(asset_rel_path)

        to_path = course_base.joinpath(asset_rel_path)

        to_dir = to_path.parent
        if not to_dir.exists():
            to_dir.mkdir(parents=True, exist_ok=True)

        shutil.copy(from_path, to_path)


    def import_pali_course(self, file_path: str) -> Optional[str]:
        try:
            t = self.parse_toml(Path(file_path))
        except Exception as e:
            raise e

        if t is None:
            return

        courses_count = self.db_session \
                            .query(func.count(Um.ChallengeCourse.id)) \
                            .scalar()

        course_name = t.get('name') or 'Unknown'

        course_base = self._course_base_from_name(course_name)
        if not course_base.exists():
            course_base.mkdir(parents=True, exist_ok=True)

        shutil.copy(file_path, course_base)

        c = Um.ChallengeCourse(
            name = course_name,
            description = t['description'],
            course_dirname = course_base.name,
            sort_index = courses_count + 1,
        )

        self.db_session.add(c)
        self.db_session.commit()

        groups: List[TomlCourseGroup] = t.get('groups') or []

        for idx_i, i in enumerate(groups):
            g = Um.ChallengeGroup(
                name = i['name'],
                description = i['description'],
                sort_index = idx_i,
            )

            g.course = c

            self.db_session.add(g)
            self.db_session.commit()

            for idx_j, j in enumerate(i['challenges']):

                ch = None

                if j['challenge_type'] == PaliChallengeType.Explanation.value:

                    ch = Um.Challenge(
                        sort_index = idx_j,
                        challenge_type = j['challenge_type'],
                        explanation_md = j['explanation_md'],
                    )

                elif j['challenge_type'] == PaliChallengeType.Vocabulary.value or \
                     j['challenge_type'] == PaliChallengeType.TranslateFromEnglish.value or \
                     j['challenge_type'] == PaliChallengeType.TranslateFromPali.value:

                    if j.get('gfx', False):
                        self._copy_to_courses(Path(file_path), Path(j['gfx']), course_base)
                        # Challenge asset paths are relative to course dir
                        gfx = j['gfx']
                    else:
                        gfx = None

                    question = PaliItem(text = j['question'], audio = None, gfx = gfx, uuid = None)

                    if j.get('audio', False):
                        self._copy_to_courses(Path(file_path), Path(j['audio']), course_base)
                        audio = j['audio']
                    else:
                        audio = None

                    answers = [PaliItem(text = j['answer'], audio = audio, gfx = None, uuid = None)]

                    ch = Um.Challenge(
                        sort_index = idx_j,
                        challenge_type = j['challenge_type'],
                        question_json = json.dumps(question),
                        answers_json = json.dumps(answers),
                    )

                if ch is not None:
                    ch.course = c
                    ch.group = g
                    self.db_session.add(ch)
                    self.db_session.commit()

        return course_name

    def get_pali_for_translated(self, sutta: USutta) -> Optional[USutta]:
        if sutta.language == 'pli':
            return None

        uid_ref = re.sub('^([^/]+)/.*', r'\1', str(sutta.uid))

        res: List[USutta] = []
        r = self.db_session \
            .query(Am.Sutta) \
            .filter(and_(
                Am.Sutta.uid != sutta.uid,
                Am.Sutta.language == 'pli',
                Am.Sutta.uid.like(f"{uid_ref}/%"),
            )) \
            .all()
        res.extend(r)

        r = self.db_session \
            .query(Um.Sutta) \
            .filter(and_(
                Um.Sutta.uid != sutta.uid,
                Um.Sutta.language == 'pli',
                Um.Sutta.uid.like(f"{uid_ref}/%"),
            )) \
            .all()
        res.extend(r)

        if len(res) > 0:
            return res[0]
        else:
            return None

    def sutta_to_segments_json(self,
                               sutta: USutta,
                               use_template: bool = True) -> Dict[str, str]:

        res = sutta.variant
        if res is None:
            variant = None
        else:
            variant = str(res.content_json)

        res = sutta.comment
        if res is None:
            comment = None
        else:
            comment = str(res.content_json)

        show_variants = self.app_settings.get('show_all_variant_readings', True)

        if use_template:
            tmpl = str(sutta.content_json_tmpl)
        else:
            tmpl = None

        segments_json = bilara_text_to_segments(
            str(sutta.content_json),
            tmpl,
            variant,
            comment,
            show_variants,
        )

        return segments_json

class AppWindowInterface(QMainWindow):
    action_Check_for_Updates: QAction
    action_Notify_About_Updates: QAction
    action_Show_Toolbar: QAction
    action_Link_Preview: QAction
    action_Show_Word_Scan_Popup: QAction
    action_Search_As_You_Type: QAction
    action_Search_Completion: QAction
    action_Double_Click_on_a_Word_for_Dictionary_Lookup: QAction
    action_Clipboard_Monitoring_for_Dictionary_Lookup: QAction
    action_Re_index_database: QAction
    action_Re_download_database: QAction
    action_Focus_Search_Input: QAction
    action_Quit: QAction
    action_Sutta_Search: QAction
    action_Sutta_Study: QAction
    action_Sutta_Index: QAction
    action_Dictionary_Search: QAction
    action_Bookmarks: QAction
    action_Pali_Courses: QAction
    action_Memos: QAction
    action_Links: QAction
    action_First_Window_on_Startup: QAction
    action_Website: QAction
    action_About: QAction
    action_Open: QAction
    action_Ebook_Reader: QAction
    action_Dictionaries_Manager: QAction
    action_Document_Reader: QAction
    action_Library: QAction
    action_Prompts: QAction

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
    _select_next_recent: Callable
    _select_prev_recent: Callable
    _toggle_sidebar: Callable

    rightside_tabs: QTabWidget
    palibuttons_frame: QFrame
    action_Reload_Page: QAction
    action_Dictionary_Search: QAction
    action_Show_Sidebar: QAction
    action_Show_Related_Suttas: QAction
    action_Show_Translation_and_Pali_Line_by_Line: QAction
    action_Show_All_Variant_Readings: QAction
    action_Show_Bookmarks: QAction
    action_Find_in_Page: QAction

    lookup_in_dictionary_signal: pyqtSignal
    lookup_in_new_sutta_window_signal: pyqtSignal

class DictionarySearchWindowInterface(AppWindowInterface):
    action_Show_Sidebar: QAction
    rightside_tabs: QTabWidget
    palibuttons_frame: QFrame
    action_Reload_Page: QAction
    _toggle_sidebar: Callable


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


QFixed = QtWidgets.QSizePolicy.Policy.Fixed
QMinimum = QtWidgets.QSizePolicy.Policy.Minimum
QMaximum = QtWidgets.QSizePolicy.Policy.Maximum
QPreferred = QtWidgets.QSizePolicy.Policy.Preferred
QMinimumExpanding = QtWidgets.QSizePolicy.Policy.MinimumExpanding
QExpanding = QtWidgets.QSizePolicy.Policy.Expanding
QIgnored = QtWidgets.QSizePolicy.Policy.Ignored


class PaliItem(TypedDict):
    text: str
    audio: Optional[str]
    gfx: Optional[str]
    uuid: Optional[str]


class PaliCourseGroup(TypedDict):
    db_schema: str
    db_id: int


class PaliListItem(TypedDict):
    db_model: PaliListModel
    db_schema: DbSchemaName
    db_id: int


class PaliChallengeType(str, Enum):
    Explanation = 'Explanation'
    Vocabulary = 'Vocabulary'
    TranslateFromEnglish = 'Translate from English'
    TranslateFromPali = 'Translate from Pali'

class QueryType(str, Enum):
    suttas = "suttas"
    words = "words"

class SuttaQuote(TypedDict):
    quote: str
    selection_range: Optional[str]

def sutta_quote_from_url(url: QUrl) -> Optional[SuttaQuote]:
    query = parse_qs(url.query())

    sutta_quote = None
    quote = None
    if 'q' in query.keys():
        quote = query['q'][0]

    if quote:
        selection_range = None
        if 'sel' in query.keys():
            selection_range = query['sel'][0]

        sutta_quote = SuttaQuote(
            quote = quote,
            selection_range = selection_range,
        )

    return sutta_quote

class OpenPromptParams(TypedDict):
    prompt_db_id: int
    sutta_uid: Optional[str]
    with_name: Optional[str]
    selection_text: Optional[str]
