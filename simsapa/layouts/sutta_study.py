import json
import queue

from functools import partial
from typing import Callable, List, Optional

from PyQt6 import QtCore
from PyQt6.QtGui import QAction, QCloseEvent, QKeySequence, QShortcut
from PyQt6.QtCore import QTimer, QUrl, pyqtSignal
from PyQt6.QtWidgets import QHBoxLayout, QSpacerItem, QSplitter, QToolBar, QVBoxLayout, QWidget

from simsapa import APP_QUEUES, ApiAction, ApiMessage, TIMER_SPEED, QuoteScope, SuttaQuote, logger, QueryType
from simsapa.assets.ui.sutta_study_window_ui import Ui_SuttaStudyWindow

from simsapa.app.types import LookupPanelParams, SuttaPanelParams, SuttaStudyParams, USutta, default_lookup_panel_params, default_sutta_panel_params
from simsapa.app.app_data import AppData
from simsapa.app.search.dictionary_queries import DictionaryQueries

from simsapa.layouts.gui_types import QExpanding, QMinimum, SuttaPanel, SuttaPanelSettingKeys, SuttaStudyWindowInterface, WindowType
from simsapa.layouts.preview_window import PreviewWindow
from simsapa.layouts.sutta_search import SuttaSearchWindowState
from simsapa.layouts.word_lookup import WordLookupState
from simsapa.layouts.parts.restore_size_pos import HasRestoreSizePos

CSS_EXTRA = "html { font-size: 14px; }"

