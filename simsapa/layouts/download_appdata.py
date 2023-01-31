import os
import glob
from functools import partial
from pathlib import Path
import shutil
import tarfile
import requests
import threading
from typing import List, Optional
from PyQt6 import QtWidgets

from PyQt6.QtCore import QRunnable, QThreadPool, Qt, pyqtSlot, QObject, pyqtSignal
from PyQt6.QtWidgets import (QCheckBox, QFrame, QMessageBox, QRadioButton, QWidget, QVBoxLayout, QHBoxLayout, QPushButton, QLabel, QMainWindow, QProgressBar)
from PyQt6.QtGui import QMovie

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import make_transient

from simsapa.app.types import QSizeExpanding, QSizeMinimum

from simsapa.app.db import appdata_models as Am
from simsapa.app.db_helpers import get_db_engine_connection_session
from simsapa.app.helpers import get_app_version, get_latest_app_compatible_assets_release, get_release_channel, get_releases_info

from simsapa import SIMSAPA_RELEASES_BASE_URL, logger, INDEX_DIR, ASSETS_DIR, COURSES_DIR, APP_DB_PATH, USER_DB_PATH

from simsapa.assets import icons_rc


class DownloadAppdataWindow(QMainWindow):
    _msg: QLabel

    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("Download Application Assets")
        self.setFixedSize(350, 400)
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)

        self.thread_pool = QThreadPool()

        self.download_worker = Worker()

        self._setup_ui()

    def _setup_ui(self):
        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        spc1 = QtWidgets.QSpacerItem(20, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spc1)

        self._setup_animation()

        spc2 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spc2)

        self._msg = QLabel("<p>The application database<br>was not found on this system.</p><p>Please select the sources to download.<p>")
        self._msg.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._layout.addWidget(self._msg)

        self._setup_info_frame()

        spc3 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spc3)

        self._progress_bar = QProgressBar()
        self._progress_bar.hide()
        self._layout.addWidget(self._progress_bar)

        spacerItem = QtWidgets.QSpacerItem(20, 0, QSizeMinimum, QSizeExpanding)
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

    def _show_error(self, msg: str):
        box = QMessageBox(self)
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle("Download Error")
        box.setText(msg)
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()

    def _run_download(self):
        # Retreive released asset versions from github.
        # Filter for app-compatible db versions, major and minor version number must agree.

        try:
            requests.head(SIMSAPA_RELEASES_BASE_URL, timeout=5)
        except Exception as e:
            msg = "No connection, cannot download database: %s" % e
            QMessageBox.information(self,
                                    "No Connection",
                                    msg,
                                    QMessageBox.StandardButton.Ok)
            logger.error(msg)
            return

        try:
            info = get_releases_info()
            compat_release = get_latest_app_compatible_assets_release(info)
            if compat_release is None:
                asset_versions = list(map(lambda x: x['version_tag'], info['assets']['releases']))
                msg = f"""
                <p>Cannot find a compatible asset release.<br>
                Release channel: {get_release_channel()}<br>
                Application version: {get_app_version()}<br>
                Available asset versions: {", ".join(asset_versions)}</p>
                """.strip()

                raise(Exception(msg))

        except Exception as e:
            msg = "Download failed: %s" % e
            logger.error(msg)
            QMessageBox.information(self,
                                    "Error",
                                    msg,
                                    QMessageBox.StandardButton.Ok)
            return

        version = compat_release["version_tag"]
        github_repo = compat_release["github_repo"]

        # ensure 'v' prefix
        if version[0] != 'v':
            version = 'v' + version

        appdata_tar_url  = f"https://github.com/{github_repo}/releases/download/{version}/appdata.tar.bz2"
        userdata_tar_url = f"https://github.com/{github_repo}/releases/download/{version}/userdata.tar.bz2"
        index_tar_url    = f"https://github.com/{github_repo}/releases/download/{version}/index.tar.bz2"

        sanskrit_appdata_tar_url  = f"https://github.com/{github_repo}/releases/download/{version}/sanskrit-appdata.tar.bz2"
        sanskrit_index_tar_url    = f"https://github.com/{github_repo}/releases/download/{version}/sanskrit-index.tar.bz2"

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


        if not USER_DB_PATH.exists():
            urls.append(userdata_tar_url)

        # Remove existing indexes here. Can't safely clear and remove them in
        # windows._redownload_database_dialog().
        self._remove_old_index()

        self.download_worker.urls = urls

        self.download_worker.signals.msg_update.connect(partial(self._msg_update))

        self.download_worker.signals.error_msg.connect(partial(self._show_error))

        self.download_worker.signals.finished.connect(partial(self._download_finished))
        self.download_worker.signals.cancelled.connect(partial(self._download_cancelled))

        self.download_worker.signals.set_current_progress.connect(partial(self._progress_bar.setValue))
        self.download_worker.signals.set_total_progress.connect(partial(self._progress_bar.setMaximum))
        self.download_worker.signals.download_max.connect(partial(self._download_max))

        logger.info("Show progress bar")
        self._progress_bar.show()

        logger.info("Start download worker")
        self.thread_pool.start(self.download_worker)

        self.info_frame.hide()

        self.setup_animation()
        self.start_animation()

    def _download_max(self):
        self._progress_bar.setValue(self._progress_bar.maximum())

    def _msg_update(self, msg: str):
        self._msg.setText(msg)

    def _remove_old_index(self):
        if INDEX_DIR.exists():
            shutil.rmtree(INDEX_DIR)

        # FIXME When removing the previous index, test if there were user imported
        # data in the userdata database, and if so, tell the user that they have to
        # re-index.

        # msg = AppMessage(
        #     kind = "warning",
        #     text = "<p>The sutta and dictionary database was updated. Re-indexing is necessary for the contents to be searchable.</p><p>You can start a re-index operation with <b>File > Re-index database</b>.</p>",
        # )
        # with open(STARTUP_MESSAGE_PATH, 'w') as f:
        #     f.write(json.dumps(msg))

    def _cancelled_cleanup_files(self):
        # Don't remove assets dir, it may contain userdata.sqlite3 with user's
        # memos, bookmarks, settings, etc.

        if INDEX_DIR.exists():
            shutil.rmtree(INDEX_DIR)

        if APP_DB_PATH.exists():
            APP_DB_PATH.unlink()

    def _download_finished(self):
        self.stop_animation()
        self._animation.deleteLater()

        self._msg.setText("<p>Download completed.</p><p>Quit and start the application again.</p>")

    def _download_cancelled(self):
        self.stop_animation()
        self._animation.deleteLater()
        self._cancelled_cleanup_files()
        self.close()

    def _handle_quit(self):
        if self.download_worker.download_started.isSet():
            self._msg.setText("<p>Setup cancelled, waiting for subprocesses to end ...</p>")
            self.download_worker.download_stop.set()
            # Don't .close() here, wait for _download_cancelled() to do it.
            #
            # Otherwise, triggering .close() while the extraction is not finished will cause a threading error.
            #
            # RuntimeError: wrapped C/C++ object of type WorkerSignals has been deleted
            # QObject: Cannot create children for a parent that is in a different thread.
            # (Parent is QApplication(0x4050170), parent's thread is QThread(0x2785280), current thread is QThreadPoolThread(0x51dbd00)
            # fish: Job 1, './run.py' terminated by signal SIGSEGV (Address boundary error)

        else:
            self.close()

