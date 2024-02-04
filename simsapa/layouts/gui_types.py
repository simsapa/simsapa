from enum import Enum
from datetime import datetime
from typing import Callable, List, Optional, TypedDict, Dict
from urllib.parse import parse_qs, unquote

from PyQt6 import QtWidgets
from PyQt6.QtCore import QUrl, pyqtSignal
from PyQt6.QtGui import QAction, QClipboard
from PyQt6.QtWidgets import QCheckBox, QComboBox, QFrame, QHBoxLayout, QLineEdit, QMainWindow, QPushButton, QSpinBox, QTabWidget, QToolBar, QVBoxLayout, QWidget

from simsapa import IS_MAC, DbSchemaName, SuttaQuote, SearchResult
from simsapa.app.types import DictionaryQueriesInterface, SearchArea, SuttaQueriesInterface, UDictWord, SearchMode
from simsapa.layouts.find_panel import FindPanel

QSizeMinimum = QtWidgets.QSizePolicy.Policy.Minimum
QSizeExpanding = QtWidgets.QSizePolicy.Policy.Expanding

QFixed = QtWidgets.QSizePolicy.Policy.Fixed
QMinimum = QtWidgets.QSizePolicy.Policy.Minimum
QMaximum = QtWidgets.QSizePolicy.Policy.Maximum
QPreferred = QtWidgets.QSizePolicy.Policy.Preferred
QMinimumExpanding = QtWidgets.QSizePolicy.Policy.MinimumExpanding
QExpanding = QtWidgets.QSizePolicy.Policy.Expanding
QIgnored = QtWidgets.QSizePolicy.Policy.Ignored

# Lowercase values.
class ReleaseChannel(str, Enum):
    Main = 'main'
    Development = 'development'

class WindowType(str, Enum):
    SuttaSearch = "Sutta Search"
    SuttaStudy = "Sutta Study"
    DictionarySearch = "Dictionary Search"
    EbookReader = "Ebook Reader"
    WordLookup = "Word Lookup"
    LastClosed = "Last Closed Window"

class WindowPosSize(TypedDict):
    x: int
    y: int
    width: int
    height: int

WindowNameToType = {
    "Sutta Search": WindowType.SuttaSearch,
    "Sutta Study": WindowType.SuttaStudy,
    "Dictionary Search": WindowType.DictionarySearch,
    "Ebook Reader": WindowType.EbookReader,
    "Word Lookup": WindowType.WordLookup,
    "Last Closed Window": WindowType.LastClosed,
}

WordSublists = Dict[str, List[str]]

class CompletionCacheResult(TypedDict):
    sutta_titles: WordSublists
    dict_words: WordSublists

class SearchResultSizes(TypedDict):
    font_family: str
    font_size: int
    vertical_margin: int
    header_height: int
    snippet_length: int
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
        font_family = 'Helvetica' if IS_MAC else 'DejaVu Sans',
        font_size = 12 if IS_MAC else 10,
        vertical_margin = 8,
        header_height = 20,
        snippet_length = 500,
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
    Gpt3_5_Turbo_4k = "GPT 3.5 Turbo (4k)"
    Gpt3_5_Turbo_16k = "GPT 3.5 Turbo (16k)"
    Gpt4_8k = "GPT-4 (8k)"
    Gpt4_32k = "GPT-4 (32k)"

def model_max_tokens(model: OpenAIModel) -> int:
    if model == OpenAIModel.Gpt3_5_Turbo_4k:
        return 4096

    if model == OpenAIModel.Gpt3_5_Turbo_16k:
        return 16385

    if model == OpenAIModel.Gpt4_8k:
        return 8192

    if model == OpenAIModel.Gpt4_32k:
        return 32768

    else:
        return 4096

OpenAIModelToEnum = {
    "GPT 3.5 Turbo (4k)": OpenAIModel.Gpt3_5_Turbo_4k,
    "GPT 3.5 Turbo (16k)": OpenAIModel.Gpt3_5_Turbo_16k,
    "GPT-4 (8k)": OpenAIModel.Gpt4_8k,
    "GPT-4 (32k)": OpenAIModel.Gpt4_32k,
}

OpenAIModelLatest = {
    "GPT 3.5 Turbo (4k)": "gpt-3.5-turbo-0613",
    "GPT 3.5 Turbo (16k)": "gpt-3.5-turbo-16k-0613",
    "GPT-4 (8k)": "gpt-4-0613",
    "GPT-4 (32k)": "gpt-4-32k-0613",
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
        model = OpenAIModel.Gpt3_5_Turbo_4k,
        temperature = 0.7,
        max_tokens = 256,
        auto_max_tokens = False,
        n_completions = 1,
        join_short_lines = 80,
        append_mode = False,
    )

