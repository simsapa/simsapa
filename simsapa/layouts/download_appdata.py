import json
import os
import glob
from functools import partial
from pathlib import Path
import shutil
import tarfile
import threading
from typing import List, Optional
from PyQt6 import QtWidgets

from PyQt6.QtCore import QRunnable, QThreadPool, Qt, pyqtSlot
from PyQt6.QtCore import QObject, pyqtSignal
from PyQt6.QtWidgets import (QCheckBox, QFrame, QMessageBox, QRadioButton, QWidget, QVBoxLayout, QHBoxLayout, QPushButton, QLabel, QMainWindow)
from PyQt6.QtGui import QMovie
import requests

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import make_transient

from simsapa.app.types import AppMessage

from simsapa.app.db import appdata_models as Am
from simsapa.app.helpers import filter_compatible_db_entries, get_db_engine_connection_session, get_feed_entries

from simsapa import INDEX_DIR, logger
from simsapa import ASSETS_DIR, APP_DB_PATH, STARTUP_MESSAGE_PATH
from simsapa.assets import icons_rc  # noqa: F401


class DownloadAppdataWindow(QMainWindow):
    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("Download Application Assets")
        self.setFixedSize(350, 400)
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)

        self.thread_pool = QThreadPool()

        self._setup_ui()

    def _setup_ui(self):
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        spacerItem = QtWidgets.QSpacerItem(20, 0, QtWidgets.QSizePolicy.Policy.Minimum, QtWidgets.QSizePolicy.Policy.Expanding)
        self._layout.addItem(spacerItem)

        self._msg = QLabel("The application database\nwas not found on this system.\n\nPlease select the sources to download.")
        self._msg.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._layout.addWidget(self._msg)

        self._setup_info_frame()
        self._setup_animation()

        spacerItem = QtWidgets.QSpacerItem(20, 0, QtWidgets.QSizePolicy.Policy.Minimum, QtWidgets.QSizePolicy.Policy.Expanding)
        self._layout.addItem(spacerItem)

        self._setup_buttons()


    def _setup_info_frame(self):
        frame = QFrame()
        frame.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        frame.setFrameShadow(QtWidgets.QFrame.Shadow.Raised)
        frame.setLineWidth(0)

        self.info_frame = frame
        self._layout.addWidget(self.info_frame)

        self.text_select_layout = QVBoxLayout()
        self.info_frame.setLayout(self.text_select_layout)

        rad = QRadioButton("General bundle")
        rad.setChecked(True)
        rad.toggled.connect(self._toggled_general_bundle)
        self.sel_general_bundle = rad
        self.text_select_layout.addWidget(self.sel_general_bundle)

        self.general_info = QLabel("Pali and English + pre-generated search index")
        self.general_info.setDisabled(False)
        self.text_select_layout.addWidget(self.general_info)

        # vertical spacing
        self.text_select_layout.addWidget(QLabel(""))

        rad = QRadioButton("Include additional texts")
        rad.setChecked(False)
        self.sel_additional = rad
        self.text_select_layout.addWidget(self.sel_additional)

        chk = QCheckBox("Pali + English (always included)")
        chk.setChecked(True)
        chk.setDisabled(True)
        self.chk_pali_english = chk
        self.text_select_layout.addWidget(self.chk_pali_english)

        chk = QCheckBox("Sanskrit texts (GRETIL)")
        chk.setChecked(True)
        chk.setDisabled(True)
        self.chk_sanskrit_texts = chk
        self.text_select_layout.addWidget(self.chk_sanskrit_texts)

        # NOTE: At the moment, the Sanskrit texts also include the index. Will need this when downloading other languages.
        # self.index_info = QLabel("(The search index will be generated on first-time\n start. This may take 30-60 mins.)")
        # self.index_info.setDisabled(True)
        # self.text_select_layout.addWidget(self.index_info)


    def _toggled_general_bundle(self):
        checked = self.sel_general_bundle.isChecked()

        self.general_info.setDisabled(not checked)

        self.chk_sanskrit_texts.setDisabled(checked)
        # self.index_info.setDisabled(checked)


    def _setup_animation(self):
        self._animation = QLabel(self)
        self._animation.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._layout.addWidget(self._animation)


    def _setup_buttons(self):
        buttons_layout = QHBoxLayout()
        buttons_layout.setContentsMargins(0, 20, 0, 10)

        self._download_button = QPushButton("Download")
        self._download_button.setFixedSize(100, 30)
        self._quit_button = QPushButton("Quit")
        self._quit_button.setFixedSize(100, 30)

        self._download_button.clicked.connect(partial(self._run_download))
        self._quit_button.clicked.connect(partial(self._handle_quit))

        buttons_layout.addWidget(self._quit_button)
        buttons_layout.addWidget(self._download_button)

        self._layout.addLayout(buttons_layout)


    def setup_animation(self):
        self._msg.setText("Downloading ...")
        self._download_button.setEnabled(False)

        self._movie = QMovie(':simsapa-loading')
        self._animation.setMovie(self._movie)


    def start_animation(self):
        self._movie.start()


    def stop_animation(self):
        self._movie.stop()


    def _run_download(self):
        # Retreive released asset versions from github.
        # Filter for app-compatible db versions, major and minor version number must agree.

        try:
            requests.head("https://github.com/", timeout=5)
        except Exception as e:
            msg = "No connection, cannot download database: %s" % e
            QMessageBox.information(self,
                                    "No Connection",
                                    msg,
                                    QMessageBox.StandardButton.Ok)
            logger.error(msg)
            return

        try:
            stable_entries = get_feed_entries("https://github.com/simsapa/simsapa-assets/releases.atom")
        except Exception as e:
            msg = "Download failed: %s" % e
            QMessageBox.information(self,
                                    "Error",
                                    msg,
                                    QMessageBox.StandardButton.Ok)
            logger.error(msg)
            return

        if len(stable_entries) == 0:
            return

        compat_entries = filter_compatible_db_entries(stable_entries)

        if len(compat_entries) == 0:
            return

        version = compat_entries[0]["version"]

        # ensure 'v' prefix
        if version[0] != 'v':
            version = 'v' + version

        appdata_tar_url  = f"https://github.com/simsapa/simsapa-assets/releases/download/{version}/appdata.tar.bz2"
        index_tar_url    = f"https://github.com/simsapa/simsapa-assets/releases/download/{version}/index.tar.bz2"

        sanskrit_appdata_tar_url  = f"https://github.com/simsapa/simsapa-assets/releases/download/{version}/sanskrit-appdata.tar.bz2"
        sanskrit_index_tar_url    = f"https://github.com/simsapa/simsapa-assets/releases/download/{version}/sanskrit-index.tar.bz2"

        # Default: General bundle
        urls = [
            appdata_tar_url,
            index_tar_url,
        ]

        if self.sel_additional.isChecked() and self.chk_sanskrit_texts.isChecked():
            urls = [
                sanskrit_appdata_tar_url,
                sanskrit_index_tar_url,
            ]

        self.download_worker = Worker(urls)

        self.download_worker.signals.finished.connect(self._download_finished)

        self.thread_pool.start(self.download_worker)

        self.info_frame.hide()

        self.setup_animation()
        self.start_animation()


    def _download_finished(self):
        self.stop_animation()
        self._animation.deleteLater()

        self._msg.setText("Download completed.\n\nQuit and start the application again.")

    def _handle_quit(self):
        self.download_worker.download_stop.set()
        self.close()


