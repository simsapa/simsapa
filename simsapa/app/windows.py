import os, sys, re, shutil, queue, json
from functools import partial
from typing import Callable, List, Optional
from datetime import datetime
from urllib.parse import parse_qs

from PyQt6.QtGui import QIcon, QAction
from PyQt6.QtCore import QObject, QThreadPool, QTimer, QUrl, pyqtSignal
from PyQt6.QtWidgets import (QApplication, QInputDialog, QMainWindow, QMessageBox, QWidget, QSystemTrayIcon, QMenu)

from simsapa import ASSETS_DIR, EBOOK_UNZIP_DIR, IS_MAC, SIMSAPA_API_PORT_PATH, START_LOW_MEM, logger, ApiAction, ApiMessage
from simsapa import SERVER_QUEUE, APP_DB_PATH, APP_QUEUES, STARTUP_MESSAGE_PATH, TIMER_SPEED
from simsapa import QueryType, SuttaQuote, QuoteScope, QuoteScopeValues

from simsapa.app.hotkeys_manager_interface import HotkeysManagerInterface
from simsapa.app.check_simsapa_updates_worker import CheckSimsapaUpdatesWorker
from simsapa.app.check_dpd_updates_worker import CheckDpdUpdatesWorker

from simsapa.app.app_data import AppData

from simsapa.layouts.gui_types import (
    AppMessage, AppWindowInterface, BookmarksBrowserWindowInterface, DictionarySearchWindowInterface,
    EbookReaderWindowInterface, OpenPromptParams, PaliCourseGroup,
    SuttaSearchWindowInterface, SuttaStudyWindowInterface, WindowNameToType, WindowType, WordLookupInterface,
    sutta_quote_from_url)

from simsapa.layouts.gui_helpers import ReleasesInfo, UpdateInfo, is_sutta_search_window, is_sutta_study_window, is_dictionary_search_window, is_ebook_reader_window
from simsapa.layouts.gui_queries import GuiSearchQueries
from simsapa.layouts.help_info import open_simsapa_website, show_about
from simsapa.layouts.preview_window import PreviewWindow

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

class AppWindowsSignals(QObject):
    open_window_signal = pyqtSignal(str)
    run_lookup_query_signal = pyqtSignal(str)

