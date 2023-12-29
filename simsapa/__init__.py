import os, sys
from pathlib import Path
from typing import Dict, Optional, TypedDict
from enum import Enum
import queue
from dotenv import load_dotenv
from platformdirs import PlatformDirs
import platform
import importlib.resources
import pkgutil
import psutil

from datetime import datetime
INIT_START_TIME = datetime.now()

EXEC_FILE_PATH = Path(sys.argv[0])

for i in ['.env', '.env.txt', 'config.txt']:
    p = EXEC_FILE_PATH.parent.joinpath(i)
    if p.exists():
        load_dotenv(p)
        break

# When running from dev folder, get_app_version() returns t['tool']['poetry']['version'].
# When running the prod app, the value below is used.
#
# In the PyInstaller build for Windows, importlib.metadata.version('simsapa') errors out with missing module.
SIMSAPA_APP_VERSION = "0.4.1-alpha.1"

SIMSAPA_PACKAGE_DIR = importlib.resources.files('simsapa')

# No trailing slash
SIMSAPA_RELEASES_BASE_URL = "https://simsapa.eu.pythonanywhere.com"

PACKAGE_ASSETS_RSC_DIR = Path('assets')
PACKAGE_ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath(str(PACKAGE_ASSETS_RSC_DIR))

PACKAGE_DPD_TEMPLATES_DIR = Path(str(SIMSAPA_PACKAGE_DIR.joinpath("assets/templates/dpd/")))

ALEMBIC_INI = SIMSAPA_PACKAGE_DIR.joinpath('alembic.ini')
ALEMBIC_DIR = SIMSAPA_PACKAGE_DIR.joinpath('alembic')

ICONS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('assets/icons/')

s = ""
if len(sys.argv) >= 2:
    s = sys.argv[1]

PLATFORM_DIRS = PlatformDirs(appname="simsapa")

if s.startswith("--simsapa-dir="):
    # Remove it so typer can parse other cli options.
    del sys.argv[1]
    SIMSAPA_DIR = Path(s.replace("--simsapa-dir=", "")).expanduser()

else:
    s = os.getenv('SIMSAPA_DIR')
    if s is not None and s != '':
        SIMSAPA_DIR = Path(s)
    else:
        # Linux: ~/.local/share/simsapa
        # Mac: ~/Library/Application\ Support/simsapa/simsapa
        # Windows: C:\Users\%USERNAME%\AppData\Local\simsapa\simsapa
        SIMSAPA_DIR = Path(PLATFORM_DIRS.user_data_dir)

SIMSAPA_LOG_PATH = SIMSAPA_DIR.joinpath('log.txt')

SIMSAPA_API_DEFAULT_PORT = 4848

SIMSAPA_API_PORT_PATH = SIMSAPA_DIR.joinpath("api-port.txt")

USER_HOME_DIR = PLATFORM_DIRS.user_desktop_path.parent

s = os.getenv('LOG_PERCENT_PROGRESS')
if s is not None and s == 'true':
    LOG_PERCENT_PROGRESS = True
else:
    LOG_PERCENT_PROGRESS = False

TEST_ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('../tests/data/assets')

TIMER_SPEED = 30

LOW_MEM_THRESHOLD = 3*1024*1024*1024

mem = psutil.virtual_memory()
if mem.available < LOW_MEM_THRESHOLD:
    START_LOW_MEM = True
else:
    START_LOW_MEM = False

if START_LOW_MEM:
    SEARCH_TIMER_SPEED = 800
else:
    SEARCH_TIMER_SPEED = 400

INDEX_WRITER_MEMORY_MB = 512

#s = os.getenv('USE_TEST_DATA')
#if s is not None and s.lower() == 'true':
#    ASSETS_DIR = TEST_ASSETS_DIR
#else:
#    ASSETS_DIR = SIMSAPA_DIR.joinpath('assets')

ASSETS_DIR = SIMSAPA_DIR.joinpath('assets')

INDEX_DIR = ASSETS_DIR.joinpath('index')
SUTTAS_INDEX_DIR = INDEX_DIR.joinpath('suttas')
DICT_WORDS_INDEX_DIR = INDEX_DIR.joinpath('dict_words')

GRAPHS_DIR = ASSETS_DIR.joinpath('graphs')

APP_DB_PATH = ASSETS_DIR.joinpath('appdata.sqlite3')
USER_DB_PATH = ASSETS_DIR.joinpath('userdata.sqlite3')

DPD_DB_PATH = ASSETS_DIR.joinpath('dpd.sqlite3')

COURSES_DIR = ASSETS_DIR.joinpath('courses')

EBOOK_UNZIP_DIR = ASSETS_DIR.joinpath('ebook_unzip')

HTML_RESOURCES_APPDATA_DIR = ASSETS_DIR.joinpath('html_resources/appdata/')
HTML_RESOURCES_USERDATA_DIR = ASSETS_DIR.joinpath('html_resources/userdata/')

STARTUP_MESSAGE_PATH = SIMSAPA_DIR.joinpath("startup_message.json")

