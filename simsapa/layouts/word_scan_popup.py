from functools import partial
import math
from typing import List, Optional
from PyQt5.QtCore import QPoint, QUrl, Qt
from PyQt5.QtGui import QClipboard, QCloseEvent, QCursor, QIcon, QKeySequence, QMouseEvent, QPixmap, QStandardItemModel, QStandardItem
from PyQt5.QtWebEngineWidgets import QWebEngineView
from PyQt5.QtWidgets import QAction, QCompleter, QDesktopWidget, QDialog, QFrame, QHBoxLayout, QLabel, QLineEdit, QListWidget, QPushButton, QSizePolicy, QSpacerItem, QSpinBox, QTabWidget, QVBoxLayout, QWidget

from simsapa import DARK_READING_BACKGROUND_COLOR, IS_MAC, READING_BACKGROUND_COLOR, SIMSAPA_PACKAGE_DIR, logger
from simsapa.app.db.search import SearchQuery, SearchResult, dict_word_hit_to_search_result
from simsapa.app.types import AppData, UDictWord, WindowPosSize
from simsapa.layouts.dictionary_queries import DictionaryQueries
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.results_list import HasResultsList

CSS_EXTRA = "html { font-size: 14px; }"

class WordScanPopup(QDialog, HasResultsList):

    search_input: QLineEdit
    content_layout: QVBoxLayout
    content_html: QWebEngineView
    _app_data: AppData
    _results: List[SearchResult]
    _clipboard: Optional[QClipboard]
    _autocomplete_model: QStandardItemModel

    def __init__(self, app_data: AppData) -> None:
        super().__init__()

        self.features: List[str] = []
        self._app_data: AppData = app_data
        self._results: List[SearchResult] = []

        self.page_len = 20
        self.search_query = SearchQuery(
            self._app_data.search_indexed.dict_words_index,
            self.page_len,
            dict_word_hit_to_search_result,
        )

        self.queries = DictionaryQueries(self._app_data)
        self._autocomplete_model = QStandardItemModel()

        self.setWindowTitle("Clipboard Scanning Word Lookup")
        self.setMinimumSize(50, 50)
        self.setMouseTracking(True)

        self._restore_size_pos()

        flags = Qt.WindowType.Dialog | \
            Qt.WindowType.CustomizeWindowHint | \
            Qt.WindowType.WindowStaysOnTopHint | \
            Qt.WindowType.FramelessWindowHint | \
            Qt.WindowType.BypassWindowManagerHint | \
            Qt.WindowType.X11BypassWindowManagerHint

        self.setWindowFlags(flags) # type: ignore

        self._clipboard = self._app_data.clipboard

        self.setObjectName("WordScanPopup")
        self.setStyleSheet("#WordScanPopup { background-color: %s; border: 1px solid #ababab; }" % READING_BACKGROUND_COLOR)

        self._resized = False
        self._left_pressed = False
        self._margin = 8
        self._cursor = QCursor()
        self._dragMenuBarOnlyWayToMoveWindowFlag = False

        self.__init_position()
        self.oldPos = self.pos()

        self._ui_setup()
        self._connect_signals()

        self.init_results_list()

    def _restore_size_pos(self):
        p: Optional[WindowPosSize] = self._app_data.app_settings.get('word_scan_popup_pos', None)
        if p is not None:
            self.resize(p['width'], p['height'])
            self.move(p['x'], p['y'])
        else:
            self.center()
            self.resize(400, 500)

    def closeEvent(self, event: QCloseEvent):
        qr = self.frameGeometry()
        p = WindowPosSize(
            x = qr.x(),
            y = qr.y(),
            width = qr.width(),
            height = qr.height(),
        )
        self._app_data.app_settings['word_scan_popup_pos'] = p
        self._app_data._save_app_settings()

        event.accept()

    def _ui_setup(self):
        self._layout = QVBoxLayout()
        self._layout.setContentsMargins(8, 8, 8, 8)
        self.setLayout(self._layout)

        top_buttons_box = QHBoxLayout()
        self._layout.addLayout(top_buttons_box)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/close"))

        button_style = "QPushButton { background-color: %s; border: none; }" % READING_BACKGROUND_COLOR

        self.close_button = QPushButton()
        self.close_button.setFixedSize(20, 20)
        self.close_button.setStyleSheet(button_style)
        self.close_button.setIcon(icon)

        self.close_button.setShortcut(QKeySequence("Ctrl+F6"))

        icon = QIcon()
        icon.addPixmap(QPixmap(":/drag"))

        self.drag_button = QPushButton()
        self.drag_button.setCheckable(True)
        self.drag_button.setFixedSize(20, 20)
        self.drag_button.setStyleSheet(button_style)
        self.drag_button.setIcon(icon)

        # FIXME tooltip doesn't activate, show in window content
        self.drag_button.setToolTip("Move the window: check move button, left-click, hold and drag, uncheck move button.")

        icon = QIcon()
        icon.addPixmap(QPixmap(":/resize"))

        self.resize_button = QPushButton()
        self.resize_button.setCheckable(True)
        self.resize_button.setFixedSize(20, 20)
        self.resize_button.setStyleSheet(button_style)
        self.resize_button.setIcon(icon)
        self.resize_button.setToolTip("Resize the window: check resize button, left-click, hold and drag, uncheck resize button.")

        spacer = QSpacerItem(100, 20, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)

        if IS_MAC:
            top_buttons_box.addWidget(self.close_button)
            top_buttons_box.addItem(spacer)
            top_buttons_box.addWidget(self.resize_button)
            top_buttons_box.addWidget(self.drag_button)
        else:
            top_buttons_box.addWidget(self.drag_button)
            top_buttons_box.addWidget(self.resize_button)
            top_buttons_box.addItem(spacer)
            top_buttons_box.addWidget(self.close_button)

        search_box = QHBoxLayout()

        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("Type or copy to clipboard")
        self.search_input.setClearButtonEnabled(True)

        self.search_input.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.search_input.setMinimumHeight(35)

        completer = QCompleter(self._autocomplete_model, self)
        completer.setMaxVisibleItems(10)
        completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        completer.setModelSorting(QCompleter.ModelSorting.CaseInsensitivelySortedModel)

        self.search_input.setCompleter(completer)

        icon = QIcon()
        icon.addPixmap(QPixmap(":/search"))

        self.search_button = QPushButton()
        self.search_button.setFixedSize(35, 35)
        self.search_button.setIcon(icon)

        search_box.addWidget(self.search_input)
        search_box.addWidget(self.search_button)

        self._layout.addLayout(search_box)

        self.search_input.setFocus()

        self._setup_search_tabs()

    def _setup_content_html(self):
        self.content_html = QWebEngineView()
        self.content_html.setPage(ReaderWebEnginePage(self))

        self.content_html.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)

        msg = """
<p>
    Select a word or phrase and copy to clipboard with Ctrl+C.
    When the clipboard content changes, this window will display dictionary lookup results.
</p>
"""
        page_html = self.queries.content_html_page(body=msg, css_extra=CSS_EXTRA)

        self.content_html.setHtml(page_html)
        self.content_html.show()
        self.content_layout.addWidget(self.content_html, 100)

    def __init_position(self):
        self.__top = False
        self.__bottom = False
        self.__left = False
        self.__right = False

    def center(self):
        qr = self.frameGeometry()
        cp = QDesktopWidget().availableGeometry().center()
        qr.moveCenter(cp)
        self.move(qr.topLeft())

    def mousePressEvent(self, e: QMouseEvent):
        self.oldPos = e.globalPos()
        self._left_pressed = (e.button() == Qt.MouseButton.LeftButton)
        # if e.button() == Qt.MouseButton.LeftButton:
        #     if self._resized:
        #         self._resize()
        #     else:
        #         if self._dragMenuBarOnlyWayToMoveWindowFlag:
        #             pass
        #         else:
        #             self._move()
        return super().mousePressEvent(e)

    def mouseReleaseEvent(self, e: QMouseEvent):
        self._left_pressed = not (e.button() == Qt.MouseButton.LeftButton)
        return super().mouseReleaseEvent(e)

    def mouseMoveEvent(self, e: QMouseEvent):
        self.__set_cursor_shape_for_current_point(e.pos())

        is_resizing_cursor_shape = (self._cursor.shape() == Qt.CursorShape.SizeBDiagCursor \
                                    or self._cursor.shape() == Qt.CursorShape.SizeFDiagCursor \
                                    or self._cursor.shape() == Qt.CursorShape.SizeHorCursor \
                                    or self._cursor.shape() == Qt.CursorShape.SizeVerCursor)

        if self.drag_button.isChecked() \
           or (not self.resize_button.isChecked() and self._left_pressed and self._cursor.shape() == Qt.CursorShape.ClosedHandCursor):
            self._move(e)

        elif self.resize_button.isChecked() \
           or (not self.drag_button.isChecked() and self._left_pressed and is_resizing_cursor_shape):
            self._resize(e)

        return super().mouseMoveEvent(e)

    # prevent accumulated cursor shape bug
    def enterEvent(self, e):
        self.__set_cursor_shape_for_current_point(e.pos())
        return super().enterEvent(e)

    def __set_cursor_shape_for_current_point(self, p):
        # give the margin to reshape cursor shape
        rect = self.rect()
        rect.setX(self.rect().x() + self._margin)
        rect.setY(self.rect().y() + self._margin)
        rect.setWidth(self.rect().width() - self._margin * 2)
        rect.setHeight(self.rect().height() - self._margin * 2)

        self._resized = rect.contains(p)
        if self._resized:
            # resize end
            self.unsetCursor()
            self._cursor = self.cursor()
            self.__init_position()
        else:
            # resize start
            x = p.x()
            y = p.y()

            x1 = self.rect().x()
            y1 = self.rect().y()
            x2 = self.rect().width()
            y2 = self.rect().height()

            self.__left = abs(x - x1) <= self._margin # if mouse cursor is at the almost far left
            self.__top = abs(y - y1) <= self._margin # far top
            self.__right = abs(x - (x2 + x1)) <= self._margin # far right
            self.__bottom = abs(y - (y2 + y1)) <= self._margin # far bottom

            # set the cursor shape based on flag above
            if self.__top and self.__left:
                self._cursor.setShape(Qt.CursorShape.ClosedHandCursor)
            elif self.__top and self.__right:
                self._cursor.setShape(Qt.CursorShape.ClosedHandCursor)
            elif self.__bottom and self.__left:
                self._cursor.setShape(Qt.CursorShape.SizeBDiagCursor)
            elif self.__bottom and self.__right:
                self._cursor.setShape(Qt.CursorShape.SizeFDiagCursor)
            elif self.__left:
                self._cursor.setShape(Qt.CursorShape.SizeHorCursor)
            elif self.__top:
                self._cursor.setShape(Qt.CursorShape.ClosedHandCursor)
            elif self.__right:
                self._cursor.setShape(Qt.CursorShape.SizeHorCursor)
            elif self.__bottom:
                self._cursor.setShape(Qt.CursorShape.SizeVerCursor)
            self.setCursor(self._cursor)

        self._resized = not self._resized

    def _resize(self, e: QMouseEvent):
        delta = QPoint(e.globalPos() - self.oldPos)
        size = self.rect()
        self.resize(size.width() + delta.x(),
                    size.height() + delta.y())
        self.oldPos = e.globalPos()

    def _move(self, e: QMouseEvent):
        delta = QPoint(e.globalPos() - self.oldPos)
        self.move(self.x() + delta.x(), self.y() + delta.y())
        self.oldPos = e.globalPos()

    def _set_content_html(self, html: str):
        self.content_html.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _render_words(self, words: List[UDictWord]):
        if len(words) == 0:
            return

        page_html = self.queries.words_to_html_page(words, CSS_EXTRA)
        self._set_content_html(page_html)

    def _setup_search_tabs(self):
        self.tabs_layout = QVBoxLayout()

        self.tabs = QTabWidget()

        self.tabs_layout.addWidget(self.tabs)
        self.tabs_layout.setContentsMargins(0, 0, 0, 0)

        self._layout.addLayout(self.tabs_layout)

        self._setup_words_tab()
        self._setup_results_tab()

    def _setup_words_tab(self):
        self.tab_word = QWidget()
        self.tab_word.setObjectName("Words")
        self.tab_word.setStyleSheet("QWidget#Words { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.tabs.addTab(self.tab_word, "Words")

        self.content_layout = QVBoxLayout()
        self.tab_word.setLayout(self.content_layout)

        self._setup_content_html()

    def _setup_results_tab(self):
        self.results_tab = QWidget()
        self.results_tab.setObjectName("Fulltext")
        self.results_tab.setStyleSheet("QWidget#Fulltext { background-color: %s; }" % READING_BACKGROUND_COLOR)

        self.tabs.addTab(self.results_tab, "Fulltext")

        self.results_tab_layout = QVBoxLayout(self.results_tab)
        self.results_tab_inner_layout = QVBoxLayout()

        self.results_pages_layout = QHBoxLayout()

        self.results_page_input = QSpinBox(self.results_tab)
        self.results_page_input.setMinimum(1)
        self.results_page_input.setMaximum(999)
        self.results_pages_layout.addWidget(self.results_page_input)

        self.results_prev_btn = QPushButton("Prev", self.results_tab)
        self.results_pages_layout.addWidget(self.results_prev_btn)

        self.results_next_btn = QPushButton("Next", self.results_tab)
        self.results_pages_layout.addWidget(self.results_next_btn)

        spacerItem2 = QSpacerItem(40, 20, QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Minimum)
        self.results_pages_layout.addItem(spacerItem2)

        self.results_first_page_btn = QPushButton("First", self.results_tab)
        self.results_pages_layout.addWidget(self.results_first_page_btn)

        self.results_last_page_btn = QPushButton("Last", self.results_tab)
        self.results_pages_layout.addWidget(self.results_last_page_btn)

        self.results_tab_inner_layout.addLayout(self.results_pages_layout)

        self.results_label = QLabel(self.results_tab)
        self.results_tab_inner_layout.addWidget(self.results_label)

        self.results_list = QListWidget(self.results_tab)
        self.results_list.setFrameShape(QFrame.NoFrame)
        self.results_tab_inner_layout.addWidget(self.results_list)

        self.results_tab_layout.addLayout(self.results_tab_inner_layout)

    def _set_query(self, s: str):
        self.search_input.setText(s)

    def _show_word(self, word: UDictWord):
        word_html = self.queries.get_word_html(word)

        page_html = self.queries.content_html_page(
            body = word_html['body'],
            css_head = word_html['css'],
            css_extra = CSS_EXTRA,
            js_head = word_html['js'])

        self._set_content_html(page_html)

    def _show_word_by_bword_url(self, url: QUrl):
        # FIXME encoding is wrong
        # araghaṭṭa
        # Show Word: xn--araghaa-jb4ca
        s = url.toString()
        query = s.replace("bword://", "")
        logger.info(f"Show Word: {query}")
        self._set_query(query)
        self._handle_query()
        self._handle_exact_query()

    def _show_word_by_uid(self, uid: str):
        results = self.queries.get_words_by_uid(uid)
        if len(results) > 0:
            self._show_word(results[0])

    def _word_search_query(self, query: str) -> List[SearchResult]:
        results = self.search_query.new_query(query, self._app_data.app_settings['disabled_dict_labels'])
        hits = self.search_query.hits

        if hits == 0:
            self.results_page_input.setMinimum(0)
            self.results_page_input.setMaximum(0)
            self.results_first_page_btn.setEnabled(False)
            self.results_last_page_btn.setEnabled(False)

        elif hits <= self.page_len:
            self.results_page_input.setMinimum(1)
            self.results_page_input.setMaximum(1)
            self.results_first_page_btn.setEnabled(False)
            self.results_last_page_btn.setEnabled(False)

        else:
            pages = math.floor(hits / self.page_len) + 1
            self.results_page_input.setMinimum(1)
            self.results_page_input.setMaximum(pages)
            self.results_first_page_btn.setEnabled(True)
            self.results_last_page_btn.setEnabled(True)

        return results

    def _handle_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        self._results = self._word_search_query(query)

        if self.search_query.hits > 0:
            self.tabs.setTabText(1, f"Fulltext ({self.search_query.hits})")
        else:
            self.tabs.setTabText(1, "Fulltext")

        self.render_results_page()

        if self.search_query.hits == 1 and self._results[0]['uid'] is not None:
            self._show_word_by_uid(self._results[0]['uid'])

    def _handle_autocomplete_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        self._autocomplete_model.clear()

        a = self.queries.autocomplete_hits(query)

        for i in a:
            self._autocomplete_model.appendRow(QStandardItem(i))

        self._autocomplete_model.sort(0)

    def _handle_exact_query(self, min_length: int = 4):
        query = self.search_input.text()

        if len(query) < min_length:
            return

        res = self.queries.word_exact_matches(query)
        self._render_words(res)

    def _handle_result_select(self):
        selected_idx = self.results_list.currentRow()
        if selected_idx < len(self._results):
            word = self.queries.dict_word_from_result(self._results[selected_idx])
            if word is not None:
                self._show_word(word)

    def _handle_clipboard_changed(self):
        if self._clipboard is None:
            return

        text = self._clipboard.text()
        text = text.strip(".,:;!? ")
        if not text.startswith('http'):
            self.search_input.setText(text)
            self._handle_query(min_length=4)
            self._handle_exact_query(min_length=4)

    def _handle_drag_button(self):
        if self.drag_button.isChecked():
            self.drag_button.grabMouse()
            button_style = "QPushButton { background-color: %s; border: none; }" % DARK_READING_BACKGROUND_COLOR
        else:
            self.drag_button.releaseMouse()
            button_style = "QPushButton { background-color: %s; border: none; }" % READING_BACKGROUND_COLOR

        self.drag_button.setStyleSheet(button_style)

    def _handle_resize_button(self):
        if self.resize_button.isChecked():
            self.resize_button.grabMouse()
            button_style = "QPushButton { background-color: %s; border: none; }" % DARK_READING_BACKGROUND_COLOR
        else:
            self.resize_button.releaseMouse()
            button_style = "QPushButton { background-color: %s; border: none; }" % READING_BACKGROUND_COLOR

        self.resize_button.setStyleSheet(button_style)

    def _handle_close_button(self):
        if self._app_data.actions_manager is not None:
            self._app_data.actions_manager.show_word_scan_popup()

    def _connect_signals(self):
        if self._clipboard is not None:
            self._clipboard.dataChanged.connect(partial(self._handle_clipboard_changed))

        self.search_button.clicked.connect(partial(self._handle_query, min_length=1))
        self.search_input.textEdited.connect(partial(self._handle_query, min_length=4))
        self.search_input.completer().activated.connect(partial(self._handle_query, min_length=1))

        self.search_input.textEdited.connect(partial(self._handle_autocomplete_query, min_length=4))

        self.search_button.clicked.connect(partial(self._handle_exact_query, min_length=1))
        self.search_input.returnPressed.connect(partial(self._handle_exact_query, min_length=1))
        self.search_input.completer().activated.connect(partial(self._handle_exact_query, min_length=1))

        self.drag_button.clicked.connect(partial(self._handle_drag_button))
        self.resize_button.clicked.connect(partial(self._handle_resize_button))
        self.close_button.clicked.connect(partial(self._handle_close_button))
