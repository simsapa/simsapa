from functools import partial
import shutil
from pathlib import Path
from typing import List, Optional
from PyQt6 import QtWidgets

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (QCheckBox, QFrame, QRadioButton, QWidget, QVBoxLayout, QPushButton, QLabel)

from simsapa import ASSETS_DIR

from simsapa.layouts.gui_types import QSizeExpanding, QSizeMinimum
from simsapa.layouts.gui_helpers import ReleasesInfo
from simsapa.layouts.asset_management import AssetManagement

class DownloadAppdataWindow(AssetManagement):
    def __init__(self,
                 assets_dir: Path = ASSETS_DIR,
                 releases_info: Optional[ReleasesInfo] = None,
                 select_sanskrit_bundle = False,
                 add_languages: List[str] = [],
                 auto_start_download = False,
                 upgrade_dpd = False) -> None:

        super().__init__()
        self.setWindowTitle("Download Application Assets")
        self.setFixedSize(500, 700)
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)

        self.assets_dir = assets_dir
        self.index_dir = assets_dir.joinpath("index")
        self.app_db_path = assets_dir.joinpath('appdata.sqlite3')
        self.dpd_db_path = assets_dir.joinpath('dpd.sqlite3')
        self.upgrade_dpd = upgrade_dpd

        self.releases_info = releases_info

        self.add_languages_title_text = "Include Languages"
        self.include_appdata_downloads = True

        p = ASSETS_DIR.joinpath("auto_start_download.txt")
        if p.exists():
            self.auto_start_download = True
            p.unlink()
        else:
            self.auto_start_download = auto_start_download

        p = ASSETS_DIR.joinpath("download_select_sanskrit_bundle.txt")
        if p.exists():
            self.init_select_sanskrit_bundle = True
            p.unlink()
        else:
            self.init_select_sanskrit_bundle = select_sanskrit_bundle

        p = ASSETS_DIR.joinpath("download_languages.txt")
        if p.exists():
            with open(p, 'r') as f:
                langs = f.read().strip()
                self.init_add_languages = [i.strip() for i in langs.split(",")]
            p.unlink()
        else:
            self.init_add_languages = add_languages

        p = ASSETS_DIR.joinpath("upgrade_dpd.txt")
        if p.exists():
            self.upgrade_dpd = True
            p.unlink()

        if self.upgrade_dpd:
            self.auto_start_download = True
            self.include_appdata_downloads = False

        self._init_workers(self.assets_dir,
                           self.init_select_sanskrit_bundle,
                           self.init_add_languages,
                           self.auto_start_download,
                           self.upgrade_dpd)
        self._setup_ui()

        def _pre_hook():
            # Remove existing indexes with this. Can't safely clear and remove them
            # in windows._redownload_database_dialog().

            if self.upgrade_dpd:
                self._remove_old_dpd_and_index()
            else:
                self._remove_old_index()

            self.bundles_frame.hide()

        self._run_download_pre_hook = _pre_hook

        if self.releases_info is None:
            self.thread_pool.start(self.releases_worker)
        else:
            self._releases_finished(self.releases_info)

        if self.init_select_sanskrit_bundle:
            self.select_sanskrit_bundle()

    def _setup_ui(self):
        """
        _layout QVBoxLayout
            _animation QLabel
                _movie QMovie
            _msg QLabel
        _close_button
        """

        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._central_widget.setLayout(self._layout)

        # Create it here, add it to a layout later
        self._close_button = QPushButton("Close")
        self._close_button.clicked.connect(partial(self._handle_quit))

        spc1 = QtWidgets.QSpacerItem(20, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spc1)

        self._setup_animation()

        spc2 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spc2)

        self._msg = QLabel("""
        <p>The application database<br>was not found on this system.</p>
        <p>Checking for available sources to download...<p>
        <p></p>
        <p>If you need to remove the database, such as a failed or partial download,<br>read the instructions at:</p>
        <p><a href="https://simsapa.github.io/install/uninstall/">https://simsapa.github.io/install/uninstall/</a></p>
        """)
        self._msg.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._msg.setWordWrap(True)
        self._msg.setOpenExternalLinks(True)
        self._layout.addWidget(self._msg)

        self.spc3 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(self.spc3)

        self.spc4 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(self.spc4)

    def _setup_selection(self):
        self.spc3 = QtWidgets.QSpacerItem(0, 0, QSizeMinimum, QSizeMinimum)
        self.spc4 = QtWidgets.QSpacerItem(0, 0, QSizeMinimum, QSizeMinimum)

        self._msg.setText("""
        <p>The application database<br>was not found on this system.</p>
        <p>Please select the sources to download.<p>
        <p></p>
        <p>If you need to remove the database, such as a failed or partial download,<br>read the instructions at:</p>
        <p><a href="https://simsapa.github.io/install/uninstall/">https://simsapa.github.io/install/uninstall/</a></p>
        """)
        self._msg.setWordWrap(True)
        self._msg.setOpenExternalLinks(True)

        self._setup_progress_bar_frame()

        self._setup_bundles_frame()
        self._setup_add_languages_frame()

        spc3 = QtWidgets.QSpacerItem(10, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spc3)

        spacerItem = QtWidgets.QSpacerItem(20, 0, QSizeMinimum, QSizeExpanding)
        self._layout.addItem(spacerItem)

        self._setup_add_buttons()

    def _setup_bundles_frame(self):
        frame = QFrame()
        frame.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        frame.setFrameShadow(QtWidgets.QFrame.Shadow.Raised)
        frame.setLineWidth(0)

        self.bundles_frame = frame
        self._layout.addWidget(self.bundles_frame)

        self.text_select_layout = QVBoxLayout()
        self.bundles_frame.setLayout(self.text_select_layout)

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

    def select_sanskrit_bundle(self):
        self.sel_additional.setChecked(True)
        self.chk_sanskrit_texts.setChecked(True)
        self._toggled_general_bundle()

    def _is_additional_urls_selected(self) -> bool:
        return (self.sel_additional.isChecked() and self.chk_sanskrit_texts.isChecked())

    def _toggled_general_bundle(self):
        checked = self.sel_general_bundle.isChecked()

        self.general_info.setDisabled(not checked)

        self.chk_sanskrit_texts.setDisabled(checked)
        # self.index_info.setDisabled(checked)

    def _remove_old_index(self):
        if self.index_dir.exists():
            shutil.rmtree(self.index_dir)

        # FIXME When removing the previous index, test if there were user imported
        # data in the userdata database, and if so, tell the user that they have to
        # re-index.

        # msg = AppMessage(
        #     kind = "warning",
        #     text = "<p>The sutta and dictionary database was updated. Re-indexing is necessary for the contents to be searchable.</p><p>You can start a re-index operation with <b>File > Re-index database</b>.</p>",
        # )
        # with open(STARTUP_MESSAGE_PATH, 'w') as f:
        #     f.write(json.dumps(msg))

    def _remove_old_dpd_and_index(self):
        if self.dpd_db_path.exists():
            self.dpd_db_path.unlink()

        p = self.index_dir.joinpath("dict_words").joinpath("en")
        if p.exists():
            shutil.rmtree(p)

    def _cancelled_cleanup_files(self):
        # Don't remove assets dir, it may contain userdata.sqlite3 with user's
        # memos, bookmarks, settings, etc.

        if not self.upgrade_dpd:
            if self.index_dir.exists():
                shutil.rmtree(self.index_dir)

            if self.app_db_path.exists():
                self.app_db_path.unlink()
