from pathlib import Path
from typing import Optional
from functools import partial
import subprocess
import mimetypes

from PyQt6 import QtCore, QtWidgets
from PyQt6.QtGui import QMouseEvent
from PyQt6.QtWidgets import (QComboBox, QFileDialog, QHBoxLayout, QLabel, QLineEdit, QMainWindow, QMessageBox, QPushButton, QTabWidget, QVBoxLayout, QWidget)

from simsapa import ASSETS_DIR, logger
from simsapa.app.export_helpers import sanitized_sutta_html_for_export, save_html_as_epub, sutta_content_plain

from ..app.types import AppData, RemarkableAction, RemarkableFileFormat, RemarkableFileFormatToEnum, QFixed, QSizeExpanding, QSizeMinimum, SendToRemarkableSettings, USutta

class SendToRemarkableWindow(QMainWindow):

    def __init__(self, app_data: AppData, tab_sutta: Optional[USutta], tab_html: str, parent=None) -> None:
        super().__init__(parent)
        logger.info("SendToRemarkableWindow()")

        self._app_data: AppData = app_data

        self.tab_sutta = tab_sutta
        self.tab_html = tab_html

        self._setup_ui()
        self._init_values()
        self._connect_signals()

    def _setup_ui(self):
        self.setWindowTitle("Send to reMarkable")
        self.setFixedSize(450, 700)

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        self.tabs = QTabWidget(self)
        self._layout.addWidget(self.tabs)

        self._setup_send_tab()
        self._setup_settings_tab()

        self.status_msg = QLabel()
        self._layout.addWidget(self.status_msg)

        # Buttons box

        self.bottom_buttons_box = QHBoxLayout()
        self.bottom_buttons_box.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        self._layout.addLayout(self.bottom_buttons_box)

        self.close_button = QPushButton("Close")
        self.close_button.setSizePolicy(QtWidgets.QSizePolicy(QFixed, QFixed))
        self.close_button.setMinimumSize(QtCore.QSize(80, 40))

        self.bottom_buttons_box.addWidget(self.close_button)

    def _setup_send_tab(self):
        self.send_tab = QWidget()
        self.tabs.addTab(self.send_tab, "Send to reMarkable")

        self.send_tab_layout = QVBoxLayout(self.send_tab)
        self.send_tab_layout.setAlignment(QtCore.Qt.AlignmentFlag.AlignLeft)
        self.send_tab_layout.addWidget(QLabel("Selected Sutta Title:"))

        self.sutta_title_input = QLineEdit()

        if self.tab_sutta:
            self.sutta_title_input.setText(f'{self.tab_sutta.sutta_ref} {self.tab_sutta.title}')
            self.send_tab_layout.addWidget(self.sutta_title_input)
        else:
            self.send_tab_layout.addWidget(QLabel(f'<b>No selected sutta</b>'))

        self.send_tab_layout.addItem(QtWidgets.QSpacerItem(0, 40, QSizeMinimum, QSizeMinimum))

        self.send_tab_layout.addWidget(QLabel("Select file format:"))

        self.select_format = QComboBox()
        self.select_format.setFixedSize(QtCore.QSize(80, 40))
        self.select_format.addItems([i.value for i in RemarkableFileFormat])
        self.send_tab_layout.addWidget(self.select_format)

        self.send_tab_layout.addItem(QtWidgets.QSpacerItem(0, 40, QSizeMinimum, QSizeMinimum))

        label_1 = QLabel("<p>If the reMarkable is connected via USB and the Web Interface is enabled, we can send files over with curl.</p>")
        label_1.setWordWrap(True)
        label_1.setMinimumWidth(400)
        self.send_tab_layout.addWidget(label_1)

        self.save_with_curl = QPushButton(RemarkableAction.SaveWithCurl.value)
        self.save_with_curl.setSizePolicy(QtWidgets.QSizePolicy(QFixed, QFixed))
        self.save_with_curl.setMinimumSize(QtCore.QSize(180, 40))

        self.send_tab_layout.addWidget(self.save_with_curl)

        self.send_tab_layout.addItem(QtWidgets.QSpacerItem(0, 40, QSizeMinimum, QSizeMinimum))

        label_2 = QLabel("<p>If the reMarkable is connected via USB and ssh is configured, we can send files over with scp.</p>")
        label_2.setWordWrap(True)
        self.send_tab_layout.addWidget(label_2)

        self.save_with_scp = QPushButton(RemarkableAction.SaveWithScp.value)
        self.save_with_scp.setSizePolicy(QtWidgets.QSizePolicy(QFixed, QFixed))
        self.save_with_scp.setMinimumSize(QtCore.QSize(180, 40))

        self.send_tab_layout.addWidget(self.save_with_scp)

        self.send_tab_layout.addItem(QtWidgets.QSpacerItem(0, 0, QSizeMinimum, QSizeExpanding))

    def _setup_settings_tab(self):
        self.settings_tab = QWidget()
        self.tabs.addTab(self.settings_tab, "Settings")

        self.settings_tab_layout = QVBoxLayout(self.settings_tab)
        self.settings_tab_layout.setAlignment(QtCore.Qt.AlignmentFlag.AlignLeft)

        # ebook-convert

        self.settings_tab_layout.addWidget(QLabel("Path to Calibre's ebook-convert tool:"))
        self.path_to_ebook_convert = QLineEdit()
        self.settings_tab_layout.addWidget(self.path_to_ebook_convert)

        self.path_to_ebook_convert_file_btn = QPushButton("Select ebook-convert path ...")
        self.settings_tab_layout.addWidget(self.path_to_ebook_convert_file_btn)

        # curl

        self.settings_tab_layout.addWidget(QLabel("Path to curl:"))
        self.path_to_curl = QLineEdit()
        self.settings_tab_layout.addWidget(self.path_to_curl)

        self.path_to_curl_file_btn = QPushButton("Select curl path ...")
        self.settings_tab_layout.addWidget(self.path_to_curl_file_btn)

        # scp

        self.settings_tab_layout.addWidget(QLabel("Path to scp:"))
        self.path_to_scp = QLineEdit()
        self.settings_tab_layout.addWidget(self.path_to_scp)

        self.path_to_scp_file_btn = QPushButton("Select scp path ...")
        self.settings_tab_layout.addWidget(self.path_to_scp_file_btn)

        # ssh pubkey

        self.settings_tab_layout.addWidget(QLabel("Path to ssh public key:"))
        self.path_to_pubkey = QLineEdit()
        self.settings_tab_layout.addWidget(self.path_to_pubkey)

        self.path_to_pubkey_btn = QPushButton("Select public key path ...")
        self.settings_tab_layout.addWidget(self.path_to_pubkey_btn)

        # scp target folder

        self.settings_tab_layout.addWidget(QLabel("Folder path on the reMarkable to scp to:"))
        self.rmk_folder_to_scp = QLineEdit()
        self.settings_tab_layout.addWidget(self.rmk_folder_to_scp)

        # SSH IP

        self.settings_tab_layout.addWidget(QLabel("reMarkable SSH IP:"))
        self.rmk_ssh_ip = QLineEdit()
        self.settings_tab_layout.addWidget(self.rmk_ssh_ip)

        # Web IP

        self.settings_tab_layout.addWidget(QLabel("reMarkable Web Interface IP:"))
        self.rmk_web_ip = QLineEdit()
        self.settings_tab_layout.addWidget(self.rmk_web_ip)

    def _init_values(self):
        app_settings = self._app_data.app_settings
        rmk_settings = self._app_data.app_settings['send_to_remarkable']

        self.select_format.setCurrentText(rmk_settings["format"])

        path_to_ebook_convert = app_settings.get('path_to_ebook_convert', None)
        if path_to_ebook_convert:
            self.path_to_ebook_convert.setText(str(path_to_ebook_convert))

        path_to_curl = app_settings.get('path_to_curl', None)
        if path_to_curl:
            self.path_to_curl.setText(str(path_to_curl))

        path_to_scp = app_settings.get('path_to_scp', None)
        if path_to_scp:
            self.path_to_scp.setText(str(path_to_scp))

        path_to_pubkey = rmk_settings['user_ssh_pubkey_path']
        if path_to_pubkey:
            self.path_to_pubkey.setText(str(path_to_pubkey))

        rmk_folder_to_scp = rmk_settings['rmk_folder_to_scp']
        if rmk_folder_to_scp:
            self.rmk_folder_to_scp.setText(str(rmk_folder_to_scp))

        rmk_ssh_ip = rmk_settings['rmk_ssh_ip']
        if rmk_ssh_ip:
            self.rmk_ssh_ip.setText(str(rmk_ssh_ip))

        rmk_web_ip = rmk_settings['rmk_web_ip']
        if rmk_web_ip:
            self.rmk_web_ip.setText(str(rmk_web_ip))

    def _save_all_settings(self):
        logger.info("_save_all_settings()")
        format = self.select_format.currentText()

        rmk_settings = SendToRemarkableSettings(
            format = RemarkableFileFormatToEnum[format],
            rmk_web_ip = self.rmk_web_ip.text(),
            rmk_ssh_ip = self.rmk_ssh_ip.text(),
            rmk_folder_to_scp = self.rmk_folder_to_scp.text(),
            user_ssh_pubkey_path = self.path_to_pubkey.text(),
        )

        app_settings = self._app_data.app_settings
        app_settings['path_to_ebook_convert'] = self.path_to_ebook_convert.text()

        app_settings['send_to_remarkable'] = rmk_settings
        self._app_data.app_settings = app_settings

        self._app_data._save_app_settings()

        self.status_msg.setText("Changes saved.")

    def _select_ebook_convert_file(self):
        path, _ = QFileDialog.getOpenFileName(self, filter="ebook-convert*")
        if path != "":
            self.path_to_ebook_convert.setText(path)
            # .setText() doesn't trigger textEdited
            self._save_all_settings()

    def _select_curl_file(self):
        path, _ = QFileDialog.getOpenFileName(self, filter="curl")
        if path != "":
            self.path_to_curl.setText(path)
            # .setText() doesn't trigger textEdited
            self._save_all_settings()

    def _select_scp_file(self):
        path, _ = QFileDialog.getOpenFileName(self, filter="scp")
        if path != "":
            self.path_to_scp.setText(path)
            self._save_all_settings()

    def _select_pubkey(self):
        path, _ = QFileDialog.getOpenFileName(self, filter="*.pub")
        if path != "":
            self.path_to_pubkey.setText(path)
            self._save_all_settings()

    def mousePressEvent(self, _: QMouseEvent):
        self.status_msg.setText("")

    def _save_with_curl(self):
        path = self._write_file_in_selected_format()
        if not path:
            self._show_warning("<p>Could not write output file.</p>")
            return

        curl_path = self._app_data.app_settings.get('path_to_curl', None)
        if not curl_path:
            self._show_warning("<p>Path to curl is not set.</p>")
            return

        # Web interface is at http://10.11.99.1/
        # https://remarkablewiki.com/tech/webinterface
        # https://web.archive.org/web/20230203130927/https://remarkablewiki.com/tech/webinterface
        # curl 'http://10.11.99.1/upload' -H 'Origin: http://10.11.99.1' -H 'Accept: */*' -H 'Referer: http://10.11.99.1/' -H 'Connection: keep-alive' -F "file=@Get_started_with_reMarkable.pdf;filename=Get_started_with_reMarkable.pdf;type=application/pdf"

        rmk_settings = self._app_data.app_settings['send_to_remarkable']
        ip = rmk_settings['rmk_web_ip']
        mime = mimetypes.guess_type(path.name)[0]

        res = subprocess.run([curl_path,
                              f'http://{ip}/upload',
                              '-H', f'Origin: http://{ip}',
                              '-H', 'Accept: */*',
                              '-H', f'Referer: http://{ip}/',
                              '-H', 'Connection: keep-alive',
                              '-F', f'file=@{path};filename={path.name};type={mime}'],
                             capture_output=True)

        path.unlink()

        if res.returncode != 0:
            self._show_warning(f"<p>curl returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")
            return

        self._show_info("<p>File saved.</p>")

        self.close()

    def _save_with_scp(self):
        path = self._write_file_in_selected_format()
        if not path:
            self._show_warning("<p>Could not write output file.</p>")
            return

        scp_path = self._app_data.app_settings.get('path_to_scp', None)
        if not scp_path:
            self._show_warning("<p>Path to scp is not set.</p>")
            return

        rmk_settings = self._app_data.app_settings['send_to_remarkable']

        # 10.11.99.2
        # 10.42.0.71
        # 192.168.1.187

        rmk_ssh_ip = rmk_settings['rmk_ssh_ip']
        rmk_folder = Path(rmk_settings['rmk_folder_to_scp'])
        pubkey_path = rmk_settings['user_ssh_pubkey_path']

        res = subprocess.run([scp_path,
                              '-i',
                              pubkey_path,
                              '-o', 'PasswordAuthentication=no',
                              '-o', 'PubkeyAuthentication=yes',
                              '-o', 'PreferredAuthentications=publickey',
                              path,
                              f'root@{rmk_ssh_ip}:{rmk_folder}'],
                             capture_output=True)

        print(res.args)

        path.unlink()

        if res.returncode != 0:
            self._show_warning(f"<p>scp returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")
            return

        self._show_info("<p>File saved.</p>")

        self.close()

    def _write_file_in_selected_format(self, target_dir: Optional[Path] = None) -> Optional[Path]:
        if self.tab_sutta is None:
            self._show_warning("<p>No selected sutta.</p>")
            return

        app_settings = self._app_data.app_settings
        rmk_settings = app_settings['send_to_remarkable']

        name = self.tab_sutta.uid.replace('/', '_')
        html = sanitized_sutta_html_for_export(self.tab_html)
        format = rmk_settings['format']

        result_path: Optional[Path] = None
        if target_dir:
            dir = target_dir
        else:
            dir = ASSETS_DIR

        if format == RemarkableFileFormat.HTML:
            result_path = dir.joinpath(f'{name}.html')
            with open(result_path, 'w') as f:
                f.write(html)

        elif format == RemarkableFileFormat.TXT:
            result_path = dir.joinpath(f'{name}.txt')
            txt = sutta_content_plain(self.tab_sutta)
            with open(result_path, 'w') as f:
                f.write(txt)

        elif format == RemarkableFileFormat.EPUB:
            result_path = dir.joinpath(f'{name}.epub')
            save_html_as_epub(output_path = result_path,
                              sanitized_html = html,
                              title = self.sutta_title_input.text(),
                              author = str(self.tab_sutta.source_uid),
                              language = str(self.tab_sutta.language))

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

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.close_button.clicked.connect(partial(self._handle_close))

        self.save_with_curl.clicked.connect(partial(self._save_with_curl))
        self.save_with_scp.clicked.connect(partial(self._save_with_scp))

        self.path_to_ebook_convert_file_btn.clicked.connect(partial(self._select_ebook_convert_file))
        self.path_to_curl_file_btn.clicked.connect(partial(self._select_curl_file))
        self.path_to_scp_file_btn.clicked.connect(partial(self._select_scp_file))
        self.path_to_pubkey_btn.clicked.connect(partial(self._select_pubkey))

        # save values on every change

        self.select_format.currentIndexChanged.connect(partial(self._save_all_settings))

        for i in [self.path_to_ebook_convert,
                  self.path_to_curl,
                  self.path_to_scp,
                  self.path_to_pubkey,
                  self.rmk_folder_to_scp,
                  self.rmk_ssh_ip,
                  self.rmk_web_ip]:
            i.textEdited.connect(partial(self._save_all_settings))

