import re
import os
import glob
from functools import partial
from pathlib import Path
import shutil
import tarfile
from PyQt6.QtGui import QCloseEvent, QMovie
import requests
import threading
from typing import Callable, List, Optional
from PyQt6 import QtWidgets

from PyQt6.QtCore import Qt, QAbstractTableModel, QModelIndex, QRunnable, QThreadPool, Qt, pyqtSlot, QObject, pyqtSignal
from PyQt6.QtWidgets import (QAbstractItemView, QFrame, QHeaderView, QLineEdit, QMessageBox, QTableView, QWidget, QVBoxLayout, QHBoxLayout, QPushButton, QLabel, QMainWindow, QProgressBar)

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import make_transient
from simsapa.app.lookup import LANG_CODE_TO_NAME

from simsapa.app.types import AppData

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db_helpers import get_db_engine_connection_session
from simsapa.app.helpers import ReleaseEntry, ReleasesInfo, get_app_version, get_latest_app_compatible_assets_release, get_release_channel, get_releases_info

from simsapa import SIMSAPA_RELEASES_BASE_URL, DbSchemaName, logger, INDEX_DIR, ASSETS_DIR, COURSES_DIR, APP_DB_PATH, USER_DB_PATH

# Keys with underscore prefix will not be shown in table columns.
LangModelColToIdx = {
    "Code": 0,
    "Language": 1,
}

class LangModel(QAbstractTableModel):
    def __init__(self, data = []):
        super().__init__()
        self._data = data
        self._columns = list(filter(lambda x: not x.startswith("_"), LangModelColToIdx.keys()))

    def data(self, index: QModelIndex, role: Qt.ItemDataRole):
        if role == Qt.ItemDataRole.DisplayRole:
            if len(self._data) == 0:
                return list(map(lambda _: "", self._columns))
            else:
                return self._data[index.row()][index.column()]
        elif role == Qt.ItemDataRole.UserRole:
            return self._data

    def rowCount(self, _):
        return len(self._data)

    def columnCount(self, _):
        if len(self._data) == 0:
            return 0
        else:
            return len(self._columns)

    def headerData(self, section, orientation, role):
       if role == Qt.ItemDataRole.DisplayRole:
            if orientation == Qt.Orientation.Horizontal:
                return self._columns[section]

            if orientation == Qt.Orientation.Vertical:
                return str(section+1)


