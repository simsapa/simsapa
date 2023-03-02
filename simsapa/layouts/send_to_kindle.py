from pathlib import Path
from typing import Optional
from functools import partial

from PyQt6 import QtCore, QtWidgets
from PyQt6.QtGui import QMouseEvent
from PyQt6.QtWidgets import (QCheckBox, QComboBox, QFileDialog, QFormLayout, QGroupBox, QHBoxLayout, QLabel, QLineEdit, QMainWindow, QMessageBox, QPushButton, QSpinBox, QTabWidget, QVBoxLayout, QWidget)

from simsapa import ASSETS_DIR, logger
from simsapa.app.export_helpers import save_html_as_epub, save_html_as_mobi, sanitized_sutta_html_for_export, sutta_content_plain
from simsapa.app.simsapa_smtp import SimsapaSMTP

from ..app.types import AppData, KindleAction, KindleFileFormat, KindleFileFormatToEnum, QFixed, QSizeExpanding, QSizeMinimum, SendToKindleSettings, SmtpLoginData, SmtpLoginDataPreset, SmtpServicePreset, SmtpServicePresetToEnum, USutta

class SendToKindleWindow(QMainWindow):

    tabs: QTabWidget

    def __init__(self, app_data: AppData, tab_sutta: Optional[USutta], tab_html: str, parent=None) -> None:
        super().__init__(parent)
        logger.info("SendToKindleWindow()")

        self._app_data: AppData = app_data

        self.tab_sutta = tab_sutta
        self.tab_html = tab_html

        self._setup_ui()
        self._init_values()
        self._connect_signals()

    def _setup_ui(self):
        self.setWindowTitle("Send to Kindle")
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
        self.tabs.addTab(self.send_tab, "Send to Kindle")

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
        self.select_format.addItems([i.value for i in KindleFileFormat])
        self.send_tab_layout.addWidget(self.select_format)

        info = """
        <p>The Kindle Library supports HTML and EPUB. Sending an EPUB format file will be converted to MOBI by Amazon.</p>
        <p>If you have <a href="https://calibre-ebook.com/">Calibre Ebook Reader</a> installed, we can automatically convert the EPUB to MOBI and save it to a Kindle folder (set the path in the Settings tab).</p>
        """

        self.format_info = QLabel(info)
        self.format_info.setMinimumWidth(400)
        self.format_info.setWordWrap(True)
        self.send_tab_layout.addWidget(self.format_info)

        self.send_tab_layout.addItem(QtWidgets.QSpacerItem(0, 40, QSizeMinimum, QSizeMinimum))

        label_1 = QLabel("<p>If the Kindle is connected via USB, select a folder to save the text to.</p>")
        label_1.setWordWrap(True)
        self.send_tab_layout.addWidget(label_1)

        self.save_via_usb = QPushButton(KindleAction.SaveViaUSB.value)
        self.save_via_usb.setSizePolicy(QtWidgets.QSizePolicy(QFixed, QFixed))
        self.save_via_usb.setMinimumSize(QtCore.QSize(180, 40))

        self.send_tab_layout.addWidget(self.save_via_usb)

        self.send_tab_layout.addItem(QtWidgets.QSpacerItem(0, 40, QSizeMinimum, QSizeMinimum))

        self.send_tab_layout.addWidget(QLabel("<p>Send the text to your Kindle Library.</p>"))

        self.send_to_kindle_email = QPushButton(KindleAction.SendToEmail.value)
        self.send_to_kindle_email.setSizePolicy(QtWidgets.QSizePolicy(QFixed, QFixed))
        self.send_to_kindle_email.setMinimumSize(QtCore.QSize(180, 40))

        self.send_tab_layout.addWidget(self.send_to_kindle_email)

        self.send_tab_layout.addItem(QtWidgets.QSpacerItem(0, 0, QSizeMinimum, QSizeExpanding))

    def _setup_settings_tab(self):
        self.settings_tab = QWidget()
        self.tabs.addTab(self.settings_tab, "Settings")

        self.settings_tab_layout = QVBoxLayout(self.settings_tab)
        self.settings_tab_layout.setAlignment(QtCore.Qt.AlignmentFlag.AlignLeft)

        self.settings_tab_layout.addWidget(QLabel("Path to Calibre's ebook-convert tool:"))
        self.path_to_ebook_convert = QLineEdit()
        self.settings_tab_layout.addWidget(self.path_to_ebook_convert)

        self.path_to_ebook_convert_file_btn = QPushButton("Select ebook-convert path ...")
        self.settings_tab_layout.addWidget(self.path_to_ebook_convert_file_btn)

        self.settings_tab_layout.addWidget(QLabel("Your 'Send to Kindle' Email:"))

        self.kindle_email_input = QLineEdit()
        self.settings_tab_layout.addWidget(self.kindle_email_input)

        label_3 = QLabel("<p>(Help page: <a href='https://www.amazon.com/gp/help/customer/display.html?nodeId=GX9XLEVV8G4DB28H'>Add an Email Address</a>)</p>")
        label_3.setWordWrap(True)
        self.settings_tab_layout.addWidget(label_3)

        self.settings_tab_layout.addWidget(QLabel("Your email address to use for sending:"))

        self.sender_email_input = QLineEdit()
        self.settings_tab_layout.addWidget(self.sender_email_input)

        self.clear_smtp_form = QPushButton("Clear SMTP Data")
        self.clear_smtp_form.setFixedSize(QtCore.QSize(125, 30))
        self.settings_tab_layout.addWidget(self.clear_smtp_form)

        self.smtp_form_box = QGroupBox("SMTP server details")
        self.smtp_form_layout = QFormLayout()
        self.smtp_form_box.setLayout(self.smtp_form_layout)

        self.settings_tab_layout.addWidget(self.smtp_form_box)

        # Offers defaults for gmail, fastmail, proton mail
        self.select_smtp_preset = QComboBox()
        self.select_smtp_preset.addItems([i.value for i in SmtpServicePreset])

        self.smtp_host = QLineEdit()

        self.smtp_port_tls = QSpinBox()
        self.smtp_port_tls.setMinimum(0)
        self.smtp_port_tls.setMaximum(999)

        self.smtp_username = QLineEdit()

        self.smtp_password = QLineEdit()
        self.smtp_password.setEchoMode(QLineEdit.EchoMode.Password)

        self.smtp_password_visible = QCheckBox()
        self.smtp_password_visible.setChecked(False)

        info = QLabel("<p>Leave the password empty to ask on each request. If you wish to save the password here, it is recommended to create an app-specific password. (See instructions for <a href='https://support.google.com/accounts/answer/185833?hl=en'>Google Mail</a>.)</p>")
        info.setWordWrap(True)
        self.settings_tab_layout.addWidget(info)

        self.smtp_form_layout.addRow(QLabel("Use defaults for:"), self.select_smtp_preset)
        self.smtp_form_layout.addRow(QLabel("Host:"), self.smtp_host)
        self.smtp_form_layout.addRow(QLabel("Port (TLS):"), self.smtp_port_tls)
        self.smtp_form_layout.addRow(QLabel("Username:"), self.smtp_username)
        self.smtp_form_layout.addRow(QLabel("Password:"), self.smtp_password)
        self.smtp_form_layout.addRow(QLabel("Show password:"), self.smtp_password_visible)

    def _init_values(self):
        app_settings = self._app_data.app_settings
        kindle_settings = self._app_data.app_settings['send_to_kindle']

        self.select_format.setCurrentText(kindle_settings["format"])

        path_to_ebook_convert = app_settings.get('path_to_ebook_convert', None)

        if path_to_ebook_convert:
            self.path_to_ebook_convert.setText(str(path_to_ebook_convert))

        s = kindle_settings['kindle_email']
        if s:
            self.kindle_email_input.setText(s)

        s = app_settings['smtp_sender_email']
        if s:
            self.sender_email_input.setText(s)

        self.select_smtp_preset.setCurrentText(app_settings['smtp_preset'])

        smtp = app_settings['smtp_login_data']
        if smtp:
            self.smtp_host.setText(smtp['host'])
            self.smtp_port_tls.setValue(smtp['port_tls'])
            self.smtp_username.setText(smtp['user'])
            self.smtp_password.setText(smtp['password'])

    def _save_all_settings(self):
        logger.info("_save_all_settings()")
        format = self.select_format.currentText()

        smtp_preset = self.select_smtp_preset.currentText()

        smtp = SmtpLoginData(
            host = self.smtp_host.text(),
            port_tls = self.smtp_port_tls.value(),
            user = self.smtp_username.text(),
            password = self.smtp_password.text(),
        )

        kindle_settings = SendToKindleSettings(
            format = KindleFileFormatToEnum[format],
            kindle_email = self.kindle_email_input.text(),
        )

        app_settings = self._app_data.app_settings
        app_settings['path_to_ebook_convert'] = self.path_to_ebook_convert.text()
        app_settings['smtp_sender_email'] = self.sender_email_input.text()
        app_settings['smtp_preset'] = SmtpServicePresetToEnum[smtp_preset]
        app_settings['smtp_login_data'] = smtp

        app_settings['send_to_kindle'] = kindle_settings
        self._app_data.app_settings = app_settings

        self._app_data._save_app_settings()

        self.status_msg.setText("Changes saved.")

    def _select_ebook_convert_file(self):
        path, _ = QFileDialog.getOpenFileName(self, filter="ebook-convert*")
        if path != "":
            self.path_to_ebook_convert.setText(path)
            # .setText() doesn't trigger textEdited
            self._save_all_settings()

    def mousePressEvent(self, _: QMouseEvent):
        self.status_msg.setText("")

    def _clear_smtp_form(self):
        self.select_smtp_preset.setCurrentIndex(0)
        self.smtp_host.setText("")
        self.smtp_port_tls.setValue(0)
        self.smtp_username.setText("")
        self.smtp_password.setText("")

        self._save_all_settings()

    def _toggle_show_password(self):
        show = (self.smtp_password_visible.checkState() == QtCore.Qt.CheckState.Checked)
        if show:
            self.smtp_password.setEchoMode(QLineEdit.EchoMode.Normal)
        else:
            self.smtp_password.setEchoMode(QLineEdit.EchoMode.Password)

    def _handle_preset_change(self):
        preset_name = SmtpServicePresetToEnum[self.select_smtp_preset.currentText()]
        if preset_name == SmtpServicePreset.NoPreset:
            return

        if preset_name in SmtpLoginDataPreset.keys():
            preset = SmtpLoginDataPreset[preset_name]

            self.smtp_host.setText(preset['host'])
            self.smtp_port_tls.setValue(preset['port_tls'])
            self.smtp_username.setText(preset['user'])
            self.smtp_password.setText("")

            self._save_all_settings()

    def _send_to_kindle_email(self):
        app_settings = self._app_data.app_settings
        kindle_settings = app_settings['send_to_kindle']

        if self.tab_sutta is None:
            self._show_warning("<p>No selected sutta.</p>")
            return

        smtp = app_settings['smtp_login_data']
        if smtp is None:
            self._show_warning("<p>SMTP data is missing</p>")
            return

        sender_email = app_settings['smtp_sender_email']
        if sender_email is None:
            self._show_warning("<p>Sender email is missing</p>")
            return

        kindle_email = kindle_settings['kindle_email']

        if kindle_email is None:
            self._show_warning("<p>Send to Kindle email is missing</p>")
            return

        path = self._write_file_in_selected_format()
        if not path:
            self._show_warning("<p>Could not write output file.</p>")
            return

        sutta_title = self.sutta_title_input.text()

        with SimsapaSMTP(smtp) as server:
            server.send_message(from_addr = sender_email,
                                to_addr = kindle_email,
                                msg = "",
                                subject = "[Simsapa] %s" % sutta_title,
                                attachment_paths = [path])

        path.unlink()

        self._show_info("<p>Email Sent.</p>")

        self.close()

    def _save_via_usb(self):
        if self.tab_sutta is None:
            self._show_warning("<p>No selected sutta.</p>")
            return

        dir = QFileDialog.getExistingDirectory(self)
        if dir == "":
            return

        path = self._write_file_in_selected_format(Path(dir))
        if not path:
            self._show_warning("<p>Could not write output file.</p>")

        self._show_info("<p>File saved.</p>")

        self.close()

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

    def _write_file_in_selected_format(self, target_dir: Optional[Path] = None) -> Optional[Path]:
        if self.tab_sutta is None:
            self._show_warning("<p>No selected sutta.</p>")
            return

        app_settings = self._app_data.app_settings
        kindle_settings = app_settings['send_to_kindle']

        name = self.tab_sutta.uid.replace('/', '_')
        html = sanitized_sutta_html_for_export(self.tab_html)
        format = kindle_settings['format']

        result_path: Optional[Path] = None
        if target_dir:
            dir = target_dir
        else:
            dir = ASSETS_DIR

        if format == KindleFileFormat.HTML:
            result_path = dir.joinpath(f'{name}.html')
            with open(result_path, 'w') as f:
                f.write(html)

        elif format == KindleFileFormat.TXT:
            result_path = dir.joinpath(f'{name}.txt')
            txt = sutta_content_plain(self.tab_sutta)
            with open(result_path, 'w') as f:
                f.write(txt)

        elif format == KindleFileFormat.EPUB:
            result_path = dir.joinpath(f'{name}.epub')
            save_html_as_epub(output_path = result_path,
                              sanitized_html = html,
                              title = self.sutta_title_input.text(),
                              author = str(self.tab_sutta.source_uid),
                              language = str(self.tab_sutta.language))

        elif format == KindleFileFormat.MOBI:
            if not app_settings['path_to_ebook_convert']:
                self._show_warning("<p>The ebook-convert tool from Calibre is required to create MOBI files. Please install Calibre Ebook Reader and set the path to ebook-covert in the settings tab.</p>")
                return

            ebook_convert_path = app_settings['path_to_ebook_convert']

            result_path = dir.joinpath(f'{name}.mobi')

            try:
                save_html_as_mobi(ebook_convert_path = Path(ebook_convert_path),
                                output_path = result_path,
                                sanitized_html = html,
                                title = self.sutta_title_input.text(),
                                author = str(self.tab_sutta.source_uid),
                                language = str(self.tab_sutta.language))

            except Exception as e:
                self._show_warning(str(e))
                return

        return result_path

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.close_button.clicked.connect(partial(self._handle_close))

        self.send_to_kindle_email.clicked.connect(partial(self._send_to_kindle_email))

        self.save_via_usb.clicked.connect(partial(self._save_via_usb))

        self.path_to_ebook_convert_file_btn.clicked.connect(partial(self._select_ebook_convert_file))

        self.clear_smtp_form.clicked.connect(partial(self._clear_smtp_form))

        self.smtp_password_visible.clicked.connect(partial(self._toggle_show_password))

        self.select_smtp_preset.currentIndexChanged.connect(partial(self._handle_preset_change))

        # save values on every change
        for i in [self.select_format, self.select_smtp_preset]:
            i.currentIndexChanged.connect(partial(self._save_all_settings))

        for i in [self.path_to_ebook_convert,
                  self.kindle_email_input,
                  self.sender_email_input,
                  self.smtp_host,
                  self.smtp_username,
                  self.smtp_password]:
            i.textEdited.connect(partial(self._save_all_settings))

        self.smtp_port_tls.valueChanged.connect(partial(self._save_all_settings))