class AppSettings(TypedDict):
    audio_device_desc: str
    audio_volume: float
    clipboard_monitoring_for_dict: bool
    double_click_word_lookup: bool
    export_format: ExportFileFormat
    first_window_on_startup: WindowType
    generate_links_graph: bool
    link_preview: bool
    notify_about_simsapa_updates: bool
    notify_about_dpd_updates: bool
    release_channel: ReleaseChannel
    openai: OpenAISettings
    search_as_you_type: bool
    search_completion: bool
    search_result_sizes: SearchResultSizes
    send_to_kindle: SendToKindleSettings
    send_to_remarkable: SendToRemarkableSettings
    show_all_variant_readings: bool
    show_glosses: bool
    show_bookmarks: bool
    show_dictionary_sidebar: bool
    show_related_suttas: bool
    show_sutta_sidebar: bool
    show_toolbar: bool
    show_translation_and_pali_line_by_line: bool
    keep_running_in_background: bool
    tray_click_opens_window: WindowType
    show_search_options: bool

    sutta_font_size: int
    dictionary_font_size: int

    path_to_curl: Optional[str]
    path_to_ebook_convert: Optional[str]
    path_to_scp: Optional[str]

    smtp_login_data: Optional[SmtpLoginData]
    smtp_preset: SmtpServicePreset
    smtp_sender_email: Optional[str]

    dictionary_language_filter_idx: int
    dictionary_search_mode: SearchMode
    dictionary_source_filter_idx: int
    dictionary_show_pali_buttons: bool

    sutta_max_width: int
    sutta_language_filter_idx: int
    sutta_search_mode: SearchMode
    sutta_source_filter_idx: int
    suttas_show_pali_buttons: bool

    sutta_search_pos: WindowPosSize
    dictionary_search_pos: WindowPosSize
    ebook_reader_pos: WindowPosSize
    sutta_study_pos: WindowPosSize

    sutta_study_one_language_filter_idx: int
    sutta_study_one_search_mode: SearchMode
    sutta_study_one_source_filter_idx: int

    sutta_study_two_language_filter_idx: int
    sutta_study_two_search_mode: SearchMode
    sutta_study_two_source_filter_idx: int

    sutta_study_three_language_filter_idx: int
    sutta_study_three_search_mode: SearchMode
    sutta_study_three_source_filter_idx: int

    sutta_study_lookup_language_filter_idx: int
    sutta_study_lookup_search_mode: SearchMode
    sutta_study_lookup_source_filter_idx: int
    sutta_study_lookup_on_side: bool
    sutta_study_deconstructor_above_words: bool

    word_lookup_pos: WindowPosSize
    word_lookup_language_filter_idx: int
    word_lookup_search_mode: SearchMode
    word_lookup_source_filter_idx: int

