import os
from pathlib import Path
from dotenv import load_dotenv
import appdirs  # type: ignore

load_dotenv()

SIMSAPA_PACKAGE_DIR = Path(os.path.dirname(__file__)).absolute()
SIMSAPA_MIGRATIONS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('migrations')

SIMSAPA_DIR = Path(appdirs.user_data_dir('simsapa'))

if os.getenv('USE_TEST_DATA').lower() == 'true':
    ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('../tests/data/assets')
else:
    ASSETS_DIR = SIMSAPA_DIR.joinpath('assets')

TEST_ASSETS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('../tests/data/assets')

APP_DB_PATH = ASSETS_DIR.joinpath('appdata.sqlite3')
USER_DB_PATH = ASSETS_DIR.joinpath('userdata.sqlite3')
