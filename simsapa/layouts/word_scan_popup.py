from functools import partial
import math
from typing import List, Optional
from PyQt5.QtCore import QUrl, Qt
from PyQt5.QtGui import QClipboard, QCursor, QIcon, QPixmap, QStandardItemModel, QStandardItem
from PyQt5.QtWebEngineWidgets import QWebEngineView
from PyQt5.QtWidgets import QCompleter, QDesktopWidget, QFrame, QHBoxLayout, QLabel, QLineEdit, QListWidget, QPushButton, QSizePolicy, QSpacerItem, QSpinBox, QTabWidget, QVBoxLayout, QWidget

from simsapa import SIMSAPA_PACKAGE_DIR, logger
from simsapa.app.db.search import SearchQuery, SearchResult, dict_word_hit_to_search_result
from simsapa.app.types import AppData, UDictWord
from simsapa.layouts.dictionary_queries import DictionaryQueries
from simsapa.layouts.reader_web import ReaderWebEnginePage
from simsapa.layouts.results_list import HasResultsList


class WordScanPopup(QWidget, HasResultsList):

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
        self.resize(400, 500)
        self.setMouseTracking(True)

        flags = Qt.WindowType.Dialog | \
            Qt.WindowType.WindowStaysOnTopHint | \
            Qt.WindowType.FramelessWindowHint

        # NOTE on Linux this succeeds to set 'always on top',
        # but loses system resize and drag,
        # plus the window appears on every virtual screen.
        # Better to let Linux users toggle always on top in their window manager.

        # flags = Qt.WindowType.Dialog | \
        #     Qt.WindowType.CustomizeWindowHint | \
        #     Qt.WindowType.WindowStaysOnTopHint | \
        #     Qt.WindowType.FramelessWindowHint | \
        #     Qt.WindowType.BypassWindowManagerHint | \
        #     Qt.WindowType.X11BypassWindowManagerHint

        self.setWindowFlags(flags) # type: ignore

        self._clipboard = self._app_data.clipboard

        self.center()
        self.setObjectName("WordScanPopup")
        self.setStyleSheet("QWidget#WordScanPopup { background-color: white; }")

        self._resized = False
        self._margin = 5
        self._cursor = QCursor()
        self._dragMenuBarOnlyWayToMoveWindowFlag = False

        self.__init_position()

        self._ui_setup()
        self._connect_signals()

        self.init_results_list()

    def _ui_setup(self):
        self._layout = QVBoxLayout()
        self._layout.setContentsMargins(8, 20, 8, 8)
        self.setLayout(self._layout)

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
<!doctype html>
<html>
<head>
    <meta charset="utf-8">
</head>
<body>
    <p>
    Select a word or phrase and copy to clipboard with Ctrl+C.
    When the clipboard content changes, this window will display dictionary lookup results.
    </p>
</body>
</html>"""

        self.content_html.setHtml(msg)
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

    def mousePressEvent(self, e):
        if e.button() == Qt.MouseButton.LeftButton:
            if self._resized:
                self._resize()
            else:
                if self._dragMenuBarOnlyWayToMoveWindowFlag:
                    pass
                else:
                    self._move()
        return super().mousePressEvent(e)

    def mouseMoveEvent(self, e):
        self.__set_cursor_shape_for_current_point(e.pos())
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
            # not using the top edge and corners, to allow space for dragging
            if self.__top and self.__left:
                pass
                # self._cursor.setShape(Qt.CursorShape.SizeFDiagCursor)
            elif self.__top and self.__right:
                pass
                # self._cursor.setShape(Qt.CursorShape.SizeBDiagCursor)
            elif self.__bottom and self.__left:
                self._cursor.setShape(Qt.CursorShape.SizeBDiagCursor)
            elif self.__bottom and self.__right:
                self._cursor.setShape(Qt.CursorShape.SizeFDiagCursor)
            elif self.__left:
                self._cursor.setShape(Qt.CursorShape.SizeHorCursor)
            elif self.__top:
                pass
                # self._cursor.setShape(Qt.CursorShape.SizeVerCursor)
            elif self.__right:
                self._cursor.setShape(Qt.CursorShape.SizeHorCursor)
            elif self.__bottom:
                self._cursor.setShape(Qt.CursorShape.SizeVerCursor)
            self.setCursor(self._cursor)

        self._resized = not self._resized

    def _resize(self):
        window = self.windowHandle()
        # reshape cursor for resize
        if self._cursor.shape() == Qt.CursorShape.SizeHorCursor:
            if self.__left:
                window.startSystemResize(Qt.Edge.LeftEdge)
            elif self.__right:
                window.startSystemResize(Qt.Edge.RightEdge)
        elif self._cursor.shape() == Qt.CursorShape.SizeVerCursor:
            if self.__top:
                window.startSystemResize(Qt.Edge.TopEdge)
            elif self.__bottom:
                window.startSystemResize(Qt.Edge.BottomEdge)
        elif self._cursor.shape() == Qt.CursorShape.SizeBDiagCursor:
            if self.__top and self.__right:
                window.startSystemResize(Qt.Edge.TopEdge | Qt.Edge.RightEdge) # type: ignore
            elif self.__bottom and self.__left:
                window.startSystemResize(Qt.Edge.BottomEdge | Qt.Edge.LeftEdge) # type: ignore
        elif self._cursor.shape() == Qt.CursorShape.SizeFDiagCursor:
            if self.__top and self.__left:
                window.startSystemResize(Qt.Edge.TopEdge | Qt.Edge.LeftEdge) # type: ignore
            elif self.__bottom and self.__right:
                window.startSystemResize(Qt.Edge.BottomEdge | Qt.Edge.RightEdge) # type: ignore

    def _move(self):
        window = self.windowHandle()
        window.startSystemMove()

    def _set_content_html(self, html: str):
        self.content_html.setHtml(html, baseUrl=QUrl(str(SIMSAPA_PACKAGE_DIR)))

    def _render_words(self, words: List[UDictWord]):
        if len(words) == 0:
            return

        page_html = self.queries.words_to_html_page(words)
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
        self.tabs.addTab(self.tab_word, "Words")

        self.content_layout = QVBoxLayout()
        self.tab_word.setLayout(self.content_layout)

        self._setup_content_html()

    def _setup_results_tab(self):
        self.results_tab = QWidget()
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

        page_html = self.queries.content_html_page(word_html['body'], word_html['css'], word_html['js'])

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
