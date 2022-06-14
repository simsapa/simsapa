import json
import math
from pathlib import Path
import queue
import re

from functools import partial
from typing import Any, List, Optional
from PyQt5 import QtCore, QtWidgets, QtGui
from PyQt5.QtCore import Qt, QTimer
from PyQt5.QtGui import QIcon, QCloseEvent, QPixmap, QStandardItem, QStandardItemModel
from PyQt5.QtWidgets import (QComboBox, QAction, QCompleter, QFrame, QHBoxLayout, QLineEdit, QListWidget, QMainWindow, QMessageBox, QPushButton, QSizePolicy, QTabWidget, QToolBar, QVBoxLayout, QWidget)
from PyQt5.QtWebEngineWidgets import QWebEnginePage, QWebEngineSettings, QWebEngineView
from sqlalchemy.sql.elements import and_
# from tomlkit import items

from simsapa import READING_BACKGROUND_COLOR, DbSchemaName, logger, ApiAction, ApiMessage
from simsapa import APP_QUEUES, GRAPHS_DIR, TIMER_SPEED
from simsapa.layouts.find_panel import FindPanel
from simsapa.layouts.import_suttas_dialog import ImportSuttasWithSpreadsheetDialog
from simsapa.layouts.reader_web import ReaderWebEnginePage
from ..app.db.search import SearchResult, SearchQuery, sutta_hit_to_search_result, RE_SUTTA_REF
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from ..app.types import AppData, USutta, UDictWord
from ..assets.ui.sutta_search_window_ui import Ui_SuttaSearchWindow
from .sutta_tab import SuttaTabWidget
from .memo_dialog import HasMemoDialog
from .memos_sidebar import HasMemosSidebar
from .links_sidebar import HasLinksSidebar
from .fulltext_list import HasFulltextList
from .html_content import html_page
from .help_info import show_search_info, setup_info_button
from .sutta_select_dialog import SuttaSelectDialog

