from datetime import datetime
from functools import partial
from typing import List, Optional

from PyQt6 import QtGui
from PyQt6.QtCore import QTimer, QSize
from PyQt6.QtGui import QAction, QIcon, QPixmap
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWidgets import (QBoxLayout, QCheckBox, QComboBox, QCompleter, QFrame, QHBoxLayout, QLabel, QLineEdit,
                             QPushButton, QSpacerItem, QSpinBox, QVBoxLayout)

from simsapa import DbSchemaName, SearchResult, logger, SEARCH_TIMER_SPEED

from simsapa.app.app_data import AppData
from simsapa.app.search.dictionary_queries import ExactQueryResult
from simsapa.app.types import SearchArea, SearchMode, AllSearchModeNameToType, SuttaSearchModeNameToType, DictionarySearchModeNameToType, UDictWord
from simsapa.app.search.helpers import get_dict_word_languages, get_dict_word_source_filter_labels, get_sutta_languages, get_sutta_source_filter_labels

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd

from simsapa.layouts.parts.pali_completer import PaliCompleter

from simsapa.layouts.gui_helpers import get_search_params
from simsapa.layouts.gui_types import QExpanding, QMinimum, QSizeMinimum, SearchBarInterface, QFixed
from simsapa.layouts.help_info import setup_info_button

