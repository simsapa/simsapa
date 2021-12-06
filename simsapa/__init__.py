import os
from pathlib import Path
from typing import Dict
from queue import Queue
from dotenv import load_dotenv
import appdirs

load_dotenv()

SIMSAPA_PACKAGE_DIR = Path(os.path.dirname(__file__)).absolute()

ALEMBIC_INI = SIMSAPA_PACKAGE_DIR.joinpath('alembic.ini')
ALEMBIC_DIR = SIMSAPA_PACKAGE_DIR.joinpath('alembic')

SIMSAPA_DIR = Path(appdirs.user_data_dir('simsapa'))

TEST_ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('../tests/data/assets')

s = os.getenv('USE_TEST_DATA')
if s is not None and s.lower() == 'true':
    ASSETS_DIR = TEST_ASSETS_DIR
else:
    ASSETS_DIR = SIMSAPA_DIR.joinpath('assets')

INDEX_DIR = ASSETS_DIR.joinpath('index')

APP_DB_PATH = ASSETS_DIR.joinpath('appdata.sqlite3')
USER_DB_PATH = ASSETS_DIR.joinpath('userdata.sqlite3')

APP_QUEUES: Dict[str, Queue] = {}
