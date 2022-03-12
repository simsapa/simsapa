import os
from pathlib import Path

from simsapa.app.types import AppData


def get_app_data():
    tests_data_dir = Path(os.path.dirname(__file__)).absolute().joinpath('../data/assets')
    app_db_path = tests_data_dir.joinpath('appdata.sqlite3')
    user_db_path = tests_data_dir.joinpath('userdata.sqlite3')

    app_data = AppData(app_db_path=app_db_path, user_db_path=user_db_path)

    return app_data
