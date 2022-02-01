import os
from functools import partial
import shutil
from typing import List, Optional
import queue
import json

from PyQt5.QtCore import QSize, QTimer, Qt
from PyQt5.QtWidgets import (QApplication, QMainWindow, QFileDialog, QMessageBox)

from simsapa import APP_DB_PATH, APP_QUEUES, INDEX_DIR, STARTUP_MESSAGE_PATH, TIMER_SPEED
from simsapa.app.helpers import get_update_info
from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface
from .types import AppData, AppMessage

from ..layouts.sutta_search import SuttaSearchWindow
from ..layouts.dictionary_search import DictionarySearchWindow
# from ..layouts.dictionaries_manager import DictionariesManagerWindow
# from ..layouts.document_reader import DocumentReaderWindow
# from ..layouts.library_browser import LibraryBrowserWindow
from ..layouts.memos_browser import MemosBrowserWindow
from ..layouts.links_browser import LinksBrowserWindow

from ..layouts.help_info import open_simsapa_website, show_about


class AppWindows:
    def __init__(self, app: QApplication, app_data: AppData, hotkeys_manager: Optional[HotkeysManagerInterface]):
        self._app = app
        self._app_data = app_data
        self._hotkeys_manager = hotkeys_manager
        self._windows: List[QMainWindow] = []

        self.queue_id = 'app_windows'
        APP_QUEUES[self.queue_id] = queue.Queue()

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

    def handle_messages(self):
        if len(APP_QUEUES) > 0 and self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                msg = json.loads(s)

                if msg['action'] == 'lookup_clipboard_in_suttas':
                    self._lookup_clipboard_in_suttas(msg)

                if msg['action'] == 'lookup_clipboard_in_dictionary':
                    self._lookup_clipboard_in_dictionary(msg)

                if msg['action'] == 'lookup_in_suttas':
                    self._lookup_clipboard_in_suttas(msg)

                elif msg['action'] == 'lookup_in_dictionary':
                    self._lookup_clipboard_in_dictionary(msg)

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _lookup_clipboard_in_suttas(self, msg):
        # Is there a sutta window to handle the message?
        for w in self._windows:
            if isinstance(w, SuttaSearchWindow) and w.isVisible():
                return

        view = self._new_sutta_search_window()

        if self._hotkeys_manager:
            data = json.dumps(msg)
            APP_QUEUES[view.queue_id].put_nowait(data)
            view.handle_messages()

    def _lookup_clipboard_in_dictionary(self, msg):
        # Is there a dictionary window to handle the message?
        for w in self._windows:
            if isinstance(w, DictionarySearchWindow) and w.isVisible():
                return

        view = self._new_dictionary_search_window()

        if self._hotkeys_manager:
            data = json.dumps(msg)
            APP_QUEUES[view.queue_id].put_nowait(data)
            view.handle_messages()

    def _set_size_and_maximize(self, view: QMainWindow):
        view.resize(1200, 800)
        view.setBaseSize(QSize(1200, 800))
        # window doesn't upen maximized
        # view.setWindowFlag(Qt.WindowType.WindowMaximizeButtonHint, True)

    def _new_sutta_search_window(self) -> SuttaSearchWindow:
        view = SuttaSearchWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                print(e)

        view.show()

        if self._app_data.sutta_to_open:
            view._show_sutta(self._app_data.sutta_to_open)
            self._app_data.sutta_to_open = None
        self._windows.append(view)

        return view

    def _new_dictionary_search_window(self) -> DictionarySearchWindow:
        view = DictionarySearchWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                print(e)

        view.show()

        if self._app_data.dict_word_to_open:
            view._show_word(self._app_data.dict_word_to_open)
            self._app_data.dict_word_to_open = None
        self._windows.append(view)

        return view

    # def _new_dictionaries_manager_window(self):
    #     view = DictionariesManagerWindow(self._app_data)
    #     self._set_size_and_maximize(view)
    #     self._connect_signals(view)
    #     view.show()
    #     self._windows.append(view)

    # def _new_library_browser_window(self):
    #     view = LibraryBrowserWindow(self._app_data)
    #     self._set_size_and_maximize(view)
    #     self._connect_signals(view)
    #     view.show()
    #     self._windows.append(view)

    def _new_memos_browser_window(self):
        view = MemosBrowserWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)
        view.show()
        self._windows.append(view)

    def _new_links_browser_window(self):
        view = LinksBrowserWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)
        view.show()
        self._windows.append(view)

    # def _new_document_reader_window(self, file_path=None):
    #     view = DocumentReaderWindow(self._app_data)
    #     self._set_size_and_maximize(view)
    #     self._connect_signals(view)
    #     view.show()

    #     if file_path is not None and file_path is not False and len(file_path) > 0:
    #         view.open_doc(file_path)

    #     self._windows.append(view)

    # def _open_selected_document(self, view: QMainWindow):
    #     doc = view.get_selected_document()
    #     if doc:
    #         self._new_document_reader_window(doc.filepath)

    # def _open_file_dialog(self, view: QMainWindow):
    #     try:
    #         view.open_file_dialog()
    #     except AttributeError:
    #         file_path, _ = QFileDialog.getOpenFileName(
    #             None,
    #             "Open File...",
    #             "",
    #             "PDF or Epub Files (*.pdf *.epub)")

    #         if len(file_path) != 0:
    #             self._new_document_reader_window(file_path)

    def show_startup_message(self, parent = None):
        if not STARTUP_MESSAGE_PATH.exists():
            return

        with open(STARTUP_MESSAGE_PATH, 'r') as f:
            msg: AppMessage = json.loads(f.read())

        os.remove(STARTUP_MESSAGE_PATH)

        if len(msg['text']) == 0:
            return

        box = QMessageBox(parent)
        if msg['kind'] == 'warning':
            box.setIcon(QMessageBox.Warning)
        else:
            box.setIcon(QMessageBox.Information)
        box.setText(msg['text'])
        box.setWindowTitle("Message")
        box.setStandardButtons(QMessageBox.Ok)

        box.exec()

    def show_update_message(self, parent = None):
        if not self._app_data.app_settings.get('notify_about_updates'):
            return

        update_info = get_update_info()
        if update_info is None:
            return

        box = QMessageBox(parent)
        box.setIcon(QMessageBox.Information)
        box.setText(update_info['message'])
        box.setWindowTitle("Update Available")
        box.setStandardButtons(QMessageBox.Ok)

        box.exec()

    def _reindex_database_dialog(self, parent = None):
        msg = """
        <p>Re-indexing the database can take several minutes.</p>
        <p>If you choose <b>Yes</b>, the current index will be removed, and the application will exit.</p>
        <p>When you start the application again, the re-indexing will begin.</p>
        <p>Start now?</p>"""

        reply = QMessageBox.question(parent,
                                     "Re-index the database",
                                     msg,
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)

        if reply == QMessageBox.Yes:
            shutil.rmtree(INDEX_DIR)
            self._quit_app()

    def _redownload_database_dialog(self, parent = None):
        msg = """
        <p>Re-downloading the database can take several minutes.</p>
        <p>If you choose <b>Yes</b>, the database will be removed, and the application will exit.</p>
        <p>When you start the application again, and the download will begin.</p>
        <p>Start now?</p>"""

        reply = QMessageBox.question(parent,
                                     "Re-download the database",
                                     msg,
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)

        if reply == QMessageBox.Yes:
            os.remove(APP_DB_PATH)
            self._quit_app()

    def _close_all_windows(self):
        for w in self._windows:
            w.close()

    def _quit_app(self):
        self._close_all_windows()
        self._app.quit()

    def _set_notify_setting(self, view: QMainWindow):
        notify: bool = view.action_Notify_About_Updates.isChecked()
        self._app_data.app_settings['notify_about_updates'] = notify
        self._app_data._save_app_settings()

    def _connect_signals(self, view: QMainWindow):
        # view.action_Open \
        #     .triggered.connect(partial(self._open_file_dialog, view))

        view.action_Re_index_database \
            .triggered.connect(partial(self._reindex_database_dialog, view))
        view.action_Re_download_database \
            .triggered.connect(partial(self._redownload_database_dialog, view))

        view.action_Quit \
            .triggered.connect(partial(self._quit_app))
        view.action_Sutta_Search \
            .triggered.connect(partial(self._new_sutta_search_window))
        view.action_Dictionary_Search \
            .triggered.connect(partial(self._new_dictionary_search_window))
        view.action_Memos \
            .triggered.connect(partial(self._new_memos_browser_window))
        view.action_Links \
            .triggered.connect(partial(self._new_links_browser_window))

        notify = self._app_data.app_settings['notify_about_updates']
        view.action_Notify_About_Updates.setChecked(notify)
        view.action_Notify_About_Updates \
            .triggered.connect(partial(self._set_notify_setting, view))

        view.action_Website \
            .triggered.connect(partial(open_simsapa_website))

        view.action_About \
            .triggered.connect(partial(show_about, view))

        s = os.getenv('ENABLE_WIP_FEATURES')
        if s is not None and s.lower() == 'true':
            print("no wip features")
            # view.action_Dictionaries_Manager \
            #     .triggered.connect(partial(self._new_dictionaries_manager_window))

            # view.action_Document_Reader \
            #     .triggered.connect(partial(self._new_document_reader_window))
            # view.action_Library \
            #     .triggered.connect(partial(self._new_library_browser_window))

            # try:
            #     view.action_Open_Selected \
            #         .triggered.connect(partial(self._open_selected_document, view))
            # except Exception as e:
            #     print(e)

        else:
            if hasattr(view,'toolBar'):
                view.toolBar.setVisible(False)

            view.action_Open.setVisible(False)
            view.action_Dictionaries_Manager.setVisible(False)
            view.action_Document_Reader.setVisible(False)
            view.action_Library.setVisible(False)
