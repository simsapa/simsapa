import os, sys, re, shutil, queue, json, webbrowser
from functools import partial
from typing import List, Optional
from datetime import datetime
from urllib.parse import parse_qs

from PyQt6.QtCore import QThreadPool, QTimer, QUrl
from PyQt6.QtWidgets import (QApplication, QInputDialog, QMainWindow, QMessageBox, QWidget)

from simsapa import ASSETS_DIR, EBOOK_UNZIP_DIR, logger, ApiAction, ApiMessage
from simsapa import SERVER_QUEUE, APP_DB_PATH, APP_QUEUES, STARTUP_MESSAGE_PATH, TIMER_SPEED

from simsapa.app.helpers import ReleasesInfo, UpdateInfo, make_active_window, show_work_in_progress
from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface
from simsapa.app.search.queries import SearchQueries
from simsapa.app.check_updates_worker import CheckUpdatesWorker
from simsapa.app.completion_cache_worker import CompletionCacheWorker

from simsapa.app.types import (AppMessage, AppWindowInterface, BookmarksBrowserWindowInterface,
                               DictionarySearchWindowInterface, CompletionCacheResult, EbookReaderWindowInterface, OpenPromptParams, PaliCourseGroup,
                               QueryType, SuttaQuote, QuoteScope, QuoteScopeValues,
                               SuttaSearchWindowInterface, SuttaStudyWindowInterface, WindowNameToType, WindowType, WordScanPopupInterface,
                               sutta_quote_from_url)

from simsapa.app.app_data import AppData

from simsapa.layouts.help_info import open_simsapa_website, show_about
from simsapa.layouts.preview_window import PreviewWindow

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

