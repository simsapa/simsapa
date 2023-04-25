import re
from datetime import datetime
from urllib.parse import urlencode

from functools import partial
from typing import Any, Callable, List, Optional

from PyQt6 import QtCore, QtWidgets, QtGui
from PyQt6.QtCore import QThreadPool, QTimer, QUrl, Qt, pyqtSignal
from PyQt6.QtGui import QIcon, QPixmap, QStandardItem, QStandardItemModel, QAction
from PyQt6.QtWidgets import (QComboBox, QCompleter, QFrame, QHBoxLayout, QLineEdit, QMenu, QPushButton, QTabWidget, QToolBar, QVBoxLayout, QWidget)

from sqlalchemy import and_
# from tomlkit import items

from simsapa import READING_BACKGROUND_COLOR, SEARCH_TIMER_SPEED, DbSchemaName, logger
from simsapa.layouts.bookmark_dialog import HasBookmarkDialog
from simsapa.layouts.find_panel import FindSearched, FindPanel
from simsapa.layouts.reader_web import LinkHoverData, ReaderWebEnginePage
from simsapa.layouts.search_query_worker import SearchQueryWorker
from simsapa.layouts.sutta_queries import QuoteScope, SuttaQueries
from simsapa.layouts.simsapa_webengine import SimsapaWebEngine
from ..app.db.search import SearchResult, sutta_hit_to_search_result, RE_ALL_BOOK_SUTTA_REF
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, OpenPromptParams, QFixed, QMinimum, QExpanding, QueryType, SearchMode, SuttaQuote, SuttaSearchModeNameToType, USutta, UDictWord, SuttaSearchWindowInterface, sutta_quote_from_url
from .sutta_tab import SuttaTabWidget
from .memo_dialog import HasMemoDialog
from .html_content import html_page
from .help_info import setup_info_button
from .sutta_select_dialog import SuttaSelectDialog


