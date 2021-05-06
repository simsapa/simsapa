from pathlib import Path
import sys
import os
import platform
import logging as _logging
import logging.config
import yaml

from PyQt5.QtWidgets import QApplication  # type: ignore

from .app.types import AppData  # type: ignore

from .layouts.sutta_search import SuttaSearchWindow, SuttaSearchCtrl  # type: ignore

WINDOWS = (platform.system() == "Windows")
LINUX = (platform.system() == "Linux")
MAC = (platform.system() == "Darwin")

logger = _logging.getLogger(__name__)


def main():
    if os.path.exists("logging.yaml"):
        with open("logging.yaml", 'r') as f:
            config = yaml.safe_load(f.read())
            _logging.config.dictConfig(config)

    logger.info("main()")

    paths = [
        Path.cwd().joinpath("appdata.sqlite3"),
        Path.home().joinpath(".config/simsapa/assets/appdata.sqlite3"),
    ]

    db_path = None

    for p in paths:
        if p.is_file():
            db_path = p

    if db_path is None:
        print("ERROR: Cannot find appdata.sqlite3")
        exit(1)

    app_data = AppData(db_path)

    app = QApplication(sys.argv)

    view = SuttaSearchWindow(app_data)
    view.show()

    SuttaSearchCtrl(view=view)

    logger.info("Exiting.")
    sys.exit(app.exec_())
