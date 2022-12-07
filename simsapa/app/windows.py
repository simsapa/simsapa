import os
import sys
import re
from functools import partial
import shutil
from typing import List, Optional
import queue
import json
import webbrowser
from urllib.parse import parse_qs

from PyQt6.QtCore import QObject, QRunnable, QSize, QThreadPool, QTimer, QUrl, Qt, pyqtSignal, pyqtSlot
from PyQt6.QtWidgets import (QApplication, QInputDialog, QMainWindow, QFileDialog, QMessageBox, QWidget)

from simsapa import logger, ApiAction, ApiMessage
from simsapa import APP_DB_PATH, APP_QUEUES, INDEX_DIR, STARTUP_MESSAGE_PATH, TIMER_SPEED
from simsapa.app.helpers import UpdateInfo, get_app_update_info, get_db_update_info, make_active_window, show_work_in_progress
from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface
from simsapa.app.types import AppData, AppMessage, AppWindowInterface, PaliCourseGroup, QueryType, SuttaSearchWindowInterface, WindowNameToType, WindowType

from simsapa.layouts.sutta_search import SuttaSearchWindow
from simsapa.layouts.sutta_study import SuttaStudyWindow
from simsapa.layouts.dictionary_search import DictionarySearchWindow
# from simsapa.layouts.dictionaries_manager import DictionariesManagerWindow
# from simsapa.layouts.document_reader import DocumentReaderWindow
# from simsapa.layouts.library_browser import LibraryBrowserWindow
from simsapa.layouts.bookmarks_browser import BookmarksBrowserWindow
from simsapa.layouts.pali_courses_browser import CoursesBrowserWindow
from simsapa.layouts.pali_course_practice import CoursePracticeWindow
from simsapa.layouts.sutta_window import SuttaWindow
from simsapa.layouts.words_window import WordsWindow
from simsapa.layouts.memos_browser import MemosBrowserWindow
from simsapa.layouts.links_browser import LinksBrowserWindow
from simsapa.layouts.word_scan_popup import WordScanPopup
from simsapa.layouts.help_info import open_simsapa_website, show_about