class AppWindows:
    _preview_window: PreviewWindow

    def __init__(self, app: QApplication, app_data: AppData, hotkeys_manager: Optional[HotkeysManagerInterface]):
        self._app = app
        self._app_data = app_data
        self._queries = SearchQueries(self._app_data.db_session,
                                      self._app_data.get_search_indexes,
                                      self._app_data.api_url)
        self._hotkeys_manager = hotkeys_manager
        self._windows: List[AppWindowInterface] = []
        self._windowed_previews: List[PreviewWindow] = []
        self._sutta_index_window: Optional[AppWindowInterface] = None

        self.word_scan_popup: Optional[WordScanPopupInterface] = None

        # Init PreviewWindow here, so that the first window can connect signals to it.
        self._init_preview_window()

        self.queue_id = 'app_windows'
        APP_QUEUES[self.queue_id] = queue.Queue()

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self.thread_pool = QThreadPool()

        self.completion_cache_worker = CompletionCacheWorker()
        self.completion_cache_worker.signals.finished.connect(partial(self._set_completion_cache))
        self.thread_pool.start(self.completion_cache_worker)

        # Wait 0.5s, then run slowish initialize tasks, e.g. init windows, check for updates.
        # By that time the first window will be opened and will not delay app.exec().
        self.init_timer = QTimer()
        self.init_timer.setSingleShot(True)
        self.init_timer.timeout.connect(partial(self._init_tasks))
        self.init_timer.start(500)

    def _init_tasks(self):
        logger.profile("AppWindows::_init_tasks(): start")

        self._init_word_scan_popup()
        self._init_sutta_index_window()
        self._init_check_updates()
        self._check_updates()

        self.import_user_data_from_assets()

        logger.profile("AppWindows::_init_tasks(): end")

    def handle_messages(self):
        try:
            s = SERVER_QUEUE.get_nowait()
            msg: ApiMessage = json.loads(s)
            if len(APP_QUEUES) > 0:
                if 'queue_id' in msg.keys():
                    queue_id = msg['queue_id']
                else:
                    queue_id = 'all'

                if queue_id in APP_QUEUES.keys():
                    APP_QUEUES[queue_id].put_nowait(s)

        except queue.Empty:
            pass

        if len(APP_QUEUES) > 0 and self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                msg: ApiMessage = json.loads(s)
                logger.info("Handle message: %s" % msg)

                if msg['action'] == ApiAction.show_word_scan_popup:
                    self._toggle_word_scan_popup()

                if msg['action'] == ApiAction.closed_word_scan_popup:
                    self._closed_word_scan_popup()

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
        from simsapa.layouts.sutta_window import SuttaWindow
        view = SuttaWindow(self._app_data, uid)
        self._finalize_view(view)

    def open_words_new(self, schemas_ids: List[tuple[str, int]]):
        from simsapa.layouts.words_window import WordsWindow
        view = WordsWindow(self._app_data, schemas_ids)
        self._finalize_view(view)

    def _new_windowed_preview(self):
        if self._preview_window._hover_data is None:
            return

        view = PreviewWindow(self._app_data,
                             hover_data = self._preview_window._hover_data,
                             frameless = False)
        self._windowed_previews.append(view)

        view.open_new.connect(partial(self._new_sutta_from_preview))

        if view.render_hover_data():
            view._do_show(check_settings=False)

    def _new_sutta_from_preview(self, href: Optional[str] = None):
        if href is None and self._preview_window._hover_data is not None:
            url = QUrl(self._preview_window._hover_data['href'])

        elif href is not None:
            url = QUrl(href)

        else:
            return

        sutta = self._preview_window.sutta_queries.get_sutta_by_url(url)

        if sutta:
            self._new_sutta_search_window(f"uid:{sutta.uid}")

    def _show_words_by_url(self,
                           url: QUrl,
                           show_results_tab = True,
                           include_exact_query = True) -> bool:
        if url.host() != QueryType.words:
            return False

        self._preview_window._do_hide()

        view = None
        for w in self._windows:
            if isinstance(w, DictionarySearchWindowInterface) and w.isVisible():
                view = w
                break

        query = re.sub(r"^/", "", url.path())

        if view is None:
            self._new_dictionary_search_window(query)
        else:
            view._show_word_by_url(url, show_results_tab, include_exact_query)

        return True

    def _show_words_url_noret(self, url: QUrl):
        self._show_words_by_url(url)

    def _show_sutta_url_noret(self, url: QUrl):
        self._show_sutta_by_url_in_search(url)

    def _show_sutta_by_url_in_search(self, url: QUrl) -> bool:
        if url.host() != QueryType.suttas:
            return False

        self._preview_window._do_hide()

        uid = re.sub(r"^/", "", url.path())
        query = parse_qs(url.query())

        quote_scope = QuoteScope.Sutta
        if 'quote_scope' in query.keys():
            sc = query['quote_scope'][0]
            if sc in QuoteScopeValues.keys():
                quote_scope = QuoteScopeValues[sc]

        self._show_sutta_by_uid_in_search(uid, sutta_quote_from_url(url), quote_scope)

        return True

    def _show_sutta_by_uid_in_search(self,
                                     uid: str,
                                     sutta_quote: Optional[SuttaQuote] = None,
                                     quote_scope = QuoteScope.Sutta):

        view = None
        for w in self._windows:
            if isinstance(w, SuttaSearchWindowInterface) and w.isVisible():
                view = w
                break

        if view is None:
            view = self._new_sutta_search_window()

        view.s._show_sutta_by_uid(uid, sutta_quote, quote_scope)

    def _show_sutta_by_uid_in_side(self, msg: ApiMessage):
        view = None
        for w in self._windows:
            if isinstance(w, SuttaStudyWindowInterface) and w.isVisible():
                view = w
                break

        self._preview_window._do_hide()

        if view is None:
            view = self._new_sutta_study_window()

        data = json.dumps(msg)

        if msg['queue_id'] == 'all':
            queue_id = view.queue_id
        else:
            queue_id = msg['queue_id']

        APP_QUEUES[queue_id].put_nowait(data)
        view.handle_messages()

    def _lookup_clipboard_in_suttas(self, msg: ApiMessage):
        # Is there a sutta window to handle the message?
        view = None
        for w in self._windows:
            if isinstance(w, SuttaSearchWindowInterface) and w.isVisible():
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
            if isinstance(w, DictionarySearchWindowInterface) and w.isVisible():
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

    def _lookup_msg(self, query: str):
        msg = ApiMessage(queue_id = 'all',
                         action = ApiAction.lookup_in_dictionary,
                         data = query)
        self._lookup_clipboard_in_dictionary(msg)

    def _study_msg_to_all(self, queue_id: str, side: str, uid: str):
        data = {'queue_id': queue_id, 'side': side, 'uid': uid}
        msg = ApiMessage(queue_id = 'all',
                         action = ApiAction.open_in_study_window,
                         data = json.dumps(obj=data))
        self._show_sutta_by_uid_in_side(msg)

    def _new_sutta_search_window_noret(self, query: Optional[str] = None) -> None:
        self._new_sutta_search_window(query)

    def _new_sutta_search_window(self, query: Optional[str] = None) -> SuttaSearchWindowInterface:
        from simsapa.layouts.sutta_search import SuttaSearchWindow

        if query is not None and not isinstance(query, str):
            query = None

        view = SuttaSearchWindow(self._app_data)

        view.lookup_in_dictionary_signal.connect(partial(self._lookup_msg))
        view.s.open_in_study_window_signal.connect(partial(self._study_msg_to_all))
        view.s.open_sutta_new_signal.connect(partial(self.open_sutta_new))

        view.s.bookmark_created.connect(partial(self._reload_bookmarks))

        view.s.bookmark_created.connect(partial(view.s.reload_page))
        view.s.bookmark_updated.connect(partial(view.s.reload_page))

        view.connect_preview_window_signals(self._preview_window)

        view.s.page_dblclick.connect(partial(self._sutta_search_quick_lookup_selection, view = view))

        view.s.open_gpt_prompt.connect(partial(self._new_gpt_prompts_window_noret))

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                logger.error(e)

        if self._app_data.sutta_to_open:
            view._show_sutta(self._app_data.sutta_to_open)
            self._app_data.sutta_to_open = None
        elif query is not None:
            view.s._set_query(query)
            view.s._handle_query()

        return self._finalize_view(view)

    def _new_sutta_study_window_noret(self) -> None:
        self._new_sutta_study_window()

    def _new_sutta_study_window(self) -> SuttaStudyWindowInterface:
        from simsapa.layouts.sutta_study import SuttaStudyWindow
        view = SuttaStudyWindow(self._app_data)

        def _study(queue_id: str, side: str, uid: str):
            data = {'side': side, 'uid': uid}
            msg = ApiMessage(queue_id = queue_id,
                             action = ApiAction.open_in_study_window,
                             data = json.dumps(obj=data))
            self._show_sutta_by_uid_in_side(msg)

        view.sutta_one_state.open_in_study_window_signal.connect(partial(_study))
        view.sutta_one_state.open_sutta_new_signal.connect(partial(self.open_sutta_new))

        view.sutta_two_state.open_in_study_window_signal.connect(partial(_study))
        view.sutta_two_state.open_sutta_new_signal.connect(partial(self.open_sutta_new))

        view.dictionary_state.show_sutta_by_url.connect(partial(self._show_sutta_url_noret))

        view.connect_preview_window_signals(self._preview_window)

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                logger.error(e)

        if self._app_data.sutta_to_open:
            view._show_sutta(self._app_data.sutta_to_open)
            self._app_data.sutta_to_open = None

        return self._finalize_view(view)

    def _init_preview_window(self):
        logger.profile("AppWindows::_init_preview_window()")

        self._preview_window = PreviewWindow(self._app_data)

        def _words(url: QUrl):
            self._show_words_by_url(url, show_results_tab=True, include_exact_query=False)

        self._preview_window.open_new.connect(partial(self._new_sutta_from_preview))
        self._preview_window.make_windowed.connect(partial(self._new_windowed_preview))
        self._preview_window.show_words_by_url.connect(partial(_words))

    def _init_sutta_index_window(self):
        logger.profile("AppWindows::_init_sutta_index_window()")
        from simsapa.layouts.sutta_index import SuttaIndexWindow
        if self._sutta_index_window is not None:
            return

        self._sutta_index_window = SuttaIndexWindow(self._app_data)

        self._sutta_index_window.show_sutta_by_url.connect(partial(self._show_sutta_url_noret))
        self._sutta_index_window.connect_preview_window_signals(self._preview_window)

        self._connect_signals_to_view(self._sutta_index_window)
        self._windows.append(self._sutta_index_window)

    def _show_sutta_index_window(self):
        if self._sutta_index_window is None:
            self._init_sutta_index_window()
        else:
            make_active_window(self._sutta_index_window)

    def _new_dictionary_search_window_noret(self, query: Optional[str] = None) -> None:
        self._new_dictionary_search_window(query)

    def _lookup_in_suttas_msg(self, query: str):
        msg = ApiMessage(queue_id = 'all',
                         action = ApiAction.lookup_in_suttas,
                         data = query)
        self._lookup_clipboard_in_suttas(msg)

    def _new_dictionary_search_window(self, query: Optional[str] = None) -> DictionarySearchWindowInterface:
        from simsapa.layouts.dictionary_search import DictionarySearchWindow
        if query is not None and not isinstance(query, str):
            query = None

        view = DictionarySearchWindow(self._app_data)

        view.show_sutta_by_url.connect(partial(self._show_sutta_url_noret))
        view.show_words_by_url.connect(partial(self._show_words_url_noret))

        view.lookup_in_new_sutta_window_signal.connect(partial(self._lookup_in_suttas_msg))
        view.open_words_new_signal.connect(partial(self.open_words_new))
        view.page_dblclick.connect(partial(view._lookup_selection_in_dictionary, show_results_tab=True, include_exact_query=False))

        view.connect_preview_window_signals(self._preview_window)

        if self._hotkeys_manager is not None:
            try:
                self._hotkeys_manager.setup_window(view)
            except Exception as e:
                logger.error(e)

        if self._app_data.dict_word_to_open:
            view._show_word(self._app_data.dict_word_to_open)
            self._app_data.dict_word_to_open = None
        elif query is not None:
            logger.info("Set and handle query: " + query)
            view._set_query(query)
            view._handle_query()
            view._handle_exact_query()

        return self._finalize_view(view)

    def _init_word_scan_popup(self):
        if self.word_scan_popup is not None:
            return

        logger.profile("AppWindows::_init_word_scan_popup()")
        from simsapa.layouts.word_scan_popup import WordScanPopup

        self.word_scan_popup = WordScanPopup(self._app_data)

        def _show_sutta_url(url: QUrl):
            self._show_sutta_by_url_in_search(url)

        def _show_words_url(url: QUrl):
            if self.word_scan_popup:
                self.word_scan_popup.s._show_word_by_url(url)

        self.word_scan_popup.s.show_sutta_by_url.connect(partial(_show_sutta_url))
        self.word_scan_popup.s.show_words_by_url.connect(partial(_show_words_url))

        self.word_scan_popup.s.connect_preview_window_signals(self._preview_window)

    def _toggle_word_scan_popup(self):
        if self.word_scan_popup is None:
            self._init_word_scan_popup()

        assert(self.word_scan_popup is not None)

        if self.word_scan_popup.isVisible():
            self.word_scan_popup.hide()

        else:
            self.word_scan_popup.show()
            self.word_scan_popup.activateWindow()

        self._set_all_show_word_scan_checked()

    def _set_all_show_word_scan_checked(self):
        if self.word_scan_popup is None:
            is_on = False
        else:
            is_on = self.word_scan_popup.isVisible()

        for w in self._windows:
            if hasattr(w, 'action_Show_Word_Scan_Popup'):
                w.action_Show_Word_Scan_Popup.setChecked(is_on)

    def _closed_word_scan_popup(self):
        if self.word_scan_popup is not None:
            self.word_scan_popup.close()
            self.word_scan_popup = None

        self._set_all_show_word_scan_checked()

    def _sutta_search_quick_lookup_selection(self, view: SuttaSearchWindowInterface):
        query = view.s._get_selection()
        self._show_word_scan_popup(query = query, show_results_tab = True, include_exact_query = False)

    def _show_word_scan_popup(self, query: Optional[str] = None, show_results_tab = True, include_exact_query = False):
        if not self._app_data.app_settings['double_click_dict_lookup']:
            return

        if self.word_scan_popup is None:
            self._init_word_scan_popup()

        else:
            self.word_scan_popup.show()
            self.word_scan_popup.activateWindow()

            if query is not None:
                self.word_scan_popup.s.lookup_in_dictionary(query, show_results_tab, include_exact_query)

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
            if isinstance(w, SuttaSearchWindowInterface) \
               and hasattr(w, 'action_Show_Related_Suttas'):
                w.action_Show_Related_Suttas.setChecked(is_on)

    def _toggle_show_line_by_line(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Show_Translation_and_Pali_Line_by_Line.isChecked()
        self._app_data.app_settings['show_translation_and_pali_line_by_line'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if isinstance(w, SuttaStudyWindowInterface) and hasattr(w, 'action_Show_Translation_and_Pali_Line_by_Line'):
                w.action_Show_Translation_and_Pali_Line_by_Line.setChecked(is_on)
                w.reload_sutta_pages()

            elif isinstance(w, SuttaSearchWindowInterface) and hasattr(w, 'action_Show_Translation_and_Pali_Line_by_Line'):
                w.action_Show_Translation_and_Pali_Line_by_Line.setChecked(is_on)
                w.s._get_active_tab().render_sutta_content()

    def _toggle_show_all_variant_readings(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Show_All_Variant_Readings.isChecked()
        self._app_data.app_settings['show_all_variant_readings'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if isinstance(w, SuttaStudyWindowInterface) and hasattr(w, 'action_Show_All_Variant_Readings'):
                w.action_Show_All_Variant_Readings.setChecked(is_on)
                w.reload_sutta_pages()

            elif isinstance(w, SuttaSearchWindowInterface) and hasattr(w, 'action_Show_All_Variant_Readings'):
                w.action_Show_All_Variant_Readings.setChecked(is_on)
                w.s._get_active_tab().render_sutta_content()

    def _toggle_show_bookmarks(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Show_Bookmarks.isChecked()
        self._app_data.app_settings['show_bookmarks'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if isinstance(w, SuttaStudyWindowInterface) and hasattr(w, 'action_Show_Bookmarks'):
                w.action_Show_Bookmarks.setChecked(is_on)
                w.reload_sutta_pages()

            elif isinstance(w, SuttaSearchWindowInterface) and hasattr(w, 'action_Show_Bookmarks'):
                w.action_Show_Bookmarks.setChecked(is_on)
                w.s._get_active_tab().render_sutta_content()

    # def _new_dictionaries_manager_window(self):
    #     from simsapa.layouts.dictionaries_manager import DictionariesManagerWindow
    #     view = DictionariesManagerWindow(self._app_data)
    #     return self._finalize_view(view)

    # def _new_library_browser_window(self):
    #     from simsapa.layouts.library_browser import LibraryBrowserWindow
    #     view = LibraryBrowserWindow(self._app_data)
    #     return self._finalize_view(view)

    def _reload_bookmarks(self):
        view = None
        for w in self._windows:
            if isinstance(w, BookmarksBrowserWindowInterface) and w.isVisible():
                view = w
                break

        if view is not None:
            view.reload_bookmarks()
            view.reload_table()

    def _new_bookmarks_browser_window_noret(self) -> None:
        self._new_bookmarks_browser_window()

    def _new_bookmarks_browser_window(self) -> AppWindowInterface:
        from simsapa.layouts.bookmarks_browser import BookmarksBrowserWindow
        view = BookmarksBrowserWindow(self._app_data)

        def _show_url(url: QUrl):
            self._show_sutta_by_url_in_search(url)

        view.show_sutta_by_url.connect(partial(_show_url))

        return self._finalize_view(view, maximize=False)

    def _new_course_practice_window(self, group: PaliCourseGroup) -> AppWindowInterface:
        from simsapa.layouts.pali_courses_browser import CoursesBrowserWindow
        from simsapa.layouts.pali_course_practice import CoursePracticeWindow

        view = CoursePracticeWindow(self._app_data, group)

        def _show_url(url: QUrl):
            self._show_sutta_by_url_in_search(url)

        view.show_sutta_by_url.connect(partial(_show_url))

        view.connect_preview_window_signals(self._preview_window)

        for w in self._windows:
            if isinstance(w, CoursesBrowserWindow):
                view.finished.connect(partial(w._reload_courses_tree))

        return self._finalize_view(view, maximize=False)

    def _start_challenge_group(self, group: PaliCourseGroup):
        self._new_course_practice_window(group)

    def _new_courses_browser_window_noret(self) -> None:
        self._new_courses_browser_window()

    def _new_courses_browser_window(self) -> AppWindowInterface:
        from simsapa.layouts.pali_courses_browser import CoursesBrowserWindow
        view = CoursesBrowserWindow(self._app_data)

        view.start_group.connect(partial(self._start_challenge_group))

        return self._finalize_view(view, maximize=False)

    def _new_memos_browser_window_noret(self) -> None:
        self._new_memos_browser_window()

    def _new_memos_browser_window(self) -> AppWindowInterface:
        from simsapa.layouts.memos_browser import MemosBrowserWindow
        view = MemosBrowserWindow(self._app_data)
        return self._finalize_view(view)

    def _new_links_browser_window_noret(self) -> None:
        self._new_links_browser_window()

    def _new_links_browser_window(self) -> AppWindowInterface:
        from simsapa.layouts.links_browser import LinksBrowserWindow
        view = LinksBrowserWindow(self._app_data)
        return self._finalize_view(view)

    def _new_gpt_prompts_window_noret(self, prompt_params: Optional[OpenPromptParams] = None) -> None:
        self._new_gpt_prompts_window(prompt_params)

    def _new_gpt_prompts_window(self, prompt_params: Optional[OpenPromptParams] = None) -> AppWindowInterface:
        from simsapa.layouts.gpt_prompts import GptPromptsWindow
        view = GptPromptsWindow(self._app_data, prompt_params)
        return self._finalize_view(view)

    def _new_ebook_reader_window_noret(self) -> None:
        self._new_ebook_reader_window()

    def _new_ebook_reader_window(self) -> EbookReaderWindowInterface:
        from simsapa.layouts.ebook_reader import EbookReaderWindow
        view = EbookReaderWindow(self._app_data)

        view.lookup_in_dictionary_signal.connect(partial(self._lookup_msg))
        view.lookup_in_new_sutta_window_signal.connect(partial(self._lookup_in_suttas_msg))

        view.reading_state.open_in_study_window_signal.connect(partial(self._study_msg_to_all))
        view.reading_state.open_sutta_new_signal.connect(partial(self.open_sutta_new))
        view.reading_state.open_gpt_prompt.connect(partial(self._new_gpt_prompts_window_noret))

        view.reading_state.connect_preview_window_signals(self._preview_window)

        view.sutta_state.open_in_study_window_signal.connect(partial(self._study_msg_to_all))
        view.sutta_state.open_sutta_new_signal.connect(partial(self.open_sutta_new))
        view.sutta_state.open_gpt_prompt.connect(partial(self._new_gpt_prompts_window_noret))

        view.sutta_state.connect_preview_window_signals(self._preview_window)

        return self._finalize_view(view)

    # def _new_document_reader_window(self, file_path=None):
    #     from simsapa.layouts.document_reader import DocumentReaderWindow
    #     view = DocumentReaderWindow(self._app_data)
    #     if file_path is not None and file_path is not False and len(file_path) > 0:
    #         view.open_doc(file_path)
    #     return self._finalize_view(view)

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

    def show_info(self, msg: str):
        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setText(msg)
        box.setWindowTitle("Info")
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()

    def show_warning(self, msg: str):
        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Warning)
        box.setText(msg)
        box.setWindowTitle("Warning")
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()

    def _check_updates(self):
        if self._app_data.app_settings.get('notify_about_updates'):
            self.thread_pool.start(self.check_updates_worker)

    def _init_check_updates(self, include_no_updates = False):
        logger.profile("AppWindows::_init_check_updates()")
        self.check_updates_worker = CheckUpdatesWorker()
        self.check_updates_worker.signals.local_db_obsolete.connect(partial(self.show_local_db_obsolete_message))
        self.check_updates_worker.signals.have_app_update.connect(partial(self.show_app_update_message))
        self.check_updates_worker.signals.have_db_update.connect(partial(self.show_db_update_message))

        if include_no_updates:
            self.check_updates_worker.signals.no_updates.connect(partial(self.show_no_updates_message))

    def _handle_check_updates(self):
        self._init_check_updates(include_no_updates=True)
        self.thread_pool.start(self.check_updates_worker)

    def show_no_updates_message(self):
        self.show_info("Application and database are up to date.")

    def show_local_db_obsolete_message(self, value: dict):
        update_info: UpdateInfo = value['update_info']

        update_info['message'] += "<h3>Download the new database and migrate data now?</h3>"

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setWindowTitle("Local Database Needs Upgrade")
        box.setText(update_info['message'])
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

        reply = box.exec()
        if reply == QMessageBox.StandardButton.Yes:
            self.export_user_data_to_assets()

            # Can't delete the db and index without triggering file-lock problems on Windows.
            # Write a file to assets to signal deleting them on next start.

            p = ASSETS_DIR.joinpath("delete_files_for_upgrade.txt")
            with open(p, 'w') as f:
                f.write("")

            p = ASSETS_DIR.joinpath("auto_start_download.txt")
            with open(p, 'w') as f:
                f.write("")

            box = QMessageBox()
            box.setIcon(QMessageBox.Icon.Information)
            box.setWindowTitle("Closing")
            box.setText("The application will now quit. Start it again to begin the database download.")
            box.setStandardButtons(QMessageBox.StandardButton.Ok)
            box.exec()

            self._quit_app()

    def export_user_data_to_assets(self):
        export_dir = ASSETS_DIR.joinpath("import-me")
        if not export_dir.exists():
            export_dir.mkdir()

        self._app_data.export_bookmarks(str(export_dir.joinpath("bookmarks.csv")))
        self._app_data.export_prompts(str(export_dir.joinpath("prompts.csv")))
        self._app_data.export_app_settings(str(export_dir.joinpath("app_settings.json")))

        # FIXME: export and import courses data and assets

        res = []

        r = self._app_data.db_session.query(Am.Sutta.language.distinct()).all()
        res.extend(r)

        r = self._app_data.db_session.query(Um.Sutta.language.distinct()).all()
        res.extend(r)

        languages: List[str] = list(sorted(set(map(lambda x: str(x[0]).lower(), res))))
        languages = [i for i in languages if i not in ['pli', 'en']]

        if 'san' in languages:
            # User has been using the Sanskrit language bundle
            p = ASSETS_DIR.joinpath("download_select_sanskrit_bundle.txt")
            with open(p, 'w') as f:
                f.write("True")

            languages.remove('san')

        if len(languages) > 0:
            p = ASSETS_DIR.joinpath("download_languages.txt")
            with open(p, 'w') as f:
                f.write(", ".join(languages))

    def import_user_data_from_assets(self):
        import_dir = ASSETS_DIR.joinpath("import-me")
        if not import_dir.exists():
            return

        try:
            # We are restoring the user's data to the state before the upgrade.
            # Remove existing records in userdata, which are default content from the new download.
            self._app_data.db_session.query(Um.Bookmark).delete()
            self._app_data.db_session.query(Um.GptPrompt).delete()
            self._app_data.db_session.commit()

            self._app_data.import_bookmarks(str(import_dir.joinpath("bookmarks.csv")))
            self._app_data.import_prompts(str(import_dir.joinpath("prompts.csv")))
            self._app_data.import_app_settings(str(import_dir.joinpath("app_settings.json")))

        except Exception as e:
            time = datetime.now().strftime("%Y-%m-%dT%H-%M-%S")
            export_dir = ASSETS_DIR.joinpath(f"exported_user_data_{time}")

            shutil.move(import_dir, export_dir)

            msg = f"""
            <p>Importing previous user data into the new database failed.</p>
            <p>The exported data has been saved, and the next application start will skip this step.</p>
            <p>The exported user data is found at: {export_dir}</p>

            <p>
                You can submit an issue report at:<br>
                <a href="https://github.com/simsapa/simsapa/issues">https://github.com/simsapa/simsapa/issues</a>
            </p>
            <p>
                Or send an email to <a href="mailto:profound.labs@gmail.com">profound.labs@gmail.com</a>
            </p>
            <p>
                Please include the text or a screenshot of the error mesage in your bug report.
            </p>

            <p>Error:</p>
            <p>{e}</p>
            """

            self.show_warning(msg)
            return

        shutil.rmtree(import_dir)

    def show_app_update_message(self, value: dict):
        update_info: UpdateInfo = value['update_info']

        update_info['message'] += "<h3>Open page in the browser now?</h3>"

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)

        msg = ""
        if update_info['visit_url'] is not None:
            msg = f"<p>Open the following URL to download the update:</p><p><a href='{update_info['visit_url']}' target='_blank'>{update_info['visit_url']}</a></p>"

        msg += update_info['message']

        box.setText(msg)
        box.setWindowTitle("Application Update Available")
        box.setStandardButtons(QMessageBox.StandardButton.Ok | QMessageBox.StandardButton.Cancel)

        reply = box.exec()
        if reply == QMessageBox.StandardButton.Yes and update_info['visit_url'] is not None:
            # NOTE: This doesn't work on some OSes, or the user doesn't see the new page, hence the above link.
            webbrowser.open_new(str(update_info['visit_url']))

    def show_db_update_message(self, value: dict):
        update_info: UpdateInfo = value['update_info']
        releases_info: ReleasesInfo = value['releases_info']

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
            res = []

            r = self._app_data.db_session.query(Am.Sutta.language.distinct()).all()
            res.extend(r)

            r = self._app_data.db_session.query(Um.Sutta.language.distinct()).all()
            res.extend(r)

            languages: List[str] = list(sorted(set(map(lambda x: str(x[0]).lower(), res))))
            languages = [i for i in languages if i not in ['pli', 'en']]

            select_sanskrit_bundle = False
            if 'san' in languages:
                # User has been using the Sanskrit language bundle
                select_sanskrit_bundle = True
                languages.remove('san')

            from simsapa.layouts.download_appdata import DownloadAppdataWindow
            w = DownloadAppdataWindow(ASSETS_DIR,
                                      releases_info,
                                      select_sanskrit_bundle,
                                      languages,
                                      auto_start_download = True)

            w._quit_action = self._quit_app
            w.show()

    def _reindex_database_dialog(self, _ = None):
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

        for w in self._windowed_previews:
            w.close()

    def _remove_temp_files(self):
        if EBOOK_UNZIP_DIR.exists():
            for p in EBOOK_UNZIP_DIR.glob('*'):
                if p.is_dir():
                    shutil.rmtree(p)
                else:
                    p.unlink()

    def _quit_app(self):
        self._close_all_windows()
        self._remove_temp_files()
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

    def _set_link_preview_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Link_Preview.isChecked()
        self._app_data.app_settings['link_preview'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w,'action_Link_Preview'):
                w.action_Link_Preview.setChecked(checked)

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

    def _set_double_click_dict_lookup_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Double_Click_on_a_Word_for_Dictionary_Lookup.isChecked()
        self._app_data.app_settings['double_click_dict_lookup'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w, 'action_Double_Click_on_a_Word_for_Dictionary_Lookup'):
                w.action_Double_Click_on_a_Word_for_Dictionary_Lookup.setChecked(checked)

    def _set_clipboard_monitoring_for_dict_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Clipboard_Monitoring_for_Dictionary_Lookup.isChecked()
        self._app_data.app_settings['clipboard_monitoring_for_dict'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w, 'action_Clipboard_Monitoring_for_Dictionary_Lookup'):
                w.action_Clipboard_Monitoring_for_Dictionary_Lookup.setChecked(checked)

    def _set_completion_cache(self, values: CompletionCacheResult):
        logger.info(f"_set_completion_cache(): sutta_titles: {len(values['sutta_titles'])}, dict_words: {len(values['dict_words'])}")
        self._queries.sutta_queries.completion_cache = values['sutta_titles']
        self._queries.dictionary_queries.completion_cache = values['dict_words']

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

    def _finalize_view(self, view, maximize = True):
        if maximize:
            self._set_size_and_maximize(view)
        self._connect_signals_to_view(view)
        make_active_window(view)
        self._windows.append(view)
        return view

    def _connect_signals_to_view(self, view):
        # if hasattr(view, 'action_Open'):
        #     view.action_Open \
        #         .triggered.connect(partial(self._open_file_dialog, view))

        if hasattr(view, 'action_Re_index_database'):
            view.action_Re_index_database \
                .triggered.connect(partial(self._reindex_database_dialog, view))

        if hasattr(view, 'action_Re_download_database'):
            view.action_Re_download_database \
                .triggered.connect(partial(self._redownload_database_dialog, view))

        if hasattr(view, 'action_Focus_Search_Input'):
            view.action_Focus_Search_Input \
                .triggered.connect(partial(self._focus_search_input, view))

        if hasattr(view, 'action_Quit'):
            view.action_Quit \
                .triggered.connect(partial(self._quit_app))

        if hasattr(view, 'action_Sutta_Search'):
            view.action_Sutta_Search \
                .triggered.connect(partial(self._new_sutta_search_window_noret))

        if hasattr(view, 'action_Sutta_Study'):
            view.action_Sutta_Study \
                .triggered.connect(partial(self._new_sutta_study_window_noret))

        if hasattr(view, 'action_Dictionary_Search'):
            view.action_Dictionary_Search \
                .triggered.connect(partial(self._new_dictionary_search_window_noret))

        if hasattr(view, 'action_Memos'):
            view.action_Memos \
                .triggered.connect(partial(self._new_memos_browser_window_noret))

        if hasattr(view, 'action_Links'):
            view.action_Links \
                .triggered.connect(partial(self._new_links_browser_window_noret))

        if hasattr(view, 'action_Sutta_Index'):
            view.action_Sutta_Index \
                .triggered.connect(partial(self._show_sutta_index_window))

        if hasattr(view, 'action_Bookmarks'):
            view.action_Bookmarks \
                .triggered.connect(partial(self._new_bookmarks_browser_window_noret))

        if hasattr(view, 'action_Pali_Courses'):
            view.action_Pali_Courses \
                .triggered.connect(partial(self._new_courses_browser_window_noret))

        if hasattr(view, 'action_Prompts'):
            view.action_Prompts \
                .triggered.connect(partial(self._new_gpt_prompts_window_noret, None))

        if hasattr(view, 'action_Ebook_Reader'):
            view.action_Ebook_Reader \
                .triggered.connect(partial(self._new_ebook_reader_window_noret))

        if isinstance(view, DictionarySearchWindowInterface):
            if hasattr(view, 'action_Show_Sidebar'):
                is_on = self._app_data.app_settings.get('show_dictionary_sidebar', True)
                view.action_Show_Sidebar.setChecked(is_on)

                view.action_Show_Sidebar \
                    .triggered.connect(partial(self._toggle_show_dictionary_sidebar, view))

        if isinstance(view, EbookReaderWindowInterface) \
           or isinstance(view, SuttaSearchWindowInterface):

            if hasattr(view, 'action_Show_Related_Suttas'):
                is_on = self._app_data.app_settings.get('show_related_suttas', True)
                view.action_Show_Related_Suttas.setChecked(is_on)

                view.action_Show_Related_Suttas \
                    .triggered.connect(partial(self._toggle_show_related_suttas, view))

        if isinstance(view, SuttaSearchWindowInterface):
            if hasattr(view, 'action_Show_Sidebar'):
                is_on = self._app_data.app_settings.get('show_sutta_sidebar', True)
                view.action_Show_Sidebar.setChecked(is_on)

                view.action_Show_Sidebar \
                    .triggered.connect(partial(self._toggle_show_sutta_sidebar, view))

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

            if hasattr(view, 'action_Show_Bookmarks'):
                is_on = self._app_data.app_settings.get('show_bookmarks', True)
                view.action_Show_Bookmarks.setChecked(is_on)

                view.action_Show_Bookmarks \
                    .triggered.connect(partial(self._toggle_show_bookmarks, view))

        if hasattr(view, 'action_Show_Word_Scan_Popup'):
            view.action_Show_Word_Scan_Popup \
                .triggered.connect(partial(self._toggle_word_scan_popup))
            if self.word_scan_popup is None:
                is_on = False
            else:
                is_on = self.word_scan_popup.isVisible()

            view.action_Show_Word_Scan_Popup.setChecked(is_on)

        if hasattr(view, 'action_First_Window_on_Startup'):
            view.action_First_Window_on_Startup \
                .triggered.connect(partial(self._first_window_on_startup_dialog, view))

        notify = self._app_data.app_settings.get('notify_about_updates', True)

        if hasattr(view, 'action_Notify_About_Updates'):
            view.action_Notify_About_Updates.setChecked(notify)
            view.action_Notify_About_Updates \
                .triggered.connect(partial(self._set_notify_setting, view))

        if hasattr(view, 'action_Website'):
            view.action_Website \
                .triggered.connect(partial(open_simsapa_website))

        if hasattr(view, 'action_About'):
            view.action_About \
                .triggered.connect(partial(show_about, view))

        show_toolbar = self._app_data.app_settings.get('show_toolbar', True)

        if hasattr(view, 'action_Show_Toolbar'):
            view.action_Show_Toolbar.setChecked(show_toolbar)
            view.action_Show_Toolbar \
                .triggered.connect(partial(self._set_show_toolbar_setting, view))

        link_preview = self._app_data.app_settings.get('link_preview', True)

        if hasattr(view, 'action_Link_Preview'):
            view.action_Link_Preview.setChecked(link_preview)
            view.action_Link_Preview \
                .triggered.connect(partial(self._set_link_preview_setting, view))

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

        if hasattr(view, 'action_Double_Click_on_a_Word_for_Dictionary_Lookup'):
            view.action_Double_Click_on_a_Word_for_Dictionary_Lookup \
                .triggered.connect(partial(self._set_double_click_dict_lookup_setting, view))

            checked = self._app_data.app_settings.get('double_click_dict_lookup', True)
            view.action_Double_Click_on_a_Word_for_Dictionary_Lookup.setChecked(checked)

        if hasattr(view, 'action_Clipboard_Monitoring_for_Dictionary_Lookup'):
            view.action_Clipboard_Monitoring_for_Dictionary_Lookup \
                .triggered.connect(partial(self._set_clipboard_monitoring_for_dict_setting, view))

            checked = self._app_data.app_settings.get('clipboard_monitoring_for_dict', True)
            view.action_Clipboard_Monitoring_for_Dictionary_Lookup.setChecked(checked)

        if hasattr(view, 'action_Check_for_Updates'):
            view.action_Check_for_Updates \
                .triggered.connect(partial(self._handle_check_updates))

        # s = os.getenv('ENABLE_WIP_FEATURES')
        # if s is not None and s.lower() == 'true':
        #     logger.info("no wip features")
        #     # view.action_Dictionaries_Manager \
        #     #     .triggered.connect(partial(self._new_dictionaries_manager_window))

        #     # view.action_Document_Reader \
        #     #     .triggered.connect(partial(self._new_document_reader_window))
        #     # view.action_Library \
        #     #     .triggered.connect(partial(self._new_library_browser_window))

        #     # try:
        #     #     view.action_Open_Selected \
        #     #         .triggered.connect(partial(self._open_selected_document, view))
        #     # except Exception as e:
        #     #     logger.error(e)

        # else:

        if hasattr(view, 'action_Open'):
            view.action_Open.setVisible(False)

        if hasattr(view, 'action_Dictionaries_Manager'):
            view.action_Dictionaries_Manager.setVisible(False)

        if hasattr(view, 'action_Document_Reader'):
            view.action_Document_Reader.setVisible(False)

        if hasattr(view, 'action_Library'):
            view.action_Library.setVisible(False)