class SuttaSearchWindowState(QWidget, HasMemoDialog, HasBookmarkDialog):

    searchbar_layout: Optional[QHBoxLayout]
    sutta_tabs_layout: Optional[QVBoxLayout]
    tabs_layout: Optional[QVBoxLayout]

    queries: SuttaQueries
    search_extras: QHBoxLayout
    palibuttons_frame: QFrame
    search_input: QLineEdit
    toggle_pali_btn: QPushButton
    _app_data: AppData
    _autocomplete_model: QStandardItemModel
    sutta_tabs: QTabWidget
    sutta_tab: SuttaTabWidget
    _related_tabs: List[SuttaTabWidget]
    _search_timer = QTimer()
    _last_query_time = datetime.now()
    search_query_workers: List[SearchQueryWorker] = []
    search_mode_dropdown: QComboBox
    show_url_action_fn: Callable

    open_sutta_new_signal = pyqtSignal(str)
    open_in_study_window_signal = pyqtSignal([str, str])
    link_mouseover = pyqtSignal(dict)
    link_mouseleave = pyqtSignal(str)
    page_dblclick = pyqtSignal()
    hide_preview = pyqtSignal()
    bookmark_edit = pyqtSignal(str)
    open_gpt_prompt = pyqtSignal(dict)

    def __init__(self,
                 app_data: AppData,
                 parent_window: SuttaSearchWindowInterface,
                 searchbar_layout: Optional[QHBoxLayout],
                 sutta_tabs_layout: Optional[QVBoxLayout],
                 tabs_layout: Optional[QVBoxLayout],
                 focus_input: bool = True,
                 enable_language_filter: bool = True,
                 enable_search_extras: bool = True,
                 enable_sidebar: bool = True,
                 enable_find_panel: bool = True,
                 show_query_results_in_active_tab: bool = False,
                 custom_create_context_menu_fn: Optional[Callable] = None) -> None:
        super().__init__()

        self.pw = parent_window

        self.enable_language_filter = enable_language_filter
        self.enable_search_extras = enable_search_extras
        self.enable_sidebar = enable_sidebar
        self.enable_find_panel = enable_find_panel

        self.searchbar_layout = searchbar_layout
        self.sutta_tabs_layout = sutta_tabs_layout
        self.tabs_layout = tabs_layout

        self.query_in_tab = show_query_results_in_active_tab
        self.showing_query_in_tab = False

        self.custom_create_context_menu_fn = custom_create_context_menu_fn

        self.features: List[str] = []
        self._app_data: AppData = app_data

        self.show_url_action_fn = self._show_sutta_by_url

        self.queries = SuttaQueries(self._app_data)

        self.page_len = 20

        self.thread_pool = QThreadPool()

        self._recent: List[USutta] = []

        self._related_tabs: List[SuttaTabWidget] = []

        self._autocomplete_model = QStandardItemModel()

        self.focus_input = focus_input

        self._setup_ui()
        self._connect_signals()

        self.init_bookmark_dialog()
        self.init_memo_dialog()

    def _init_search_query_workers(self, query: str = ""):
        if self.enable_search_extras:
            idx = self.sutta_language_filter_dropdown.currentIndex()
            language = self.sutta_language_filter_dropdown.itemText(idx)
            if language == "Language":
                only_lang = None
            else:
                only_lang = language

            if hasattr(self, 'sutta_source_filter_dropdown'):
                idx = self.sutta_source_filter_dropdown.currentIndex()
                source = self.sutta_source_filter_dropdown.itemText(idx)
                if source == "Source":
                    only_source = None
                else:
                    only_source = source
            else:
                only_source = None

        else:
            only_lang = None
            only_source = None

        disabled_labels = self._app_data.app_settings.get('disabled_sutta_labels', None)
        self._last_query_time = datetime.now()

        idx = self.search_mode_dropdown.currentIndex()
        s = self.search_mode_dropdown.itemText(idx)
        mode = SuttaSearchModeNameToType[s]

        for i in self.search_query_workers:
            i.will_emit_finished = False

        self.search_query_workers = []

        # Sutta query worker

        w = SearchQueryWorker(self._app_data.search_indexed.suttas_index,
                              self.page_len,
                              mode,
                              sutta_hit_to_search_result)

        w.set_query(query,
                    self._last_query_time,
                    disabled_labels,
                    only_lang,
                    only_source)

        w.signals.finished.connect(partial(self._search_query_finished))

        self.search_query_workers.append(w)

        # Language query workers

        index_names = self._app_data.search_indexed.suttas_lang_index.keys()
        for i in index_names:

            w = SearchQueryWorker(self._app_data.search_indexed.suttas_lang_index[i],
                                  self.page_len,
                                  mode,
                                  sutta_hit_to_search_result)

            w.set_query(query,
                        self._last_query_time,
                        disabled_labels,
                        only_lang,
                        only_source)

            w.signals.finished.connect(partial(self._search_query_finished))

            self.search_query_workers.append(w)

    def _get_active_tab(self) -> SuttaTabWidget:
        current_idx = self.sutta_tabs.currentIndex()
        if current_idx == 0:
            tab = self.sutta_tab
        else:
            tab = self._related_tabs[current_idx-1]

        return tab

    def _get_selection(self) -> Optional[str]:
        tab = self._get_active_tab()
        text = tab.qwe.selectedText()
        # U+2029 Paragraph Separator to blank line
        text = text.replace('\u2029', "\n\n")
        text = text.strip()
        if len(text) > 0:
            return text
        else:
            return None

    def _setup_ui(self):
        self._setup_search_bar();

        self._setup_sutta_tabs()

        if self.enable_language_filter:
            self._setup_language_filter()

        if self.enable_search_extras:
            self._setup_source_filter()
            # self._setup_sutta_select_button() # TODO: list form is too long, not usable like this
            # self._setup_toggle_pali_button() # TODO: reimplement as hover window
            setup_info_button(self.search_extras, self)

            # self._setup_pali_buttons() # TODO: reimplement as hover window

        if self.enable_find_panel:
            self._find_panel = FindPanel()

            self.find_toolbar = QToolBar()
            self.find_toolbar.addWidget(self._find_panel)

            self.pw.addToolBar(QtCore.Qt.ToolBarArea.BottomToolBarArea, self.find_toolbar)
            self.find_toolbar.hide()

    def _setup_search_bar(self):
        if self.searchbar_layout is None:
            return

        self.back_recent_button = QtWidgets.QPushButton()
        sizePolicy = QtWidgets.QSizePolicy(QFixed, QFixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.back_recent_button.sizePolicy().hasHeightForWidth())

        self.back_recent_button.setSizePolicy(sizePolicy)
        self.back_recent_button.setMinimumSize(QtCore.QSize(40, 40))

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/arrow-left"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.back_recent_button.setIcon(icon)
        self.back_recent_button.setObjectName("back_recent_button")

        self.searchbar_layout.addWidget(self.back_recent_button)

        self.forward_recent_button = QtWidgets.QPushButton()

        sizePolicy = QtWidgets.QSizePolicy(QFixed, QFixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.forward_recent_button.sizePolicy().hasHeightForWidth())

        self.forward_recent_button.setSizePolicy(sizePolicy)
        self.forward_recent_button.setMinimumSize(QtCore.QSize(40, 40))

        icon1 = QtGui.QIcon()
        icon1.addPixmap(QtGui.QPixmap(":/arrow-right"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.forward_recent_button.setIcon(icon1)

        self.searchbar_layout.addWidget(self.forward_recent_button)

        self.search_input = QtWidgets.QLineEdit()

        sizePolicy = QtWidgets.QSizePolicy(QFixed, QFixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.search_input.sizePolicy().hasHeightForWidth())
        self.search_input.setSizePolicy(sizePolicy)

        self.search_input.setMinimumSize(QtCore.QSize(250, 35))
        self.search_input.setClearButtonEnabled(True)

        self.searchbar_layout.addWidget(self.search_input)

        if self.focus_input:
            self.search_input.setFocus()

        self.search_button = QtWidgets.QPushButton()

        sizePolicy = QtWidgets.QSizePolicy(QFixed, QFixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.search_button.sizePolicy().hasHeightForWidth())

        self.search_button.setSizePolicy(sizePolicy)
        self.search_button.setMinimumSize(QtCore.QSize(40, 40))

        icon2 = QtGui.QIcon()
        icon2.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.search_button.setIcon(icon2)
        self.searchbar_layout.addWidget(self.search_button)

        self.search_mode_dropdown = QComboBox()
        items = SuttaSearchModeNameToType.keys()
        self.search_mode_dropdown.addItems(items)
        self.search_mode_dropdown.setFixedHeight(40)

        mode = self._app_data.app_settings.get('sutta_search_mode', SearchMode.FulltextMatch)
        values = list(map(lambda x: x[1], SuttaSearchModeNameToType.items()))
        idx = values.index(mode)
        self.search_mode_dropdown.setCurrentIndex(idx)

        self.searchbar_layout.addWidget(self.search_mode_dropdown)

        self.search_extras = QtWidgets.QHBoxLayout()
        self.searchbar_layout.addLayout(self.search_extras)

        spacerItem = QtWidgets.QSpacerItem(40, 20, QExpanding, QMinimum)

        self.searchbar_layout.addItem(spacerItem)

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/angles-right"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.show_sidebar_btn = QPushButton()
        self.show_sidebar_btn.setIcon(icon)
        self.show_sidebar_btn.setMinimumSize(QtCore.QSize(40, 40))
        self.show_sidebar_btn.setToolTip("Toggle Sidebar")

        if self.enable_sidebar:
            self.searchbar_layout.addWidget(self.show_sidebar_btn)

        style = """
QWidget { border: 1px solid #272727; }
QWidget:focus { border: 1px solid #1092C3; }
        """

        self.search_input.setStyleSheet(style)

        completer = QCompleter(self._autocomplete_model, self)
        completer.setMaxVisibleItems(20)
        completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        completer.setModelSorting(QCompleter.ModelSorting.CaseInsensitivelySortedModel)

        self.search_input.setCompleter(completer)

        self.search_input.setFocus()

    def _setup_sutta_tabs(self):
        if self.sutta_tabs_layout is None:
            return

        self.sutta_tabs = QTabWidget()
        self.sutta_tabs.setStyleSheet("*[style_class='sutta_tab'] { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.sutta_tab = SuttaTabWidget(self._app_data, "Sutta", 0, self._new_webengine())
        self.sutta_tab.setProperty('style_class', 'sutta_tab')
        self.sutta_tab.layout().setContentsMargins(0, 0, 0, 0)

        self.sutta_tabs.addTab(self.sutta_tab, "Sutta")

        html = html_page('', self._app_data.api_url)
        self.sutta_tab.set_qwe_html(html)

        self.sutta_tabs_layout.addWidget(self.sutta_tabs)

    def _link_mouseover(self, hover_data: LinkHoverData):
        self.link_mouseover.emit(hover_data)

    def _link_mouseleave(self, href: str):
        self.link_mouseleave.emit(href)

    def _page_dblclick(self):
        if self._app_data.app_settings['double_click_dict_lookup']:
            self.page_dblclick.emit()

    def _emit_hide_preview(self):
        self.hide_preview.emit()

    def _new_webengine(self) -> SimsapaWebEngine:
        page = ReaderWebEnginePage(self)
        page.helper.mouseover.connect(partial(self._link_mouseover))
        page.helper.mouseleave.connect(partial(self._link_mouseleave))
        page.helper.dblclick.connect(partial(self._page_dblclick))
        page.helper.hide_preview.connect(partial(self._emit_hide_preview))

        page.helper.bookmark_edit.connect(partial(self.handle_edit_bookmark))

        if self.custom_create_context_menu_fn:
            qwe = SimsapaWebEngine(page, self.custom_create_context_menu_fn)
        else:
            qwe = SimsapaWebEngine(page, self._create_qwe_context_menu)

        return qwe

    def _add_new_tab(self, title: str, sutta: Optional[USutta]):
        # don't substract one because the _related_tabs start after sutta_tab,
        # and tab indexing start from 0
        tab_index = len(self._related_tabs)
        tab = SuttaTabWidget(self._app_data,
                             title,
                             tab_index,
                             self._new_webengine(),
                             sutta)

        tab.render_sutta_content()

        self._related_tabs.append(tab)

        self.sutta_tabs.addTab(tab, title)

    def _toggle_pali_buttons(self):
        show = self.toggle_pali_btn.isChecked()
        self.pw.palibuttons_frame.setVisible(show)

        self._app_data.app_settings['suttas_show_pali_buttons'] = show
        self._app_data._save_app_settings()

    def _setup_toggle_pali_button(self):
        icon = QIcon()
        icon.addPixmap(QPixmap(":/keyboard"))

        btn = QPushButton()
        btn.setFixedSize(40, 40)
        btn.setToolTip("Toggle Pali Buttons")
        btn.clicked.connect(partial(self._toggle_pali_buttons))
        btn.setIcon(icon)

        show = self._app_data.app_settings.get('suttas_show_pali_buttons', False)
        btn.setCheckable(True)
        btn.setChecked(show)

        self.toggle_pali_btn = btn
        self.search_extras.addWidget(self.toggle_pali_btn)

    def _setup_pali_buttons(self):
        palibuttons_layout = QHBoxLayout()
        self.pw.palibuttons_frame.setLayout(palibuttons_layout)

        lowercase = 'ā ī ū ṃ ṁ ṅ ñ ṭ ḍ ṇ ḷ ṛ ṣ ś'.split(' ')

        for i in lowercase:
            btn = QPushButton(i)
            btn.setFixedSize(35, 35)
            btn.clicked.connect(partial(self._append_to_query, i))
            palibuttons_layout.addWidget(btn)

        show = self._app_data.app_settings.get('suttas_show_pali_buttons', False)
        self.pw.palibuttons_frame.setVisible(show)

    def _get_language_labels(self):
        res = []

        r = self._app_data.db_session.query(Am.Sutta.language.distinct()).all()
        res.extend(r)

        r = self._app_data.db_session.query(Um.Sutta.language.distinct()).all()
        res.extend(r)

        labels = sorted(set(map(lambda x: str(x[0]).lower(), res)))

        return labels

    def _get_source_uid_labels(self):
        res = []

        r = self._app_data.db_session.query(Am.Sutta.source_uid.distinct()).all()
        res.extend(r)

        r = self._app_data.db_session.query(Um.Sutta.source_uid.distinct()).all()
        res.extend(r)

        labels = sorted(set(map(lambda x: str(x[0]).lower(), res)))

        return labels

    def _setup_language_filter(self):
        cmb = QComboBox()
        items = ["Language",]
        items.extend(self._get_language_labels())
        idx = self._app_data.app_settings.get('sutta_language_filter_idx', 0)

        cmb.addItems(items)
        cmb.setFixedHeight(40)
        cmb.setCurrentIndex(idx)
        self.sutta_language_filter_dropdown = cmb
        self.search_extras.addWidget(self.sutta_language_filter_dropdown)

    def _setup_source_filter(self):
        cmb = QComboBox()
        items = ["Source",]
        items.extend(self._get_source_uid_labels())
        idx = self._app_data.app_settings.get('sutta_source_filter_idx', 0)

        cmb.addItems(items)
        cmb.setFixedHeight(40)
        cmb.setCurrentIndex(idx)
        self.sutta_source_filter_dropdown = cmb
        self.search_extras.addWidget(self.sutta_source_filter_dropdown)

    def _setup_sutta_select_button(self):
        # TODO create a better layout, this is too long to use like this.
        icon = QIcon()
        icon.addPixmap(QPixmap(":/book"))

        btn = QPushButton()
        btn.setFixedSize(40, 40)
        btn.setToolTip("Select Sutta Sources")
        btn.clicked.connect(partial(self._show_sutta_select_dialog))
        btn.setIcon(icon)

        self.sutta_select_btn = btn
        self.search_extras.addWidget(self.sutta_select_btn)

    def _show_sutta_select_dialog(self):
        d = SuttaSelectDialog(self._app_data, self)

        if d.exec():
            self._handle_query()

    def _set_query(self, s: str):
        self.search_input.setText(s)

    def _append_to_query(self, s: str):
        a = self.search_input.text()
        n = self.search_input.cursorPosition()
        pre = a[:n]
        post = a[n:]
        self.search_input.setText(pre + s + post)
        self.search_input.setCursorPosition(n + len(s))
        self.search_input.setFocus()

    def query_hits(self) -> int:
        if len(self.search_query_workers) == 0:
            return 0
        else:
            return sum([i.query_hits() for i in self.search_query_workers])

    def results_page(self, page_num: int) -> List[SearchResult]:
        logger.info(f"results_page(): page_num = {page_num}")
        n = len(self.running_queries())
        if n != 0:
            logger.info(f"Running queries: {n}, return empty results")
            return []
        else:
            a: List[SearchResult] = []
            for i in self.search_query_workers:
                a.extend(i.results_page(page_num))

            # The higher the score, the better. Reverse to get descending order.
            res = sorted(a, key=lambda x: x['score'] or 0, reverse = True)
            return res

    def running_queries(self) -> List[SearchQueryWorker]:
        return [i for i in self.search_query_workers if i.query_finished is None]

    def _search_query_finished(self):
        n = len(self.running_queries())
        logger.info(f"_search_query_finished(), still running: {n}")

        if n > 0:
            return

        self.pw.stop_loading_animation()

        if len(self.search_query_workers) == 0:
            return

        if self._last_query_time != self.search_query_workers[0].query_started:
            return

        # Restore the search icon, processing finished
        icon_search = QtGui.QIcon()
        icon_search.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.search_button.setIcon(icon_search)

        if self.enable_sidebar:
            self.pw._update_sidebar_fulltext(self.query_hits())

        results = self.results_page(0)

        if self.query_hits() == 1 and results[0]['uid'] is not None:
            self._show_sutta_by_uid(results[0]['uid'])

        elif self.query_in_tab:
            self._render_results_in_active_tab(self.query_hits())

    def _start_query_worker(self, query: str):
        logger.info("_start_query_worker()")
        self.pw.start_loading_animation()

        self._init_search_query_workers(query)
        for i in self.search_query_workers:
            self.thread_pool.start(i)

    def _handle_query(self, min_length: int = 4):
        query = self.search_input.text()
        logger.info(f"_handle_query(): {query}, {min_length}")

        idx = self.sutta_language_filter_dropdown.currentIndex()
        self._app_data.app_settings['sutta_language_filter_idx'] = idx

        if hasattr(self, 'sutta_source_filter_dropdown'):
            idx = self.sutta_source_filter_dropdown.currentIndex()
            self._app_data.app_settings['sutta_source_filter_idx'] = idx

        self._app_data._save_app_settings()

        # Re-render the current sutta, in case user is trying to restore sutta
        # after a search in the Study Window with the clear input button.
        if len(query) == 0 and self.showing_query_in_tab and self._get_active_tab().sutta is not None:
            self._get_active_tab().render_sutta_content()
            return

        if re.search(RE_ALL_BOOK_SUTTA_REF, query) is None and len(query) < min_length:
            return

        # Not aborting, show the user that the app started processsing
        icon_processing = QtGui.QIcon()
        icon_processing.addPixmap(QtGui.QPixmap(":/stopwatch"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)
        self.search_button.setIcon(icon_processing)

        self._start_query_worker(query)

    def _render_results_in_active_tab(self, hits: int):
        if hits == 0:
            return

        self.showing_query_in_tab = True
        if len(self.running_queries()) == 0:
            a = []
            for i in self.search_query_workers:
                a.extend(i.all_results())
            res = sorted(a, key=lambda x: x['score'] or 0, reverse = True)
            self._get_active_tab().render_search_results(res)

    def _handle_autocomplete_query(self, min_length: int = 4):
        if not self.pw.action_Search_Completion.isChecked():
            return

        query = self.search_input.text()

        if len(query) < min_length:
            return

        self._autocomplete_model.clear()

        a = set(filter(lambda x: x.lower().startswith(query.lower()), self._app_data.completion_cache['sutta_titles']))

        for i in a:
            self._autocomplete_model.appendRow(QStandardItem(i))

        # NOTE: completion cache is already sorted.
        # self._autocomplete_model.sort(0)

    def _sutta_search_query(self, query: str, only_lang: Optional[str] = None, only_source: Optional[str] = None) -> List[SearchResult]:
        # TODO This is a synchronous version of _start_query_worker(), still
        # used in links_browser.py. Update and use the background thread worker.

        self._init_search_query_workers(query)

        disabled_labels = self._app_data.app_settings.get('disabled_sutta_labels', None)

        # first page results
        a = []
        for i in self.search_query_workers:
            i.search_query.new_query(query, disabled_labels, only_lang, only_source)
            a.extend(i.results_page(0))

        res = sorted(a, key=lambda x: x['score'] or 0, reverse = True)

        return res

    def _set_qwe_html(self, html: str):
        self.sutta_tab.set_qwe_html(html)

    def _add_recent(self, sutta: USutta):
        # de-duplicate: if item already exists, remove it
        if sutta in self._recent:
            self._recent.remove(sutta)
        # insert new item on top
        self._recent.insert(0, sutta)

        # Rebuild Qt recents list
        if self.enable_sidebar:
            def _to_title(x: USutta):
                return " - ".join([str(x.uid), str(x.title)])

            titles = list(map(lambda x: _to_title(x), self._recent))

            self.pw._set_recent_list(titles)

    def _sutta_from_result(self, x: SearchResult) -> Optional[USutta]:
        if x['schema_name'] == DbSchemaName.AppData.value:
            sutta = self._app_data.db_session \
                                  .query(Am.Sutta) \
                                  .filter(Am.Sutta.uid == x['uid']) \
                                  .first()
        else:
            sutta = self._app_data.db_session \
                                  .query(Um.Sutta) \
                                  .filter(Um.Sutta.uid == x['uid']) \
                                  .first()
        return sutta

    @QtCore.pyqtSlot(dict)
    def on_searched(self, find_searched: FindSearched):
        tab = self._get_active_tab()
        if find_searched['flag'] is None:
            tab.qwe.findText(find_searched['text'])
        else:
            tab.qwe.findText(find_searched['text'], find_searched['flag'])

    def _select_prev_tab(self):
        selected_idx = self.sutta_tabs.currentIndex()
        if selected_idx == -1:
            self.sutta_tabs.setCurrentIndex(0)
        elif selected_idx == 0:
            return
        else:
            self.sutta_tabs.setCurrentIndex(selected_idx - 1)

    def _select_next_tab(self):
        selected_idx = self.sutta_tabs.currentIndex()
        if selected_idx == -1:
            self.sutta_tabs.setCurrentIndex(0)
        elif selected_idx + 1 < len(self.sutta_tabs):
            self.sutta_tabs.setCurrentIndex(selected_idx + 1)

    def _show_url(self, url: QUrl):
        if url.host() == QueryType.suttas:
            self.show_url_action_fn(url)

        # elif url.host() == QueryType.words:
        #     self._show_words_by_url(url)

    def _show_sutta_from_message(self, info: Any):
        sutta: Optional[USutta] = None

        if not 'table' in info.keys() or not 'id' in info.keys():
            return

        if info['table'] == 'appdata.suttas':
            sutta = self._app_data.db_session \
                .query(Am.Sutta) \
                .filter(Am.Sutta.id == info['id']) \
                .first()

        elif info['table'] == 'userdata.suttas':
            sutta = self._app_data.db_session \
                .query(Um.Sutta) \
                .filter(Um.Sutta.id == info['id']) \
                .first()

        if sutta:
            self._show_sutta(sutta)

    def _show_sutta_by_url(self, url: QUrl):
        if url.host() != QueryType.suttas:
            return False

        uid = re.sub(r"^/", "", url.path())

        self._show_sutta_by_uid(uid, sutta_quote_from_url(url))

    def _show_sutta_by_quote(self, sutta_quote: SuttaQuote):
        if len(sutta_quote['quote']) == 0:
            return

        self._set_query(sutta_quote['quote'])
        self._start_query_worker(sutta_quote['quote'])

        results = self.queries.get_suttas_by_quote(sutta_quote['quote'])

        if len(results) > 0:
            self._show_sutta(results[0], sutta_quote)
            self._add_recent(results[0])

    def _show_sutta_by_partial_uid(self,
                                   part_uid: str,
                                   sutta_quote: Optional[SuttaQuote] = None,
                                   quote_scope = QuoteScope.Sutta):

        res_sutta = self.queries.get_sutta_by_partial_uid(part_uid, sutta_quote, quote_scope)
        if not res_sutta:
            return

        if sutta_quote:
            self._set_query(sutta_quote['quote'])
            self._start_query_worker(sutta_quote['quote'])

        self._show_sutta(res_sutta, sutta_quote)
        self._add_recent(res_sutta)

    def _show_sutta_by_uid(self,
                           uid: str,
                           sutta_quote: Optional[SuttaQuote] = None,
                           quote_scope = QuoteScope.Sutta):

        if len(uid) == 0 and sutta_quote is None:
            return

        if len(uid) == 0 and sutta_quote is not None:
            self._show_sutta_by_quote(sutta_quote)
            return

        if len(uid) > 0 and not self.queries.is_complete_uid(uid):
            self._show_sutta_by_partial_uid(uid, sutta_quote, quote_scope)
            return

        if sutta_quote:
            self._set_query(sutta_quote['quote'])
            self._start_query_worker(sutta_quote['quote'])

        sutta = self.queries.get_sutta_by_uid(uid, sutta_quote, quote_scope)

        if sutta:
            self._show_sutta(sutta, sutta_quote)
            self._add_recent(sutta)
        else:
            logger.info(f"Sutta not found: {uid}")

    def _show_word_by_uid(self, uid: str):
        results: List[UDictWord] = []

        res = self._app_data.db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.uid == uid) \
            .all()
        results.extend(res)

        if len(results) > 0:
            self._app_data.dict_word_to_open = results[0]
            self.pw.action_Dictionary_Search.activate(QAction.ActionEvent.Trigger)

    def _show_sutta(self, sutta: USutta, sutta_quote: Optional[SuttaQuote] = None):
        logger.info(f"_show_sutta() : {sutta.uid}")
        self.showing_query_in_tab = False
        self.sutta_tab.sutta = sutta
        self.sutta_tab.render_sutta_content(sutta_quote)

        self.sutta_tabs.setTabText(0, str(sutta.uid))

        self._add_related_tabs(sutta)

        if self.enable_sidebar:
            self.pw.update_memos_list_for_sutta(sutta)
            self.pw.show_network_graph(sutta)

    def _show_next_recent(self):
        active_sutta = self._get_active_tab().sutta
        if active_sutta is None:
            return

        res = [x for x in range(len(self._recent)) if self._recent[x].uid == active_sutta.uid]

        if len(res) == 0:
            return
        else:
            current_idx = res[0]

        if current_idx + 1 >= len(self._recent):
            # This is already the last, no next item.
            if self.showing_query_in_tab:
                # Re-render it, in case user is trying to restore sutta after a search in the Study Window.
                self._show_sutta(self._recent[current_idx])
            else:
                return
        else:
            self._show_sutta(self._recent[current_idx + 1])

    def _show_prev_recent(self):
        active_sutta = self._get_active_tab().sutta
        if active_sutta is None:
            return

        res = [x for x in range(len(self._recent)) if self._recent[x].uid == active_sutta.uid]

        if len(res) == 0:
            return
        else:
            current_idx = res[0]

        if current_idx == 0:
            # This is already the first, no previous.
            if self.showing_query_in_tab:
                # Re-render it, in case user is trying to restore sutta after a search in the Study Window.
                self._show_sutta(self._recent[current_idx])
            else:
                return
        else:
            self._show_sutta(self._recent[current_idx - 1])

    def _remove_related_tabs(self):
        n = 0
        max_tries = 5
        # Tabs are not removed immediately. Have to repeatedly try to remove the
        # tabs until they are all gone.
        while len(self._related_tabs) > 0 and n < max_tries:
            for idx, tab in enumerate(self._related_tabs):
                del self._related_tabs[idx]
                tab.close()
                tab.deleteLater()

            n += 1

    def _add_related_tabs(self, sutta: USutta):
        self.sutta_tabs.setCurrentIndex(0)
        self._remove_related_tabs()

        # read state from the window action, not from app_data.app_settings, b/c
        # that will be set from windows.py
        if hasattr(self.pw, 'action_Show_Related_Suttas') \
           and not self.pw.action_Show_Related_Suttas.isChecked():
            return

        uid_ref = re.sub('^([^/]+)/.*', r'\1', str(sutta.uid))

        res: List[USutta] = []
        r = self._app_data.db_session \
                          .query(Am.Sutta) \
                          .filter(and_(
                              Am.Sutta.uid != sutta.uid,
                              Am.Sutta.uid.like(f"{uid_ref}/%"),
                          )) \
                          .all()
        res.extend(r)

        r = self._app_data.db_session \
                          .query(Um.Sutta) \
                          .filter(and_(
                              Um.Sutta.uid != sutta.uid,
                              Um.Sutta.uid.like(f"{uid_ref}/%"),
                          )) \
                          .all()
        res.extend(r)

        res_sorted: List[USutta] = []
        res_remain: List[USutta] = []

        # Pali first
        for i in res:
            if i.language == 'pli':
                res_sorted.append(i)
            else:
                res_remain.append(i)

        # sort the remaining items by language
        res_remain.sort(key=lambda x: str(x.language))

        res_sorted.extend(res_remain)

        for sutta in res_sorted:
            if sutta.uid is not None:
                title = str(sutta.uid)
            else:
                title = ""

            self._add_new_tab(title, sutta)

    def reload_page(self):
        self._get_active_tab().render_sutta_content()

    def _handle_copy(self):
        text = self._get_selection()
        if text is not None:
            self._app_data.clipboard_setText(text)

    def _handle_copy_link_to_sutta(self):
        active_sutta = self._get_active_tab().sutta
        if active_sutta is None:
            return

        url = QUrl(f"ssp://{QueryType.suttas.value}/{active_sutta.uid}")

        quote = self._get_selection()
        if quote is not None and len(quote) > 0:
            url.setQuery(urlencode({'q': quote}))

        self._app_data.clipboard_setText(url.toString())

    def _handle_copy_uid(self):
        active_sutta = self._get_active_tab().sutta
        if active_sutta is None:
            return

        uid = 'uid:' + active_sutta.uid
        self._app_data.clipboard_setText(uid)

    def _handle_paste(self):
        s = self._app_data.clipboard_getText()
        if s is not None:
            self._append_to_query(s)
            self._handle_query()

    def _open_in_study_window(self, side: str):
        tab = self._get_active_tab()
        sutta = tab.sutta
        if sutta is None:
            return

        uid: str = sutta.uid # type: ignore
        self.open_in_study_window_signal.emit(side, uid)

    def _lookup_selection_in_suttas(self):
        self.pw.activateWindow()
        text = self._get_selection()
        if text is not None:
            self._set_query(text)
            self._handle_query()

    def _lookup_selection_in_new_sutta_window(self):
        text = self._get_selection()
        if text is not None:
            self.pw.lookup_in_new_sutta_window_signal.emit(text)

    def _lookup_selection_in_dictionary(self):
        text = self._get_selection()
        if text is not None:
            self.pw.lookup_in_dictionary_signal.emit(text)

    def _create_qwe_context_menu(self, menu: QMenu):
        self.qwe_copy_selection = QAction("Copy Selection")
        # NOTE: don't bind Ctrl-C, will be ambiguous to the window menu action
        self.qwe_copy_selection.triggered.connect(partial(self._handle_copy))
        menu.addAction(self.qwe_copy_selection)

        self.qwe_copy_link_to_sutta = QAction("Copy Link to Sutta and Selection")
        self.qwe_copy_link_to_sutta.triggered.connect(partial(self._handle_copy_link_to_sutta))
        menu.addAction(self.qwe_copy_link_to_sutta)

        self.qwe_copy_uid = QAction("Copy uid")
        self.qwe_copy_uid.triggered.connect(partial(self._handle_copy_uid))
        menu.addAction(self.qwe_copy_uid)

        self.qwe_bookmark = QAction("Create Bookmark from Selection")
        self.qwe_bookmark.triggered.connect(partial(self.handle_create_bookmark_for_sutta))
        menu.addAction(self.qwe_bookmark)

        self.qwe_memo = QAction("Create Memo")
        self.qwe_memo.triggered.connect(partial(self.handle_create_memo_for_sutta))
        menu.addAction(self.qwe_memo)

        self.qwe_study_menu = QMenu("Open in Study Window")
        menu.addMenu(self.qwe_study_menu)

        self.qwe_study_left = QAction("Left")
        self.qwe_study_left.triggered.connect(partial(self._open_in_study_window, 'left'))
        self.qwe_study_menu.addAction(self.qwe_study_left)

        self.qwe_study_middle = QAction("Middle")
        self.qwe_study_middle.triggered.connect(partial(self._open_in_study_window, 'middle'))
        self.qwe_study_menu.addAction(self.qwe_study_middle)

        self.qwe_lookup_menu = QMenu("Lookup Selection")
        menu.addMenu(self.qwe_lookup_menu)

        self.qwe_lookup_in_suttas = QAction("In Suttas")
        self.qwe_lookup_in_suttas.triggered.connect(partial(self._lookup_selection_in_suttas))
        self.qwe_lookup_menu.addAction(self.qwe_lookup_in_suttas)

        self.qwe_lookup_in_dictionary = QAction("In Dictionary")
        self.qwe_lookup_in_dictionary.triggered.connect(partial(self._lookup_selection_in_dictionary))
        self.qwe_lookup_menu.addAction(self.qwe_lookup_in_dictionary)

        self.gpt_prompts_menu = QMenu("GPT Prompts")
        menu.addMenu(self.gpt_prompts_menu)

        prompts = self._app_data.db_session \
                                .query(Um.GptPrompt) \
                                .filter(Um.GptPrompt.show_in_context == True) \
                                .all()

        self.gpt_prompts_actions = []

        def _add_action_to_menu(x: Um.GptPrompt):
            a = QAction(str(x.name_path))
            db_id: int = x.id # type: ignore
            a.triggered.connect(partial(self._open_gpt_prompt_with_params, db_id))
            self.gpt_prompts_actions.append(a)
            self.gpt_prompts_menu.addAction(a)

        for i in prompts:
            _add_action_to_menu(i)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/new-window"))

        self.qwe_open_new_action = QAction("Open in New Window")
        self.qwe_open_new_action.setIcon(icon)
        self.qwe_open_new_action.triggered.connect(partial(self._handle_open_content_new))
        menu.addAction(self.qwe_open_new_action)

        tab = self._get_active_tab()

        self.qwe_devtools = QAction("Show Inspector")
        self.qwe_devtools.setCheckable(True)
        self.qwe_devtools.setChecked(tab.devtools_open)
        self.qwe_devtools.triggered.connect(partial(self._toggle_devtools_inspector))
        menu.addAction(self.qwe_devtools)

    def _open_gpt_prompt_with_params(self, prompt_db_id: int):
        tab = self._get_active_tab()
        if tab.sutta is None:
            uid = None
        else:
            uid = str(tab.sutta.uid)

        params = OpenPromptParams(
            prompt_db_id = prompt_db_id,
            with_name = '', # Empty string to clear existing name
            sutta_uid = uid,
            selection_text = self._get_selection(),
        )

        self.open_gpt_prompt.emit(params)

    def _toggle_devtools_inspector(self):
        tab = self._get_active_tab()

        if self.qwe_devtools.isChecked():
            tab._show_devtools()
        else:
            tab._hide_devtools()

    def _handle_open_content_new(self):
        tab = self._get_active_tab()
        if tab.sutta is not None:
            self.open_sutta_new_signal.emit(str(tab.sutta.uid))
        else:
            logger.warn("Sutta is not set")

    def _handle_show_related_suttas(self):
        active_sutta = self._get_active_tab().sutta
        if active_sutta is None:
            return

        if active_sutta is not None:
            self._add_related_tabs(active_sutta)

    def _handle_show_find_panel(self):
        self.find_toolbar.show()
        self._find_panel.search_input.setFocus()

    def _user_typed(self):
        self._handle_autocomplete_query(min_length=4)

        if not self.pw.action_Search_As_You_Type.isChecked():
            return

        matches = re.match(RE_ALL_BOOK_SUTTA_REF, self.search_input.text())
        if matches is not None:
            min_length = 1
        else:
            min_length = 4

        if not self._search_timer.isActive():
            self._search_timer = QTimer()
            self._search_timer.timeout.connect(partial(self._handle_query, min_length=min_length))
            self._search_timer.setSingleShot(True)

        self._search_timer.start(SEARCH_TIMER_SPEED)

    def _handle_search_mode_changed(self):
        idx = self.search_mode_dropdown.currentIndex()
        s = self.search_mode_dropdown.itemText(idx)

        self._app_data.app_settings['sutta_search_mode'] = SuttaSearchModeNameToType[s]
        self._app_data._save_app_settings()

    def _connect_signals(self):
        if hasattr(self, 'search_button'):
            self.search_button.clicked.connect(partial(self._handle_query, min_length=1))

        if hasattr(self, 'search_input'):
            self.search_input.textEdited.connect(partial(self._user_typed))
            self.search_input.returnPressed.connect(partial(self._handle_query, min_length=1))
            self.search_input.completer().activated.connect(partial(self._handle_query, min_length=1))

        if hasattr(self, 'search_mode_dropdown'):
            self.search_mode_dropdown.currentIndexChanged.connect(partial(self._handle_search_mode_changed))

        if hasattr(self, 'back_recent_button'):
            if self.enable_sidebar:
                self.back_recent_button.clicked.connect(partial(self.pw._select_next_recent))
                self.forward_recent_button.clicked.connect(partial(self.pw._select_prev_recent))

                def _handle_sidebar():
                    self.pw.action_Show_Sidebar.activate(QAction.ActionEvent.Trigger)

                self.show_sidebar_btn.clicked.connect(partial(_handle_sidebar))

            else:
                self.back_recent_button.clicked.connect(partial(self._show_next_recent))
                self.forward_recent_button.clicked.connect(partial(self._show_prev_recent))

        if self.enable_language_filter and hasattr(self, 'sutta_language_filter_dropdown'):
            self.sutta_language_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))

        if self.enable_search_extras and hasattr(self, 'sutta_source_filter_dropdown'):
            self.sutta_source_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))

        if self.enable_find_panel:
            self._find_panel.searched.connect(self.on_searched)
            self._find_panel.closed.connect(self.find_toolbar.hide)

            self.pw.action_Find_in_Page \
                .triggered.connect(self._handle_show_find_panel)