class AssetManagement(QMainWindow):
    _app_data: AppData

    releases_info: Optional[ReleasesInfo] = None
    compat_release: Optional[ReleaseEntry] = None
    thread_pool: QThreadPool

    add_languages_title_text = "Download Languages"
    include_appdata_downloads: bool = False
    removable_suttas_lang: List[str] = []

    _central_widget: QWidget
    _layout: QVBoxLayout
    _msg: QLabel
    add_languages_input: QLineEdit
    _progress_bar: QProgressBar

    _quit_action: Optional[Callable] = None
    _run_download_pre_hook: Optional[Callable] = None
    _run_download_post_hook: Optional[Callable] = None

    def _init_workers(self):
        self.thread_pool = QThreadPool()
        self.releases_worker = ReleasesWorker()
        self.asset_worker = AssetsWorker()

        self.releases_worker.signals.finished.connect(partial(self._releases_finished))

        def _show_error_retry(msg: str):
            msg += "<p>Retry?</p>"
            ret = self._show_error(msg, QMessageBox.StandardButton.Ok | QMessageBox.StandardButton.Cancel)
            if ret == QMessageBox.StandardButton.Ok:
                self.releases_worker = ReleasesWorker()
                self.releases_worker.signals.error_msg.connect(partial(_show_error_retry))
                self.thread_pool.start(self.releases_worker)

        self.releases_worker.signals.error_msg.connect(partial(_show_error_retry))

    def closeEvent(self, event: QCloseEvent):
        self._handle_quit()
        event.accept()

    def _setup_ui(self):
        raise NotImplementedError

    def _setup_animation(self):
        self._animation = QLabel(self)
        self._animation.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self._movie = QMovie(':simsapa-loading')
        self._animation.setMovie(self._movie)

        self._layout.addWidget(self._animation)

    def _setup_selection(self):
        raise NotImplementedError

    def _releases_finished(self, info: ReleasesInfo):
        self.releases_info = info
        self.compat_release = get_latest_app_compatible_assets_release(self.releases_info)
        self._setup_selection()

    def _setup_progress_bar_frame(self):
        frame = QFrame()
        frame.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        frame.setLineWidth(0)

        layout = QVBoxLayout()
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        frame.setLayout(layout)

        self._progress_bar = QProgressBar()
        layout.addWidget(self._progress_bar)

        button_layout = QHBoxLayout()
        self._progress_cancel_button = QPushButton("Cancel")
        self._progress_cancel_button.setFixedSize(100, 30)
        button_layout.addWidget(self._progress_cancel_button)

        layout.addLayout(button_layout)

        self._progress_cancel_button.clicked.connect(partial(self._handle_cancel_download))

        self.progress_bar_frame = frame
        self.progress_bar_frame.hide()
        self._layout.addWidget(self.progress_bar_frame)

    def _handle_cancel_download(self):
        self._stop_download()

        self._progress_cancel_button.setText("Quit")
        self._progress_cancel_button.clicked.connect(partial(self._handle_quit))

    def _setup_add_languages_frame(self):
        frame = QFrame()
        frame.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        frame.setFrameShadow(QtWidgets.QFrame.Shadow.Raised)
        frame.setLineWidth(0)

        self.add_languages_frame = frame
        self._layout.addWidget(self.add_languages_frame)

        self.add_languages_select_layout = QVBoxLayout()
        self.add_languages_frame.setLayout(self.add_languages_select_layout)

        self.add_languages_title = QLabel(f"<h3>{self.add_languages_title_text}</h3>")
        self.add_languages_select_layout.addWidget(self.add_languages_title)

        self.add_languages_msg = QLabel("<p>Type in the short codes of sutta languages to download, or * to include all.</p>")
        self.add_languages_msg.setWordWrap(True)
        self.add_languages_select_layout.addWidget(self.add_languages_msg)

        self.add_languages_input = QLineEdit()
        self.add_languages_input.setPlaceholderText("E.g.: it, fr, pt, th")
        self.add_languages_select_layout.addWidget(self.add_languages_input)

        self.add_languages_select_layout.addWidget(QLabel("<p>Available languages:</p>"))

        self.add_languages_table = QTableView()
        self.add_languages_table.setShowGrid(False)
        self.add_languages_table.setWordWrap(False)
        self.add_languages_table.verticalHeader().setVisible(False)

        self.add_languages_table.setSelectionMode(QAbstractItemView.SelectionMode.NoSelection)
        self.add_languages_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.add_languages_table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
        self.add_languages_table.horizontalHeader().setStretchLastSection(True)

        self.add_languages_select_layout.addWidget(self.add_languages_table)

        assert(self.releases_info is not None)
        assert(self.compat_release is not None)

        def _to_item(code: str) -> List[str]:
            if code in LANG_CODE_TO_NAME.keys():
                return [code, LANG_CODE_TO_NAME[code]]
            else:
                return [code, ""]

        items = list(map(_to_item, self.compat_release['suttas_lang']))

        self.lang_model = LangModel(items)
        self.add_languages_table.setModel(self.lang_model)

    def _setup_remove_languages_frame(self):
        frame = QFrame()
        frame.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        frame.setFrameShadow(QtWidgets.QFrame.Shadow.Raised)
        frame.setLineWidth(0)

        self.remove_languages_frame = frame
        self._layout.addWidget(self.remove_languages_frame)

        self.remove_languages_select_layout = QVBoxLayout()
        self.remove_languages_frame.setLayout(self.remove_languages_select_layout)

        self.remove_languages_title = QLabel("<h3>Remove Languages</h3>")
        self.remove_languages_select_layout.addWidget(self.remove_languages_title)

        self.remove_languages_msg = QLabel("<p>Type in the short codes of sutta languages to remove, or * to include all.</p>")
        self.remove_languages_msg.setWordWrap(True)
        self.remove_languages_select_layout.addWidget(self.remove_languages_msg)

        self.remove_languages_input = QLineEdit()
        self.remove_languages_input.setPlaceholderText("E.g.: it, fr, pt, th")
        self.remove_languages_select_layout.addWidget(self.remove_languages_input)

        self.remove_languages_select_layout.addWidget(QLabel("<p>Languages in database:</p>"))

        self.remove_languages_table = QTableView()
        self.remove_languages_table.setShowGrid(False)
        self.remove_languages_table.setWordWrap(False)
        self.remove_languages_table.verticalHeader().setVisible(False)

        self.remove_languages_table.setSelectionMode(QAbstractItemView.SelectionMode.NoSelection)
        self.remove_languages_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.remove_languages_table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
        self.remove_languages_table.horizontalHeader().setStretchLastSection(True)

        self.remove_languages_select_layout.addWidget(self.remove_languages_table)

        # Languages in index
        self.removable_suttas_lang = []
        for p in INDEX_DIR.glob('_suttas_lang_*.toc'):
            lang = re.sub(r'.*_suttas_lang_([^_]+)_.*', r'\1', p.name)
            if lang != "" and lang not in ['en', 'pli', 'san']:
                self.removable_suttas_lang.append(lang)

        def _to_item(code: str) -> List[str]:
            if code in LANG_CODE_TO_NAME.keys():
                return [code, LANG_CODE_TO_NAME[code]]
            else:
                return [code, ""]

        items = list(map(_to_item, self.removable_suttas_lang))

        self.lang_model = LangModel(items)
        self.remove_languages_table.setModel(self.lang_model)

    def _validate_and_run_download(self):
        # Check that all entered language codes are available.

        assert(self.compat_release is not None)

        s = self.add_languages_input.text().lower().strip()
        if s != "" and s != "*":
            s = s.replace(',', ' ')
            s = re.sub(r'  +', ' ', s)
            selected_langs = s.split(' ')

            for lang in selected_langs:
                if lang in ['en', 'pli', 'san']:
                    continue
                if lang not in self.compat_release["suttas_lang"]:
                    QMessageBox.warning(self,
                                        "Warning",
                                        f"<p>This language is not available:</p><p>{lang}</p>",
                                        QMessageBox.StandardButton.Ok)
                    return

        self._run_download()

    def _is_additional_urls_selected(self) -> bool:
        return False

    def _run_download(self):
        # Retreive released asset versions from github.
        # Filter for app-compatible db versions, major and minor version number must agree.

        self._msg.setText("")

        self.asset_worker.signals.msg_update.connect(partial(self._msg_update))

        self.asset_worker.signals.error_msg.connect(partial(self._show_error_no_ret))

        self.asset_worker.signals.finished.connect(partial(self._download_finished))
        self.asset_worker.signals.cancelled.connect(partial(self._download_cancelled))
        self.asset_worker.signals.failed.connect(partial(self._download_failed))

        self.asset_worker.signals.set_current_progress.connect(partial(self._progress_bar.setValue))
        self.asset_worker.signals.set_total_progress.connect(partial(self._progress_bar.setMaximum))
        self.asset_worker.signals.download_max.connect(partial(self._download_max))

        if self.releases_info is None:
            logger.error("releases_info is None")
            self.asset_worker.signals.failed.emit()
            return

        try:
            requests.head(SIMSAPA_RELEASES_BASE_URL, timeout=5)
        except Exception as e:
            msg = "No connection, cannot download database: %s" % e
            QMessageBox.warning(self,
                                "No Connection",
                                msg,
                                QMessageBox.StandardButton.Ok)
            logger.error(msg)
            self.asset_worker.signals.failed.emit()
            return

        try:
            if self.compat_release is None:
                asset_versions = list(map(lambda x: x['version_tag'], self.releases_info['assets']['releases']))
                msg = f"""
                <p>Cannot find a compatible asset release.<br>
                Release channel: {get_release_channel()}<br>
                Application version: {get_app_version()}<br>
                Available asset versions: {", ".join(asset_versions)}</p>
                """.strip()

                raise(Exception(msg))

        except Exception as e:
            msg = "<p>Download failed:</p><p>%s</p>" % e
            logger.error(msg)
            QMessageBox.warning(self,
                                "Error",
                                msg,
                                QMessageBox.StandardButton.Ok)
            self.asset_worker.signals.failed.emit()
            return

        if self._run_download_pre_hook:
            self._run_download_pre_hook()

        version = self.compat_release["version_tag"]
        github_repo = self.compat_release["github_repo"]

        # ensure 'v' prefix
        if version[0] != 'v':
            version = 'v' + version

        urls = []

        if self.include_appdata_downloads:
            appdata_tar_url  = f"https://github.com/{github_repo}/releases/download/{version}/appdata.tar.bz2"
            userdata_tar_url = f"https://github.com/{github_repo}/releases/download/{version}/userdata.tar.bz2"
            index_tar_url    = f"https://github.com/{github_repo}/releases/download/{version}/index.tar.bz2"

            sanskrit_appdata_tar_url  = f"https://github.com/{github_repo}/releases/download/{version}/sanskrit-appdata.tar.bz2"
            sanskrit_index_tar_url    = f"https://github.com/{github_repo}/releases/download/{version}/sanskrit-index.tar.bz2"

            if not self._is_additional_urls_selected():
                # Default: General bundle
                urls.extend([
                    appdata_tar_url,
                    index_tar_url,
                ])

            else:
                urls.extend([
                    sanskrit_appdata_tar_url,
                    sanskrit_index_tar_url,
                ])

            # Userdata must come before the languages, so that it is extracted and
            # ready to import the sutta languages into.
            if not USER_DB_PATH.exists():
                urls.append(userdata_tar_url)

        # Languages
        s = self.add_languages_input.text().lower().strip()

        selected_langs = []

        if s != "" and s != "*":
            s = s.replace(',', ' ')
            s = re.sub(r'  +', ' ', s)
            selected_langs = list(filter(lambda x: x not in ['en', 'pli', 'san'], s.split(' ')))

        elif s == "*":
            selected_langs = self.compat_release["suttas_lang"]

        for lang in selected_langs:
            s = f"https://github.com/{github_repo}/releases/download/{version}/suttas_lang_{lang}.tar.bz2"
            urls.append(s)

        self.asset_worker.urls = urls

        logger.info("Show progress bar")
        self.progress_bar_frame.show()

        logger.info("Start download worker")
        self.thread_pool.start(self.asset_worker)

        self.add_languages_frame.hide()

        self.start_animation()

        if self._run_download_post_hook:
            self._run_download_post_hook()

    def _validate_and_run_remove(self):
        # Check that all entered language codes are removable.

        selected_langs = []

        s = self.remove_languages_input.text().lower().strip()
        if s != "" and s != "*":
            s = s.replace(',', ' ')
            s = re.sub(r'  +', ' ', s)
            selected_langs = s.split(' ')

            for lang in selected_langs:
                if lang in ['en', 'pli', 'san']:
                    QMessageBox.warning(self,
                                        "Warning",
                                        f"<p>This language is part of the read-only application database and cannot be removed:</p><p>{lang}</p>",
                                        QMessageBox.StandardButton.Ok)
                    return

                if lang not in self.removable_suttas_lang:
                    QMessageBox.warning(self,
                                        "Warning",
                                        f"<p>This language is not found on the system:</p><p>{lang}</p>",
                                        QMessageBox.StandardButton.Ok)
                    return

        if s == "*":
            selected_langs = self.removable_suttas_lang

        self._run_remove_languages(selected_langs)

    def _run_remove_languages(self, remove_languages: List[str]):
        # Remove languagae indexes on next startup. Windows doesn't allow
        # removing the index files while it thinkgs the application is using
        # them.

        s = ",".join(remove_languages)
        p = ASSETS_DIR.joinpath('indexes_to_remove.txt')
        with open(p, 'w', encoding='utf-8') as f:
            f.write(s)

        # Remove the selected languages from the userdata db.
        for lang in remove_languages:
            suttas = self._app_data.db_session.query(Um.Sutta).filter(Um.Sutta.language == lang).all()
            for i in suttas:
                self._app_data.db_session.delete(i)

        self._app_data.db_session.commit()

        self._language_remove_finished()

    def _language_remove_finished(self):
        self.add_languages_frame.hide()
        self.remove_languages_frame.hide()

        self._animation.deleteLater()
        self._progress_bar.deleteLater()

        self.progress_bar_frame.show()

        self._progress_cancel_button.setText("Quit")
        self._progress_cancel_button.clicked.connect(partial(self._handle_quit))

        self._msg.setText("<p>Removal completed.</p><p>Quit and start the application again.</p>")

    def _setup_add_buttons(self):
        add_buttons_layout = QHBoxLayout()
        add_buttons_layout.setContentsMargins(0, 20, 0, 10)

        self._download_button = QPushButton("Download")
        self._download_button.setFixedSize(100, 30)

        self._download_button.clicked.connect(partial(self._validate_and_run_download))

        add_buttons_layout.addWidget(self._download_button)

        self.add_languages_select_layout.addLayout(add_buttons_layout)

    def _setup_remove_buttons(self):
        remove_buttons_layout = QHBoxLayout()
        remove_buttons_layout.setContentsMargins(0, 20, 0, 10)

        self._remove_button = QPushButton("Remove")
        self._remove_button.setFixedSize(100, 30)

        self._remove_button.clicked.connect(partial(self._validate_and_run_remove))

        remove_buttons_layout.addWidget(self._remove_button)

        self.remove_languages_select_layout.addLayout(remove_buttons_layout)

    def start_animation(self):
        self._msg.setText("Downloading ...")
        self._download_button.setEnabled(False)
        self._movie.start()

    def stop_animation(self):
        self._movie.stop()

    def _download_max(self):
        self._progress_bar.setValue(self._progress_bar.maximum())

    def _cancelled_cleanup_files(self):
        raise NotImplementedError

    def _download_finished(self):
        self.stop_animation()
        self._animation.deleteLater()

        self._progress_cancel_button.setText("Quit")
        self._progress_cancel_button.clicked.connect(partial(self._handle_quit))

        self._msg.setText("<p>Download completed.</p><p>Quit and start the application again.</p>")

    def _download_cancelled(self):
        self.stop_animation()
        self._animation.deleteLater()
        self._cancelled_cleanup_files()

    def _download_failed(self):
        self.stop_animation()
        self._animation.deleteLater()
        self._progress_bar.deleteLater()

        self._msg.setText("<p>Download failed.</p><p>Quit and try again, or report the issue here:</p><p><a href='https://github.com/simsapa/simsapa/issues'>https://github.com/simsapa/simsapa/issues</a></p>")

    def _msg_update(self, msg: str):
        self._msg.setText(msg)

    def _show_error_no_ret(self, msg: str):
        self._show_error(msg)

    def _show_error(self, msg: str, buttons = QMessageBox.StandardButton.Ok):
        box = QMessageBox(self)
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle("Download Error")
        box.setText(msg)
        box.setStandardButtons(buttons)
        return box.exec()

    def _stop_download(self):
        if not self.asset_worker.download_started.isSet():
            # Download hasn't started yet.
            return

        self._msg.setText("<p>Setup cancelled, waiting for subprocesses to end ...</p>")
        self.asset_worker.download_stop.set()
        # Don't .close() here, wait for _download_cancelled() to do it.
        #
        # Otherwise, triggering .close() while the extraction is not finished will cause a threading error.
        #
        # RuntimeError: wrapped C/C++ object of type WorkerSignals has been deleted
        # QObject: Cannot create children for a parent that is in a different thread.
        # (Parent is QApplication(0x4050170), parent's thread is QThread(0x2785280), current thread is QThreadPoolThread(0x51dbd00)
        # fish: Job 1, './run.py' terminated by signal SIGSEGV (Address boundary error)

    def _handle_quit(self):
        self._stop_download()
        if self._quit_action:
            self._quit_action()
        else:
            self.close()