class AppWindows:
    _preview_window: PreviewWindow
    signals: AppWindowsSignals
    tray: QSystemTrayIcon

    def __init__(self, app: QApplication, app_data: AppData, hotkeys_manager: Optional[HotkeysManagerInterface]):
        self.signals = AppWindowsSignals()
        self._app = app
        self._app_data = app_data
        self._queries = GuiSearchQueries(self._app_data.db_session,
                                         None,
                                         self._app_data.get_search_indexes,
                                         self._app_data.api_url)
        self._hotkeys_manager = hotkeys_manager
        self._windows: List[AppWindowInterface] = []
        self._windowed_previews: List[PreviewWindow] = []
        self._sutta_index_window: Optional[AppWindowInterface] = None

        self.word_lookup: Optional[WordLookupInterface] = None

        self.tray = self._setup_system_tray()

        # Init PreviewWindow here, so that the first window can connect signals to it.
        self._init_preview_window()

        self.queue_id = 'app_windows'
        APP_QUEUES[self.queue_id] = queue.Queue()

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self.thread_pool = QThreadPool()

        # Wait 0.5s, then run slowish initialize tasks, e.g. init windows, check for updates.
        # By that time the first window will be opened and will not delay app.exec().
        self.init_timer = QTimer()
        self.init_timer.setSingleShot(True)
        self.init_timer.timeout.connect(partial(self._init_tasks))
        self.init_timer.start(500)

        self.signals.open_window_signal.connect(partial(self._handle_open_window_signal))

    def _init_tasks(self):
        logger.profile("AppWindows::_init_tasks(): start")

        if not START_LOW_MEM:
            self._init_word_lookup()
            self._init_sutta_index_window()

        self._init_check_simsapa_updates()
        self._init_check_dpd_updates()
        self._check_simsapa_updates()
        self._check_dpd_updates()

        self.import_user_data_from_assets()

        if not START_LOW_MEM:
            self._init_main_windows()

        logger.profile("AppWindows::_init_tasks(): end")

        msg = """
==============================================================
==                                                          ==
==  Simsapa is now running. Minimize this terminal window,  ==
==  but don't close it (which would also close Simsapa).    ==
==                                                          ==
==============================================================
        """.strip()

        if IS_MAC:
            print("\n\n" + msg)

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

                if msg['action'] == ApiAction.remove_closed_window_from_list:
                    window_queue_id = msg['data']
                    self._remove_closed_window_from_list(window_queue_id)

                elif msg['action'] == ApiAction.show_word_lookup:
                    self._toggle_word_lookup()

                elif msg['action'] == ApiAction.closed_word_lookup:
                    self._closed_word_lookup()

                elif msg['action'] == ApiAction.hidden_word_lookup:
                    self._hidden_word_lookup()

                elif msg['action'] == ApiAction.open_sutta_new:
                    self.open_sutta_new(uid = msg['data'])

                elif msg['action'] == ApiAction.show_sutta_by_url:
                    url = QUrl(msg['data'])
                    self._show_sutta_by_url_in_search(url)

                elif msg['action'] == ApiAction.open_words_new:
                    schemas_tables_uids = json.loads(msg['data'])
                    self.open_words_new(schemas_tables_uids)

                elif msg['action'] == ApiAction.show_word_by_url:
                    url = QUrl(msg['data'])
                    self._show_words_by_url(url)

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

    def handle_system_tray_clicked(self):
        logger.info("handle_system_tray_clicked()")

        show_window = True
        for w in self._windows:
            if w.isVisible():
                w.hide()
                show_window = False

        if show_window:
            window_type = self._app_data.app_settings.get('tray_click_opens_window', WindowType.SuttaSearch)
            self._open_window_type(window_type)

    def _handle_open_window_signal(self, window_type_name: str = ''):
        logger.info(f"_handle_open_window_signal(): '{window_type_name}'")

        if len(window_type_name) == 0 \
           or window_type_name not in WindowNameToType.keys():

            window_type = self._app_data.app_settings.get('tray_click_opens_window', WindowType.SuttaSearch)
            self._open_window_type(window_type)
            return

        window_type = WindowNameToType[window_type_name]
        self._open_window_type(window_type)

    def _remove_closed_window_from_list(self, window_queue_id: str):
        # Remove the window from self._windows to free up memory, unless it is
        # the last of its type, in which case leaving it will allow to use
        # .show() when the use clicks that menu item again.

        view_idx: Optional[int] = None

        for idx, w in enumerate(self._windows):
            if hasattr(w, 'queue_id') and w.queue_id == window_queue_id:
                view_idx = idx

        if view_idx is None:
            return

        # Is it the last of its type?
        view_type = type(self._windows[view_idx])
        a = [type(w) for w in self._windows if type(w) == view_type]

        if len(a) > 1:
            del self._windows[view_idx]
        else:
            logger.info(f"Last window of type {view_type}, not removing.")

    def _setup_system_tray(self) -> QSystemTrayIcon:
        logger.profile("_create_system_tray_menu(): start")

        tray = QSystemTrayIcon(QIcon(":simsapa-tray"))
        tray.setVisible(True)

        tray.activated.connect(partial(self.handle_system_tray_clicked))

        menu = QMenu()

        action_Sutta_Search = QAction(QIcon(":book"), "Sutta Search")
        action_Sutta_Search.triggered.connect(partial(self._new_sutta_search_window_noret))
        menu.addAction(action_Sutta_Search)

        action_Sutta_Study = QAction(QIcon(":book"), "Sutta Study")
        action_Sutta_Study.triggered.connect(partial(self._new_sutta_study_window_noret))
        menu.addAction(action_Sutta_Study)

        action_Sutta_Index = QAction(QIcon(":book"), "Sutta Index")
        action_Sutta_Index.triggered.connect(partial(self._show_sutta_index_window))
        menu.addAction(action_Sutta_Index)

        action_Dictionary_Search = QAction(QIcon(":dictionary"), "Dictionary Search")
        action_Dictionary_Search.triggered.connect(partial(self._new_dictionary_search_window_noret))
        menu.addAction(action_Dictionary_Search)

        action_Show_Word_Lookup = QAction(QIcon(":dictionary"), "Word Lookup")
        action_Show_Word_Lookup.triggered.connect(partial(self._toggle_word_lookup))
        menu.addAction(action_Show_Word_Lookup)

        action_Ebook_Reader = QAction(QIcon(":book-open-solid"), "Ebook Reader")
        action_Ebook_Reader.triggered.connect(partial(self._new_ebook_reader_window_noret))
        menu.addAction(action_Ebook_Reader)

        action_Quit = QAction(QIcon(":close"), "Quit")
        action_Quit.triggered.connect(partial(self._quit_app))
        menu.addAction(action_Quit)

        tray.setContextMenu(menu)

        logger.profile("_create_system_tray_menu(): end")

        return tray

    def open_sutta_new(self, uid: str):
        from simsapa.layouts.sutta_window import SuttaWindow
        view = SuttaWindow(self._app_data, uid)
        self._finalize_view(view)

    def open_words_new(self, schemas_tables_uids: List[tuple[str, str, str]]):
        from simsapa.layouts.words_window import WordsWindow
        view = WordsWindow(self._app_data, schemas_tables_uids)
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

        sutta = self._preview_window._queries.sutta_queries.get_sutta_by_url(url)

        if sutta:
            self._new_sutta_search_window(f"uid:{sutta.uid}")

    def _show_words_by_url(self, url: QUrl, show_results_tab = False) -> bool:
        # http://localhost:4848/words/rupa
        # http://localhost:4848/words/55151
        # http://localhost:4848/words/55151/dpd
        #
        # ssp://words/rupa
        # ssp://words/rupa/mw

        if url.host() != 'localhost' and \
           url.host() != QueryType.words:
            return False

        self._preview_window._do_hide()

        view = None
        for w in self._windows:
            if isinstance(w, DictionarySearchWindowInterface) and w.isVisible():
                view = w
                break

        # url.path() = /words/rupa
        # url.path() = /rupa
        query = re.sub(r"^/words", "", url.path())
        query = query.strip('/')

        if view is None:
            self._new_dictionary_search_window(query)
        else:
            view._show_word_by_url(url, show_results_tab)

        return True

    def _show_words_url_noret(self, url: QUrl):
        self._show_words_by_url(url)

    def _show_sutta_url_noret(self, url: QUrl):
        self._show_sutta_by_url_in_search(url)

    def _show_sutta_by_url_in_search(self, url: QUrl) -> bool:
        # http://localhost:4848/suttas/sn23.11?q=grows+disillusioned+with+form
        # ssp://suttas/sn23.11?q=grows+disillusioned+with+form"
        if url.host() != 'localhost' and \
           url.host() != QueryType.suttas:
            return False

        self._preview_window._do_hide()

        # url.path() = /suttas/sn23.11
        # url.path() = /sn23.11
        uid = re.sub(r"^/suttas", "", url.path())
        uid = re.sub(r"^/", "", uid)

        query = parse_qs(url.query())

        quote_scope = QuoteScope.Sutta
        if 'quote_scope' in query.keys():
            sc = query['quote_scope'][0]
            if sc in QuoteScopeValues.keys():
                quote_scope = QuoteScopeValues[sc]

        # Default to SuttaSearch window.
        # Only allow SuttaSearch or SuttaStudy in query parameter.

        window_type = WindowType.SuttaSearch

        if 'window_type' in query.keys():
            s = query['window_type'][0]
            logger.info(s)
            if s in WindowNameToType.keys():
                t = WindowNameToType[s]

                if t in [WindowType.SuttaSearch, WindowType.SuttaStudy]:
                    window_type = t

        if window_type == WindowType.SuttaSearch:
            self._show_sutta_by_uid_in_search(uid, sutta_quote_from_url(url), quote_scope, new_window=True)

        elif window_type == WindowType.SuttaStudy:
            self._show_sutta_by_uid_in_study(uid, sutta_quote_from_url(url), quote_scope, new_window=True)

        return True

    def _show_sutta_by_uid_in_search(self,
                                     uid: str,
                                     sutta_quote: Optional[SuttaQuote] = None,
                                     quote_scope = QuoteScope.Sutta,
                                     new_window = False):

        view = None

        if not new_window:
            for w in self._windows:
                if isinstance(w, SuttaSearchWindowInterface) and w.isVisible():
                    view = w
                    break

        if view is None:
            view = self._new_sutta_search_window()

        view.s._show_sutta_by_uid(uid, sutta_quote, quote_scope)

    def _show_sutta_by_uid_in_study(self,
                                    uid: str,
                                    sutta_quote: Optional[SuttaQuote] = None,
                                    quote_scope = QuoteScope.Sutta,
                                    new_window = False):

        view = None

        if not new_window:
            for w in self._windows:
                if isinstance(w, SuttaStudyWindowInterface) and w.isVisible():
                    view = w
                    break

        if view is None:
            view = self._new_sutta_study_window()

        view.sutta_panels[0]['state']._show_sutta_by_uid(uid, sutta_quote, quote_scope)

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

    def _open_window_type(self, window_type: WindowType) -> Optional[QMainWindow]:
        if window_type == WindowType.SuttaSearch:
            return self._new_sutta_search_window()

        if window_type == WindowType.SuttaStudy:
            return self._new_sutta_study_window()

        elif window_type == WindowType.DictionarySearch:
            return self._new_dictionary_search_window()

        elif window_type == WindowType.EbookReader:
            return self._new_ebook_reader_window()

        elif window_type == WindowType.WordLookup:
            return self._toggle_word_lookup()

        else:
            return self._new_sutta_search_window()

    def open_first_window(self, window_type: Optional[WindowType] = None):
        if window_type is None:
            window_type = self._app_data.app_settings.get('first_window_on_startup', WindowType.SuttaSearch)
        self._open_window_type(window_type)

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

    def _new_sutta_search_window(self, query: Optional[str] = None, show = True) -> SuttaSearchWindowInterface:
        from simsapa.layouts.sutta_search import SuttaSearchWindow

        if query is not None and not isinstance(query, str):
            query = None

        view = None
        is_new = True

        for w in self._windows:
            if is_sutta_search_window(w) and w.isHidden():
                is_new = False
                view = w

        if view is None:
            view = SuttaSearchWindow(self._app_data)

            view.lookup_in_dictionary_signal.connect(partial(self._lookup_msg))
            view.s.open_in_study_window_signal.connect(partial(self._study_msg_to_all))
            view.s.open_sutta_new_signal.connect(partial(self.open_sutta_new))

            view.s.bookmark_created.connect(partial(self._reload_bookmarks))

            view.s.bookmark_created.connect(partial(view.s.reload_page))
            view.s.bookmark_updated.connect(partial(view.s.reload_page))

            view.s.page_dblclick.connect(partial(self._sutta_search_quick_lookup_selection, view = view))
            view.s.open_gpt_prompt.connect(partial(self._new_gpt_prompts_window_noret))

            view.connect_preview_window_signals(self._preview_window)

            if self._hotkeys_manager is not None:
                try:
                    self._hotkeys_manager.setup_window(view)
                except Exception as e:
                    logger.error(e)

            if self._app_data.sutta_to_open:
                view._show_sutta(self._app_data.sutta_to_open)
                self._app_data.sutta_to_open = None

        assert(isinstance(view, SuttaSearchWindowInterface))

        if query is not None:
            view.s._set_query(query)
            view.s._handle_query()

        return self._finalize_view(view, maximize=is_new, is_new=is_new, show=show)

    def _new_sutta_study_window_noret(self) -> None:
        self._new_sutta_study_window()

    def _new_sutta_study_window(self, show = True) -> SuttaStudyWindowInterface:
        from simsapa.layouts.sutta_study import SuttaStudyWindow

        view = None
        is_new = True

        for w in self._windows:
            if is_sutta_study_window(w) and w.isHidden():
                is_new = False
                view = w

        if view is None:
            view = SuttaStudyWindow(self._app_data)

            def _study(queue_id: str, side: str, uid: str):
                data = {'side': side, 'uid': uid}
                msg = ApiMessage(queue_id = queue_id,
                                action = ApiAction.open_in_study_window,
                                data = json.dumps(obj=data))
                self._show_sutta_by_uid_in_side(msg)

            for panel in view.sutta_panels:
                panel['state'].open_in_study_window_signal.connect(partial(_study))
                panel['state'].open_sutta_new_signal.connect(partial(self.open_sutta_new))

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

        assert(isinstance(view, SuttaStudyWindowInterface))

        return self._finalize_view(view, maximize=is_new, is_new=is_new, show=show)

    def _init_preview_window(self):
        logger.profile("AppWindows::_init_preview_window()")

        self._preview_window = PreviewWindow(self._app_data)

        def _words(url: QUrl):
            self._show_words_by_url(url, show_results_tab=False)

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

    def _init_main_windows(self):
        # Init one of each of the main windows, so that it only needs to .show()
        # when the user clicks the menu.

        def _not_has_window(_test_fn: Callable[[AppWindowInterface], bool]) -> bool:
            a = [w for w in self._windows if _test_fn(w)]
            return (len(a) == 0)

        if _not_has_window(is_sutta_search_window):
            self._new_sutta_search_window(query=None, show=False)

        if _not_has_window(is_sutta_study_window):
            self._new_sutta_study_window(show=False)

        if _not_has_window(is_dictionary_search_window):
            self._new_dictionary_search_window(query=None, show=False)

        if _not_has_window(is_ebook_reader_window):
            self._new_ebook_reader_window(show=False)

    def _new_dictionary_search_window_noret(self, query: Optional[str] = None) -> None:
        self._new_dictionary_search_window(query)

    def _lookup_in_suttas_msg(self, query: str):
        msg = ApiMessage(queue_id = 'all',
                         action = ApiAction.lookup_in_suttas,
                         data = query)
        self._lookup_clipboard_in_suttas(msg)

    def _new_dictionary_search_window(self, query: Optional[str] = None, show = True) -> DictionarySearchWindowInterface:
        from simsapa.layouts.dictionary_search import DictionarySearchWindow

        if query is not None and not isinstance(query, str):
            query = None

        view = None
        is_new = True

        for w in self._windows:
            if is_dictionary_search_window(w) and w.isHidden():
                is_new = False
                view = w

        if view is None:
            view = DictionarySearchWindow(self._app_data)

            view.show_sutta_by_url.connect(partial(self._show_sutta_url_noret))
            view.show_words_by_url.connect(partial(self._show_words_url_noret))

            view.lookup_in_new_sutta_window_signal.connect(partial(self._lookup_in_suttas_msg))
            view.open_words_new_signal.connect(partial(self.open_words_new))
            view.page_dblclick.connect(partial(view._lookup_selection_in_dictionary, show_results_tab=False))

            view.connect_preview_window_signals(self._preview_window)

            if self._hotkeys_manager is not None:
                try:
                    self._hotkeys_manager.setup_window(view)
                except Exception as e:
                    logger.error(e)

            if self._app_data.dict_word_to_open:
                view._show_word(self._app_data.dict_word_to_open)
                self._app_data.dict_word_to_open = None

        assert(isinstance(view, DictionarySearchWindowInterface))

        if query is not None:
            logger.info("Set and handle query: " + query)
            view._set_query(query)
            view._handle_query()
            view._handle_exact_query()

        return self._finalize_view(view, maximize=is_new, is_new=is_new, show=show)

    def _init_word_lookup(self):
        if self.word_lookup is not None:
            return

        logger.profile("AppWindows::_init_word_lookup()")
        from simsapa.layouts.word_lookup import WordLookup

        self.word_lookup = WordLookup(self._app_data)

        self.word_lookup.action_Quit \
            .triggered.connect(partial(self._quit_app))

        self._connect_windows_menu_signals_to_view(self.word_lookup)

        def _show_sutta_url(url: QUrl):
            self._show_sutta_by_url_in_search(url)

        def _show_words_url(url: QUrl):
            if self.word_lookup:
                self.word_lookup.s._show_word_by_url(url)

        self.word_lookup.s.show_sutta_by_url.connect(partial(_show_sutta_url))
        self.word_lookup.s.show_words_by_url.connect(partial(_show_words_url))

        self.word_lookup.s.connect_preview_window_signals(self._preview_window)

        def _run_lookup_query(query_text: str):
            self._show_word_lookup(query_text, show_results_tab=False)

        self.signals.run_lookup_query_signal.connect(_run_lookup_query)

    def _toggle_word_lookup(self):
        if self.word_lookup is None:
            self._init_word_lookup()

        assert(self.word_lookup is not None)

        if self.word_lookup.isVisible():
            self.word_lookup.hide()

        else:
            self.word_lookup.show()
            self.word_lookup.raise_()
            self.word_lookup.activateWindow()

        self._set_all_show_word_lookup_checked()

    def _set_all_show_word_lookup_checked(self):
        if self.word_lookup is None:
            is_on = False
        else:
            is_on = self.word_lookup.isVisible()

        for w in self._windows:
            if hasattr(w, 'action_Show_Word_Lookup'):
                w.action_Show_Word_Lookup.setChecked(is_on)

    def _closed_word_lookup(self):
        if self.word_lookup is not None:
            self.word_lookup.close()
            self.word_lookup = None

        self._set_all_show_word_lookup_checked()

    def _hidden_word_lookup(self):
        if self.word_lookup is None:
            return

        if self.word_lookup.isVisible():
            self.word_lookup.hide()

        self._set_all_show_word_lookup_checked()

    def _sutta_search_quick_lookup_selection(self, view: SuttaSearchWindowInterface):
        query = view.s._get_selection()
        self._show_word_lookup(query = query, show_results_tab = False)

    def _show_word_lookup(self, query: Optional[str] = None, show_results_tab = False):
        if self.word_lookup is None:
            self._init_word_lookup()

        else:
            self.word_lookup.show()
            self.word_lookup.raise_()
            self.word_lookup.activateWindow()

            if query is not None:
                self.word_lookup.s.lookup_in_dictionary(query, show_results_tab)

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
            if hasattr(w, 'action_Show_Translation_and_Pali_Line_by_Line'):
                if is_sutta_study_window(w):
                    assert(isinstance(w, SuttaStudyWindowInterface))
                    w.action_Show_Translation_and_Pali_Line_by_Line.setChecked(is_on)
                    w.reload_sutta_pages()

                elif is_sutta_search_window(w):
                    assert(isinstance(w, SuttaSearchWindowInterface))
                    w.action_Show_Translation_and_Pali_Line_by_Line.setChecked(is_on)
                    w.s._get_active_tab().render_sutta_content()

    def _toggle_show_all_variant_readings(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Show_All_Variant_Readings.isChecked()
        self._app_data.app_settings['show_all_variant_readings'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w, 'action_Show_All_Variant_Readings'):
                if is_sutta_study_window(w):
                    assert(isinstance(w, SuttaStudyWindowInterface))
                    w.action_Show_All_Variant_Readings.setChecked(is_on)
                    w.reload_sutta_pages()

                elif is_sutta_search_window(w):
                    assert(isinstance(w, SuttaSearchWindowInterface))
                    w.action_Show_All_Variant_Readings.setChecked(is_on)
                    w.s._get_active_tab().render_sutta_content()

    def _toggle_show_bookmarks(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Show_Bookmarks.isChecked()
        self._app_data.app_settings['show_bookmarks'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w, 'action_Show_Bookmarks'):
                if is_sutta_study_window(w):
                    assert(isinstance(w, SuttaStudyWindowInterface))
                    w.action_Show_Bookmarks.setChecked(is_on)
                    w.reload_sutta_pages()

                elif is_sutta_search_window(w):
                    assert(isinstance(w, SuttaSearchWindowInterface))
                    w.action_Show_Bookmarks.setChecked(is_on)
                    w.s._get_active_tab().render_sutta_content()

    def _toggle_generate_links_graph(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Generate_Links_Graph.isChecked()
        self._app_data.app_settings['generate_links_graph'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if isinstance(w, SuttaSearchWindowInterface) and hasattr(w, 'action_Generate_Links_Graph'):
                w.action_Generate_Links_Graph.setChecked(is_on)
                if is_on:
                    w.show_network_graph()
                else:
                    w.hide_network_graph()

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

    def _new_ebook_reader_window(self, show = True) -> EbookReaderWindowInterface:
        from simsapa.layouts.ebook_reader import EbookReaderWindow

        view = None
        is_new = True

        for w in self._windows:
            if is_ebook_reader_window(w) and w.isHidden():
                is_new = False
                view = w

        if view is None:
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

        assert(isinstance(view, EbookReaderWindowInterface))

        return self._finalize_view(view, maximize=is_new, is_new=is_new, show=show)

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

    def show_setting_after_restart(self):
        self.show_info("This setting takes effect after restarting the application.")

    def _check_simsapa_updates(self):
        if self._app_data.app_settings.get('notify_about_simsapa_updates'):
            self.thread_pool.start(self.check_simsapa_updates_worker)

    def _check_dpd_updates(self):
        if self._app_data.app_settings.get('notify_about_dpd_updates'):
            self.thread_pool.start(self.check_dpd_updates_worker)

    def _init_check_simsapa_updates(self, include_no_updates = False):
        logger.profile("AppWindows::_init_check_simsapa_updates()")

        if self._app_data.screen_size is not None:
            w = self._app_data.screen_size.width()
            h = self._app_data.screen_size.height()
            screen_size = f"{w} x {h}"
        else:
            screen_size = ''

        self.check_simsapa_updates_worker = CheckSimsapaUpdatesWorker(screen_size=screen_size)
        self.check_simsapa_updates_worker.signals.local_db_obsolete.connect(partial(self.show_local_db_obsolete_message))
        self.check_simsapa_updates_worker.signals.have_app_update.connect(partial(self.show_app_update_message))
        self.check_simsapa_updates_worker.signals.have_db_update.connect(partial(self.show_db_update_message))

        if include_no_updates:
            self.check_simsapa_updates_worker.signals.no_updates.connect(partial(self.show_no_simsapa_updates_message))

    def _init_check_dpd_updates(self, include_no_updates = False):
        logger.profile("AppWindows::_init_check_dpd_updates()")

        if self._app_data.screen_size is not None:
            w = self._app_data.screen_size.width()
            h = self._app_data.screen_size.height()
            screen_size = f"{w} x {h}"
        else:
            screen_size = ''

        self.check_dpd_updates_worker = CheckDpdUpdatesWorker(screen_size=screen_size)
        self.check_dpd_updates_worker.signals.have_dpd_update.connect(partial(self.show_dpd_update_message))

        if include_no_updates:
            self.check_dpd_updates_worker.signals.no_updates.connect(partial(self.show_no_dpd_updates_message))

    def _handle_check_simsapa_updates(self):
        self._init_check_simsapa_updates(include_no_updates=True)
        self.thread_pool.start(self.check_simsapa_updates_worker)

    def _handle_check_dpd_updates(self):
        self._init_check_dpd_updates(include_no_updates=True)
        self.thread_pool.start(self.check_dpd_updates_worker)

    def show_no_simsapa_updates_message(self):
        self.show_info("Simsapa application and database are up to date.")

    def show_no_dpd_updates_message(self):
        self.show_info("Digital Pāḷi Dictionary is up to date.")

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

        update_info['message'] += "<h3>Click on the link to open the download page.</h3>"

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)

        msg = ""
        if update_info['visit_url'] is not None:
            msg = f"<p>Open the following URL to download the update:</p><p><a href='{update_info['visit_url']}' target='_blank'>{update_info['visit_url']}</a></p>"

        msg += update_info['message']

        box.setText(msg)
        box.setWindowTitle("Application Update Available")
        box.setStandardButtons(QMessageBox.StandardButton.Close)

        box.exec()

    def show_dpd_update_message(self, value: dict):
        update_info: UpdateInfo = value['update_info']

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setWindowTitle("Digital Pāḷi Dictionary Update")

        msg = "<h1>Digital Pāḷi Dictionary Update</h1>"
        msg += "<p>This update is optional.</p>"

        if update_info['message'] != "":
            msg += "<p><b>Release Notes:</b></p>"
            msg += update_info['message']

        msg += "<h3>Quit and update now?</h3>"

        msg += """
        <p>To update the DPD, choose <b>Yes</b>. The application will exit.</p>
        <p>When you start the application again, the download will begin.</p>
        """

        box.setText(msg)
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

        reply = box.exec()

        if reply == QMessageBox.StandardButton.Yes:
            # NOTE: Can't safely clear and remove indexes here. rmtree()
            # triggers an error on Windows when the index files are still
            # locked.

            p = ASSETS_DIR.joinpath("upgrade_dpd.txt")
            with open(p, 'w') as f:
                f.write("")

            self._quit_app()

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

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setWindowTitle("Database Update Available")

        update_info['message'] += "<h3>This update is optional, and the download may take a while.</h3>"
        update_info['message'] += "<h3>Download and update now?</h3>"

        # Download update without deleting existing database.
        # When the download is successful, delete old db and replace with new.
        #
        # Remove half-downloaded assets if download is cancelled.
        # Remove half-downloaded assets if found on startup.

        box.setText(update_info['message'])
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

        if SIMSAPA_API_PORT_PATH.exists():
            SIMSAPA_API_PORT_PATH.unlink()

        self._remove_temp_files()
        self._app.quit()

        logger.info("_quit_app() Exiting with status 0.")
        sys.exit(0)

    def _set_notify_simsapa_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Notify_About_Simsapa_Updates.isChecked()
        self._app_data.app_settings['notify_about_simsapa_updates'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w,'action_Notify_About_Simsapa_Updates'):
                w.action_Notify_About_Simsapa_Updates.setChecked(checked)

    def _set_notify_dpd_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Notify_About_DPD_Updates.isChecked()
        self._app_data.app_settings['notify_about_dpd_updates'] = checked
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w,'action_Notify_About_DPD_Updates'):
                w.action_Notify_About_DPD_Updates.setChecked(checked)

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

                if is_sutta_search_window(w) and isinstance(w, SuttaSearchWindowInterface):
                    if checked:
                        w.s._init_search_input_completer()
                    else:
                        w.s._disable_search_input_completer()

                elif is_dictionary_search_window and isinstance(w, DictionarySearchWindowInterface):
                    if checked:
                        w._init_search_input_completer()
                    else:
                        w._disable_search_input_completer()

    def _set_double_click_word_lookup_setting(self, view: AppWindowInterface):
        checked: bool = view.action_Double_Click_on_a_Word_for_Dictionary_Lookup.isChecked()
        self._app_data.app_settings['double_click_word_lookup'] = checked
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

    def _first_window_on_startup_dialog(self, view: AppWindowInterface):
        options = WindowNameToType.keys()

        window_type = self._app_data.app_settings.get('first_window_on_startup', WindowType.SuttaSearch)
        option_idx = 0
        for idx, i in enumerate(WindowNameToType.values()):
            if i == window_type:
                option_idx = idx
                break

        item, ok = QInputDialog.getItem(view,
                                        "Select Window",
                                        "Select the first window to open when the app is stated:",
                                        options,
                                        option_idx,
                                        False)
        if ok and item:
            self._app_data.app_settings['first_window_on_startup'] = WindowNameToType[item]
            self._app_data._save_app_settings()

    def _select_track_click_window_dialog(self, view: AppWindowInterface):
        options = WindowNameToType.keys()

        window_type = self._app_data.app_settings.get('tray_click_opens_window', WindowType.SuttaSearch)
        option_idx = 0
        for idx, i in enumerate(WindowNameToType.values()):
            if i == window_type:
                option_idx = idx
                break

        item, ok = QInputDialog.getItem(view,
                                        "Select Window",
                                        "Select the window to open when clicking on the tray icon:",
                                        options,
                                        option_idx,
                                        False)
        if ok and item:
            self._app_data.app_settings['tray_click_opens_window'] = WindowNameToType[item]
            self._app_data._save_app_settings()

    def _toggle_keep_running(self, view: SuttaSearchWindowInterface):
        is_on = view.action_Keep_Running_in_the_Background.isChecked()
        self._app_data.app_settings['keep_running_in_background'] = is_on
        self._app_data._save_app_settings()

        for w in self._windows:
            if hasattr(w, 'action_Keep_Running_in_the_Background'):
                w.action_Keep_Running_in_the_Background.setChecked(is_on)

        self.show_setting_after_restart()

    def _focus_search_input(self, view: AppWindowInterface):
        if hasattr(view, 'search_input'):
            view.search_input.setFocus()
        elif hasattr(view, '_focus_search_input'):
            view._focus_search_input()

    def _finalize_view(self, view, maximize = True, is_new = True, show = True):
        if maximize or is_new:
            self._set_size_and_maximize(view)

        if is_new:
            self._connect_signals_to_view(view)
            self._windows.append(view)

        if show:
            make_active_window(view)

        return view

    def _connect_windows_menu_signals_to_view(self, view):
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

        if hasattr(view, 'action_Show_Word_Lookup'):
            view.action_Show_Word_Lookup \
                .triggered.connect(partial(self._toggle_word_lookup))
            if self.word_lookup is None:
                is_on = False
            else:
                is_on = self.word_lookup.isVisible()

            view.action_Show_Word_Lookup.setChecked(is_on)

        if hasattr(view, 'action_First_Window_on_Startup'):
            view.action_First_Window_on_Startup \
                .triggered.connect(partial(self._first_window_on_startup_dialog, view))

    def _connect_signals_to_view(self, view):
        # if hasattr(view, 'action_Open'):
        #     view.action_Open \
        #         .triggered.connect(partial(self._open_file_dialog, view))

        self._connect_windows_menu_signals_to_view(view)

        if hasattr(view, 'action_Keep_Running_in_the_Background'):
            is_on = self._app_data.app_settings.get('keep_running_in_background', True)
            view.action_Keep_Running_in_the_Background.setChecked(is_on)

            view.action_Keep_Running_in_the_Background \
                .triggered.connect(partial(self._toggle_keep_running, view))

        if hasattr(view, 'action_Tray_Click_Opens_Window'):
            view.action_Tray_Click_Opens_Window \
                .triggered.connect(partial(self._select_track_click_window_dialog, view))

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

        if isinstance(view, DictionarySearchWindowInterface):
            if hasattr(view, 'action_Show_Sidebar'):
                is_on = self._app_data.app_settings.get('show_dictionary_sidebar', True)
                view.action_Show_Sidebar.setChecked(is_on)

                view.action_Show_Sidebar \
                    .triggered.connect(partial(self._toggle_show_dictionary_sidebar, view))

        if isinstance(view, SuttaSearchWindowInterface):
            if hasattr(view, 'action_Show_Sidebar'):
                is_on = self._app_data.app_settings.get('show_sutta_sidebar', True)
                view.action_Show_Sidebar.setChecked(is_on)

                view.action_Show_Sidebar \
                    .triggered.connect(partial(self._toggle_show_sutta_sidebar, view))

        if isinstance(view, EbookReaderWindowInterface) \
           or isinstance(view, SuttaSearchWindowInterface):

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

            if hasattr(view, 'action_Show_Bookmarks'):
                is_on = self._app_data.app_settings.get('show_bookmarks', True)
                view.action_Show_Bookmarks.setChecked(is_on)

                view.action_Show_Bookmarks \
                    .triggered.connect(partial(self._toggle_show_bookmarks, view))

            if hasattr(view, 'action_Generate_Links_Graph'):
                is_on = self._app_data.app_settings.get('generate_links_graph', False)
                view.action_Generate_Links_Graph.setChecked(is_on)

                view.action_Generate_Links_Graph \
                    .triggered.connect(partial(self._toggle_generate_links_graph, view))

        notify = self._app_data.app_settings.get('notify_about_simsapa_updates', True)

        if hasattr(view, 'action_Notify_About_Simsapa_Updates'):
            view.action_Notify_About_Simsapa_Updates.setChecked(notify)
            view.action_Notify_About_Simsapa_Updates \
                .triggered.connect(partial(self._set_notify_simsapa_setting, view))

        if hasattr(view, 'action_Notify_About_DPD_Updates'):
            view.action_Notify_About_DPD_Updates.setChecked(notify)
            view.action_Notify_About_DPD_Updates \
                .triggered.connect(partial(self._set_notify_dpd_setting, view))

        if hasattr(view, 'action_Website'):
            view.action_Website \
                .triggered.connect(partial(open_simsapa_website))

        if hasattr(view, 'action_About'):
            view.action_About \
                .triggered.connect(partial(show_about, view))

        show_toolbar = self._app_data.app_settings.get('show_toolbar', False)

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
                .triggered.connect(partial(self._set_double_click_word_lookup_setting, view))

            checked = self._app_data.app_settings.get('double_click_word_lookup', True)
            view.action_Double_Click_on_a_Word_for_Dictionary_Lookup.setChecked(checked)

        if hasattr(view, 'action_Clipboard_Monitoring_for_Dictionary_Lookup'):
            view.action_Clipboard_Monitoring_for_Dictionary_Lookup \
                .triggered.connect(partial(self._set_clipboard_monitoring_for_dict_setting, view))

            checked = self._app_data.app_settings.get('clipboard_monitoring_for_dict', True)
            view.action_Clipboard_Monitoring_for_Dictionary_Lookup.setChecked(checked)

        if hasattr(view, 'action_Check_for_Simsapa_Updates'):
            view.action_Check_for_Simsapa_Updates \
                .triggered.connect(partial(self._handle_check_simsapa_updates))

        if hasattr(view, 'action_Check_for_DPD_Updates'):
            view.action_Check_for_DPD_Updates \
                .triggered.connect(partial(self._handle_check_dpd_updates))

        # s = os.getenv('ENABLE_WIP_FEATURES')
        # if s is not None and s.lower() == 'true':
        #     logger.info("no wip features")
        #     # view.action_Dictionaries_Manager \
        #     #     .triggered.connect(partial(self._new_dictionaries_manager_window))
        #     #
        #     # view.action_Document_Reader \
        #     #     .triggered.connect(partial(self._new_document_reader_window))
        #     # view.action_Library \
        #     #     .triggered.connect(partial(self._new_library_browser_window))
        #     #
        #     # try:
        #     #     view.action_Open_Selected \
        #     #         .triggered.connect(partial(self._open_selected_document, view))
        #     # except Exception as e:
        #     #     logger.error(e)
        #     #
        # else:

        if hasattr(view, 'action_Pali_Courses'):
            view.action_Pali_Courses.setVisible(False)

        if hasattr(view, 'action_Links'):
            view.action_Links.setVisible(False)

        if hasattr(view, 'action_Open'):
            view.action_Open.setVisible(False)

        if hasattr(view, 'action_Dictionaries_Manager'):
            view.action_Dictionaries_Manager.setVisible(False)

        if hasattr(view, 'action_Document_Reader'):
            view.action_Document_Reader.setVisible(False)

        if hasattr(view, 'action_Library'):
            view.action_Library.setVisible(False)

def make_active_window(view: QMainWindow):
    view.show() # bring window to top on OSX
    view.raise_() # bring window from minimized state on OSX
    view.activateWindow() # bring window to front/unminimize on Windows

def show_work_in_progress():
    d = QMessageBox()
    d.setWindowTitle("Work in Progress")
    d.setText("Work in Progress")
    d.exec()