class WorkerSignals(QObject):
    finished = pyqtSignal()


class Worker(QRunnable):
    def __init__(self, urls: List[str]):
        super(Worker, self).__init__()

        self.signals = WorkerSignals()

        self.download_stop = threading.Event()

        self.urls = urls

    @pyqtSlot()
    def run(self):
        try:
            for i in self.urls:
                self.download_extract_tar_bz2(i)

            def _not_core_db(s: str) -> bool:
                p = Path(s)
                return not p.name == 'appdata.sqlite3' and not p.name == 'userdata.sqlite3'

            # If there are any other .sqlite3 files than appdata and userdata, import it to appdata
            p = ASSETS_DIR.joinpath("*.sqlite3")
            sqlite_files = list(filter(_not_core_db, glob.glob(f"{p}")))

            for i in sqlite_files:
                self.import_suttas_to_appdata(Path(i))

            for i in sqlite_files:
                Path(i).unlink()

        except Exception as e:
            # FIXME return error to download window and show the user
            msg = "%s" % e
            logger.error(msg)

        finally:
            self.signals.finished.emit()

    def download_file(self, url: str, folder_path: Path) -> Optional[Path]:

        logger.info(f"download_file() : {url}, {folder_path}")
        file_name = url.split('/')[-1]
        file_path = folder_path.joinpath(file_name)

        try:
            with requests.get(url, stream=True) as r:
                r.raise_for_status()
                with open(file_path, 'wb') as f:
                    for chunk in r.iter_content(chunk_size=8192):
                        f.write(chunk)
                        if self.download_stop.is_set():
                            logger.info("Aborting download, removing partial file.")
                            file_path.unlink()
                            return None
        except Exception as e:
            raise e

        return file_path

    def download_extract_tar_bz2(self, url) -> bool:
        try:
            tar_file_path = self.download_file(url, ASSETS_DIR)
        except Exception as e:
            raise e

        if tar_file_path is None:
            return False

        tar = tarfile.open(tar_file_path, "r:bz2")
        temp_dir = ASSETS_DIR.joinpath('extract_temp')
        tar.extractall(temp_dir)
        tar.close()

        # FIXME on Mac, it downloads the extra database (i.e.
        # sanskrit-texts.sqlite3) but then deletes it. Path name problems?

        os.remove(tar_file_path)

        for p in glob.glob(f"{temp_dir}/*.sqlite3"):
            shutil.move(p, ASSETS_DIR)

        # Remove existing indexes here. Can't safely clear and remove them in
        # windows._redownload_database_dialog().
        if INDEX_DIR.exists():
            shutil.rmtree(INDEX_DIR)

        temp_index = temp_dir.joinpath("index")
        if temp_index.exists():
            shutil.move(temp_index, ASSETS_DIR)

        # FIXME When removing the previous index, test if there were user imported
        # data in the userdata database, and if so, tell the user that they have to
        # re-index.

        # msg = AppMessage(
        #     kind = "warning",
        #     text = "<p>The sutta and dictionary database was updated. Re-indexing is necessary for the contents to be searchable.</p><p>You can start a re-index operation with <b>File > Re-index database</b>.</p>",
        # )
        # with open(STARTUP_MESSAGE_PATH, 'w') as f:
        #     f.write(json.dumps(msg))

        shutil.rmtree(temp_dir)

        if not APP_DB_PATH.exists():
            logger.error(f"File not found: {APP_DB_PATH}")
            return False
        else:
            return True

    def import_suttas_to_appdata(self, db_path: Path):
        if not db_path.exists():
            logger.error(f"Doesn't exist: {db_path}")
            return

        try:
            app_db_eng, app_db_conn, app_db_session = get_db_engine_connection_session(include_userdata=False)

            db_eng = create_engine("sqlite+pysqlite://", echo=False)
            db_conn = db_eng.connect()
            db_conn.execute(f"ATTACH DATABASE '{db_path}' AS appdata;")
            Session = sessionmaker(db_eng)
            Session.configure(bind=db_eng)

            import_db_session = Session()

            res = import_db_session.query(Am.Sutta).all()

            logger.info(f"Importing to Appdata, {len(res)} suttas from {db_path}")

            # https://stackoverflow.com/questions/28871406/how-to-clone-a-sqlalchemy-object-with-new-primary-key

            for i in res:
                try:
                    import_db_session.expunge(i)
                    make_transient(i)
                    i.id = None

                    app_db_session.add(i)
                    app_db_session.commit()
                except Exception as e:
                    logger.error(f"Import problem: {e}")

            app_db_session.close_all()
            app_db_conn.close()
            app_db_eng.dispose()

            import_db_session.close()
            db_conn.close()
            db_eng.dispose()

        except Exception as e:
            logger.error(f"Database problem: {e}")