APP_QUEUES: Dict[str, queue.Queue] = {}
SERVER_QUEUE = queue.Queue()

IS_GUI = False

def set_is_gui(value: bool):
    global IS_GUI
    IS_GUI = value

def get_is_gui() -> bool:
    global IS_GUI
    return IS_GUI

IS_LINUX = (platform.system() == 'Linux')
IS_WINDOWS = (platform.system() == 'Windows')
IS_MAC = (platform.system() == 'Darwin')

if IS_LINUX:
    s = os.getenv('XDG_CURRENT_DESKTOP')
    IS_SWAY = s is not None and s == 'sway'
else:
    IS_SWAY = False

DESKTOP_FILE_PATH: Optional[Path] = None

if IS_LINUX:
    # ~/.local/share/applications/simsapa.desktop
    DESKTOP_FILE_PATH = USER_HOME_DIR.joinpath(".local/share/applications/simsapa.desktop")

b =  pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath("releases_fallback.json")))
if b is None:
    RELEASES_FALLBACK_JSON = ""
else:
    # Value to use in case the network request fails.
    RELEASES_FALLBACK_JSON = b.decode("utf-8")

READING_TEXT_COLOR = "#1a1a1a" # 90% black
READING_BACKGROUND_COLOR = "#FAE6B2"
DARK_READING_BACKGROUND_COLOR = "#F0B211"
BUTTON_BG_COLOR = "#007564"

b = pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath("templates/page.html")))
if b is None:
    PAGE_HTML = "<b>Missing page.html</b>"
else:
    PAGE_HTML = b.decode("utf-8")

b = pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath("templates/loading.html")))
if b is None:
    LOADING_HTML = "<b>Loading...</b>"
else:
    LOADING_HTML = b.decode("utf-8")

b =  pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath("templates/click_generate.html")))
if b is None:
    CLICK_GENERATE_HTML = "<b>Missing click_generate.html</b>"
else:
    CLICK_GENERATE_HTML = b.decode("utf-8")


b = pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath('css/suttas.css')))
if b is None:
    SUTTAS_CSS = ""
else:
    SUTTAS_CSS = b.decode("utf-8")

b = pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath('js/suttas.js')))
if b is None:
    SUTTAS_JS = ""
else:
    SUTTAS_JS = b.decode("utf-8")

b = pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath('css/ebook_extra.css')))
if b is None:
    EBOOK_EXTRA_CSS = ""
else:
    EBOOK_EXTRA_CSS = b.decode("utf-8")

b = pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath('js/ebook_extra.js')))
if b is None:
    EBOOK_EXTRA_JS = ""
else:
    EBOOK_EXTRA_JS = b.decode("utf-8")

class DbSchemaName(str, Enum):
    AppData = 'appdata'
    UserData = 'userdata'
    Dpd = 'dpd'

class DictTypeName(str, Enum):
    Sql = 'sql'
    Stardict = 'stardict'
    Custom = 'custom'

class ApiAction(str, Enum):
    lookup_clipboard_in_dictionary = 'lookup_clipboard_in_dictionary'
    lookup_clipboard_in_suttas = 'lookup_clipboard_in_suttas'
    lookup_in_dictionary = 'lookup_in_dictionary'
    lookup_in_suttas = 'lookup_in_suttas'
    open_in_study_window = 'open_in_study_window'
    open_sutta_new = 'open_sutta_new'
    open_words_new = 'open_words_new'
    show_sutta_by_uid = 'show_sutta_by_uid'
    show_sutta_by_url = 'show_sutta_by_url'
    show_sutta = 'show_sutta'
    show_word_by_uid = 'show_word_by_uid'
    show_word_by_url = 'show_word_by_url'
    show_word_lookup = 'show_word_lookup'
    closed_word_lookup = 'closed_word_lookup'
    hidden_word_lookup = 'hidden_word_lookup'
    set_selected = "set_selected"
    remove_closed_window_from_list = "remove_closed_window_from_list"

# Messages sent via the localhost web API
class ApiMessage(TypedDict):
    queue_id: str
    action: ApiAction
    data: str

class ShowLabels(str, Enum):
    SuttaRef = "Sutta Ref."
    RefAndTitle = "Ref. + Title"
    NoLabels = "No Labels"

class DetailsTab(str, Enum):
    Examples = "Examples"
    Inflections = "Inflections"
    RootFamily = "Root Family"
    WordFamily = "Word Family"
    CompoundFamily = "Compound Family"
    SetFamily = "Set Family"
    FrequencyMap = "Frequency Map"
    Feedback = "Feedback"
    RootInfo = "Root Info"

class QueryType(str, Enum):
    suttas = "suttas"
    words = "words"

class QuoteScope(str, Enum):
    Sutta = 'sutta'
    Nikaya = 'nikaya'
    All = 'all'

QuoteScopeValues = {
    'sutta': QuoteScope.Sutta,
    'nikaya': QuoteScope.Nikaya,
    'all': QuoteScope.All,
}

class SuttaQuote(TypedDict):
    quote: str
    selection_range: Optional[str]