class HasSearchBar(SearchBarInterface):
    features: List[str]
    _app_data: AppData
    _search_area: SearchArea
    _current_results_page: List[SearchResult] = []
    _current_results_page_num = 0
    _results_page_render_len = 10
    enable_sidebar: bool
    action_Show_Sidebar: QAction

    row_one: QHBoxLayout
    row_two: QHBoxLayout

    qwe: QWebEngineView

    def init_search_bar(self,
                        wrap_layout: QBoxLayout,
                        search_area: SearchArea,
                        enable_nav_buttons = True,
                        enable_language_filter = True,
                        enable_search_extras = True,
                        enable_regex_fuzzy = True,
                        enable_info_button = True,
                        enable_sidebar_button = True,
                        input_fixed_size: Optional[QSize] = None,
                        icons_height = 40,
                        focus_input = True,
                        two_rows_layout = False):

        self.features.append('search_bar')

        self._search_area = search_area
        self._icons_height = icons_height

        self.enable_language_filter = enable_language_filter
        self.enable_search_extras = enable_search_extras
        self.enable_regex_fuzzy = enable_regex_fuzzy
        self.enable_sidebar_button = enable_sidebar_button

        self._init_search_icons()
        self._setup_layout(wrap_layout = wrap_layout,
                           enable_nav_buttons = enable_nav_buttons,
                           enable_sidebar_button = enable_sidebar_button,
                           input_fixed_size = input_fixed_size,
                           two_rows_layout = two_rows_layout)

        if self._app_data.app_settings.get('search_completion', True):
            self._init_search_input_completer()

        if focus_input:
            self.search_input.setFocus()

        if enable_language_filter:
            self._setup_language_include_btn()
            self._setup_language_filter()

        if enable_search_extras:
            self._setup_source_include_btn()
            self._setup_source_filter()

            if enable_regex_fuzzy:
                self._setup_regex_and_fuzzy()

            # self._setup_sutta_select_button() # TODO: list form is too long, not usable like this
            # self._setup_toggle_pali_button() # TODO: reimplement as hover window

            if enable_info_button:
                self.row_two.addItem(QSpacerItem(5, 0, QSizeMinimum, QSizeMinimum))
                setup_info_button(self.row_two, self._icons_height)

            # self._setup_pali_buttons() # TODO: reimplement as hover window

        self._connect_search_bar_signals()

    def _setup_show_sidebar_btn(self, wrap_layout: QBoxLayout):
        if not self.enable_sidebar:
            return

        spacerItem = QSpacerItem(0, 20, QExpanding, QMinimum)

        wrap_layout.addItem(spacerItem)

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/angles-right"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.show_sidebar_btn = QPushButton()
        self.show_sidebar_btn.setIcon(icon)
        self.show_sidebar_btn.setMinimumSize(QSize(self._icons_height, self._icons_height))
        self.show_sidebar_btn.setToolTip("Toggle Sidebar")

        wrap_layout.addWidget(self.show_sidebar_btn)

    def _setup_layout(self,
                      wrap_layout: QBoxLayout,
                      enable_nav_buttons: bool,
                      enable_sidebar_button: bool,
                      input_fixed_size: Optional[QSize] = None,
                      two_rows_layout = False):
        """
        wrap_layout

            search_bar_frame
                search_bar_box
                    row_one_frame
                        row_one
                            back_recent_button
                            forward_recent_button
                            search_input
                            search_button

                    row_two_frame
                        row_two
                            search_mode_dropdown
                            language_include_btn
                            language_filter_dropdown
                            source_include_btn
                            source_filter_dropdown
                            regex_fuzzy_frame
                                regex_checkbox
                                fuzzy_spin

            spacerItem
            show_sidebar_btn
        """

        wrap_layout.setContentsMargins(0, 0, 0, 0)

        if two_rows_layout:
            search_bar_box = QVBoxLayout()
        else:
            search_bar_box = QHBoxLayout()

        if two_rows_layout:
            frame_height = self._icons_height*2 + 30
        else:
            frame_height = self._icons_height + 30

        frame = QFrame()
        frame.setFrameShape(QFrame.Shape.NoFrame)
        frame.setFrameShadow(QFrame.Shadow.Raised)
        frame.setLineWidth(0)
        frame.setContentsMargins(0, 0, 0, 0)
        frame.setMinimumWidth(10)

        frame.setMaximumHeight(frame_height)

        self.search_bar_frame = frame
        self.search_bar_frame.setLayout(search_bar_box)

        wrap_layout.addWidget(self.search_bar_frame)

        self.row_one = QHBoxLayout()
        self.row_two = QHBoxLayout()

        self.row_one.setContentsMargins(0, 0, 0, 0)
        self.row_two.setContentsMargins(0, 0, 0, 0)

        frame = QFrame()
        frame.setFrameShape(QFrame.Shape.NoFrame)
        frame.setFrameShadow(QFrame.Shadow.Raised)
        frame.setLineWidth(0)
        frame.setContentsMargins(0, 0, 0, 0)
        frame.setMinimumWidth(10)

        self.row_one_frame = frame
        self.row_one_frame.setLayout(self.row_one)

        frame = QFrame()
        frame.setFrameShape(QFrame.Shape.NoFrame)
        frame.setFrameShadow(QFrame.Shadow.Raised)
        frame.setLineWidth(0)
        frame.setContentsMargins(0, 0, 0, 0)
        frame.setMinimumWidth(10)

        self.row_two_frame = frame
        self.row_two_frame.setLayout(self.row_two)

        search_bar_box.addWidget(self.row_one_frame)
        search_bar_box.addWidget(self.row_two_frame)

        # === Back / Forward Nav Buttons ===

        if enable_nav_buttons:

            self.back_recent_button = QPushButton()
            self.back_recent_button.setFixedSize(self._icons_height, self._icons_height)

            icon = QIcon()
            icon.addPixmap(QPixmap(":/arrow-left"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

            self.back_recent_button.setIcon(icon)
            self.back_recent_button.setObjectName("back_recent_button")

            self.row_one.addWidget(self.back_recent_button)

            self.forward_recent_button = QPushButton()
            self.forward_recent_button.setFixedSize(self._icons_height, self._icons_height)

            icon1 = QIcon()
            icon1.addPixmap(QPixmap(":/arrow-right"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
            self.forward_recent_button.setIcon(icon1)

            self.row_one.addWidget(self.forward_recent_button)

        # === Search Input ====

        self.search_input = QLineEdit()
        self.search_input.setContentsMargins(0, 0, 0, 0)

        if self._search_area == SearchArea.Suttas:
            placeholder_text = "Search in suttas"
        else:
            placeholder_text = "Search in dictionary"

        self.search_input.setPlaceholderText(placeholder_text)
        self.search_input.setClearButtonEnabled(True)

        if input_fixed_size is None:
            self.search_input.setSizePolicy(QExpanding, QFixed)
        else:
            self.search_input.setMinimumSize(input_fixed_size)

        self.search_input.setMinimumHeight(self._icons_height)

        style = """
QWidget { border: 1px solid #272727; }
QWidget:focus { border: 1px solid #1092C3; }
        """

        self.search_input.setStyleSheet(style)

        self.search_button = QPushButton()
        self.search_button.setFixedSize(self._icons_height, self._icons_height)
        self.search_button.setIcon(self._normal_search_icon)

        self.row_one.addWidget(self.search_input)
        self.row_one.addWidget(self.search_button)

        # === Row Two: Search Options ====

        self.search_mode_dropdown = QComboBox()

        if self._search_area == SearchArea.Suttas:
            names = SuttaSearchModeNameToType
        else:
            names = DictionarySearchModeNameToType

        items = names.keys()
        self.search_mode_dropdown.addItems(items)
        self.search_mode_dropdown.setFixedHeight(self._icons_height)

        mode = self._app_data.app_settings.get(self._search_mode_setting_key, SearchMode.FulltextMatch)
        values = list(map(lambda x: x[1], names.items()))
        idx = values.index(mode)
        self.search_mode_dropdown.setCurrentIndex(idx)

        self.row_two.addWidget(self.search_mode_dropdown)

        if enable_sidebar_button:
            self._setup_show_sidebar_btn(search_bar_box)

    def _init_search_icons(self):
        search_icon = QIcon()
        search_icon.addPixmap(QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self._normal_search_icon = search_icon

        stopwatch_icon = QIcon()
        stopwatch_icon.addPixmap(QPixmap(":/stopwatch"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self._stopwatch_icon = stopwatch_icon

        warning_icon = QIcon()
        warning_icon.addPixmap(QPixmap(":/warning"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self._warning_icon = warning_icon

    def _show_search_normal_icon(self):
        self.search_button.setIcon(self._normal_search_icon)
        self.search_button.setToolTip("Click to start search")

    def _show_search_stopwatch_icon(self):
        self.search_button.setIcon(self._stopwatch_icon)
        self.search_button.setToolTip("Search query running ...")

    def _show_search_warning_icon(self, warning_msg: str = ''):
        self.search_button.setIcon(self._warning_icon)
        self.search_button.setToolTip(warning_msg)

    def _init_search_input_completer(self):
        if self._search_area == SearchArea.Suttas:
            items = self._app_data._sutta_titles_completion_cache
        else:
            items = self._app_data._dict_words_completion_cache

        completer = PaliCompleter(parent = self, word_sublists = items)
        self.search_input.setCompleter(completer)

        input_completer = self.search_input.completer()
        if input_completer is not None:
            input_completer.activated.connect(partial(self._handle_query, min_length=1))
            # input_completer.activated.connect(partial(self._handle_exact_query, min_length=1))

    def _disable_search_input_completer(self):
        empty_completer = QCompleter()
        self.search_input.setCompleter(empty_completer)

    def _get_language_labels(self):
        if self._search_area == SearchArea.Suttas:
            return get_sutta_languages(self._app_data.db_session)
        else:
            return get_dict_word_languages(self._app_data.db_session)

    def _get_filter_labels(self):
        if self._search_area == SearchArea.Suttas:
            return get_sutta_source_filter_labels(self._app_data.db_session)
        else:
            return get_dict_word_source_filter_labels(self._app_data.db_session)

    def _save_search_bar_settings(self):
        if hasattr(self, 'language_filter_dropdown'):
            idx = self.language_filter_dropdown.currentIndex()
            self._app_data.app_settings[self._language_filter_setting_key] = idx

        if hasattr(self, 'source_filter_dropdown'):
            idx = self.source_filter_dropdown.currentIndex()
            self._app_data.app_settings[self._source_filter_setting_key] = idx

        self._app_data._save_app_settings()

    def _setup_regex_and_fuzzy(self):
        # === Frame ===

        frame = QFrame()
        frame.setFrameShape(QFrame.Shape.NoFrame)
        frame.setFrameShadow(QFrame.Shadow.Raised)
        frame.setLineWidth(0)
        frame.setMinimumWidth(10)
        frame.setContentsMargins(0, 0, 0, 0)

        box = QHBoxLayout()

        self.regex_fuzzy_frame = frame
        self.regex_fuzzy_frame.setLayout(box)
        self.row_two.addWidget(self.regex_fuzzy_frame)

        # === Regex checkbox ===

        icon = QIcon()
        icon.addPixmap(QPixmap(":/regex"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        chk = QCheckBox()
        chk.setIcon(icon)
        chk.setToolTip("Enable regex globbing patterns in the query (.* .+ a* a+)")

        self.regex_checkbox = chk

        box.addWidget(self.regex_checkbox)

        def _disable_fuzzy():
            self.fuzzy_spin.setDisabled(self.regex_checkbox.isChecked())

        self.regex_checkbox.clicked.connect(partial(_disable_fuzzy))

        idx = self.search_mode_dropdown.currentIndex()
        m = self.search_mode_dropdown.itemText(idx)
        mode = AllSearchModeNameToType[m]

        is_fulltext = (mode == SearchMode.FulltextMatch)
        self.regex_fuzzy_frame.setEnabled(is_fulltext)

        # === Fuzzy spinner ===
        spin = QSpinBox()
        spin.setMinimum(0)
        spin.setMaximum(9)

        self.fuzzy_spin = spin
        box.addWidget(self.fuzzy_spin)

        label = QLabel()
        p = QPixmap(":/tilde").scaledToWidth(20)
        label.setPixmap(p)
        box.addWidget(label)

        msg = "Fuzzy match words with n-character distance"
        label.setToolTip(msg)
        spin.setToolTip(msg)

    def _setup_language_filter(self):
        cmb = QComboBox()
        items = ["Language",]
        items.extend(self._get_language_labels())
        idx = self._app_data.app_settings.get(self._language_filter_setting_key, 0)

        cmb.addItems(items)
        cmb.setFixedHeight(self._icons_height)

        if idx < len(items):
            cmb.setCurrentIndex(idx)
        else:
            cmb.setCurrentIndex(0)

        self.language_filter_dropdown = cmb
        self.row_two.addWidget(self.language_filter_dropdown)

        self.language_filter_dropdown.currentIndexChanged.connect(partial(self._save_search_bar_settings))
        self.language_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))
        # self.language_filter_dropdown.currentIndexChanged.connect(partial(self._handle_exact_query, min_length=4))

    def _setup_language_include_btn(self):
        icon_plus = QIcon()
        icon_plus.addPixmap(QPixmap(":/plus-solid"))

        btn = QPushButton()
        btn.setFixedSize(self._icons_height, self._icons_height)
        btn.setIcon(icon_plus)
        btn.setToolTip("+ means 'must include', - means 'must exclude'")
        btn.setCheckable(True)
        btn.setChecked(True)

        self.language_include_btn = btn
        self.row_two.addWidget(self.language_include_btn)

        def _clicked():
            is_on = self.language_include_btn.isChecked()

            if is_on:
                icon = QIcon()
                icon.addPixmap(QPixmap(":/plus-solid"))
            else:
                icon = QIcon()
                icon.addPixmap(QPixmap(":/minus-solid"))

            self.language_include_btn.setIcon(icon)

            self._handle_query(min_length=4)

        self.language_include_btn.clicked.connect(partial(_clicked))

    def _setup_source_include_btn(self):
        icon_plus = QIcon()
        icon_plus.addPixmap(QPixmap(":/plus-solid"))

        btn = QPushButton()
        btn.setFixedSize(self._icons_height, self._icons_height)
        btn.setIcon(icon_plus)
        btn.setToolTip("+ means 'must include', - means 'must exclude'")
        btn.setCheckable(True)
        btn.setChecked(True)

        self.source_include_btn = btn
        self.row_two.addWidget(self.source_include_btn)

        def _clicked():
            is_on = self.source_include_btn.isChecked()

            if is_on:
                icon = QIcon()
                icon.addPixmap(QPixmap(":/plus-solid"))
            else:
                icon = QIcon()
                icon.addPixmap(QPixmap(":/minus-solid"))

            self.source_include_btn.setIcon(icon)

            self._handle_query(min_length=4)

        self.source_include_btn.clicked.connect(partial(_clicked))

    def _setup_source_filter(self):
        cmb = QComboBox()

        if self._search_area == SearchArea.Suttas:
            items = ["Sources",]
        else:
            items = ["Dictionaries",]

        items.extend(self._get_filter_labels())
        idx = self._app_data.app_settings.get(self._source_filter_setting_key, 0)

        cmb.addItems(items)
        cmb.setFixedHeight(self._icons_height)

        if idx < len(items):
            cmb.setCurrentIndex(idx)
        else:
            cmb.setCurrentIndex(0)

        self.source_filter_dropdown = cmb
        self.row_two.addWidget(self.source_filter_dropdown)

        self.source_filter_dropdown.currentIndexChanged.connect(partial(self._save_search_bar_settings))
        self.source_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))
        # self.source_filter_dropdown.currentIndexChanged.connect(partial(self._handle_exact_query, min_length=4))

    def _set_query(self, s: str):
        self.search_input.setText(s)

    def _append_to_query(self, s: str):
        a = self.search_input.text().strip()
        n = self.search_input.cursorPosition()
        pre = a[:n]
        post = a[n:]
        self.search_input.setText(pre + s + post)
        self.search_input.setCursorPosition(n + len(s))
        self.search_input.setFocus()

    def _focus_search_input(self):
        self.search_input.setFocus()

    def _user_typed(self):
        self._show_search_normal_icon()

        if not self._app_data.app_settings.get('search_as_you_type', True):
            return

        if not self._search_timer.isActive():
            self._search_timer = QTimer()
            self._search_timer.timeout.connect(partial(self._handle_query, min_length=4))
            self._search_timer.setSingleShot(True)

        self._search_timer.start(SEARCH_TIMER_SPEED)

    def _handle_search_mode_changed(self):
        idx = self.search_mode_dropdown.currentIndex()
        m = self.search_mode_dropdown.itemText(idx)
        mode = AllSearchModeNameToType[m]

        self._app_data.app_settings[self._search_mode_setting_key] = mode
        self._app_data._save_app_settings()

        if self.enable_regex_fuzzy:
            is_fulltext = (mode == SearchMode.FulltextMatch)
            self.regex_fuzzy_frame.setEnabled(is_fulltext)

    def _start_query_workers(self, query_text_orig: str):
        if len(query_text_orig) == 0:
            return
        logger.info(f"_start_query_workers(): {query_text_orig}")

        if self._app_data.search_indexes is None:
            return

        params = get_search_params(self)

        if params['mode'] == SearchMode.FulltextMatch:
            try:
                self._app_data.search_indexes.test_correct_query_syntax(self._search_area, query_text_orig)

            except ValueError as e:
                self._show_search_warning_icon(str(e))
                return

        self.start_loading_animation()

        self._last_query_time = datetime.now()

        self._queries.start_search_query_workers(
            query_text_orig,
            self._search_area,
            self._last_query_time,
            partial(self._search_query_finished),
            params,
        )

    def _search_results_to_dict_words(self, search_results: List[SearchResult]) -> List[UDictWord]:
        # Must maintain the order of search_results in the db results, hence not
        # using one Am.DictWord.uid.in_(uids['appdata']) request. Must retreive
        # each db item in the same sequence.

        res: List[UDictWord] = []

        for i in search_results:
            if i['uid'] is None:
                continue

            if i['schema_name'] == DbSchemaName.AppData.value:
                r = self._app_data.db_session \
                    .query(Am.DictWord) \
                    .filter(Am.DictWord.uid == i['uid']) \
                    .first()
                if r is not None:
                    res.append(r)

            elif i['schema_name'] == DbSchemaName.UserData.value:
                r = self._app_data.db_session \
                    .query(Um.DictWord) \
                    .filter(Um.DictWord.uid == i['uid']) \
                    .first()
                if r is not None:
                    res.append(r)

            elif i['schema_name'] == DbSchemaName.Dpd.value:
                if i['table_name'] == "pali_words":
                    r = self._app_data.db_session \
                        .query(Dpd.PaliWord) \
                        .filter(Dpd.PaliWord.uid == i['uid']) \
                        .first()
                    if r is not None:
                        res.append(r)

                elif i['table_name'] == "pali_roots":
                    r = self._app_data.db_session \
                        .query(Dpd.PaliRoot) \
                        .filter(Dpd.PaliRoot.uid == i['uid']) \
                        .first()
                    if r is not None:
                        res.append(r)

            else:
                raise Exception(f"Unknown schema_name: {i['schema_name']}")

        return res

    def _render_dict_words_search_results(self, search_results: List[SearchResult]):
        logger.info("_render_dict_words_search_results()")

        self.stop_loading_animation()
        self._show_search_normal_icon()

        res = self._search_results_to_dict_words(search_results)

        self._render_words(res)

    def _load_more_results(self):
        self._current_results_page_num += 1

        r = self._current_results_page
        render_len = self._results_page_render_len

        start_idx = (self._current_results_page_num * render_len) + 1
        end_idx = start_idx + render_len
        has_more = (len(r) > end_idx)

        words = self._search_results_to_dict_words(r[start_idx:end_idx])

        html = ""

        for w in words:
            d = self._queries.dictionary_queries.get_word_html(w)
            html += d['body']

        js = """
        document.SSP.html_template = `<div>%s</div>`;
        document.SSP.page_bottom_el = document.getElementById('page_bottom');
        document.SSP.page_bottom_el.before(document.SSP.html_to_element(document.SSP.html_template));
        """ % html

        if not has_more:
            js += """
            document.SSP.remove_infinite_scroll();
            document.SSP.add_bottom_message("End of page. Use the results tab to see more results and open words.");
            """

        self.qwe.page().runJavaScript(js)

    def _toggle_show_search_bar(self, view):
        is_on = view.action_Show_Search_Bar.isChecked()
        self.search_bar_frame.setVisible(is_on)

    def _toggle_show_search_options(self, view):
        is_on = view.action_Show_Search_Options.isChecked()
        self.row_two_frame.setVisible(is_on)

        self._app_data.app_settings['show_search_options'] = is_on
        self._app_data._save_app_settings()

    def _connect_search_bar_signals(self):
        if hasattr(self, 'pw'):
            view = self.pw # type: ignore
        else:
            view = self

        if hasattr(view, 'action_Show_Search_Bar'):
            view.action_Show_Search_Bar \
                .triggered.connect(partial(self._toggle_show_search_bar, view))

        if hasattr(view, 'action_Show_Search_Options'):
            is_on = self._app_data.app_settings.get('show_search_options', True)
            view.action_Show_Search_Options.setChecked(is_on)
            self.row_two_frame.setVisible(is_on)

            view.action_Show_Search_Options \
                .triggered.connect(partial(self._toggle_show_search_options, view))

        if self.enable_sidebar_button:
            def _handle_sidebar():
                if hasattr(view, 'action_Show_Sidebar'):
                    view.action_Show_Sidebar.activate(QAction.ActionEvent.Trigger)

            self.show_sidebar_btn.clicked.connect(partial(_handle_sidebar))

        if hasattr(self, 'search_button'):
            self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
            # self.search_button.clicked.connect(partial(self._handle_exact_query, min_length=1))

        if hasattr(self, 'search_input'):
            self.search_input.textEdited.connect(partial(self._user_typed))
            self.search_input.returnPressed.connect(partial(self._handle_query, min_length=1))
            # self.search_input.returnPressed.connect(partial(self._handle_exact_query, min_length=1))

        if hasattr(self, 'search_mode_dropdown'):
            self.search_mode_dropdown.currentIndexChanged.connect(partial(self._handle_search_mode_changed))

        if self.enable_language_filter and hasattr(self, 'language_filter_dropdown'):
            self.language_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))

        if self.enable_search_extras and hasattr(self, 'source_filter_dropdown'):
            self.source_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))

    def _handle_query(self, min_length: int = 4):
        print("NotImplementedError %s" % str(min_length))
        raise NotImplementedError

    def _handle_exact_query(self, min_length: int = 4):
        print("NotImplementedError %s" % str(min_length))
        raise NotImplementedError

    def get_page_num(self) -> int:
        raise NotImplementedError

    # start- and stop_loading_animation() are implemented by HasFulltextList, so
    # the class which inherits HasSearchBar, has to first inherit
    # HasFulltextList or implement the functions directly.
    def start_loading_animation(self):
        raise NotImplementedError

    def stop_loading_animation(self):
        raise NotImplementedError

    def _search_query_finished(self, query_started_time: datetime):
        print("NotImplementedError %s" % str(query_started_time))
        raise NotImplementedError

    def _exact_query_finished(self, q_res: ExactQueryResult):
        print("NotImplementedError %s" % str(q_res))
        raise NotImplementedError

    def _render_words(self, words: List[UDictWord]):
        print("NotImplementedError %s" % str(len(words)))
        raise NotImplementedError
