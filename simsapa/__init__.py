import os
from pathlib import Path
from typing import Dict
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

SIMSAPA_DIR = Path(appdirs.user_data_dir('simsapa'))

SIMSAPA_LOG_PATH = SIMSAPA_DIR.joinpath('log.txt')

TEST_ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('../tests/data/assets')

TIMER_SPEED = 50

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