class ReleasesWorkerSignals(QObject):
    finished = pyqtSignal(dict)
    error_msg = pyqtSignal(str)

class ReleasesWorker(QRunnable):
    def __init__(self):
        super(ReleasesWorker, self).__init__()

        self.signals = ReleasesWorkerSignals()

    @pyqtSlot()
    def run(self):
        logger.info("ReleasesWorker::run()")

        try:
            requests.head(SIMSAPA_RELEASES_BASE_URL, timeout=5)
        except Exception as e:
            msg = "<p>Cannot download releases info.</p><p>%s</p>" % e
            logger.error(msg)
            self.signals.error_msg.emit(msg)
            return

        try:
            info = get_releases_info()
            self.signals.finished.emit(info)

        except Exception as e:
            msg = "<p>Cannot download releases info.</p><p>%s</p>" % e
            logger.error(msg)
            self.signals.error_msg.emit(msg)
            return

class AssetsWorkerSignals(QObject):
    finished = pyqtSignal()
    cancelled = pyqtSignal()
    failed = pyqtSignal()
    msg_update = pyqtSignal(str)
    error_msg = pyqtSignal(str)
    set_total_progress = pyqtSignal(int)
    set_current_progress = pyqtSignal(int)
    download_max = pyqtSignal()

class AssetsWorker(QRunnable):
    urls: List[str]
    signals: AssetsWorkerSignals
    download_started: threading.Event
    download_stop: threading.Event

    def __init__(self):
        super(AssetsWorker, self).__init__()

        self.signals = AssetsWorkerSignals()

        self.download_started = threading.Event()
        self.download_stop = threading.Event()

    @pyqtSlot()
    def run(self):
        logger.info("AssetsWorker::run()")
        try:
            self.download_started.set()

            logger.info(f"Download urls: {self.urls}")

            for i in self.urls:
                self.download_extract_tar_bz2(i)
                if self.download_stop.is_set():
                    logger.info("run(): Cancelling")
                    self.signals.msg_update.emit("Download cancelled.")
                    self.signals.cancelled.emit()
                    return

            def _not_core_db(s: str) -> bool:
                p = Path(s)
                return not p.name == 'appdata.sqlite3' and not p.name == 'userdata.sqlite3'

            p = ASSETS_DIR.joinpath("*.sqlite3")
            sqlite_files = list(filter(_not_core_db, glob.glob(f"{p}")))

            # # NOTE: Not currently using this mechanism for anything
            # # If there are any other .sqlite3 files than appdata and userdata, import it to appdata
            # for i in sqlite_files:
            #     self.import_suttas_to_appdata(Path(i))

            for i in sqlite_files:
                Path(i).unlink()

        except Exception as e:
            msg = "%s" % e
            logger.error(msg)
            self.signals.error_msg.emit(msg)
            self.signals.failed.emit()
            return

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

        # Move appdata to assets.
        p = Path(f"{temp_dir}/appdata.sqlite3")
        if p.exists():
            shutil.move(p, APP_DB_PATH)

        # If a userdata db was included in the download, move it to assets.
        # Check not to overwrite user's existing userdata.
        p = Path(f"{temp_dir}/userdata.sqlite3")
        if p.exists() and not USER_DB_PATH.exists():
            shutil.move(p, USER_DB_PATH)

        # If Pali Course assets were included, move them to assets.
        if temp_dir.joinpath("courses").exists():
            for p in glob.glob(f"{temp_dir}/courses/*"):
                shutil.move(p, COURSES_DIR)

        # If a sutta language DB is found, import it to userdata.
        for p in glob.glob(f"{temp_dir}/suttas_lang_*.sqlite3"):
            lang_db_path = Path(p)
            self.import_suttas(lang_db_path, DbSchemaName.UserData)
            lang_db_path.unlink()

        # If indexed segments were included (e.g. with sutta languages), move
        # the segment files to the index folder.
        temp_index = temp_dir.joinpath("index")
        if temp_index.exists():
            if INDEX_DIR.exists():
                for p in glob.glob(f"{temp_index}/*"):
                    shutil.move(p, INDEX_DIR)
            else:
                shutil.move(temp_index, ASSETS_DIR)

        # Remove the temp folder where the assets were extraced to.
        shutil.rmtree(temp_dir)

    def import_suttas(self, import_db_path: Path, schema: DbSchemaName):
        if not import_db_path.exists():
            logger.error(f"Doesn't exist: {import_db_path}")
            return

        try:
            _, _, app_db_session = get_db_engine_connection_session()

            import_db_eng = create_engine("sqlite+pysqlite://", echo=False)
            import_db_conn = import_db_eng.connect()

            if schema == DbSchemaName.AppData:
                import_db_conn.execute(f"ATTACH DATABASE '{import_db_path}' AS appdata;")
            else:
                import_db_conn.execute(f"ATTACH DATABASE '{import_db_path}' AS userdata;")

            Session = sessionmaker(import_db_eng)
            Session.configure(bind=import_db_eng)

            import_db_session = Session()

            if schema == DbSchemaName.AppData:
                res = import_db_session.query(Am.Sutta).all()
            else:
                res = import_db_session.query(Um.Sutta).all()

            logger.info(f"Importing to {schema.value}, {len(res)} suttas from {import_db_path}")

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

            import_db_session.close()
            import_db_conn.close()
            import_db_eng.dispose()

        except Exception as e:
            logger.error(f"Database problem: {e}")