class SuttaStudyWindow(SuttaStudyWindowInterface, HasRestoreSizePos, Ui_SuttaStudyWindow):
    splitter: QSplitter
    sutta_panels: List[SuttaPanel]
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

        self.lookup_on_side = self._app_data.app_settings.get('sutta_study_lookup_on_side', True)

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

    def _show_sutta_by_uid_in_side(self,
                                   uid: str,
                                   side: str,
                                   sutta_quote: Optional[SuttaQuote] = None,
                                   quote_scope = QuoteScope.Sutta):
        if side == 'panel_one':
            self.sutta_panels[0]['state']._show_sutta_by_uid(uid, sutta_quote, quote_scope)

        elif side == 'panel_two':
            self.sutta_panels[1]['state']._show_sutta_by_uid(uid, sutta_quote, quote_scope)

        elif side == 'panel_three':
            self.sutta_panels[2]['state']._show_sutta_by_uid(uid, sutta_quote, quote_scope)

    def _open_in_study_window(self, side: str, sutta: Optional[USutta]):
        if sutta is None:
            return

        uid: str = sutta.uid
        self._show_sutta_by_uid_in_side(side, uid)

    def _setup_ui(self):
        self.setWindowTitle("Sutta Study - Simsapa")
        show = self._app_data.app_settings.get('show_related_suttas', True)
        self.action_Show_Related_Suttas.setChecked(show)

        # Setup a vertical two-row splitter, to optionally place the dictionary at the bottom.

        self.vert_splitter = QSplitter(self.central_widget)
        self.vert_splitter.setHandleWidth(10)
        self.vert_splitter.setMinimumHeight(200)
        self.vert_splitter.setOrientation(QtCore.Qt.Orientation.Vertical)

        self.main_layout.addWidget(self.vert_splitter)

        # Setup a four-column splitter: three columns for suttas and one for a dictionary.

        self.horiz_splitter = QSplitter(self.vert_splitter)
        self.horiz_splitter.setHandleWidth(10)

        # Allow the splitter to be squeezed on small screens.
        self.horiz_splitter.setMinimumWidth(200)
        self.horiz_splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)

        self.sutta_panels: List[SuttaPanel] = []

        settings_keys = [
            SuttaPanelSettingKeys(
                language_include_setting_key = 'sutta_study_one_language_include_is_on',
                language_filter_setting_key = 'sutta_study_one_language_filter_idx',
                search_mode_setting_key = 'sutta_study_one_search_mode',
                source_include_setting_key = 'sutta_study_one_source_include_is_on',
                source_filter_setting_key = 'sutta_study_one_source_filter_idx',
            ),
            SuttaPanelSettingKeys(
                language_include_setting_key = 'sutta_study_two_language_include_is_on',
                language_filter_setting_key = 'sutta_study_two_language_filter_idx',
                search_mode_setting_key = 'sutta_study_two_search_mode',
                source_include_setting_key = 'sutta_study_two_source_include_is_on',
                source_filter_setting_key = 'sutta_study_two_source_filter_idx',
            ),
            SuttaPanelSettingKeys(
                language_include_setting_key = 'sutta_study_three_language_include_is_on',
                language_filter_setting_key = 'sutta_study_three_language_filter_idx',
                search_mode_setting_key = 'sutta_study_three_search_mode',
                source_include_setting_key = 'sutta_study_three_source_include_is_on',
                source_filter_setting_key = 'sutta_study_three_source_filter_idx',
            ),
        ]

        for panel_idx in [0, 1, 2]:

            layout_widget = QWidget(self.horiz_splitter)

            layout = QVBoxLayout(layout_widget)
            layout.setContentsMargins(0, 0, 0, 0)

            spacer = QSpacerItem(100, 0, QExpanding, QMinimum)
            layout.addItem(spacer)

            searchbar_layout = QHBoxLayout()
            layout.addLayout(searchbar_layout)

            tabs_layout = QVBoxLayout()
            layout.addLayout(tabs_layout)

            state = SuttaSearchWindowState(app_data=self._app_data,
                                           parent_window=self,
                                           searchbar_layout=searchbar_layout,
                                           sutta_tabs_layout=tabs_layout,
                                           tabs_layout=None,
                                           focus_input=False,
                                           enable_nav_buttons=False,
                                           enable_language_filter=True,
                                           enable_search_extras=True,
                                           enable_regex_fuzzy=False,
                                           enable_info_button=False,
                                           enable_sidebar_button=False,
                                           enable_sidebar=False,
                                           enable_find_panel=True,
                                           create_find_toolbar=False,
                                           show_query_results_in_active_tab=True,
                                           search_bar_two_rows_layout=True,
                                           language_filter_setting_key = settings_keys[panel_idx]['language_filter_setting_key'],
                                           search_mode_setting_key = settings_keys[panel_idx]['search_mode_setting_key'],
                                           source_filter_setting_key = settings_keys[panel_idx]['source_filter_setting_key'])

            state.page_dblclick.connect(partial(state._lookup_selection_in_dictionary))

            self.sutta_panels.append(
                SuttaPanel(
                    layout_widget = layout_widget,
                    layout = layout,
                    searchbar_layout = searchbar_layout,
                    tabs_layout = tabs_layout,
                    state = state,
                    setting_keys = settings_keys[panel_idx],
                )
            )

        # Focus the first input field

        self.sutta_panels[0]['state'].search_input.setFocus()

        # Setup the dictionary search.

        if self.lookup_on_side:
            self.dictionary_layout_widget = QWidget(self.horiz_splitter)
        else:
            self.dictionary_layout_widget = QWidget(self.vert_splitter)

        self.dictionary_layout = QVBoxLayout(self.dictionary_layout_widget)
        self.dictionary_layout.setContentsMargins(0, 0, 0, 0)

        self.dictionary_layout.addItem(QSpacerItem(100, 0, QExpanding, QMinimum))

        self.dictionary_state = WordLookupState(app_data = self._app_data,
                                                parent_window = self,
                                                central_widget = self.central_widget,
                                                wrap_layout = self.dictionary_layout,
                                                focus_input = False,
                                                enable_regex_fuzzy = False,
                                                enable_find_panel = True,
                                                language_filter_setting_key = 'sutta_study_lookup_language_filter_idx',
                                                search_mode_setting_key = 'sutta_study_lookup_search_mode',
                                                source_filter_setting_key = 'sutta_study_lookup_source_filter_idx')

        self.dictionary_state.show_sutta_by_url.connect(partial(self._show_sutta_by_url))
        self.dictionary_state.show_words_by_url.connect(partial(self._show_words_by_url))

        def _dict(text: str):
            self.dictionary_state.lookup_in_dictionary(text, show_results_tab = False)

        # Sutta states emit this signal via parent window's handle
        self.lookup_in_dictionary_signal.connect(partial(_dict))

        # Tab order for input fields

        for idx in [0, len(self.sutta_panels)-2]:
            self.setTabOrder(self.sutta_panels[idx]['state'].search_input, self.sutta_panels[idx+1]['state'].search_input)

        last_idx = len(self.sutta_panels)-1
        self.setTabOrder(self.sutta_panels[last_idx]['state'].search_input, self.dictionary_state.search_input)

        # Create the shared find toolbar.

        self.find_toolbar = QToolBar(self)
        self.find_panel_layout = QHBoxLayout()
        self.find_panel_layout.setContentsMargins(0, 0, 0, 0)

        def _show_find_panel_with_text(text = ''):
            logger.info(f"sutta_study::_handle_show_find_panel(): {text}")
            self.find_toolbar.show()
            self.sutta_panels[0]['state']._find_panel.search_input.setFocus()
            self.sutta_panels[0]['state']._find_panel.search_input.setText(text)

        # show_find_panel = pyqtSignal(str)
        # the signal will send a text argument
        self.sutta_panels[0]['state'].show_find_panel.connect(partial(_show_find_panel_with_text))

        for panel in self.sutta_panels:
            self.find_panel_layout.addWidget(panel['state']._find_panel)
            panel['state']._find_panel.closed.connect(self.find_toolbar.hide)

        self.find_panel_layout.addWidget(self.dictionary_state._find_panel)
        self.dictionary_state._find_panel.closed.connect(self.find_toolbar.hide)

        self.find_panel_widget = QWidget()
        self.find_panel_widget.setLayout(self.find_panel_layout)

        self.find_toolbar.addWidget(self.find_panel_widget)

        self.addToolBar(QtCore.Qt.ToolBarArea.BottomToolBarArea, self.find_toolbar)
        self.find_toolbar.hide()

        # Set splitter sizes to max, thrid panel is hidden by default.

        if self.lookup_on_side:
            self.vert_splitter.setSizes([2000, 0])
            self.horiz_splitter.setSizes([2000, 2000, 0, 2000])
        else:
            self.vert_splitter.setSizes([2000, 2000])
            self.horiz_splitter.setSizes([2000, 2000, 0])

        self.sutta_panels[2]['state']._find_panel.setVisible(False)

        # Create the panel toggle toolbar.

        self.panel_toggle_toolbar = QToolBar(self)
        self.addToolBar(QtCore.Qt.ToolBarArea.LeftToolBarArea, self.panel_toggle_toolbar)
        self.panel_toggle_toolbar.show()

        self.action_toggle_panel_one = QAction("1")
        self.action_toggle_panel_one.setCheckable(True)
        self.action_toggle_panel_one.setChecked(True)
        self.action_toggle_panel_one.setShortcut(QKeySequence("Ctrl+1"))
        self.panel_toggle_toolbar.addAction(self.action_toggle_panel_one)

        def _toggle_one():
            is_on = self.action_toggle_panel_one.isChecked()
            sizes = self._get_maxed_horiz_splitter_sizes()
            if is_on:
                sizes[0] = 2000
                self.sutta_panels[0]['state'].search_input.setFocus()
                self.sutta_panels[0]['state']._find_panel.setVisible(True)
            else:
                sizes[0] = 0
                self.sutta_panels[0]['state']._find_panel.setVisible(False)

            self.horiz_splitter.setSizes(sizes)

        self.action_toggle_panel_one.triggered.connect(partial(_toggle_one))

        self.action_toggle_panel_two = QAction("2")
        self.action_toggle_panel_two.setCheckable(True)
        self.action_toggle_panel_two.setChecked(True)
        self.action_toggle_panel_two.setShortcut(QKeySequence("Ctrl+2"))
        self.panel_toggle_toolbar.addAction(self.action_toggle_panel_two)

        def _toggle_two():
            is_on = self.action_toggle_panel_two.isChecked()
            sizes = self._get_maxed_horiz_splitter_sizes()
            if is_on:
                sizes[1] = 2000
                self.sutta_panels[1]['state'].search_input.setFocus()
                self.sutta_panels[1]['state']._find_panel.setVisible(True)
            else:
                sizes[1] = 0
                self.sutta_panels[1]['state']._find_panel.setVisible(False)

            self.horiz_splitter.setSizes(sizes)

        self.action_toggle_panel_two.triggered.connect(partial(_toggle_two))

        self.action_toggle_panel_three = QAction("3")
        self.action_toggle_panel_three.setCheckable(True)
        self.action_toggle_panel_three.setChecked(False)
        self.action_toggle_panel_three.setShortcut(QKeySequence("Ctrl+3"))
        self.panel_toggle_toolbar.addAction(self.action_toggle_panel_three)

        def _toggle_three():
            is_on = self.action_toggle_panel_three.isChecked()
            sizes = self._get_maxed_horiz_splitter_sizes()
            if is_on:
                sizes[2] = 2000
                self.sutta_panels[2]['state'].search_input.setFocus()
                self.sutta_panels[2]['state']._find_panel.setVisible(True)
            else:
                sizes[2] = 0
                self.sutta_panels[2]['state']._find_panel.setVisible(False)

            self.horiz_splitter.setSizes(sizes)

        self.action_toggle_panel_three.triggered.connect(partial(_toggle_three))

        self.action_toggle_panel_dict = QAction("D")
        self.action_toggle_panel_dict.setCheckable(True)
        self.action_toggle_panel_dict.setChecked(True)
        self.action_toggle_panel_dict.setShortcut(QKeySequence("Ctrl+D"))
        self.panel_toggle_toolbar.addAction(self.action_toggle_panel_dict)

        def _toggle_dict():
            is_on = self.action_toggle_panel_dict.isChecked()

            if self.lookup_on_side:
                sizes = self._get_maxed_horiz_splitter_sizes()
                if is_on:
                    sizes[3] = 2000
                    self.dictionary_state.search_input.setFocus()
                    self.dictionary_state._find_panel.setVisible(True)
                else:
                    sizes[3] = 0
                    self.dictionary_state._find_panel.setVisible(False)

                self.horiz_splitter.setSizes(sizes)

            else:
                sizes = [2000, 0]
                if is_on:
                    sizes[1] = 2000
                    self.dictionary_state.search_input.setFocus()
                    self.dictionary_state._find_panel.setVisible(True)
                else:
                    sizes[1] = 0
                    self.dictionary_state._find_panel.setVisible(False)

                self.vert_splitter.setSizes(sizes)

        self.action_toggle_panel_dict.triggered.connect(partial(_toggle_dict))

    def _get_maxed_horiz_splitter_sizes(self) -> List[int]:
        sizes = []
        for i in self.horiz_splitter.sizes():
            if i > 0:
                sizes.append(2000)
            else:
                sizes.append(0)

        return sizes

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
            self.sutta_panels[0]['state']._set_query(s)
            self.sutta_panels[0]['state']._handle_query()

    def _lookup_clipboard_in_dictionary(self):
        text = self._app_data.clipboard_getText()
        if text is not None:
            self.dictionary_state._set_query(text)
            self.dictionary_state._handle_query()

    def _lookup_selection_in_suttas(self):
        self.activateWindow()
        for panel in self.sutta_panels:
            text = panel['state']._get_selection()
            if text is not None:
                panel['state']._set_query(text)
                panel['state']._handle_query()
                return

    def _lookup_selection_in_dictionary(self, show_results_tab = False):
        for panel in self.sutta_panels:
            text = panel['state']._get_selection()
            if text is not None:
                self.lookup_in_dictionary(text, show_results_tab)
                return

    def lookup_in_dictionary(self, query: str, show_results_tab = False):
        self.dictionary_state.lookup_in_dictionary(query, show_results_tab)

    def apply_params(self, params: SuttaStudyParams):
        if 'sutta_panels' not in params.keys():
            params['sutta_panels'] = []

        if len(params['sutta_panels']) != 0:
            for idx, d in enumerate(params['sutta_panels'][0:3]):
                sutta_p: SuttaPanelParams = {**default_sutta_panel_params(), **d}

                self.sutta_panels[idx]['state']._set_query(sutta_p['query_text'])
                self.sutta_panels[idx]['state']._handle_query()

                if sutta_p['find_text'] == "":
                    sutta_quote = None
                else:
                    sutta_quote = SuttaQuote(quote=sutta_p['find_text'], selection_range=None)

                self.sutta_panels[idx]['state']._show_sutta_by_uid(sutta_p['sutta_uid'], sutta_quote)

        if 'lookup_panel' not in params.keys():
            lookup_p = default_lookup_panel_params()
        else:
            lookup_p: LookupPanelParams = {**default_lookup_panel_params(), **params['lookup_panel']}

        self.dictionary_state.lookup_in_dictionary(lookup_p['query_text'], lookup_p['show_results_tab'])

    def _handle_copy(self):
        for panel in self.sutta_panels:
            text = panel['state']._get_selection()
            if text is not None:
                self._app_data.clipboard_setText(text)
                return

        text = self.dictionary_state._get_selection()
        if text is not None:
            self._app_data.clipboard_setText(text)
            return

    def _focus_search_input(self):
        # Action on Ctrl + L
        self._focus_sutta_search_input()

    def _focus_sutta_search_input(self):
        # Cycle the sutta search inputs if one was already selected
        # Or select the first one
        selected_idx: Optional[int] = None
        for idx, panel in enumerate(self.sutta_panels):
            if panel['state'].search_input.hasFocus():
                selected_idx = idx
                break

        if selected_idx is None:
            self.sutta_panels[0]['state'].search_input.setFocus()

        else:
            visible_indexes = []
            focused_visible_index_list_pos = 0
            splitter_sizes = self.horiz_splitter.sizes()
            for idx, panel in enumerate(self.sutta_panels):
                if splitter_sizes[idx] > 0:
                    visible_indexes.append(idx)
                    if self.sutta_panels[idx]['state'].search_input.hasFocus():
                        focused_visible_index_list_pos = len(visible_indexes)-1

            if len(visible_indexes) == 0:
                return

            elif len(visible_indexes) == 1:
                idx = visible_indexes[0]
                self.sutta_panels[idx]['state'].search_input.setFocus()
                return

            next_visible_list_pos = focused_visible_index_list_pos+1
            if next_visible_list_pos > len(visible_indexes)-1:
                next_visible_list_pos = 0

            next_panel_idx = visible_indexes[next_visible_list_pos]
            self.sutta_panels[next_panel_idx]['state'].search_input.setFocus()

    def _focus_dict_search_input(self):
        # Action on Ctrl + ;
        self.dictionary_state.search_input.setFocus()

    def reload_sutta_pages(self):
        for panel in self.sutta_panels:
            panel['state'].reload_page()

    def _increase_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        self._app_data.app_settings['sutta_font_size'] = font_size + 2

        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        self._app_data.app_settings['dictionary_font_size'] = font_size + 2

        self._app_data._save_app_settings()

        for panel in self.sutta_panels:
            panel['state']._get_active_tab().render_sutta_content()

        self.dictionary_state._render_words(self.dictionary_state._current_words)

    def _decrease_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        if font_size >= 5:
            self._app_data.app_settings['sutta_font_size'] = font_size - 2
            self._app_data._save_app_settings()

            for panel in self.sutta_panels:
                panel['state']._get_active_tab().render_sutta_content()

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

        for panel in self.sutta_panels:
            panel['state']._get_active_tab().render_sutta_content()

    def _decrease_text_margins(self):
        # decrease margins = greater max with
        max_width = self._app_data.app_settings.get('sutta_max_width', 75)
        self._app_data.app_settings['sutta_max_width'] = max_width + 2
        self._app_data._save_app_settings()

        for panel in self.sutta_panels:
            panel['state']._get_active_tab().render_sutta_content()

    def _show_search_result_sizes_dialog(self):
        from simsapa.layouts.search_result_sizes_dialog import SearchResultSizesDialog
        d = SearchResultSizesDialog(self._app_data, self)
        if d.exec():
            self.dictionary_state.render_fulltext_page()

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.dictionary_state.link_mouseover.connect(partial(preview_window.link_mouseover))
        self.dictionary_state.link_mouseleave.connect(partial(preview_window.link_mouseleave))
        self.dictionary_state.hide_preview.connect(partial(preview_window._do_hide))

    def _handle_show_find_panel(self):
        self.find_toolbar.show()
        self.sutta_panels[0]['state']._find_panel.search_input.setFocus()

    def closeEvent(self, event: QCloseEvent):
        self._app_data.save_last_closed_window(WindowType.SuttaStudy)
        self.save_size_pos()

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

        self.action_Search_Result_Sizes \
            .triggered.connect(partial(self._show_search_result_sizes_dialog))

        self.action_Find_in_Page \
            .triggered.connect(partial(self._handle_show_find_panel))

        self.action_Focus_Dict_Search_Input = QShortcut(QKeySequence("Ctrl+;"), self)
        self.action_Focus_Dict_Search_Input \
            .activated.connect(partial(self._focus_dict_search_input))
