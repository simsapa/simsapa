from datetime import datetime
from functools import partial
from typing import Any, Callable, List, Optional

import re
from urllib.parse import urlencode

from PyQt6 import QtCore
from PyQt6.QtCore import QTimer, QUrl, pyqtSignal, QSize
from PyQt6.QtGui import QIcon, QPixmap, QStandardItemModel, QAction
from PyQt6.QtWidgets import (QComboBox, QFrame, QHBoxLayout, QLineEdit, QMenu, QPushButton, QTabWidget, QToolBar, QVBoxLayout)

from sqlalchemy import and_

from simsapa import READING_BACKGROUND_COLOR, DbSchemaName, SearchResult, logger, QueryType, SuttaQuote, QuoteScope

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.helpers import is_book_sutta_ref, is_complete_sutta_uid

from simsapa.app.types import SearchArea, USutta, UDictWord
from simsapa.app.app_data import AppData
from simsapa.layouts.gui_helpers import is_sutta_search_window, is_sutta_study_window

from simsapa.layouts.gui_types import OpenPromptParams, SuttaSearchWindowStateInterface, SuttaSearchWindowInterface, LinkHoverData, WindowType
from simsapa.layouts.gui_types import sutta_quote_from_url
from simsapa.layouts.gui_queries import GuiSearchQueries
from simsapa.layouts.preview_window import PreviewWindow
from simsapa.layouts.find_panel import FindSearched, FindPanel
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.simsapa_webengine import SimsapaWebEngine
from simsapa.layouts.sutta_tab import SuttaTabWidget
from simsapa.layouts.html_content import html_page

from simsapa.layouts.parts.search_bar import HasSearchBar
from simsapa.layouts.parts.memo_dialog import HasMemoDialog
from simsapa.layouts.parts.bookmark_dialog import HasBookmarkDialog


