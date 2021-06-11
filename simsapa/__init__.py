import os
from pathlib import Path
import appdirs  # type: ignore

SIMSAPA_PACKAGE_DIR = Path(os.path.dirname(__file__)).absolute()
SIMSAPA_MIGRATIONS_DIR = SIMSAPA_PACKAGE_DIR.joinpath('migrations')

SIMSAPA_DIR = Path(appdirs.user_data_dir('simsapa'))
ASSETS_DIR = SIMSAPA_DIR.joinpath('assets')
APP_DB_PATH = ASSETS_DIR.joinpath('appdata.sqlite3')
USER_DB_PATH = ASSETS_DIR.joinpath('userdata.sqlite3')
