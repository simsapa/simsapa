from pathlib import Path
from typing import Optional
from functools import partial

from PyQt6.QtCore import QSize
from PyQt6.QtWidgets import (QComboBox, QFileDialog, QHBoxLayout, QDialog, QLabel, QMessageBox, QPushButton, QSpacerItem, QVBoxLayout)

from simsapa import ASSETS_DIR
from simsapa.app.export_helpers import save_sutta_as_html, save_suttas_as_epub, save_suttas_as_mobi, sutta_content_plain
from simsapa.app.types import ExportFileFormat, ExportFileFormatToEnum, QSizeExpanding, QSizeMinimum, USutta
from simsapa.app.app_data import AppData

class SuttaExportDialog(QDialog):

    def __init__(self, app_data: AppData, sutta: USutta):
        super().__init__()

        self.setWindowTitle("Export Sutta As...")
        self.setFixedSize(300, 200)

        self._app_data = app_data
        self.sutta = sutta

        self._setup_ui()
        self._init_values()
        self._connect_signals()

    def _setup_ui(self):
        self._layout = QVBoxLayout()
        self.setLayout(self._layout)

        title = f"{self.sutta.sutta_ref} {self.sutta.title}"
        self.sutta_title = QLabel(f"<b>{title}</b>")
        self._layout.addWidget(self.sutta_title)

        self._layout.addItem(QSpacerItem(0, 20, QSizeMinimum, QSizeExpanding))

        self._layout.addWidget(QLabel("Select file format:"))

        self.select_format = QComboBox()
        self.select_format.setFixedSize(QSize(80, 35))
        self.select_format.addItems([i.value for i in ExportFileFormat])
        self._layout.addWidget(self.select_format)

        self._layout.addItem(QSpacerItem(0, 20, QSizeMinimum, QSizeExpanding))

        self.buttons_box = QHBoxLayout()
        self._layout.addLayout(self.buttons_box)

        self.export_btn = QPushButton("Export")
        self.export_btn.setFixedSize(QSize(60, 35))
        self.export_btn.setFocus()
        self.buttons_box.addWidget(self.export_btn)

        self.buttons_box.addItem(QSpacerItem(40, 0, QSizeExpanding, QSizeMinimum))

        self.close_btn = QPushButton('Close')
        self.close_btn.setFixedSize(QSize(60, 35))
        self.close_btn.clicked.connect(self._handle_close)
        self.buttons_box.addWidget(self.close_btn)

    def _init_values(self):
        format = self._app_data.app_settings['export_format']
        self.select_format.setCurrentText(format)

    def _do_export(self):
        dir = QFileDialog.getExistingDirectory(self)
        if dir == "":
            return

        path = self._write_file_in_selected_format(Path(dir))
        if not path:
            self._show_warning("<p>Could not write output file.</p>")

        self._show_info("<p>File saved.</p>")

        self.close()

    def _write_file_in_selected_format(self, target_dir: Optional[Path] = None) -> Optional[Path]:
        app_settings = self._app_data.app_settings
        format = app_settings['export_format']

        name = self.sutta.uid.replace('/', '_')
        # html = sanitized_sutta_html_for_export(self.tab_html)

        result_path: Optional[Path] = None
        if target_dir:
            dir = target_dir
        else:
            dir = ASSETS_DIR

        if format == ExportFileFormat.HTML:
            result_path = dir.joinpath(f'{name}.html')
            save_sutta_as_html(self._app_data, result_path, self.sutta)

        elif format == ExportFileFormat.TXT:
            result_path = dir.joinpath(f'{name}.txt')
            txt = sutta_content_plain(self.sutta)
            with open(result_path, 'w') as f:
                f.write(txt)

        elif format == ExportFileFormat.EPUB:
            result_path = dir.joinpath(f'{name}.epub')
            title = f"{self.sutta.sutta_ref} {self.sutta.title}"

            try:
                save_suttas_as_epub(self._app_data,
                                    output_path = result_path,
                                    suttas = [self.sutta],
                                    title = title,
                                    author = str(self.sutta.source_uid),
                                    language = str(self.sutta.language))

            except Exception as e:
                self._show_warning(str(e))
                return

        elif format == ExportFileFormat.MOBI:
            result_path = dir.joinpath(f'{name}.mobi')
            title = f"{self.sutta.sutta_ref} {self.sutta.title}"

            try:
                save_suttas_as_mobi(self._app_data,
                                    output_path = result_path,
                                    suttas = [self.sutta],
                                    title = title,
                                    author = str(self.sutta.source_uid),
                                    language = str(self.sutta.language))

            except Exception as e:
                self._show_warning(str(e))
                return

        return result_path

    def _show_info(self, text: str):
        box = QMessageBox(self)
        box.setIcon(QMessageBox.Icon.Information)
        box.setWindowTitle("Information")
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.setText(text)
        box.exec()

    def _show_warning(self, text: str):
        box = QMessageBox(self)
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle("Warning")
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.setText(text)
        box.exec()

    def _save_settings(self):
        s = self._app_data.app_settings
        format = self.select_format.currentText()
        s['export_format'] = ExportFileFormatToEnum[format]

        self._app_data.app_settings = s
        self._app_data._save_app_settings()

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.select_format.currentIndexChanged.connect(partial(self._save_settings))

        self.export_btn.clicked.connect(partial(self._do_export))