class WorkerSignals(QObject):
    finished = pyqtSignal()
    cancelled = pyqtSignal()
    msg_update = pyqtSignal(str)
    error_msg = pyqtSignal(str)
    set_total_progress = pyqtSignal(int)
    set_current_progress = pyqtSignal(int)
    download_max = pyqtSignal()


class Worker(QRunnable):
    urls: List[str]
    signals: WorkerSignals
    download_started: threading.Event
    download_stop: threading.Event

    def __init__(self):
        super(Worker, self).__init__()

        self.signals = WorkerSignals()

        self.download_started = threading.Event()
        self.download_stop = threading.Event()

    @pyqtSlot()
    def run(self):
        logger.info("Worker::run()")
        try:
            self.download_started.set()

            logger.info(f"Download urls: {self.urls}")

            for i in self.urls:
                self.download_extract_tar_bz2(i)
                if self.download_stop.is_set():
                    logger.info("run(): Cancelling")
                    self.signals.cancelled.emit()
                    return

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
            msg = "%s" % e
            logger.error(msg)
            self.signals.error_msg.emit(msg)

        finally:
            self.download_started.clear()
            self.signals.finished.emit()

    def download_file(self, url: str, folder_path: Path) -> Optional[Path]:
        logger.info(f"download_file() : {url}, {folder_path}")

        file_name = url.split('/')[-1]
        file_path = folder_path.joinpath(file_name)

        try:
            with requests.get(url, stream=True) as r:
                chunk_size = 8192
                read_bytes = 0
                total_bytes = int(r.headers['Content-Length'])
                self.signals.set_total_progress.emit(total_bytes)

                total_mb = "%.2f" % (total_bytes / 1024 / 1024)

                r.raise_for_status()
                with open(file_path, 'wb') as f:
                    for chunk in r.iter_content(chunk_size=chunk_size):
                        f.write(chunk)
                        read_bytes += chunk_size
                        read_mb = "%.2f" % (read_bytes / 1024 / 1024)

                        self.signals.set_current_progress.emit(read_bytes)
                        self.signals.msg_update.emit(f"<p>Downloading {file_name}</p><p>({read_mb} / {total_mb} MB) ...</p>")

                        if self.download_stop.is_set():
                            logger.info("download_file(): Stopping download, removing partial file.")
                            file_path.unlink()
                            return None
        except Exception as e:
            raise e

        self.signals.download_max.emit()
        return file_path

    def download_extract_tar_bz2(self, url):
        try:
            tar_file_path = self.download_file(url, ASSETS_DIR)
        except Exception as e:
            raise e

        if tar_file_path is None:
            return False

        file_name = url.split('/')[-1]
        self.signals.msg_update.emit(f"Extracting {file_name} ...")

        tar = tarfile.open(tar_file_path, "r:bz2")
        temp_dir = ASSETS_DIR.joinpath('extract_temp')
        tar.extractall(temp_dir)
        tar.close()

        os.remove(tar_file_path)

        for p in glob.glob(f"{temp_dir}/*.sqlite3"):
            shutil.move(p, ASSETS_DIR)

        if temp_dir.joinpath("courses").exists():
            for p in glob.glob(f"{temp_dir}/courses/*"):
                shutil.move(p, COURSES_DIR)

        temp_index = temp_dir.joinpath("index")
        if temp_index.exists():
            shutil.move(temp_index, ASSETS_DIR)

        shutil.rmtree(temp_dir)


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
