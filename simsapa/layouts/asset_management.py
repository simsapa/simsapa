import re
import os
import glob
from functools import partial
from pathlib import Path
import shutil
import tarfile
from PyQt6.QtGui import QCloseEvent, QMovie
import threading
from typing import Callable, List, Optional
from PyQt6 import QtWidgets

from PyQt6.QtCore import Qt, QAbstractTableModel, QModelIndex, QRunnable, QThreadPool, pyqtSlot, QObject, pyqtSignal
from PyQt6.QtWidgets import (QAbstractItemView, QFrame, QHeaderView, QLineEdit, QMessageBox, QTableView, QWidget, QVBoxLayout, QHBoxLayout, QPushButton, QLabel, QMainWindow, QProgressBar)

from sqlalchemy import create_engine
from sqlalchemy.sql import text
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import make_transient

from simsapa import SIMSAPA_RELEASES_BASE_URL, DbSchemaName, logger, ASSETS_DIR, APP_DB_PATH, USER_DB_PATH

from simsapa.app.lookup import LANG_CODE_TO_NAME
from simsapa.app.app_data import AppData

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db_session import get_db_session_with_schema

from simsapa.layouts.gui_helpers import ReleaseEntry, ReleasesInfo, get_app_version, get_latest_app_compatible_assets_release, get_release_channel, get_releases_info

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

    def _init_workers(self,
                      assets_dir = ASSETS_DIR,
                      select_sanskrit_bundle = False,
                      add_languages: List[str] = [],
                      auto_start_download = False):

        self.assets_dir = assets_dir
        self.index_dir = assets_dir.joinpath('index')
        self.courses_dir = assets_dir.joinpath('courses')
        self.html_resources_dir = assets_dir.joinpath('html_resources')

        self.init_select_sanskrit_bundle = select_sanskrit_bundle
        self.init_add_languages = add_languages
        self.auto_start_download = auto_start_download

        self.thread_pool = QThreadPool()
        self.releases_worker = ReleasesWorker()
        self.asset_worker = AssetsWorker(assets_dir = self.assets_dir,
                                         index_dir = self.index_dir,
                                         courses_dir = self.courses_dir,
                                         html_resources_dir = self.html_resources_dir)

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
        self._handle_close()
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
        # Implemented in the window class, e.g. DownloadAppdataWindow
        raise NotImplementedError

    def _releases_finished(self, info: ReleasesInfo):
        self.releases_info = info
        compat = get_latest_app_compatible_assets_release(self.releases_info)

        # Filter out the already downloaded languages from the list of available
        # ones. Otherwise the user might try to re-download a language and cause
        # an error. To re-download, the language should be removed first.
        if compat is not None:
            suttas_index_path = self.index_dir.joinpath('suttas')
            if suttas_index_path.exists():
                already_have_langs = [p.name for p in suttas_index_path.iterdir()]
                compat["suttas_lang"] = [lang for lang in compat["suttas_lang"] if lang not in already_have_langs]

        self.compat_release = compat

        msg: Optional[str] = None

        if self.releases_info is None:
            msg = f"""
            <p>Error: The network request to get the list of databases failed.</p>
            <p>Release channel: {get_release_channel()}</p>
            """

        if self.compat_release is None:
            msg = f"""
            <p>Error: There is no compatible database to download.</p>
            <p>Release channel: {get_release_channel()}</p>
            """

        if msg:
            self._msg.setText(msg)

            self._quit_button = QPushButton("Quit")
            self._quit_button.setFixedSize(100, 30)
            self._quit_button.clicked.connect(partial(self._handle_quit))

            button_layout = QHBoxLayout()
            button_layout.addWidget(self._quit_button)

            self._layout.addLayout(button_layout)

            return

        self._setup_selection()

        if self.auto_start_download:
            self._validate_and_run_download()

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

        self.add_languages_msg = QLabel("<p>Type in the short codes of sutta languages to download, or * to download all.</p>")
        self.add_languages_msg.setWordWrap(True)
        self.add_languages_select_layout.addWidget(self.add_languages_msg)

        self.add_languages_input = QLineEdit()

        if len(self.init_add_languages) > 0:
            self.add_languages_input.setText(", ".join(self.init_add_languages))
        else:
            self.add_languages_input.setPlaceholderText("E.g.: it, fr, pt, th")

        self.add_languages_select_layout.addWidget(self.add_languages_input)

        self.add_languages_select_layout.addWidget(QLabel("<p>Available languages:</p>"))

        self.add_languages_table = QTableView()
        self.add_languages_table.setShowGrid(False)
        self.add_languages_table.setWordWrap(False)
        vert_header = self.add_languages_table.verticalHeader()
        if vert_header is not None:
            vert_header.setVisible(False)

        self.add_languages_table.setSelectionMode(QAbstractItemView.SelectionMode.NoSelection)
        self.add_languages_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)

        horiz_header = self.add_languages_table.horizontalHeader()
        if horiz_header is not None:
            horiz_header.setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
            horiz_header.setStretchLastSection(True)

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

        self.remove_languages_msg = QLabel("<p>Type in the short codes of sutta languages to remove, or * to remove all.</p>")
        self.remove_languages_msg.setWordWrap(True)
        self.remove_languages_select_layout.addWidget(self.remove_languages_msg)

        self.remove_languages_input = QLineEdit()
        self.remove_languages_input.setPlaceholderText("E.g.: it, fr, pt, th")
        self.remove_languages_select_layout.addWidget(self.remove_languages_input)

        self.remove_languages_select_layout.addWidget(QLabel("<p>Languages in database:</p>"))

        self.remove_languages_table = QTableView()
        self.remove_languages_table.setShowGrid(False)
        self.remove_languages_table.setWordWrap(False)
        vert_header = self.remove_languages_table.verticalHeader()
        if vert_header is not None:
            vert_header.setVisible(False)

        self.remove_languages_table.setSelectionMode(QAbstractItemView.SelectionMode.NoSelection)
        self.remove_languages_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        horiz_header = self.remove_languages_table.horizontalHeader()
        if horiz_header is not None:
            horiz_header.setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
            horiz_header.setStretchLastSection(True)

        self.remove_languages_select_layout.addWidget(self.remove_languages_table)

        # Languages in index
        self.removable_suttas_lang = []
        # Folder structure of indexes:
        # assets/index/suttas/{en, pli, hu, it}/meta.json
        for p in self.index_dir.joinpath('suttas').iterdir():
            lang = p.name
            if lang not in ['en', 'pli', 'san']:
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
            import requests
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
        p = self.assets_dir.joinpath('indexes_to_remove.txt')
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

    def _handle_close(self):
        self.close()

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
            import requests
            requests.head(SIMSAPA_RELEASES_BASE_URL, timeout=5)
        except Exception as e:
            msg = "<p>Cannot download releases info.</p><p>%s</p>" % e
            logger.error(msg)
            self.signals.error_msg.emit(msg)
            return

        try:
            info = get_releases_info(save_stats=False)
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

    def __init__(self, assets_dir: Path, index_dir: Path, courses_dir: Path, html_resources_dir: Path):
        super(AssetsWorker, self).__init__()

        self.signals = AssetsWorkerSignals()

        self.assets_dir = assets_dir
        self.index_dir = index_dir
        self.courses_dir = courses_dir
        self.html_resources_dir = html_resources_dir

        self.user_db_path = assets_dir.joinpath("userdata.sqlite3")

        self.download_started = threading.Event()
        self.download_stop = threading.Event()

    @pyqtSlot()
    def run(self):
        logger.info("AssetsWorker::run()")
        try:
            self.download_started.set()

            logger.info(f"Download urls: {self.urls}")

            tar_paths = []

            for i in self.urls:
                try:
                    p = self.download_file(i, self.assets_dir)
                    tar_paths.append(p)

                    if self.download_stop.is_set():
                        logger.info("run(): Cancelling")
                        self.signals.msg_update.emit("Download cancelled.")
                        self.signals.cancelled.emit()
                        return

                except Exception as e:
                    raise e

            extract_temp_dir = self.assets_dir.joinpath('extract_temp')

            for i in tar_paths:
                self.extract_tar_bz2(i, extract_temp_dir)
                if self.download_stop.is_set():
                    logger.info("run(): Cancelling")
                    self.signals.msg_update.emit("Extracting cancelled.")
                    self.signals.cancelled.emit()
                    shutil.rmtree(extract_temp_dir)
                    return

            self.import_assets_from_extract_temp(extract_temp_dir)

            # Remove the temp folder where the assets were extraced to.
            shutil.rmtree(extract_temp_dir)

            # NOTE: Not using this mechanism at the moment. This was an idea to
            # deliver additional .sqlite3 files with appdata.tar.gz, import them
            # and remove the .sqlite3 which are not core to the application.

            # def _not_core_db(s: str) -> bool:
            #     p = Path(s)
            #     return not p.name == 'appdata.sqlite3' \
            #         and not p.name == 'dpd.sqlite3' \
            #         and not p.name == 'userdata.sqlite3'

            # p = self.assets_dir.joinpath("*.sqlite3")
            # sqlite_files = list(filter(_not_core_db, glob.glob(f"{p}")))

            # # NOTE: Not currently using this mechanism for anything
            # # If there are any other .sqlite3 files than appdata and userdata, import it to appdata
            # for i in sqlite_files:
            #     self.import_suttas_to_appdata(Path(i))

            # for i in sqlite_files:
            #     Path(i).unlink()

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
            import requests
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

    def extract_tar_bz2(self, tar_file_path: Path, extract_temp_dir: Path):
        self.signals.msg_update.emit(f"Extracting {tar_file_path.name} ...")

        tar = tarfile.open(tar_file_path, "r:bz2")
        tar.extractall(extract_temp_dir)
        tar.close()

        os.remove(tar_file_path)

    def import_move_appdata(self, extract_temp_dir: Path):
        # Move appdata to assets.
        p = Path(f"{extract_temp_dir}/appdata.sqlite3")
        if p.exists():
            shutil.move(p, self.assets_dir.joinpath('appdata.sqlite3'))

    def import_move_dpd(self, extract_temp_dir: Path):
        # Move dpd to assets.
        p = Path(f"{extract_temp_dir}/dpd.sqlite3")
        if p.exists():
            shutil.move(p, self.assets_dir.joinpath('dpd.sqlite3'))

    def import_move_courses_data(self, extract_temp_dir: Path):
        # If Pali Course assets were included, move them to assets.
        if extract_temp_dir.joinpath("courses").exists():
            for p in glob.glob(f"{extract_temp_dir}/courses/*"):
                shutil.move(p, self.courses_dir)

    def import_move_html_resources_data(self, extract_temp_dir: Path):
        temp_html_resources = extract_temp_dir.joinpath("html_resources")
        if temp_html_resources.exists():
            for i in ['appdata', 'userdata']:

                temp_schema_folder = temp_html_resources.joinpath(i)
                target_schema_folder = self.html_resources_dir.joinpath(i)

                if not temp_schema_folder.exists():
                    continue

                if not target_schema_folder.exists():
                    target_schema_folder.mkdir(parents=True)

                # Expecting the HTML files in their own folder, e.g.
                # html_resources/appdata/sutta-index-khemaratana/index.html
                for p in temp_schema_folder.iterdir():
                    shutil.move(p, target_schema_folder)

    def import_move_index(self, extract_temp_dir: Path):
        # If indexed segments were included (e.g. with sutta languages), move
        # the segment files to the index folder.
        #
        # Folder structure of indexes:
        # assets/index/suttas/{en, pli, hu, it}/meta.json

        temp_index = extract_temp_dir.joinpath("index")
        if temp_index.exists():
            if self.index_dir.exists():

                for i in ['suttas', 'dict_words']:
                    temp_index_sub = temp_index.joinpath(i)
                    index_sub = self.index_dir.joinpath(i)

                    if not temp_index_sub.exists():
                        continue

                    for p in temp_index_sub.iterdir():
                        # If the lang index (en, hu, etc.) folder already
                        # exists, this will fail and report the error to the
                        # user.
                        shutil.move(p, index_sub)

            else:
                # If there is no assets/index/ yet, this is probably first
                # install, move the temp index/ over as it is.
                shutil.move(temp_index, self.assets_dir)

    def import_move_userdata(self, extract_temp_dir: Path):
        # If a userdata db was included in the download, move it to assets.
        # Check not to overwrite user's existing userdata.
        p = Path(f"{extract_temp_dir}/userdata.sqlite3")
        if p.exists() and not self.user_db_path.exists():
            shutil.move(p, self.user_db_path)

    def import_suttas_lang_to_userdata(self, extract_temp_dir: Path):
        # If a sutta language DB is found, import it to userdata.
        for p in glob.glob(f"{extract_temp_dir}/suttas_lang_*.sqlite3"):
            lang_db_path = Path(p)
            self.import_suttas(lang_db_path,
                               DbSchemaName.UserData,
                               target_db_path = self.user_db_path)
            lang_db_path.unlink()

    def import_assets_from_extract_temp(self, extract_temp_dir: Path):
        self.import_move_appdata(extract_temp_dir)
        self.import_move_dpd(extract_temp_dir)
        self.import_move_courses_data(extract_temp_dir)
        self.import_move_html_resources_data(extract_temp_dir)
        self.import_move_index(extract_temp_dir)
        self.import_move_userdata(extract_temp_dir)
        self.import_suttas_lang_to_userdata(extract_temp_dir)

    def import_suttas(self, import_db_path: Path, schema: DbSchemaName, target_db_path: Optional[Path] = None):
        if not import_db_path.exists():
            logger.error(f"Doesn't exist: {import_db_path}")
            return

        try:
            if target_db_path is None:
                if schema == DbSchemaName.AppData:
                    target_db_path = APP_DB_PATH
                elif schema == DbSchemaName.UserData:
                    target_db_path = USER_DB_PATH
                else:
                    raise Exception("Only appdata and userdata schema are allowed.")

            target_db_eng, target_db_conn, target_db_session = get_db_session_with_schema(target_db_path, schema)

            import_db_eng = create_engine("sqlite+pysqlite://", echo=False)
            import_db_conn = import_db_eng.connect()

            if schema == DbSchemaName.AppData:
                import_db_conn.execute(text(f"ATTACH DATABASE '{import_db_path}' AS appdata;"))
            elif schema == DbSchemaName.UserData:
                import_db_conn.execute(text(f"ATTACH DATABASE '{import_db_path}' AS userdata;"))
            else:
                raise Exception("Only appdata and userdata schema are allowed.")

            Session = sessionmaker(import_db_eng)
            Session.configure(bind=import_db_eng)

            import_db_session = Session()

            if schema == DbSchemaName.AppData:
                res = import_db_session.query(Am.Sutta).all()
            elif schema == DbSchemaName.UserData:
                res = import_db_session.query(Um.Sutta).all()
            else:
                raise Exception("Only appdata and userdata schema are allowed.")

            logger.info(f"Importing to {schema.value}, {len(res)} suttas from {import_db_path}")

            # https://stackoverflow.com/questions/28871406/how-to-clone-a-sqlalchemy-object-with-new-primary-key

            for i in res:
                try:
                    import_db_session.expunge(i)
                    make_transient(i)
                    # Necessary to reset id, otherwise will not get a new id for appdata.
                    i.id = None # type: ignore

                    if schema == DbSchemaName.AppData:
                        old_sutta = target_db_session.query(Am.Sutta).filter(Am.Sutta.uid == i.uid).first()
                    elif schema == DbSchemaName.UserData:
                        old_sutta = target_db_session.query(Um.Sutta).filter(Um.Sutta.uid == i.uid).first()
                    else:
                        raise Exception("Only appdata and userdata schema are allowed.")

                    if old_sutta is not None:
                        target_db_session.delete(old_sutta)

                    target_db_session.add(i)
                    target_db_session.commit()
                except Exception as e:
                    logger.error(f"Import problem: {e}")

            target_db_conn.close()
            target_db_session.close()
            target_db_eng.dispose()

            import_db_conn.close()
            import_db_session.close()
            import_db_eng.dispose()

        except Exception as e:
            logger.error(f"Database problem: {e}")
