from enum import Enum
from typing import Callable, Dict, List, Optional, TypedDict, Union
from urllib.parse import parse_qs

from PyQt6 import QtWidgets
from PyQt6.QtCore import QUrl, pyqtSignal
from PyQt6.QtGui import QAction
from PyQt6.QtWidgets import QFrame, QLineEdit, QMainWindow, QTabWidget, QToolBar

from simsapa import IS_MAC, DbSchemaName, ShowLabels

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

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

class CompletionCacheResult(TypedDict):
    sutta_titles: List[str]
    dict_words: List[str]

class SearchArea(int, Enum):
    Suttas = 0
    DictWords = 1

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

class SearchParams(TypedDict):
    mode: SearchMode
    page_len: Optional[int]
    only_lang: Optional[str]
    only_source: Optional[str]

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

# Message to show to the user.
class AppMessage(TypedDict):
    kind: str
    text: str

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
    results_page: Callable
    query_hits: Callable
    result_pages_count: Callable

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

    queue_id: str
    lookup_in_dictionary_signal: pyqtSignal
    lookup_in_new_sutta_window_signal: pyqtSignal

class DictionarySearchWindowInterface(AppWindowInterface):
    action_Show_Sidebar: QAction
    rightside_tabs: QTabWidget
    palibuttons_frame: QFrame
    action_Reload_Page: QAction
    _toggle_sidebar: Callable
    results_page: Callable
    query_hits: Callable
    result_pages_count: Callable


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