class SuttaSearchWindowState(QWidget, HasMemoDialog):

    searchbar_layout: Optional[QHBoxLayout]
    sutta_tabs_layout: Optional[QVBoxLayout]
    tabs_layout: Optional[QVBoxLayout]

    search_extras: QHBoxLayout
    palibuttons_frame: QFrame
    search_input: QLineEdit
    toggle_pali_btn: QPushButton
    _app_data: AppData
    _autocomplete_model: QStandardItemModel
    sutta_tabs: QTabWidget
    sutta_tab: SuttaTabWidget
    _related_tabs: List[SuttaTabWidget]

    def __init__(self,
                 app_data: AppData,
                 parent_window: QMainWindow,
                 searchbar_layout: Optional[QHBoxLayout],
                 sutta_tabs_layout: Optional[QVBoxLayout],
                 tabs_layout: Optional[QVBoxLayout],
                 focus_input: bool = True,
                 enable_search_extras: bool = True,
                 enable_sidebar: bool = True,
                 enable_find_panel: bool = True) -> None:
        super().__init__()

        self.pw = parent_window

        self.enable_search_extras = enable_search_extras
        self.enable_sidebar = enable_sidebar
        self.enable_find_panel = enable_find_panel

        self.searchbar_layout = searchbar_layout
        self.sutta_tabs_layout = sutta_tabs_layout
        self.tabs_layout = tabs_layout

        self.features: List[str] = []
        self._app_data: AppData = app_data

        self.page_len = 20
        self.search_query = SearchQuery(
            self._app_data.search_indexed.suttas_index,
            self.page_len,
            sutta_hit_to_search_result,
        )

        self._results: List[SearchResult] = []
        self._recent: List[USutta] = []

        self._current_sutta: Optional[USutta] = None
        self._related_tabs: List[SuttaTabWidget] = []

        self._autocomplete_model = QStandardItemModel()

        self.focus_input = focus_input

        self._ui_setup()
        self._connect_signals()

        self.init_memo_dialog()

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

    def _ui_setup(self):
        self._setup_search_bar();

        self._setup_sutta_tabs()

        if self.enable_search_extras:
            self._setup_sutta_filter_dropdown()
            self._setup_sutta_select_button()
            self._setup_toggle_pali_button()
            setup_info_button(self.search_extras, self)

            self._setup_pali_buttons()

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
        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Fixed, QtWidgets.QSizePolicy.Fixed)
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

        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Fixed, QtWidgets.QSizePolicy.Fixed)
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

        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Fixed, QtWidgets.QSizePolicy.Fixed)
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

        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Fixed, QtWidgets.QSizePolicy.Fixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.search_button.sizePolicy().hasHeightForWidth())

        self.search_button.setSizePolicy(sizePolicy)
        self.search_button.setMinimumSize(QtCore.QSize(0, 40))

        icon2 = QtGui.QIcon()
        icon2.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.search_button.setIcon(icon2)
        self.searchbar_layout.addWidget(self.search_button)

        self.search_extras = QtWidgets.QHBoxLayout()
        self.searchbar_layout.addLayout(self.search_extras)

        spacerItem = QtWidgets.QSpacerItem(40, 20, QtWidgets.QSizePolicy.Expanding, QtWidgets.QSizePolicy.Minimum)

        self.searchbar_layout.addItem(spacerItem)

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

    def _new_webengine(self) -> QWebEngineView:
        qwe = QWebEngineView()
        qwe.setPage(ReaderWebEnginePage(self))

        qwe.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        # Enable dev tools
        qwe.settings().setAttribute(QWebEngineSettings.JavascriptEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.LocalContentCanAccessRemoteUrls, True)
        qwe.settings().setAttribute(QWebEngineSettings.ErrorPageEnabled, True)
        qwe.settings().setAttribute(QWebEngineSettings.PluginsEnabled, True)

        self._setup_qwe_context_menu(qwe)

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

    def _get_filter_labels(self):
        res = []

        r = self._app_data.db_session.query(Am.Sutta.uid).all()
        res.extend(r)

        r = self._app_data.db_session.query(Um.Sutta.uid).all()
        res.extend(r)

        def _uid_to_label(x):
            return re.sub(r'[^/]+/([^/]+/.*)', r'\1', x['uid'])

        labels = sorted(set(map(_uid_to_label, res)))

        return labels

    def _setup_sutta_filter_dropdown(self):
        cmb = QComboBox()
        items = ["Sources",]
        items.extend(self._get_filter_labels())

        cmb.addItems(items)
        cmb.setFixedHeight(40)
        self.sutta_filter_dropdown = cmb
        self.search_extras.addWidget(self.sutta_filter_dropdown)

    def _setup_sutta_select_button(self):
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

    def _handle_query(self, min_length: int = 4):
        query = self.search_input.text()

        if re.search(RE_SUTTA_REF, query) is None and len(query) < min_length:
            return

        if self.enable_search_extras:
            idx = self.sutta_filter_dropdown.currentIndex()
            source = self.sutta_filter_dropdown.itemText(idx)
            if source == "Sources":
                only_source = None
            else:
                only_source = source
        else:
            only_source = None

        self._handle_autocomplete_query(min_length)
        self._results = self._sutta_search_query(query, only_source)

        if self.enable_sidebar:
            self.pw._update_sidebar_fulltext(self.search_query.hits)

        if self.search_query.hits == 1 and self._results[0]['uid'] is not None:
            self._show_sutta_by_uid(self._results[0]['uid'])

    def _handle_autocomplete_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        self._autocomplete_model.clear()

        res: List[USutta] = []
        r = self._app_data.db_session \
                            .query(Am.Sutta.title) \
                            .filter(Am.Sutta.title.like(f"{query}%")) \
                            .all()
        res.extend(r)

        r = self._app_data.db_session \
                            .query(Um.Sutta.title) \
                            .filter(Um.Sutta.title.like(f"{query}%")) \
                            .all()
        res.extend(r)

        a = set(map(lambda x: x[0], res))

        for i in a:
            self._autocomplete_model.appendRow(QStandardItem(i))

        self._autocomplete_model.sort(0)

    def _sutta_search_query(self, query: str, only_source: Optional[str] = None) -> List[SearchResult]:
        disabled_labels = self._app_data.app_settings.get('disabled_sutta_labels', None)
        results = self.search_query.new_query(query, disabled_labels, only_source)

        return results

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
                                  .filter(Am.Sutta.id == x['db_id']) \
                                  .first()
        else:
            sutta = self._app_data.db_session \
                                  .query(Um.Sutta) \
                                  .filter(Um.Sutta.id == x['db_id']) \
                                  .first()
        return sutta

    @QtCore.pyqtSlot(str, QWebEnginePage.FindFlag)
    def on_searched(self, text: str, flag: QWebEnginePage.FindFlag):
        def callback(found):
            if text and not found:
                logger.info('Not found')

        tab = self._get_active_tab()
        tab.qwe.findText(text, flag, callback)

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

    def _show_sutta_by_uid(self, uid: str):
        results: List[USutta] = []

        res = self._app_data.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.Sutta) \
            .filter(Um.Sutta.uid == uid) \
            .all()
        results.extend(res)

        if len(results) > 0:
            self._show_sutta(results[0])
            self._add_recent(results[0])

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
            self.action_Dictionary_Search.activate(QAction.ActionEvent.Trigger)

    def _show_sutta(self, sutta: USutta):
        logger.info(f"_show_sutta() : {sutta.uid}")
        self._current_sutta = sutta
        self.sutta_tab.sutta = sutta
        self.sutta_tab.render_sutta_content()

        self.sutta_tabs.setTabText(0, str(sutta.uid))

        self._add_related_tabs(sutta)

        if self.enable_sidebar:
            self.pw.update_memos_list_for_sutta(sutta)
            self.pw.show_network_graph(sutta)

    def _show_next_recent(self):
        if self._current_sutta is None or len(self._recent) < 2:
            return

        res = [x for x in range(len(self._recent)) if self._recent[x].uid == self._current_sutta.uid]

        if len(res) == 0:
            return
        else:
            current_idx = res[0]

        # This is already the last, no next item
        if current_idx + 1 >= len(self._recent):
            return

        self._show_sutta(self._recent[current_idx + 1])

    def _show_prev_recent(self):
        if self._current_sutta is None or len(self._recent) < 2:
            return

        res = [x for x in range(len(self._recent)) if self._recent[x].uid == self._current_sutta.uid]

        if len(res) == 0:
            return
        else:
            current_idx = res[0]

        # This is already the first, no previous
        if current_idx == 0:
            return

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
        if not self.pw.action_Show_Related_Suttas.isChecked():
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

    def _handle_copy(self):
        text = self._get_selection()
        if text is not None:
            self._app_data.clipboard_setText(text)

    def _handle_copy_uid(self):
        if self._current_sutta is None:
            return

        uid = 'uid:' + self._current_sutta.uid
        self._app_data.clipboard_setText(uid)

    def _handle_paste(self):
        s = self._app_data.clipboard_getText()
        if s is not None:
            self._append_to_query(s)
            self._handle_query()

    def _open_in_study_window(self, side: str):
        tab = self._get_active_tab()
        sutta = tab.sutta
        if sutta is None or self._app_data.actions_manager is None:
            return

        uid: str = sutta.uid # type: ignore
        self._app_data.actions_manager.open_in_study_window(side, uid)

    def _setup_qwe_context_menu(self, qwe: QWebEngineView):
        qwe.setContextMenuPolicy(Qt.ContextMenuPolicy.ActionsContextMenu)

        copyAction = QAction("Copy Selection", qwe)
        # NOTE: don't bind Ctrl-C, will be ambiguous to the window menu action
        copyAction.triggered.connect(partial(self._handle_copy))

        qwe.addAction(copyAction)

        copyUidAction = QAction("Copy uid", qwe)
        copyUidAction.triggered.connect(partial(self._handle_copy_uid))

        qwe.addAction(copyUidAction)

        memoAction = QAction("Create Memo", qwe)
        memoAction.triggered.connect(partial(self.handle_create_memo_for_sutta))

        qwe.addAction(memoAction)

        studyLeftAction = QAction("Open in Study Window: Left", qwe)
        studyLeftAction.triggered.connect(partial(self._open_in_study_window, 'left'))

        qwe.addAction(studyLeftAction)

        studyMiddleAction = QAction("Open in Study Window: Middle", qwe)
        studyMiddleAction.triggered.connect(partial(self._open_in_study_window, 'middle'))

        qwe.addAction(studyMiddleAction)

        lookupSelectionInSuttas = QAction("Lookup Selection in Suttas", qwe)
        lookupSelectionInSuttas.triggered.connect(partial(self.pw._lookup_selection_in_suttas))

        qwe.addAction(lookupSelectionInSuttas)

        lookupSelectionInDictionary = QAction("Lookup Selection in Dictionary", qwe)
        lookupSelectionInDictionary.triggered.connect(partial(self.pw._lookup_selection_in_dictionary))

        qwe.addAction(lookupSelectionInDictionary)

    def _handle_show_related_suttas(self):
        if self._current_sutta is not None:
            self._add_related_tabs(self._current_sutta)

    def _handle_show_find_panel(self):
        self.find_toolbar.show()
        self._find_panel.search_input.setFocus()

    def _connect_signals(self):
        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textEdited.connect(partial(self._handle_query, min_length=4))
        # NOTE search_input.returnPressed removes the selected completion and uses the typed query
        self.search_input.completer().activated.connect(partial(self._handle_query, min_length=1))

        if self.enable_sidebar:
            self.back_recent_button.clicked.connect(partial(self.pw._select_next_recent))
            self.forward_recent_button.clicked.connect(partial(self.pw._select_prev_recent))
        else:
            self.back_recent_button.clicked.connect(partial(self._show_next_recent))
            self.forward_recent_button.clicked.connect(partial(self._show_prev_recent))

        if self.enable_search_extras:
            self.sutta_filter_dropdown.currentIndexChanged.connect(partial(self._handle_query, min_length=4))

        if self.enable_find_panel:
            self._find_panel.searched.connect(self.on_searched) # type: ignore
            self._find_panel.closed.connect(self.find_toolbar.hide)

            self.pw.action_Find_in_Page \
                .triggered.connect(self._handle_show_find_panel)