def default_app_settings() -> AppSettings:
    return AppSettings(
        audio_device_desc = '',
        audio_volume = 0.9,
        clipboard_monitoring_for_dict = False,
        double_click_word_lookup = True,
        export_format = ExportFileFormat.EPUB,
        first_window_on_startup = WindowType.SuttaSearch,
        generate_links_graph = False,
        link_preview = True,
        notify_about_simsapa_updates = True,
        notify_about_dpd_updates = True,
        release_channel = ReleaseChannel.Main,
        openai = default_openai_settings(),
        search_as_you_type = True,
        search_completion = True,
        search_result_sizes = default_search_result_sizes(),
        send_to_kindle = default_send_to_kindle_settings(),
        send_to_remarkable = default_send_to_remarkable_settings(),
        show_all_variant_readings = False,
        show_glosses = False,
        show_bookmarks = True,
        show_dictionary_sidebar = True,
        show_related_suttas = True,
        show_sutta_sidebar = True,
        show_toolbar = False,
        show_translation_and_pali_line_by_line = False,
        keep_running_in_background = True,
        tray_click_opens_window = WindowType.LastClosed,
        show_search_options = True,

        sutta_font_size = 22,
        dictionary_font_size = 16,

        path_to_curl = None,
        path_to_ebook_convert = None,
        path_to_scp = None,

        smtp_login_data = SmtpLoginDataPreset[SmtpServicePreset.GoogleMail],
        smtp_preset = SmtpServicePreset.NoPreset,
        smtp_sender_email = None,

        dictionary_language_filter_idx = 0,
        dictionary_search_mode = SearchMode.Combined,
        dictionary_source_filter_idx = 0,
        dictionary_show_pali_buttons = True,

        sutta_max_width = 75,
        sutta_language_filter_idx = 0,
        sutta_search_mode = SearchMode.FulltextMatch,
        sutta_source_filter_idx = 0,
        suttas_show_pali_buttons = True,

        sutta_search_pos = WindowPosSize(x = 100, y = 100, width = 1200, height = 800),
        dictionary_search_pos = WindowPosSize(x = 100, y = 100, width = 1200, height = 800),
        ebook_reader_pos = WindowPosSize(x = 100, y = 100, width = 1200, height = 800),
        sutta_study_pos = WindowPosSize(x = 100, y = 100, width = 1200, height = 800),

        sutta_study_one_language_filter_idx = 0,
        sutta_study_one_search_mode = SearchMode.FulltextMatch,
        sutta_study_one_source_filter_idx = 0,

        sutta_study_two_language_filter_idx = 0,
        sutta_study_two_search_mode = SearchMode.FulltextMatch,
        sutta_study_two_source_filter_idx = 0,

        sutta_study_three_language_filter_idx = 0,
        sutta_study_three_search_mode = SearchMode.FulltextMatch,
        sutta_study_three_source_filter_idx = 0,

        sutta_study_lookup_language_filter_idx = 0,
        sutta_study_lookup_search_mode = SearchMode.Combined,
        sutta_study_lookup_source_filter_idx = 0,
        sutta_study_lookup_on_side = True,
        sutta_study_deconstructor_above_words = True,

        word_lookup_pos = WindowPosSize(x = 100, y = 100, width = 500, height = 700),
        word_lookup_language_filter_idx = 0,
        word_lookup_search_mode = SearchMode.Combined,
        word_lookup_source_filter_idx = 0,
    )

# Message to show to the user.
class AppMessage(TypedDict):
    kind: str
    text: str


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


class AppWindowInterface(QMainWindow):
    action_Check_for_Simsapa_Updates: QAction
    action_Check_for_DPD_Updates: QAction
    action_Notify_About_Simsapa_Updates: QAction
    action_Notify_About_DPD_Updates: QAction
    action_Show_Toolbar: QAction
    action_Link_Preview: QAction
    action_Show_Word_Lookup: QAction
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
    action_Close_Window: QAction
    action_Keep_Running_in_the_Background: QAction
    action_Start_in_Low_Memory_Mode: QAction
    action_Tray_Click_Opens_Window: QAction

    toolBar: QToolBar
    queue_id: str
    search_input: QLineEdit
    start_loading_animation: Callable
    stop_loading_animation: Callable

    _focus_search_input: Callable

class GuiSearchQueriesInterface:
    sutta_queries: SuttaQueriesInterface
    dictionary_queries: DictionaryQueriesInterface

    start_search_query_workers: Callable
    start_exact_query_worker: Callable
    results_page: Callable[[int], List[SearchResult]]
    query_hits: Callable[[], Optional[int]]
    all_finished: Callable[[], bool]

class SearchBarInterface(QWidget):
    search_input: QLineEdit
    page_len: int
    get_page_num: Callable[[], int]
    _search_area: SearchArea
    _set_query: Callable[[str], None]
    _handle_query: Callable
    _handle_exact_query: Callable
    _search_query_finished: Callable[[datetime], None]
    _exact_query_finished: Callable
    _queries: GuiSearchQueriesInterface
    _search_mode_setting_key: str
    _language_filter_setting_key: str
    _source_filter_setting_key: str
    _init_search_input_completer: Callable[[], None]
    _disable_search_input_completer: Callable[[], None]

    action_Show_Search_Bar: QAction
    action_Show_Search_Options: QAction

    language_filter_dropdown: QComboBox
    language_include_btn: QPushButton

    source_filter_dropdown: QComboBox
    source_include_btn: QPushButton

    search_mode_dropdown: QComboBox

    regex_checkbox: QCheckBox
    fuzzy_spin: QSpinBox

    start_loading_animation: Callable[[], None]
    stop_loading_animation: Callable[[], None]

class SuttaSearchWindowStateInterface(SearchBarInterface):
    open_sutta_new_signal: pyqtSignal
    open_in_study_window_signal: pyqtSignal
    link_mouseover: pyqtSignal
    link_mouseleave: pyqtSignal
    page_dblclick: pyqtSignal
    hide_preview: pyqtSignal
    bookmark_edit: pyqtSignal
    show_find_panel: pyqtSignal
    open_gpt_prompt: pyqtSignal

    _show_sutta_by_uid: Callable
    _get_selection: Callable
    _get_active_tab: Callable
    _find_panel: FindPanel

    reload_page: Callable[[], None]