class AppWindows:
    def __init__(self, app: QApplication, app_data: AppData, hotkeys_manager: Optional[HotkeysManagerInterface]):
        self._app = app
        self._app_data = app_data
        self._hotkeys_manager = hotkeys_manager
        self._windows: List[AppWindowInterface] = []

        self.queue_id = 'app_windows'
        APP_QUEUES[self.queue_id] = queue.Queue()

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self.thread_pool = QThreadPool()

        self.check_updates_worker = CheckUpdatesWorker()
        self.check_updates_worker.signals.have_app_update.connect(partial(self.show_app_update_message))
        self.check_updates_worker.signals.have_db_update.connect(partial(self.show_db_update_message))

        self.word_scan_popup: Optional[WordScanPopup] = None

    def handle_messages(self):
        if len(APP_QUEUES) > 0 and self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                msg: ApiMessage = json.loads(s)
                logger.info("Handle message: %s" % msg)

                if msg['action'] == ApiAction.show_word_scan_popup:
                    self._toggle_word_scan_popup()

                elif msg['action'] == ApiAction.open_sutta_new:
                    self.open_sutta_new(uid = msg['data'])

                elif msg['action'] == ApiAction.open_words_new:
                    schemas_ids = json.loads(msg['data'])
                    self.open_words_new(schemas_ids)

                elif msg['action'] == ApiAction.open_in_study_window:
                    self._show_sutta_by_uid_in_side(msg)

                elif msg['action'] == ApiAction.lookup_clipboard_in_suttas:
                    self._lookup_clipboard_in_suttas(msg)

                elif msg['action'] == ApiAction.lookup_clipboard_in_dictionary:
                    self._lookup_clipboard_in_dictionary(msg)

                elif msg['action'] == ApiAction.lookup_in_suttas:
                    self._lookup_clipboard_in_suttas(msg)

                elif msg['action'] == ApiAction.lookup_in_dictionary:
                    self._lookup_clipboard_in_dictionary(msg)

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def open_sutta_new(self, uid: str):
        view = SuttaWindow(self._app_data, uid)
        self._windows.append(view)
        make_active_window(view)

    def open_words_new(self, schemas_ids: List[tuple[str, int]]):
        view = WordsWindow(self._app_data, schemas_ids)
        self._windows.append(view)
        make_active_window(view)

    def _show_words_by_url(self, url: QUrl) -> bool:
        if url.host() != QueryType.words:
            return False

        query = re.sub(r"^/", "", url.path())
        self._new_dictionary_search_window(query)

        return True

    def _show_sutta_by_url_in_search(self, url: QUrl) -> bool:
        if url.host() != QueryType.suttas:
            return False

        uid = re.sub(r"^/", "", url.path())
        query = parse_qs(url.query())
        quote = None
        if 'q' in query.keys():
            quote = query['q'][0]

        self._show_sutta_by_uid_in_search(uid, quote)

        return True

    def _show_sutta_by_uid_in_search(self, uid: str, highlight_text: Optional[str] = None):
        view = None
        for w in self._windows:
            if isinstance(w, SuttaSearchWindow) and w.isVisible():
                view = w
                break

        if view is None:
            view = self._new_sutta_search_window()

        view.s._show_sutta_by_uid(uid, highlight_text)

    def _show_sutta_by_uid_in_side(self, msg: ApiMessage):
        view = None
        for w in self._windows:
            if isinstance(w, SuttaStudyWindow) and w.isVisible():
                view = w
                break

        if view is None:
            view = self._new_sutta_study_window()

        data = json.dumps(msg)
        APP_QUEUES[view.queue_id].put_nowait(data)
        view.handle_messages()

    def _lookup_clipboard_in_suttas(self, msg: ApiMessage):
        # Is there a sutta window to handle the message?
        view = None
        for w in self._windows:
            if isinstance(w, SuttaSearchWindow) and w.isVisible():
                view = w
                break

        if view is None:
            if len(msg['data']) > 0:
                view = self._new_sutta_search_window(msg['data'])
            else:
                view = self._new_sutta_search_window()

        else:
            data = json.dumps(msg)
            APP_QUEUES[view.queue_id].put_nowait(data)
            view.handle_messages()

    def _lookup_clipboard_in_dictionary(self, msg: ApiMessage):
        # Is there a dictionary window to handle the message?
        view = None
        for w in self._windows:
            if isinstance(w, DictionarySearchWindow) and w.isVisible():
                view = w
                break

        if view is None:
            if len(msg['data']) > 0:
                view = self._new_dictionary_search_window(msg['data'])
            else:
                view = self._new_dictionary_search_window()

        else:
            data = json.dumps(msg)
            APP_QUEUES[view.queue_id].put_nowait(data)
            view.handle_messages()

    def _set_size_and_maximize(self, view: QMainWindow):
        view.resize(1200, 800)
        view.setBaseSize(QSize(1200, 800))
        # window doesn't open maximized
        # view.setWindowFlag(Qt.WindowType.WindowMaximizeButtonHint, True)

    def open_first_window(self) -> QMainWindow:
        window_type = self._app_data.app_settings.get('first_window_on_startup', WindowType.SuttaSearch)

        if window_type == WindowType.SuttaSearch:
            return self._new_sutta_search_window()

        if window_type == WindowType.SuttaStudy:
            return self._new_sutta_study_window()

        elif window_type == WindowType.DictionarySearch:
            return self._new_dictionary_search_window()

        elif window_type == WindowType.Memos:
            return self._new_memos_browser_window()

        elif window_type == WindowType.Links:
            return self._new_links_browser_window()

        else:
            return self._new_sutta_search_window()

    def _new_sutta_search_window(self, query: Optional[str] = None) -> SuttaSearchWindow:
        if query is not None and not isinstance(query, str):
            query = None

        view = SuttaSearchWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)

        def _lookup(query: str):
            msg = ApiMessage(action = ApiAction.lookup_in_dictionary,
                            data = query)
            self._lookup_clipboard_in_dictionary(msg)

        def _study(side: str, uid: str):
            data = {'side': side, 'uid': uid}
            msg = ApiMessage(action = ApiAction.open_in_study_window,
                            data = json.dumps(obj=data))
            self._show_sutta_by_uid_in_side(msg)

        view.lookup_in_dictionary_signal.connect(partial(_lookup))
        view.s.open_in_study_window_signal.connect(partial(_study))
        view.s.sutta_tab.open_sutta_new_signal.connect(partial(self.open_sutta_new))

        view.s.bookmark_created.connect(partial(self._reload_bookmarks))

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                logger.error(e)

        make_active_window(view)

        if self._app_data.sutta_to_open:
            view._show_sutta(self._app_data.sutta_to_open)
            self._app_data.sutta_to_open = None
        elif query is not None:
            view.s._set_query(query)
            view.s._handle_query()

        self._windows.append(view)

        return view

    def _new_sutta_study_window(self) -> SuttaStudyWindow:
        view = SuttaStudyWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)

        def _study(side: str, uid: str):
            data = {'side': side, 'uid': uid}
            msg = ApiMessage(action = ApiAction.open_in_study_window,
                            data = json.dumps(obj=data))
            self._show_sutta_by_uid_in_side(msg)

        view.sutta_one_state.open_in_study_window_signal.connect(partial(_study))
        view.sutta_one_state.sutta_tab.open_sutta_new_signal.connect(partial(self.open_sutta_new))
        view.sutta_two_state.open_in_study_window_signal.connect(partial(_study))
        view.sutta_two_state.sutta_tab.open_sutta_new_signal.connect(partial(self.open_sutta_new))

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                logger.error(e)

        make_active_window(view)

        if self._app_data.sutta_to_open:
            view._show_sutta(self._app_data.sutta_to_open)
            self._app_data.sutta_to_open = None
        self._windows.append(view)

        return view

    def _new_dictionary_search_window(self, query: Optional[str] = None) -> DictionarySearchWindow:
        if query is not None and not isinstance(query, str):
            query = None

        view = DictionarySearchWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)

        def _lookup_in_suttas(query: str):
            msg = ApiMessage(action = ApiAction.lookup_in_suttas,
                            data = query)
            self._lookup_clipboard_in_suttas(msg)

        def _show_url(url: QUrl):
            self._show_sutta_by_url_in_search(url)

        view.show_sutta_by_url.connect(partial(_show_url))

        view.lookup_in_suttas_signal.connect(partial(_lookup_in_suttas))
        view.open_words_new_signal.connect(partial(self.open_words_new))

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                logger.error(e)

        make_active_window(view)

        if self._app_data.dict_word_to_open:
            view._show_word(self._app_data.dict_word_to_open)
            self._app_data.dict_word_to_open = None
        elif query is not None:
            logger.info(f"Set and handle query: " + query)
            view._set_query(query)
            view._handle_query()
            view._handle_exact_query()

        self._windows.append(view)

        return view

    def _toggle_word_scan_popup(self):
        if self.word_scan_popup is None:
            self.word_scan_popup = WordScanPopup(self._app_data)
            self.word_scan_popup.show()
            self.word_scan_popup.activateWindow()

        else:
            self.word_scan_popup.close()
            self.word_scan_popup = None

        is_on = self.word_scan_popup is not None
        for w in self._windows:
            if hasattr(w, 'action_Show_Word_Scan_Popup'):
                w.action_Show_Word_Scan_Popup.setChecked(is_on)

    def _toggle_show_dictionary_sidebar(self, view):
        is_on = view.action_Show_Sidebar.isChecked()
        self._app_data.app_settings['show_dictionary_sidebar'] = is_on
        self._app_data._save_app_settings()

    def _toggle_show_sutta_sidebar(self, view):
        is_on = view.action_Show_Sidebar.isChecked()
        self._app_data.app_settings['show_sutta_sidebar'] = is_on
        self._app_data._save_app_settings()

    def _toggle_show_related_suttas(self, view):
        is_on = view.action_Show_Related_Suttas.isChecked()
        self._app_data.app_settings['show_related_suttas'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if isinstance(w, SuttaSearchWindow) and hasattr(w, 'action_Show_Related_Suttas'):
                w.action_Show_Related_Suttas.setChecked(is_on)

    def _toggle_show_line_by_line(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Show_Translation_and_Pali_Line_by_Line.isChecked()
        self._app_data.app_settings['show_translation_and_pali_line_by_line'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if isinstance(w, SuttaSearchWindow) and hasattr(w, 'action_Show_Translation_and_Pali_Line_by_Line'):
                w.action_Show_Translation_and_Pali_Line_by_Line.setChecked(is_on)
                w.s._get_active_tab().render_sutta_content()

    def _toggle_show_all_variant_readings(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Show_All_Variant_Readings.isChecked()
        self._app_data.app_settings['show_all_variant_readings'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if isinstance(w, SuttaSearchWindow) and hasattr(w, 'action_Show_All_Variant_Readings'):
                w.action_Show_All_Variant_Readings.setChecked(is_on)
                w.s._get_active_tab().render_sutta_content()


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

    def _reload_bookmarks(self):
        view = None
        for w in self._windows:
            if isinstance(w, BookmarksBrowserWindow) and w.isVisible():
                view = w
                break

        if view is not None:
            view.reload_bookmarks()
            view.reload_table()

    def _new_bookmarks_browser_window(self) -> BookmarksBrowserWindow:
        view = BookmarksBrowserWindow(self._app_data)

        def _show_url(url: QUrl):
            self._show_sutta_by_url_in_search(url)

        view.show_sutta_by_url.connect(partial(_show_url))

        make_active_window(view)
        self._windows.append(view)
        return view

    def _new_course_practice_window(self, group: PaliCourseGroup) -> CoursePracticeWindow:
        view = CoursePracticeWindow(self._app_data, group)

        def _show_url(url: QUrl):
            self._show_sutta_by_url_in_search(url)

        view.show_sutta_by_url.connect(partial(_show_url))

        for w in self._windows:
            if isinstance(w, CoursesBrowserWindow):
                view.finished.connect(partial(w._reload_courses_tree))

        make_active_window(view)
        self._windows.append(view)
        return view

    def _start_challenge_group(self, group: PaliCourseGroup):
        self._new_course_practice_window(group)

    def _new_courses_browser_window(self) -> CoursesBrowserWindow:
        view = CoursesBrowserWindow(self._app_data)

        view.start_group.connect(partial(self._start_challenge_group))

        make_active_window(view)
        self._windows.append(view)
        return view

    def _new_memos_browser_window(self) -> MemosBrowserWindow:
        view = MemosBrowserWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)
        make_active_window(view)
        self._windows.append(view)
        return view

    def _new_links_browser_window(self) -> LinksBrowserWindow:
        view = LinksBrowserWindow(self._app_data)
        self._set_size_and_maximize(view)
        self._connect_signals(view)
        make_active_window(view)
        self._windows.append(view)
        return view

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
            box.setIcon(QMessageBox.Icon.Warning)
        else:
            box.setIcon(QMessageBox.Icon.Information)
        box.setText(msg['text'])
        box.setWindowTitle("Message")
        box.setStandardButtons(QMessageBox.StandardButton.Ok)

        box.exec()

    def check_updates(self):
        if self._app_data.app_settings.get('notify_about_updates'):
            self.thread_pool.start(self.check_updates_worker)

    def show_app_update_message(self, update_info: UpdateInfo):
        update_info['message'] += "<h3>Open page in the browser now?</h3>"

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setText(update_info['message'])
        box.setWindowTitle("Application Update Available")
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

        reply = box.exec()
        if reply == QMessageBox.StandardButton.Yes and update_info['visit_url'] is not None:
            webbrowser.open_new(update_info['visit_url'])

    def show_db_update_message(self, update_info: UpdateInfo):
        # Db version must be compatible with app version.
        # Major and minor version must agree, patch version means updated content.
        #
        # On first install, app should download latest compatible db version.
        # On app startup, if obsolete db is found, delete it and show download window.
        #
        # An installed app should filter available db versions.
        # Show db update notification only about compatible versions.
        #
        # App notifications will alert to new app version.
        # When the new app is installed, it will remove old db and download a compatible version.

        update_info['message'] += "<h3>This update is optional, and the download may take a while.</h3>"
        update_info['message'] += "<h3>Download and update now?</h3>"

        # Download update without deleting existing database.
        # When the download is successful, delete old db and replace with new.
        #
        # Remove half-downloaded assets if download is cancelled.
        # Remove half-downloaded assets if found on startup.

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setText(update_info['message'])
        box.setWindowTitle("Database Update Available")
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

        reply = box.exec()

        if reply == QMessageBox.StandardButton.Yes:
            self._redownload_database_dialog()

    def _reindex_database_dialog(self, parent = None):
        show_work_in_progress()

        '''
        msg = """
        <p>Re-indexing the database can take several minutes.</p>
        <p>If you choose <b>Yes</b>, the current index will be removed, and the application will exit.</p>
        <p>When you start the application again, the re-indexing will begin.</p>
        <p>Start now?</p>"""

        reply = QMessageBox.question(parent,
                                     "Re-index the database",
                                     msg,
                                     QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                                     QMessageBox.StandardButton.No)

        if reply == QMessageBox.StandardButton.Yes:
            shutil.rmtree(INDEX_DIR)
            self._quit_app()
        '''

    def _redownload_database_dialog(self, parent = None):
        msg = """
        <p>Re-downloading the database and index can take several minutes.</p>
        <p>If you choose <b>Yes</b>, the database and the index data will be removed, and the application will exit.</p>
        <p>When you start the application again, the download will begin.</p>
        <p>Start now?</p>"""

        if parent is None:
            parent = QWidget()

        reply = QMessageBox.question(parent,
                                     "Re-download the database and index",
                                     msg,
                                     QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                                     QMessageBox.StandardButton.No)

        if reply == QMessageBox.StandardButton.Yes:
            self._app_data.db_session.commit()
            self._app_data.db_session.close_all()
            self._app_data.db_conn.close()
            self._app_data.db_eng.dispose()

            if APP_DB_PATH.exists():
                os.remove(APP_DB_PATH)

            # NOTE: Can't safely clear and remove indexes here. rmtree()
            # triggers an error on Windows about .seg files still being locked.
            # The index will be removed when download_extract_tar_bz2() runs on
            # the next run.

            self._quit_app()

    def _close_all_windows(self):
        for w in self._windows:
            w.close()

    def _quit_app(self):
        self._close_all_windows()
        self._app.quit()

        logger.info("_quit_app() Exiting with status 0.")
        sys.exit(0)

    def _set_notify_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Notify_About_Updates.isChecked()
        self._app_data.app_settings['notify_about_updates'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w,'action_Notify_About_Updates'):
                w.action_Notify_About_Updates.setChecked(checked)

    def _set_show_toolbar_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Show_Toolbar.isChecked()
        self._app_data.app_settings['show_toolbar'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w,'action_Show_Toolbar'):
                w.action_Show_Toolbar.setChecked(checked)

            if hasattr(w,'toolBar'):
                w.toolBar.setVisible(checked)

    def _set_search_as_you_type_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Search_As_You_Type.isChecked()
        self._app_data.app_settings['search_as_you_type'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w,'action_Search_As_You_Type'):
                w.action_Search_As_You_Type.setChecked(checked)

    def _set_search_completion_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Search_Completion.isChecked()
        self._app_data.app_settings['search_completion'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w,'action_Search_Completion'):
                w.action_Search_Completion.setChecked(checked)

    def _first_window_on_startup_dialog(self, view: AppWindowInterface):
        options = WindowNameToType.keys()

        item, ok = QInputDialog.getItem(view,
                                        "Select Window",
                                        "Select First Window to Open on Startup:",
                                        options,
                                        0,
                                        False)
        if ok and item:
            self._app_data.app_settings['first_window_on_startup'] = WindowNameToType[item]
            self._app_data._save_app_settings()


    def _focus_search_input(self, view: AppWindowInterface):
        if hasattr(view, 'search_input'):
            view.search_input.setFocus()
        elif hasattr(view, '_focus_search_input'):
            view._focus_search_input()

    def _connect_signals(self, view: AppWindowInterface):
        # view.action_Open \
        #     .triggered.connect(partial(self._open_file_dialog, view))

        view.action_Re_index_database \
            .triggered.connect(partial(self._reindex_database_dialog, view))
        view.action_Re_download_database \
            .triggered.connect(partial(self._redownload_database_dialog, view))

        if hasattr(view, 'action_Focus_Search_Input'):
            view.action_Focus_Search_Input \
                .triggered.connect(partial(self._focus_search_input, view))

        view.action_Quit \
            .triggered.connect(partial(self._quit_app))
        view.action_Sutta_Search \
            .triggered.connect(partial(self._new_sutta_search_window))
        view.action_Sutta_Study \
            .triggered.connect(partial(self._new_sutta_study_window))
        view.action_Dictionary_Search \
            .triggered.connect(partial(self._new_dictionary_search_window))
        view.action_Memos \
            .triggered.connect(partial(self._new_memos_browser_window))
        view.action_Links \
            .triggered.connect(partial(self._new_links_browser_window))

        if hasattr(view, 'action_Bookmarks'):
            view.action_Bookmarks \
                .triggered.connect(partial(self._new_bookmarks_browser_window))

        if hasattr(view, 'action_Pali_Courses'):
            view.action_Pali_Courses \
                .triggered.connect(partial(self._new_courses_browser_window))

        if isinstance(view, DictionarySearchWindow):
            if hasattr(view, 'action_Show_Sidebar'):
                is_on = self._app_data.app_settings.get('show_dictionary_sidebar', True)
                view.action_Show_Sidebar.setChecked(is_on)

                view.action_Show_Sidebar \
                    .triggered.connect(partial(self._toggle_show_dictionary_sidebar, view))

        if isinstance(view, SuttaSearchWindow) or isinstance(view, SuttaStudyWindow):
            if hasattr(view, 'action_Show_Sidebar'):
                is_on = self._app_data.app_settings.get('show_sutta_sidebar', True)
                view.action_Show_Sidebar.setChecked(is_on)

                view.action_Show_Sidebar \
                    .triggered.connect(partial(self._toggle_show_sutta_sidebar, view))

            if hasattr(view, 'action_Show_Related_Suttas'):
                is_on = self._app_data.app_settings.get('show_related_suttas', True)
                view.action_Show_Related_Suttas.setChecked(is_on)

                view.action_Show_Related_Suttas \
                    .triggered.connect(partial(self._toggle_show_related_suttas, view))

            if hasattr(view, 'action_Show_Translation_and_Pali_Line_by_Line'):
                is_on = self._app_data.app_settings.get('show_translation_and_pali_line_by_line', True)
                view.action_Show_Translation_and_Pali_Line_by_Line.setChecked(is_on)

                view.action_Show_Translation_and_Pali_Line_by_Line \
                    .triggered.connect(partial(self._toggle_show_line_by_line, view))

            if hasattr(view, 'action_Show_All_Variant_Readings'):
                is_on = self._app_data.app_settings.get('show_all_variant_readings', True)
                view.action_Show_All_Variant_Readings.setChecked(is_on)

                view.action_Show_All_Variant_Readings \
                    .triggered.connect(partial(self._toggle_show_all_variant_readings, view))

        if hasattr(view, 'action_Show_Word_Scan_Popup'):
            view.action_Show_Word_Scan_Popup \
                .triggered.connect(partial(self._toggle_word_scan_popup))
            is_on = self.word_scan_popup is not None
            view.action_Show_Word_Scan_Popup.setChecked(is_on)

        view.action_First_Window_on_Startup \
            .triggered.connect(partial(self._first_window_on_startup_dialog, view))

        notify = self._app_data.app_settings.get('notify_about_updates', True)
        view.action_Notify_About_Updates.setChecked(notify)
        view.action_Notify_About_Updates \
            .triggered.connect(partial(self._set_notify_setting, view))

        view.action_Website \
            .triggered.connect(partial(open_simsapa_website))

        view.action_About \
            .triggered.connect(partial(show_about, view))

        show_toolbar = self._app_data.app_settings.get('show_toolbar', True)
        view.action_Show_Toolbar.setChecked(show_toolbar)
        view.action_Show_Toolbar \
            .triggered.connect(partial(self._set_show_toolbar_setting, view))

        if hasattr(view, 'toolBar') and not show_toolbar:
            view.toolBar.setVisible(False)

        if hasattr(view, 'action_Search_As_You_Type'):
            view.action_Search_As_You_Type \
                .triggered.connect(partial(self._set_search_as_you_type_setting, view))

            search_as_you_type = self._app_data.app_settings.get('search_as_you_type', True)
            view.action_Search_As_You_Type.setChecked(search_as_you_type)

        if hasattr(view, 'action_Search_Completion'):
            view.action_Search_Completion \
                .triggered.connect(partial(self._set_search_completion_setting, view))

            search_completion = self._app_data.app_settings.get('search_completion', True)
            view.action_Search_Completion.setChecked(search_completion)

        s = os.getenv('ENABLE_WIP_FEATURES')
        if s is not None and s.lower() == 'true':
            logger.info("no wip features")
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
            #     logger.error(e)

        else:
            view.action_Open.setVisible(False)
            view.action_Dictionaries_Manager.setVisible(False)
            view.action_Document_Reader.setVisible(False)
            view.action_Library.setVisible(False)


class UpdatesWorkerSignals(QObject):
    have_app_update = pyqtSignal(dict)
    have_db_update = pyqtSignal(dict)

class CheckUpdatesWorker(QRunnable):
    signals: UpdatesWorkerSignals

    def __init__(self):
        super().__init__()
        self.signals = UpdatesWorkerSignals()

    @pyqtSlot()
    def run(self):
        try:
            update_info = get_app_update_info()
            if update_info is not None:
                self.signals.have_app_update.emit(update_info)

            update_info = get_db_update_info()
            if update_info is not None:
                self.signals.have_db_update.emit(update_info)

        except Exception as e:
            logger.error(e)
