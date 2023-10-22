import json
import queue

from functools import partial
from typing import Callable, List, Optional

from PyQt6 import QtCore, QtWidgets
from PyQt6.QtGui import QCloseEvent
from PyQt6.QtCore import QTimer, QUrl, pyqtSignal
from PyQt6.QtWidgets import QHBoxLayout, QSpacerItem, QSplitter, QToolBar, QVBoxLayout, QWidget

from simsapa import APP_QUEUES, ApiAction, ApiMessage, TIMER_SPEED, logger
from simsapa.assets.ui.sutta_study_window_ui import Ui_SuttaStudyWindow

from simsapa.app.types import QueryType, USutta
from simsapa.app.app_data import AppData
from simsapa.app.search.dictionary_queries import DictionaryQueries

from simsapa.layouts.gui_types import SuttaStudyWindowInterface
from simsapa.layouts.preview_window import PreviewWindow
from simsapa.layouts.sutta_search import SuttaSearchWindowState
from simsapa.layouts.word_lookup import WordLookupState

CSS_EXTRA = "html { font-size: 14px; }"

class SuttaStudyWindow(SuttaStudyWindowInterface, Ui_SuttaStudyWindow):

    splitter: QSplitter
    sutta_one_layout: QVBoxLayout
    sutta_two_layout: QVBoxLayout
    dict_layout: QVBoxLayout
    _show_sutta: Callable

    show_sutta_by_url = pyqtSignal(QUrl)
    link_mouseover = pyqtSignal(dict)
    link_mouseleave = pyqtSignal(str)
    page_dblclick = pyqtSignal()
    hide_preview = pyqtSignal()

    lookup_in_dictionary_signal = pyqtSignal(str)

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("SuttaStudyWindow()")

        self.features: List[str] = []
        self._app_data: AppData = app_data

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self.page_len = 20

        self.queries = DictionaryQueries(self._app_data.db_session, self._app_data.api_url)

        self._setup_ui()
        self._connect_signals()

    def handle_messages(self):
        if self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                msg: ApiMessage = json.loads(s)

                if msg['action'] == ApiAction.open_in_study_window:
                    info = json.loads(msg['data'])
                    self._show_sutta_by_uid_in_side(uid = info['uid'],
                                                    side = info['side'])

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _show_sutta_by_uid_in_side(self, uid: str, side: str):
        if side == 'left':
            self.sutta_one_state._show_sutta_by_uid(uid)

        if side == 'middle':
            self.sutta_two_state._show_sutta_by_uid(uid)

    def _open_in_study_window(self, side: str, sutta: Optional[USutta]):
        if sutta is None:
            return

        uid: str = sutta.uid
        self._show_sutta_by_uid_in_side(side, uid)

    def _setup_ui(self):
        self.setWindowTitle("Sutta Study - Simsapa")
        show = self._app_data.app_settings.get('show_related_suttas', True)
        self.action_Show_Related_Suttas.setChecked(show)

        # Setup the splitter and the three columns.

        # One

        self.splitter = QSplitter(self.central_widget)
        # Allow the splitter to be squeezed on small screens.
        self.splitter.setMinimumWidth(800)
        self.splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)

        self.sutta_one_layout_widget = QWidget(self.splitter)

        self.sutta_one_layout = QVBoxLayout(self.sutta_one_layout_widget)
        self.sutta_one_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(100, 0, QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Minimum)
        self.sutta_one_layout.addItem(spacer)

        self.sutta_one_searchbar_layout = QHBoxLayout()
        self.sutta_one_layout.addLayout(self.sutta_one_searchbar_layout)

        self.sutta_one_tabs_layout = QVBoxLayout()
        self.sutta_one_layout.addLayout(self.sutta_one_tabs_layout)

        self.sutta_one_state = SuttaSearchWindowState(app_data=self._app_data,
                                                      parent_window=self,
                                                      searchbar_layout=self.sutta_one_searchbar_layout,
                                                      sutta_tabs_layout=self.sutta_one_tabs_layout,
                                                      tabs_layout=None,
                                                      focus_input=False,
                                                      enable_language_filter=True,
                                                      enable_search_extras=True,
                                                      enable_info_button=False,
                                                      enable_sidebar=False,
                                                      enable_find_panel=True,
                                                      create_find_toolbar=False,
                                                      show_query_results_in_active_tab=True,
                                                      search_bar_two_rows_layout=True,
                                                      language_filter_setting_key = 'sutta_study_one_language_filter_idx',
                                                      search_mode_setting_key = 'sutta_study_one_search_mode',
                                                      source_filter_setting_key = 'sutta_study_one_source_filter_idx')

        self.sutta_one_state.page_dblclick.connect(partial(self.sutta_one_state._lookup_selection_in_dictionary))

        # Two

        self.sutta_two_layout_widget = QWidget(self.splitter)

        self.sutta_two_layout = QVBoxLayout(self.sutta_two_layout_widget)
        self.sutta_two_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(100, 0, QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Minimum)
        self.sutta_two_layout.addItem(spacer)

        self.main_layout.addWidget(self.splitter)

        self.sutta_two_searchbar_layout = QHBoxLayout()
        self.sutta_two_layout.addLayout(self.sutta_two_searchbar_layout)

        self.sutta_two_tabs_layout = QVBoxLayout()
        self.sutta_two_layout.addLayout(self.sutta_two_tabs_layout)

        self.sutta_two_state = SuttaSearchWindowState(app_data=self._app_data,
                                                      parent_window=self,
                                                      searchbar_layout=self.sutta_two_searchbar_layout,
                                                      sutta_tabs_layout=self.sutta_two_tabs_layout,
                                                      tabs_layout=None,
                                                      focus_input=False,
                                                      enable_language_filter=True,
                                                      enable_search_extras=True,
                                                      enable_info_button=False,
                                                      enable_sidebar=False,
                                                      enable_find_panel=True,
                                                      create_find_toolbar=False,
                                                      show_query_results_in_active_tab=True,
                                                      search_bar_two_rows_layout=True,
                                                      language_filter_setting_key = 'sutta_study_two_language_filter_idx',
                                                      search_mode_setting_key = 'sutta_study_two_search_mode',
                                                      source_filter_setting_key = 'sutta_study_two_source_filter_idx')

        self.sutta_two_state.page_dblclick.connect(partial(self.sutta_two_state._lookup_selection_in_dictionary))

        # Focus the first input field

        self.sutta_one_state.search_input.setFocus()

        # Setup the two sutta layouts.

        # Setup the dictionary search.

        self.dictionary_layout_widget = QWidget(self.splitter)

        self.dictionary_layout = QVBoxLayout(self.dictionary_layout_widget)
        self.dictionary_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(100, 0, QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Minimum)
        self.dictionary_layout.addItem(spacer)

        self.dictionary_state = WordLookupState(app_data = self._app_data,
                                                wrap_layout = self.dictionary_layout,
                                                focus_input = False,
                                                enable_find_panel = True,
                                                language_filter_setting_key = 'sutta_study_lookup_language_filter_idx',
                                                search_mode_setting_key = 'sutta_study_lookup_search_mode',
                                                source_filter_setting_key = 'sutta_study_lookup_source_filter_idx')

        self.dictionary_state.show_sutta_by_url.connect(partial(self._show_sutta_by_url))
        self.dictionary_state.show_words_by_url.connect(partial(self._show_words_by_url))

        def _dict(text: str):
            self.dictionary_state.lookup_in_dictionary(text, show_results_tab = True, include_exact_query = False)

        # Sutta states emit this signal via parent window's handle
        self.lookup_in_dictionary_signal.connect(partial(_dict))

        # Tab order for input fields

        self.setTabOrder(self.sutta_one_state.search_input, self.sutta_two_state.search_input)
        self.setTabOrder(self.sutta_two_state.search_input, self.dictionary_state.search_input)

        # Create the shared find toolbar.

        self.find_toolbar = QToolBar()
        self.find_panel_layout = QHBoxLayout()
        self.find_panel_layout.setContentsMargins(0, 0, 0, 0)

        for panel in [self.sutta_one_state._find_panel,
                      self.sutta_two_state._find_panel,
                      self.dictionary_state._find_panel]:

            self.find_panel_layout.addWidget(panel)
            panel.closed.connect(self.find_toolbar.hide)

        self.find_panel_widget = QWidget()
        self.find_panel_widget.setLayout(self.find_panel_layout)

        self.find_toolbar.addWidget(self.find_panel_widget)

        self.addToolBar(QtCore.Qt.ToolBarArea.BottomToolBarArea, self.find_toolbar)
        self.find_toolbar.hide()

    def _show_url(self, url: QUrl):
        if url.host() == QueryType.suttas:
            self._show_sutta_by_url(url)

        elif url.host() == QueryType.words:
            self._show_words_by_url(url)

    def _show_words_by_url(self, url: QUrl):
        if url.host() != QueryType.words:
            return

        self.dictionary_state._show_word_by_url(url)

    def _show_sutta_by_url(self, url: QUrl):
        if url.host() != QueryType.suttas:
            return

        self.show_sutta_by_url.emit(url)

    def start_loading_animation(self):
        pass

    def stop_loading_animation(self):
        pass

    def _lookup_clipboard_in_suttas(self):
        self.activateWindow()
        s = self._app_data.clipboard_getText()
        if s is not None:
            self.sutta_one_state._set_query(s)
            self.sutta_one_state._handle_query()

    def _lookup_clipboard_in_dictionary(self):
        text = self._app_data.clipboard_getText()
        if text is not None:
            self.dictionary_state._set_query(text)
            self.dictionary_state._handle_query()

    def _lookup_selection_in_suttas(self):
        self.activateWindow()
        text = self.sutta_one_state._get_selection()
        if text is not None:
            self.sutta_one_state._set_query(text)
            self.sutta_one_state._handle_query()
        else:
            text = self.sutta_two_state._get_selection()
            if text is not None:
                self.sutta_two_state._set_query(text)
                self.sutta_two_state._handle_query()

    def _lookup_selection_in_dictionary(self, show_results_tab = False, include_exact_query = True):
        text = self.sutta_one_state._get_selection()
        if text is None:
            text = self.sutta_two_state._get_selection()

        if text is not None:
            self.lookup_in_dictionary(text, show_results_tab, include_exact_query)

    def lookup_in_dictionary(self, query: str, show_results_tab = False, include_exact_query = True):
        self.dictionary_state.lookup_in_dictionary(query, show_results_tab, include_exact_query)

    def _handle_copy(self):
        text = self.sutta_one_state._get_selection()
        if text is None:
            text = self.sutta_two_state._get_selection()

        if text is not None:
            self._app_data.clipboard_setText(text)

    def _focus_search_input(self):
        self.sutta_one_state.search_input.setFocus()

    def reload_sutta_pages(self):
        self.sutta_one_state.reload_page()
        self.sutta_two_state.reload_page()

    def _increase_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        self._app_data.app_settings['sutta_font_size'] = font_size + 2

        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        self._app_data.app_settings['dictionary_font_size'] = font_size + 2

        self._app_data._save_app_settings()

        self.sutta_one_state._get_active_tab().render_sutta_content()
        self.sutta_two_state._get_active_tab().render_sutta_content()
        self.dictionary_state._render_words(self.dictionary_state._current_words)

    def _decrease_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        if font_size >= 5:
            self._app_data.app_settings['sutta_font_size'] = font_size - 2
            self._app_data._save_app_settings()

            self.sutta_one_state._get_active_tab().render_sutta_content()
            self.sutta_two_state._get_active_tab().render_sutta_content()

        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        if font_size >= 5:
            self._app_data.app_settings['dictionary_font_size'] = font_size - 2
            self._app_data._save_app_settings()
            self.dictionary_state._render_words(self.dictionary_state._current_words)

    def _increase_text_margins(self):
        # increase margins = smaller max with
        max_width = self._app_data.app_settings.get('sutta_max_width', 75)
        if max_width < 10:
            return
        self._app_data.app_settings['sutta_max_width'] = max_width - 2
        self._app_data._save_app_settings()

        self.sutta_one_state._get_active_tab().render_sutta_content()
        self.sutta_two_state._get_active_tab().render_sutta_content()

    def _decrease_text_margins(self):
        # decrease margins = greater max with
        max_width = self._app_data.app_settings.get('sutta_max_width', 75)
        self._app_data.app_settings['sutta_max_width'] = max_width + 2
        self._app_data._save_app_settings()

        self.sutta_one_state._get_active_tab().render_sutta_content()
        self.sutta_two_state._get_active_tab().render_sutta_content()

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.dictionary_state.link_mouseover.connect(partial(preview_window.link_mouseover))
        self.dictionary_state.link_mouseleave.connect(partial(preview_window.link_mouseleave))
        self.dictionary_state.hide_preview.connect(partial(preview_window._do_hide))

    def _handle_show_find_panel(self):
        self.find_toolbar.show()
        self.sutta_one_state._find_panel.search_input.setFocus()

    def closeEvent(self, event: QCloseEvent):
        if self.queue_id in APP_QUEUES.keys():
            del APP_QUEUES[self.queue_id]

        msg = ApiMessage(queue_id = 'app_windows',
                         action = ApiAction.remove_closed_window_from_list,
                         data = self.queue_id)
        s = json.dumps(msg)
        APP_QUEUES['app_windows'].put_nowait(s)

        event.accept()

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self._handle_close))

        self.action_Copy \
            .triggered.connect(partial(self._handle_copy))

        self.action_Lookup_Selection_in_Dictionary \
            .triggered.connect(partial(self._lookup_selection_in_dictionary))

        self.action_Reload_Page \
            .triggered.connect(partial(self.reload_sutta_pages))

        self.action_Increase_Text_Size \
            .triggered.connect(partial(self._increase_text_size))

        self.action_Decrease_Text_Size \
            .triggered.connect(partial(self._decrease_text_size))

        self.action_Increase_Text_Margins \
            .triggered.connect(partial(self._increase_text_margins))

        self.action_Decrease_Text_Margins \
            .triggered.connect(partial(self._decrease_text_margins))

        self.action_Find_in_Page \
            .triggered.connect(self._handle_show_find_panel)
