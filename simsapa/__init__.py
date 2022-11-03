import os
from pathlib import Path
from typing import Dict, TypedDict
from enum import Enum
from queue import Queue
from dotenv import load_dotenv
import appdirs
import platform

load_dotenv()

SIMSAPA_PACKAGE_DIR = Path(os.path.dirname(__file__)).absolute()

PACKAGE_ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('assets')

ALEMBIC_INI = SIMSAPA_PACKAGE_DIR.joinpath('alembic.ini')
ALEMBIC_DIR = SIMSAPA_PACKAGE_DIR.joinpath('alembic')

ICONS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('assets/icons/')

s = os.getenv('SIMSAPA_DIR')
if s is not None and s != '':
    SIMSAPA_DIR = Path(s)
else:
    SIMSAPA_DIR = Path(appdirs.user_data_dir('simsapa'))

SIMSAPA_LOG_PATH = SIMSAPA_DIR.joinpath('log.txt')

TEST_ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('../tests/data/assets')

TIMER_SPEED = 50

SEARCH_TIMER_SPEED = 300

s = os.getenv('USE_TEST_DATA')
if s is not None and s.lower() == 'true':
    ASSETS_DIR = TEST_ASSETS_DIR
else:
    ASSETS_DIR = SIMSAPA_DIR.joinpath('assets')

INDEX_DIR = ASSETS_DIR.joinpath('index')

GRAPHS_DIR = ASSETS_DIR.joinpath('graphs')

APP_DB_PATH = ASSETS_DIR.joinpath('appdata.sqlite3')
USER_DB_PATH = ASSETS_DIR.joinpath('userdata.sqlite3')

STARTUP_MESSAGE_PATH = SIMSAPA_DIR.joinpath("startup_message.json")

APP_QUEUES: Dict[str, Queue] = {}

IS_LINUX = (platform.system() == 'Linux')
IS_WINDOWS = (platform.system() == 'Windows')
IS_MAC = (platform.system() == 'Darwin')

READING_TEXT_COLOR = "#1a1a1a" # 90% black
READING_BACKGROUND_COLOR = "#FAE6B2"
DARK_READING_BACKGROUND_COLOR = "#F0B211"

LOADING_HTML = open(PACKAGE_ASSETS_DIR.joinpath('templates/loading.html'), 'r').read()

class DbSchemaName(str, Enum):
    AppData = 'appdata'
    UserData = 'userdata'

class ApiAction(str, Enum):
    lookup_clipboard_in_dictionary = 'lookup_clipboard_in_dictionary'
    lookup_clipboard_in_suttas = 'lookup_clipboard_in_suttas'
    lookup_in_dictionary = 'lookup_in_dictionary'
    lookup_in_suttas = 'lookup_in_suttas'
    open_in_study_window = 'open_in_study_window'
    open_sutta_new = 'open_sutta_new'
    open_words_new = 'open_words_new'
    show_sutta_by_uid = 'show_sutta_by_uid'
    show_sutta = 'show_sutta'
    show_word_by_uid = 'show_word_by_uid'
    show_word_scan_popup = 'show_word_scan_popup'
    set_selected = "set_selected"

# Messages sent via the localhost web API
class ApiMessage(TypedDict):
    action: ApiAction
    data: str

class ShowLabels(str, Enum):
    SuttaRef = "Sutta Ref."
    RefAndTitle = "Ref. + Title"
    NoLabels = "No Labels"
