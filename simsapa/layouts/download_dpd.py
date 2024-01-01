from functools import partial
import os, shutil, tarfile
from pathlib import Path
from typing import Callable

from PyQt6 import QtWidgets
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QCloseEvent
from PyQt6.QtWidgets import (QMainWindow, QWidget, QVBoxLayout, QPushButton, QLabel)

from simsapa import DPD_RELEASES_BASE_URL, logger
from simsapa.app.db_helpers import find_or_create_dpd_dictionary, migrate_dpd
from simsapa.app.db_session import get_db_engine_connection_session

from simsapa.layouts.gui_types import QSizeExpanding, QSizeMinimum

class DownloadDpdWindow(QMainWindow):
    def __init__(self,
                 assets_dir: Path,
                 dpd_release_version_tag: str,
                 quit_action_fn: Callable) -> None:
        super().__init__()

        self.setWindowTitle("Download Digital Pāḷi Dictionary Update")
        self.setFixedSize(400, 400)
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)

        self.assets_dir = assets_dir
        self.dpd_release_version_tag = dpd_release_version_tag
        self._quit_action = quit_action_fn

        self._setup_ui()

        self._run_download()

        self._quit_button.setVisible(True)

    def _setup_ui(self):
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._central_widget.setLayout(self._layout)

        spc1 = QtWidgets.QSpacerItem(20, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spc1)

        self._msg = QLabel("<p>Downloading...<p>")
        self._msg.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._layout.addWidget(self._msg)

        self.spc4 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(self.spc4)

        self._quit_button = QPushButton("Quit")
        self._quit_button.setVisible(False)
        self._quit_button.clicked.connect(partial(self._handle_quit))

        self._layout.addWidget(self._quit_button)

    def _handle_close(self):
        self.close()

    def _handle_quit(self):
        self._quit_action()

    def closeEvent(self, event: QCloseEvent):
        self._handle_close()
        event.accept()

    def _run_download(self):
        version = self.dpd_release_version_tag
        dpd_tar_url  = f"{DPD_RELEASES_BASE_URL}/releases/download/{version}/dpd.tar.bz2"
        logger.info(f"Download url: {dpd_tar_url}")

        try:
            tar_path = self.download_file(dpd_tar_url, self.assets_dir)

            extract_temp_dir = self.assets_dir.joinpath('extract_temp')

            self._msg.setText("<p>Extracting...</p>")

            self.extract_tar_bz2(tar_path, extract_temp_dir)

            self._msg.setText("<p>Migrating database...</p>")

            db_eng, db_conn, db_session = get_db_engine_connection_session()

            dpd_dict = find_or_create_dpd_dictionary(db_session)
            dpd_dict_id = dpd_dict.id

            db_conn.close()
            db_session.close()
            db_eng.dispose()

            dpd_path = extract_temp_dir.joinpath("dpd.db")

            migrate_dpd(dpd_path, dpd_dict_id)

            shutil.move(dpd_path, self.assets_dir.joinpath('dpd.sqlite3'))

            # Remove the temp folder where the assets were extraced to.
            shutil.rmtree(extract_temp_dir)

            self._msg.setText("<p>Completed.</p>")

        except Exception as e:
            raise e

    def download_file(self, url: str, folder_path: Path) -> Path:
        logger.info(f"download_file() : {url}, {folder_path}")

        file_name = url.split('/')[-1]
        file_path = folder_path.joinpath(file_name)

        try:
            import requests
            with requests.get(url, stream=True) as r:
                chunk_size = 8192
                read_bytes = 0

                r.raise_for_status()
                with open(file_path, 'wb') as f:
                    for chunk in r.iter_content(chunk_size=chunk_size):
                        f.write(chunk)
                        read_bytes += chunk_size

        except Exception as e:
            raise e

        return file_path

    def extract_tar_bz2(self, tar_file_path: Path, extract_temp_dir: Path):
        tar = tarfile.open(tar_file_path, "r:bz2")
        tar.extractall(extract_temp_dir)
        tar.close()

        os.remove(tar_file_path)