class SuttaSearchWindowState(SuttaSearchWindowStateInterface,
                             HasSearchBar, HasMemoDialog, HasBookmarkDialog):

    searchbar_layout: Optional[QHBoxLayout]
    sutta_tabs_layout: Optional[QVBoxLayout]
    tabs_layout: Optional[QVBoxLayout]

    search_extras: QHBoxLayout
    palibuttons_frame: QFrame
    search_input: QLineEdit
    toggle_pali_btn: QPushButton
    sutta_tabs: QTabWidget
    sutta_tab: SuttaTabWidget
    search_mode_dropdown: QComboBox
    show_url_action_fn: Callable
    _app_data: AppData
    _queries: GuiSearchQueries
    _related_tabs: List[SuttaTabWidget]
    _search_timer = QTimer()
    _last_query_time = datetime.now()

    open_sutta_new_signal = pyqtSignal(str)
    # queue_id, side, uid
    open_in_study_window_signal = pyqtSignal([str, str, str])
    link_mouseover = pyqtSignal(dict)
    link_mouseleave = pyqtSignal(str)
    page_dblclick = pyqtSignal()
    hide_preview = pyqtSignal()
    bookmark_edit = pyqtSignal(str)
    show_find_panel = pyqtSignal(str)
    open_gpt_prompt = pyqtSignal(dict)

    def __init__(self,
                 app_data: AppData,
                 parent_window: SuttaSearchWindowInterface,
                 searchbar_layout: Optional[QHBoxLayout],
                 sutta_tabs_layout: Optional[QVBoxLayout],
                 tabs_layout: Optional[QVBoxLayout],
                 focus_input: bool = True,
                 enable_nav_buttons: bool = True,
                 enable_language_filter: bool = True,
                 enable_search_extras: bool = True,
                 enable_regex_fuzzy: bool = True,
                 enable_info_button: bool = True,
                 enable_sidebar_button: bool = True,
                 enable_sidebar: bool = True,
                 enable_find_panel: bool = True,
                 create_find_toolbar: bool = True,
                 show_query_results_in_active_tab: bool = False,
                 search_bar_two_rows_layout=False,
                 language_filter_setting_key = 'sutta_language_filter_idx',
                 search_mode_setting_key = 'sutta_search_mode',
                 source_filter_setting_key = 'sutta_source_filter_idx',
                 custom_create_context_menu_fn: Optional[Callable] = None) -> None:
        super().__init__()

        self.pw = parent_window

        self.enable_language_filter = enable_language_filter
        self.enable_search_extras = enable_search_extras
        self.enable_sidebar = enable_sidebar
        self.enable_find_panel = enable_find_panel
        self.create_find_toolbar = create_find_toolbar

        self.searchbar_layout = searchbar_layout
        self.sutta_tabs_layout = sutta_tabs_layout
        self.tabs_layout = tabs_layout

        self.query_in_tab = show_query_results_in_active_tab
        self.showing_query_in_tab = False

        self.custom_create_context_menu_fn = custom_create_context_menu_fn

        self.features: List[str] = []
        self._app_data: AppData = app_data

        self._queries = GuiSearchQueries(self._app_data.db_session,
                                         None,
                                         self._app_data.get_search_indexes,
                                         self._app_data.api_url)

        self.show_url_action_fn = self._show_sutta_by_url

        self.page_len = 20

        self._recent: List[USutta] = []

        self._related_tabs: List[SuttaTabWidget] = []

        self._autocomplete_model = QStandardItemModel()

        self.focus_input = focus_input

        self._search_mode_setting_key = search_mode_setting_key
        self._language_filter_setting_key = language_filter_setting_key
        self._source_filter_setting_key = source_filter_setting_key

        self._setup_ui()

        if self.searchbar_layout is not None:
            if search_bar_two_rows_layout:
                icons_height = 35
            else:
                icons_height = 40

            self.init_search_bar(wrap_layout            = self.searchbar_layout,
                                 search_area            = SearchArea.Suttas,
                                 enable_nav_buttons     = enable_nav_buttons,
                                 enable_language_filter = enable_language_filter,
                                 enable_search_extras   = enable_search_extras,
                                 enable_regex_fuzzy     = enable_regex_fuzzy,
                                 enable_info_button     = enable_info_button,
                                 enable_sidebar_button  = enable_sidebar_button,
                                 input_fixed_size       = QSize(250, icons_height),
                                 icons_height           = icons_height,
                                 focus_input            = True,
                                 two_rows_layout        = search_bar_two_rows_layout)

        self._connect_signals()

        self.init_bookmark_dialog()
        self.init_memo_dialog()

    def get_page_num(self) -> int:
        return self.pw.get_page_num()

    def start_loading_animation(self):
        self.pw.start_loading_animation()

    def stop_loading_animation(self):
        self.pw.start_loading_animation()

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
        self.pw.setWindowTitle("Sutta Search - Simsapa")
        self._setup_sutta_tabs()

        if self.enable_find_panel:
            self._find_panel = FindPanel()

            if self.create_find_toolbar:
                self.find_toolbar = QToolBar()
                self.find_toolbar.addWidget(self._find_panel)

                self.pw.addToolBar(QtCore.Qt.ToolBarArea.BottomToolBarArea, self.find_toolbar)
                self.find_toolbar.hide()

    def _setup_sutta_tabs(self):
        if self.sutta_tabs_layout is None:
            return

        self.sutta_tabs = QTabWidget()
        self.sutta_tabs.setStyleSheet("*[style_class='sutta_tab'] { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.sutta_tab = SuttaTabWidget(self._app_data, "Sutta", 0, self._new_webengine())
        self.sutta_tab.setProperty('style_class', 'sutta_tab')
        layout = self.sutta_tab.layout()
        if layout is not None:
            layout.setContentsMargins(0, 0, 0, 0)

        self.sutta_tabs.addTab(self.sutta_tab, "Sutta")

        html = html_page('', self._app_data.api_url)
        self.sutta_tab.set_qwe_html(html)

        self.sutta_tabs_layout.addWidget(self.sutta_tabs)

        self.sutta_tabs.currentChanged.connect(partial(self._set_window_title_from_active_tab))

    def _link_mouseover(self, hover_data: LinkHoverData):
        self.link_mouseover.emit(hover_data)

    def _link_mouseleave(self, href: str):
        self.link_mouseleave.emit(href)

    def _page_dblclick(self):
        if self._app_data.app_settings['double_click_word_lookup']:
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
        page.helper.show_find_panel.connect(partial(self._emit_show_find_panel))

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

    def _exact_query_finished(self, _):
        pass

    def _search_query_finished(self, query_started_time: datetime):
        logger.info("_search_query_finished()")

        if query_started_time != self._last_query_time:
            return

        if not self._queries.all_finished():
            return

        self.pw.stop_loading_animation()

        # Restore the search icon, processing finished
        self._show_search_normal_icon()

        hits = self._queries.query_hits()
        if self.enable_sidebar:
            results = self.pw._update_sidebar_fulltext(hits)
        else:
            results = self._queries.results_page(0)

        if len(results) > 0 and hits == 1 and results[0]['uid'] is not None:
            self._show_sutta_by_uid(results[0]['uid'])

        elif self.query_in_tab:
            self._render_results_in_active_tab(hits)

    def _handle_query(self, min_length: int = 4):
        query_text_orig = self.search_input.text().strip()
        logger.info(f"_handle_query(): {query_text_orig}, {min_length}")

        # Re-render the current sutta, in case user is trying to restore sutta
        # after a search in the Study Window with the clear input button.
        if len(query_text_orig) == 0 and self.showing_query_in_tab and self._get_active_tab().sutta is not None:
            self._get_active_tab().render_sutta_content()
            return

        if is_book_sutta_ref(query_text_orig):
            min_length = 1

        if len(query_text_orig) < min_length:
            return

        # Not aborting, show the user that the app started processsing
        self._show_search_stopwatch_icon()

        self._start_query_workers(query_text_orig)

    def _handle_exact_query(self, min_length: int = 4):
        logger.info("STUB: _handle_exact_query() %s" % str(min_length))
        pass

    def _render_results_in_active_tab(self, hits: Optional[int]):
        if hits is not None and hits == 0:
            return

        self.showing_query_in_tab = True

        # .all_results() takes too long to highlight and render.
        # res = self._queries.all_results(sort_by_score=True)
        #
        # Collect and render only the first page of results. In the Study
        # Window, the user is not typically searching around, but wants to
        # retreive a specific sutta by title or sutta reference.

        res = self._queries.results_page(0)
        self._get_active_tab().render_search_results(res)

    def _sutta_search_query(self, __query__: str, __only_lang__: Optional[str] = None, __only_source__: Optional[str] = None) -> List[SearchResult]:
        # TODO This is a synchronous version of _start_query_worker(), still
        # used in links_browser.py. Update and use the background thread worker.

        # FIXME
        return []

        # self._init_search_query_workers(query)

        # # first page results
        # a = []
        # # for i in self.search_query_workers:
        # #     i.search_query.new_query(query, only_source)
        # #     a.extend(i.results_page(0))

        # res = sorted(a, key=lambda x: x['score'] or 0, reverse = True)

        # return res

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

        elif x['schema_name'] == DbSchemaName.UserData.value:
            sutta = self._app_data.db_session \
                                  .query(Um.Sutta) \
                                  .filter(Um.Sutta.uid == x['uid']) \
                                  .first()

        else:
            raise Exception("Only appdata and userdata schema are allowed.")

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

        if 'table' not in info.keys() or 'id' not in info.keys():
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
        self._start_query_workers(sutta_quote['quote'])

        results = self._queries.sutta_queries.get_suttas_by_quote(sutta_quote['quote'])

        if len(results) > 0:
            self._show_sutta(results[0], sutta_quote)
            self._add_recent(results[0])

    def _show_sutta_by_partial_uid(self,
                                   part_uid: str,
                                   sutta_quote: Optional[SuttaQuote] = None,
                                   quote_scope = QuoteScope.Sutta):

        res_sutta = self._queries.sutta_queries.get_sutta_by_partial_uid(part_uid, sutta_quote, quote_scope)
        if not res_sutta:
            return

        if sutta_quote:
            self._set_query(sutta_quote['quote'])
            self._start_query_workers(sutta_quote['quote'])

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

        if len(uid) > 0 and not is_complete_sutta_uid(uid):
            self._show_sutta_by_partial_uid(uid, sutta_quote, quote_scope)
            return

        # NOTE: Don't use _set_query(sutta_quote['quote']) here, it intereferes with Sutta Study links.

        sutta = self._queries.sutta_queries.get_sutta_by_uid(uid, sutta_quote, quote_scope)

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

        res = self._app_data.db_session \
            .query(Dpd.PaliWord) \
            .filter(Dpd.PaliWord.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Dpd.PaliRoot) \
            .filter(Dpd.PaliRoot.uid == uid) \
            .all()
        results.extend(res)

        if len(results) > 0:
            self._app_data.dict_word_to_open = results[0]
            self.pw.action_Dictionary_Search.activate(QAction.ActionEvent.Trigger)

    def _set_window_title_from_active_tab(self):
        s = self._get_active_tab().sutta
        if s is not None:
            a = [str(i) for i in [s.sutta_ref, s.title, f"({s.uid})"] if i is not None and i != ""]
            title = " ".join(a)
            self.pw.setWindowTitle(f"{title} - Simsapa")

    def _show_sutta(self, sutta: USutta, sutta_quote: Optional[SuttaQuote] = None):
        logger.info(f"_show_sutta() : {sutta.uid}")
        self.showing_query_in_tab = False
        self.sutta_tab.sutta = sutta
        self.sutta_tab.render_sutta_content(sutta_quote)

        self.sutta_tabs.setTabText(0, str(sutta.uid))

        self._set_window_title_from_active_tab()

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

        api_url = self._app_data.api_url
        if api_url is None:
            api_url = "ssp://"

        # http://localhost:4848/suttas/dhp167-178/pli/ms?quote=andhabhūto+ayaṁ+loko&window_type=Sutta+Study
        url = QUrl(f"{api_url}/{QueryType.suttas.value}/{active_sutta.uid}")

        query = dict()

        quote = self._get_selection()
        if quote is not None and len(quote) > 0:
            query['quote'] = quote

        window_type: Optional[WindowType] = None
        if is_sutta_search_window(self.pw):
            window_type = WindowType.SuttaSearch
        elif is_sutta_study_window(self.pw):
            window_type = WindowType.SuttaStudy

        if window_type is not None:
            query['window_type'] = window_type.value

        url.setQuery(urlencode(query))

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

        uid: str = sutta.uid
        self.open_in_study_window_signal.emit(self.pw.queue_id, side, uid)

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

        self.qwe_study_one = QAction("Panel 1")
        self.qwe_study_one.triggered.connect(partial(self._open_in_study_window, 'panel_one'))
        self.qwe_study_menu.addAction(self.qwe_study_one)

        self.qwe_study_two = QAction("Panel 2")
        self.qwe_study_two.triggered.connect(partial(self._open_in_study_window, 'panel_two'))
        self.qwe_study_menu.addAction(self.qwe_study_two)

        self.qwe_study_three = QAction("Panel 3")
        self.qwe_study_three.triggered.connect(partial(self._open_in_study_window, 'panel_three'))
        self.qwe_study_menu.addAction(self.qwe_study_three)

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
            db_id: int = x.id
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

    def _emit_show_find_panel(self, text = ''):
        self.show_find_panel.emit(text)

    def connect_preview_window_signals(self, preview_window: PreviewWindow):
        self.link_mouseover.connect(partial(preview_window.link_mouseover))
        self.link_mouseleave.connect(partial(preview_window.link_mouseleave))
        self.hide_preview.connect(partial(preview_window._do_hide))

    def _connect_signals(self):
        if hasattr(self, 'back_recent_button'):
            if self.enable_sidebar:
                self.back_recent_button.clicked.connect(partial(self.pw._select_next_recent))
                self.forward_recent_button.clicked.connect(partial(self.pw._select_prev_recent))

            else:
                self.back_recent_button.clicked.connect(partial(self._show_next_recent))
                self.forward_recent_button.clicked.connect(partial(self._show_prev_recent))

        if self.enable_find_panel:
            self._find_panel.searched.connect(self.on_searched)

            if self.create_find_toolbar:
                self._find_panel.closed.connect(self.find_toolbar.hide)
                self.pw.action_Find_in_Page \
                    .triggered.connect(partial(self._handle_show_find_panel))