class SuttaSearchWindow(QMainWindow, Ui_SuttaSearchWindow, HasLinksSidebar,
                        HasMemosSidebar, HasFulltextList):

    searchbar_layout: QHBoxLayout
    sutta_tabs_layout: QVBoxLayout
    tabs_layout: QVBoxLayout
    selected_info: Any
    recent_list: QListWidget

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        self.setupUi(self)
        logger.info("SuttaSearchWindow()")

        self._app_data = app_data

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        APP_QUEUES[self.queue_id] = queue.Queue()
        self.messages_url = f'{self._app_data.api_url}/queues/{self.queue_id}'

        self.selected_info = {}

        self.graph_path: Path = GRAPHS_DIR.joinpath(f"{self.queue_id}.html")

        self.timer = QTimer()
        self.timer.timeout.connect(self.handle_messages)
        self.timer.start(TIMER_SPEED)

        self._ui_setup()

        self.s = SuttaSearchWindowState(app_data,
                                        self,
                                        self.searchbar_layout,
                                        self.sutta_tabs_layout,
                                        self.tabs_layout)

        self.page_len = self.s.page_len
        self.search_query = self.s.search_query

        self._connect_signals()

        self.init_fulltext_list()
        self.init_memos_sidebar()
        self.init_links_sidebar()

    def _ui_setup(self):
        self.links_tab_idx = 1
        self.memos_tab_idx = 2

        show = self._app_data.app_settings.get('show_related_suttas', True)
        self.action_Show_Related_Suttas.setChecked(show)

    def closeEvent(self, event: QCloseEvent):
        if self.queue_id in APP_QUEUES.keys():
            del APP_QUEUES[self.queue_id]

        if self.graph_path.exists():
            self.graph_path.unlink()

        event.accept()

    def handle_messages(self):
        if self.queue_id in APP_QUEUES.keys():
            try:
                s = APP_QUEUES[self.queue_id].get_nowait()
                msg: ApiMessage = json.loads(s)
                if msg['action'] == ApiAction.show_sutta:
                    info = json.loads(msg['data'])
                    self.s._show_sutta_from_message(info)

                elif msg['action'] == ApiAction.show_sutta_by_uid:
                    info = json.loads(msg['data'])
                    if 'uid' in info.keys():
                        self.s._show_sutta_by_uid(info['uid'])

                elif msg['action'] == ApiAction.show_word_by_uid:
                    info = json.loads(msg['data'])
                    if 'uid' in info.keys():
                        self.s._show_word_by_uid(info['uid'])

                elif msg['action'] == ApiAction.lookup_clipboard_in_suttas:
                    self._lookup_clipboard_in_suttas()

                elif msg['action'] == ApiAction.lookup_in_suttas:
                    text = msg['data']
                    self.s._set_query(text)
                    self.s._handle_query()

                elif msg['action'] == ApiAction.set_selected:
                    info = json.loads(msg['data'])
                    self.selected_info = info

                APP_QUEUES[self.queue_id].task_done()
            except queue.Empty:
                pass

    def _lookup_clipboard_in_suttas(self):
        self.activateWindow()
        s = self._app_data.clipboard_getText()
        if s is not None:
            self.s._set_query(s)
            self.s._handle_query()

    def _lookup_clipboard_in_dictionary(self):
        text = self._app_data.clipboard_getText()
        if text is not None and self._app_data.actions_manager is not None:
            self._app_data.actions_manager.lookup_in_dictionary(text)

    def _lookup_selection_in_suttas(self):
        self.activateWindow()
        text = self.s._get_selection()
        if text is not None:
            self.s._set_query(text)
            self.s._handle_query()

    def _lookup_selection_in_dictionary(self):
        text = self.s._get_selection()
        if text is not None and self._app_data.actions_manager is not None:
            self._app_data.actions_manager.lookup_in_dictionary(text)

    def show_network_graph(self, sutta: Optional[USutta] = None):
        if sutta is None:
            if self.s._current_sutta is None:
                return
            else:
                sutta = self.s._current_sutta

        self.generate_and_show_graph(sutta, None, self.queue_id, self.graph_path, self.messages_url)

    def _update_sidebar_fulltext(self, hits: int):
        if hits > 0:
            self.rightside_tabs.setTabText(0, f"Fulltext ({hits})")
        else:
            self.rightside_tabs.setTabText(0, "Fulltext")

        self.render_fulltext_page()

        if hits == 0:
            self.fulltext_page_input.setMinimum(0)
            self.fulltext_page_input.setMaximum(0)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        elif hits <= self.page_len:
            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(1)
            self.fulltext_first_page_btn.setEnabled(False)
            self.fulltext_last_page_btn.setEnabled(False)

        else:
            pages = math.floor(hits / self.page_len) + 1
            self.fulltext_page_input.setMinimum(1)
            self.fulltext_page_input.setMaximum(pages)
            self.fulltext_first_page_btn.setEnabled(True)
            self.fulltext_last_page_btn.setEnabled(True)

    def _show_selected(self):
        self.s._show_sutta_from_message(self.selected_info)

    def _handle_result_select(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx < len(self.s._results):
            sutta = self.s._sutta_from_result(self.s._results[selected_idx])
            if sutta is not None:
                self.s._add_recent(sutta)
                self.s._show_sutta(sutta)

    def _handle_recent_select(self):
        selected_idx = self.recent_list.currentRow()
        sutta: USutta = self.s._recent[selected_idx]
        self.s._show_sutta(sutta)

    def _set_recent_list(self, titles: List[str]):
        self.recent_list.clear()
        self.recent_list.insertItems(0, titles)

    def _select_prev_recent(self):
        selected_idx = self.recent_list.currentRow()
        if selected_idx == -1:
            self.recent_list.setCurrentRow(0)
        elif selected_idx == 0:
            return
        else:
            self.recent_list.setCurrentRow(selected_idx - 1)

    def _select_next_recent(self):
        selected_idx = self.recent_list.currentRow()
        # List is empty or lost focus (no selected item)
        if selected_idx == -1:
            if len(self.recent_list) == 1:
                # Only one viewed item, which is presently being shown, and no
                # next item
                return
            else:
                # The 0 index is already the presently show item
                self.recent_list.setCurrentRow(1)

        elif selected_idx + 1 < len(self.recent_list):
            self.recent_list.setCurrentRow(selected_idx + 1)

    def _select_prev_result(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx == -1:
            self.fulltext_list.setCurrentRow(0)
        elif selected_idx == 0:
            return
        else:
            self.fulltext_list.setCurrentRow(selected_idx - 1)

    def _select_next_result(self):
        selected_idx = self.fulltext_list.currentRow()
        if selected_idx == -1:
            self.fulltext_list.setCurrentRow(0)
        elif selected_idx + 1 < len(self.fulltext_list):
            self.fulltext_list.setCurrentRow(selected_idx + 1)

    def _focus_search_input(self):
        self.s.search_input.setFocus()

    def _increase_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        self._app_data.app_settings['sutta_font_size'] = font_size + 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _decrease_text_size(self):
        font_size = self._app_data.app_settings.get('sutta_font_size', 22)
        if font_size < 5:
            return
        self._app_data.app_settings['sutta_font_size'] = font_size - 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _increase_text_margins(self):
        # increase margins = smaller max with
        max_width = self._app_data.app_settings.get('sutta_max_width', 75)
        if max_width < 10:
            return
        self._app_data.app_settings['sutta_max_width'] = max_width - 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _decrease_text_margins(self):
        # decrease margins = greater max with
        max_width = self._app_data.app_settings.get('sutta_max_width', 75)
        self._app_data.app_settings['sutta_max_width'] = max_width + 2
        self._app_data._save_app_settings()
        self.s._get_active_tab().render_sutta_content()

    def _show_import_suttas_dialog(self):
        d = ImportSuttasWithSpreadsheetDialog(self._app_data, self)
        if d.exec():
            logger.info("Finished importing suttas")
            self._show_quit_and_restart()

    def _remove_imported_suttas(self):
        msg = """
        <p>Remove all imported suttas from the user database?</p>
        <p>(Suttas stored in the default application database will remain.)</p>
        """

        reply = QMessageBox.question(self,
                                     "Remove Imported Suttas",
                                     msg,
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)

        if reply == QMessageBox.Yes:
            suttas = self._app_data.db_session.query(Um.Sutta).all()

            uids = list(map(lambda x: x.uid, suttas))

            for db_item in suttas:
                self._app_data.db_session.delete(db_item)

            self._app_data.db_session.commit()

            w = self._app_data.search_indexed.suttas_index.writer()
            for x in uids:
                w.delete_by_term('uid', x)

            w.commit()

            self._show_quit_and_restart()

    def _show_quit_and_restart(self):
        msg = """
        <p>Restart Simsapa for the changes to take effect.</p>
        <p>Quit now?</p>
        """

        reply = QMessageBox.question(self,
                                     "Restart Simsapa",
                                     msg,
                                     QMessageBox.Yes | QMessageBox.No,
                                     QMessageBox.No)

        if reply == QMessageBox.Yes:
            self.action_Quit.activate(QAction.ActionEvent.Trigger)

    def _connect_signals(self):
        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Copy \
            .triggered.connect(partial(self.s._handle_copy))

        self.action_Paste \
            .triggered.connect(partial(self.s._handle_paste))

        self.action_Search_Query_Terms \
            .triggered.connect(partial(show_search_info, self))

        self.action_Select_Sutta_Authors \
            .triggered.connect(partial(self.s._show_sutta_select_dialog))

        self.action_Show_Related_Suttas \
            .triggered.connect(partial(self.s._handle_show_related_suttas))

        self.action_Lookup_Selection_in_Dictionary \
            .triggered.connect(partial(self._lookup_selection_in_dictionary))

        self.action_Lookup_Clipboard_in_Suttas \
            .triggered.connect(partial(self._lookup_clipboard_in_suttas))

        self.action_Lookup_Clipboard_in_Dictionary \
            .triggered.connect(partial(self._lookup_clipboard_in_dictionary))

        self.action_Previous_Result \
            .triggered.connect(partial(self._select_prev_result))

        self.action_Next_Result \
            .triggered.connect(partial(self._select_next_result))

        self.action_Previous_Tab \
            .triggered.connect(partial(self.s._select_prev_tab))

        self.action_Next_Tab \
            .triggered.connect(partial(self.s._select_next_tab))

        self.recent_list.itemSelectionChanged.connect(partial(self._handle_recent_select))

        self.add_memo_button \
            .clicked.connect(partial(self.add_memo_for_sutta))

        self.action_Increase_Text_Size \
            .triggered.connect(partial(self._increase_text_size))

        self.action_Decrease_Text_Size \
            .triggered.connect(partial(self._decrease_text_size))

        self.action_Increase_Text_Margins \
            .triggered.connect(partial(self._increase_text_margins))

        self.action_Decrease_Text_Margins \
            .triggered.connect(partial(self._decrease_text_margins))

        self.action_Import_Suttas_with_Spreadsheet \
            .triggered.connect(partial(self._show_import_suttas_dialog))

        self.action_Remove_Imported_Suttas \
            .triggered.connect(partial(self._remove_imported_suttas))