class SuttaSearchWindowInterface(AppWindowInterface):
    addToolBar: Callable
    _update_sidebar_fulltext: Callable[[Optional[int]], List[SearchResult]]
    _set_recent_list: Callable
    get_links_tab_text: Callable[[], str]
    set_links_tab_text: Callable[[str], None]
    show_network_graph: Callable
    hide_network_graph: Callable
    update_memos_list_for_sutta: Callable
    _select_next_recent: Callable
    _select_prev_recent: Callable
    _toggle_sidebar: Callable
    results_page: Callable[[int], List[SearchResult]]
    query_hits: Callable[[], Optional[int]]
    result_pages_count: Callable[[], Optional[int]]
    get_page_num: Callable[[], int]

    queue_id: str
    handle_messages: Callable

    rightside_tabs: QTabWidget
    palibuttons_frame: QFrame
    action_Reload_Page: QAction
    action_Dictionary_Search: QAction
    action_Show_Sidebar: QAction
    action_Show_Related_Suttas: QAction
    action_Show_Translation_and_Pali_Line_by_Line: QAction
    action_Show_All_Variant_Readings: QAction
    action_Show_Bookmarks: QAction
    action_Generate_Links_Graph: QAction
    action_Find_in_Page: QAction

    lookup_in_dictionary_signal: pyqtSignal
    graph_link_mouseover: pyqtSignal
    lookup_in_new_sutta_window_signal: pyqtSignal

    s: SuttaSearchWindowStateInterface

class SuttaPanelSettingKeys(TypedDict):
    language_filter_setting_key: str
    search_mode_setting_key: str
    source_filter_setting_key: str

class SuttaPanel(TypedDict):
    layout_widget: QWidget
    layout: QVBoxLayout
    searchbar_layout: QHBoxLayout
    tabs_layout: QVBoxLayout
    state: SuttaSearchWindowStateInterface
    setting_keys: SuttaPanelSettingKeys

class SuttaStudyWindowInterface(SuttaSearchWindowInterface):
    reload_sutta_pages: Callable
    find_toolbar: QToolBar
    find_panel_layout: QHBoxLayout
    sutta_panels: List[SuttaPanel]
    _show_sutta_by_uid_in_side: Callable
    action_Study_Dictionary_Placement: QAction
    action_Deconstructor_Placement: QAction

class EbookReaderWindowInterface(SuttaSearchWindowInterface):
    addToolBar: Callable

class DictionarySearchWindowInterface(AppWindowInterface, SearchBarInterface):
    action_Show_Sidebar: QAction
    rightside_tabs: QTabWidget
    palibuttons_frame: QFrame
    action_Reload_Page: QAction
    results_page: Callable[[int], List[SearchResult]]
    query_hits: Callable[[], Optional[int]]
    result_pages_count: Callable[[], Optional[int]]
    get_page_num: Callable[[], int]
    _show_word_by_url: Callable[[QUrl, bool], None]
    _toggle_sidebar: Callable

    queue_id: str
    handle_messages: Callable
    _handle_exact_query: Callable

class BookmarksBrowserWindowInterface(AppWindowInterface):
    reload_bookmarks: Callable
    reload_table: Callable

class WordLookupStateInterface(SearchBarInterface):
    lookup_in_dictionary: Callable
    connect_preview_window_signals: Callable

    _clipboard: Optional[QClipboard]
    _show_word_by_url: Callable[[QUrl], None]
    _current_words: List[UDictWord]

    show_sutta_by_url: pyqtSignal
    show_words_by_url: pyqtSignal
    link_mouseleave: pyqtSignal
    link_mouseover: pyqtSignal
    hide_preview: pyqtSignal

class WordLookupInterface(QMainWindow):
    center: Callable
    s: WordLookupStateInterface

class LinkHoverData(TypedDict):
    href: str
    x: int
    y: int
    width: int
    height: int

def sutta_quote_from_url(url: QUrl) -> Optional[SuttaQuote]:
    query = parse_qs(url.query())

    sutta_quote = None
    quote: Optional[str] = None
    if 'quote' in query.keys():
        quote = unquote(query['quote'][0])

    if quote:
        selection_range = None
        if 'sel' in query.keys():
            selection_range = query['sel'][0]
        elif 'selection_range' in query.keys():
            selection_range = query['selection_range'][0]

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
