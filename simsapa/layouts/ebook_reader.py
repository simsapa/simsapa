import re, json
import urllib.parse
from pathlib import Path
from functools import partial
from itertools import chain
from typing import List, Optional, Tuple, TypedDict
import zipfile
import shutil

from sqlalchemy.orm.session import Session

from ebooklib import epub

from PyQt6 import QtWidgets, QtCore, QtGui
from PyQt6.QtCore import QItemSelection, QItemSelectionModel, QModelIndex, QObject, QRunnable, QSize, QThreadPool, QUrl, pyqtSignal, pyqtSlot
from PyQt6.QtGui import QAction, QCloseEvent, QKeySequence, QStandardItem, QStandardItemModel
from PyQt6.QtWidgets import QAbstractItemView, QFileDialog, QHBoxLayout, QMenu, QMenuBar, QMessageBox, QPushButton, QSpacerItem, QSplitter, QTabWidget, QTreeView, QVBoxLayout, QWidget

from simsapa import EBOOK_EXTRA_CSS, EBOOK_UNZIP_DIR, logger, APP_QUEUES, ApiMessage, ApiAction
# from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.export_helpers import add_sutta_links
from simsapa.layouts.sutta_search_window_state import SuttaSearchWindowState

from simsapa.app.types import QueryType
from simsapa.app.app_data import AppData

from simsapa.layouts.gui_types import EbookReaderWindowInterface, QExpanding, QMinimum, sutta_quote_from_url

class ChapterItem(QStandardItem):
    title: str
    path: Optional[Path] = None
    suttas_linked: bool

    def __init__(self, title: str, path: Optional[Path] = None, suttas_linked = False):
        super().__init__()

        self.title = title
        self.path = path
        self.suttas_linked = suttas_linked

        self.setEditable(False)
        self.setText(self.title)

class EbookReaderWindow(EbookReaderWindowInterface):

    lookup_in_dictionary_signal = pyqtSignal(str)
    lookup_in_new_sutta_window_signal = pyqtSignal(str)

    def __init__(self, app_data: AppData, parent = None) -> None:
        super().__init__(parent)
        logger.info("EbookReaderWindow()")

        self._app_data: AppData = app_data

        self.queue_id = 'window_' + str(len(APP_QUEUES))
        # self.queue_id is needed for the close event, but not using the queues
        # in this window at the moment.
        #
        # APP_QUEUES[self.queue_id] = queue.Queue()

        self.ebook_unzip_dir: Optional[Path] = None
        self.ebook_opf_dir: Optional[Path] = None
        self.ebook_toc_items: List[ChapterItem] = []

        self.sutta_links_worker: Optional[SuttaLinksWorker] = None
        self.thread_pool = QThreadPool()

        self.toc_panel_visible = True
        self.sutta_panel_visible = True

        self._setup_ui()
        self._connect_signals()
        self._update_vert_splitter_widths()

    def _setup_ui(self):
        self.setWindowTitle("Ebook Reader - Simsapa")
        self.resize(1068, 625)

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        # top buttons

        self._top_buttons_box = QHBoxLayout()
        self._layout.addLayout(self._top_buttons_box)

        self.toggle_toc_panel_btn = QPushButton()

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/angles-left"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.toggle_toc_panel_btn.setIcon(icon)
        self.toggle_toc_panel_btn.setMinimumSize(QSize(40, 40))
        self.toggle_toc_panel_btn.setToolTip("Toggle Table of Contents")

        self._top_buttons_box.addWidget(self.toggle_toc_panel_btn)

        self._open_btn = QPushButton("Open...")
        self._open_btn.setMinimumSize(QSize(80, 40))
        self._top_buttons_box.addWidget(self._open_btn)

        self._top_buttons_box.addItem(QSpacerItem(0, 0, QExpanding, QMinimum))

        self.toggle_sutta_panel_btn = QPushButton()

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/angles-right"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.toggle_sutta_panel_btn.setIcon(icon)
        self.toggle_sutta_panel_btn.setMinimumSize(QSize(40, 40))
        self.toggle_sutta_panel_btn.setToolTip("Toggle Sutta Panel")

        self._top_buttons_box.addWidget(self.toggle_sutta_panel_btn)

        # horizontal splitter

        self.vert_splitter = QSplitter(self._central_widget)
        self.vert_splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)

        self._layout.addWidget(self.vert_splitter)

        self.toc_panel_widget = QWidget(self.vert_splitter)
        self.toc_panel_layout = QVBoxLayout(self.toc_panel_widget)
        self.toc_panel_layout.setContentsMargins(0, 0, 0, 0)

        self.reading_panel_widget = QWidget(self.vert_splitter)
        self.reading_panel_layout = QVBoxLayout(self.reading_panel_widget)
        self.reading_panel_layout.setContentsMargins(0, 0, 0, 0)

        self.sutta_panel_widget = QWidget(self.vert_splitter)
        self.sutta_panel_layout = QVBoxLayout(self.sutta_panel_widget)
        self.sutta_panel_layout.setContentsMargins(0, 0, 0, 0)

        # TODO Sutta tabs

        self._setup_menubar()
        self._setup_sutta_panel()
        self._setup_reading_panel()
        self._setup_toc_panel()

        show = self._app_data.app_settings.get('show_related_suttas', True)
        self.action_Show_Related_Suttas.setChecked(show)

    def _setup_menubar(self):
        self.menubar = QMenuBar()
        self.setMenuBar(self.menubar)

        self.menu_file = QMenu("&File")
        self.menubar.addMenu(self.menu_file)

        self.action_close_window = QAction("&Close Window")
        self.menu_file.addAction(self.action_close_window)

        self.action_open = QAction("&Open...")
        self.action_open.setShortcut(QKeySequence("Ctrl+O"))
        self.menu_file.addAction(self.action_open)

        self.menu_suttas = QMenu("&Suttas")
        self.menubar.addMenu(self.menu_suttas)

        # NOTE: Camel_Case expected in hasattr() checks
        self.action_Show_Related_Suttas = QAction("&Show Related Suttas")
        self.action_Show_Related_Suttas.setCheckable(True)
        self.action_Show_Related_Suttas.setChecked(True)
        self.menu_suttas.addAction(self.action_Show_Related_Suttas)

    def _setup_toc_panel(self):
        self.toc_tabs = QTabWidget()
        self.toc_panel_layout.addWidget(self.toc_tabs)

        self.toc_tab_widget = QWidget()
        self.toc_tab_layout = QVBoxLayout()
        self.toc_tab_widget.setLayout(self.toc_tab_layout)

        self.toc_tabs.addTab(self.toc_tab_widget, "Contents")

        self._init_toc_tree()

    def _init_toc_tree(self):
        self.toc_tree_view = QTreeView()
        self.toc_tab_layout.addWidget(self.toc_tree_view)

        self.toc_tree_model = QStandardItemModel(0, 2, self)

        item = QStandardItem()
        item.setText("L")
        item.setToolTip("Sutta references linked")
        self.toc_tree_model.setHorizontalHeaderItem(0, item)

        item = QStandardItem()
        item.setText("Chapter")
        self.toc_tree_model.setHorizontalHeaderItem(1, item)

        self.toc_tree_view.setHeaderHidden(False)
        self.toc_tree_view.setRootIsDecorated(True)
        self.toc_tree_view.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.toc_tree_view.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)

        self.toc_tree_view.setModel(self.toc_tree_model)

        self.toc_tree_view.resizeColumnToContents(0)
        self.toc_tree_view.resizeColumnToContents(1)

    def _reload_toc_tree(self, ebook: epub.EpubBook):
        self.toc_tree_model.clear()
        self._create_toc_tree_items(self.toc_tree_model, ebook)
        self.toc_tree_model.layoutChanged.emit()
        self.toc_tree_view.expandAll()

    def _create_toc_tree_items(self, model: QStandardItemModel, ebook: epub.EpubBook):
        item = QStandardItem()
        item.setText("L")
        item.setToolTip("Sutta references linked")
        model.setHorizontalHeaderItem(0, item)

        item = QStandardItem()
        item.setText("Chapter")
        model.setHorizontalHeaderItem(1, item)

        root_node = model.invisibleRootItem()

        self.ebook_toc_items = []

        def _parse_toc(x) -> List[ChapterItem]:
            items = []
            if isinstance(x, epub.Link) or isinstance(x, epub.Section):
                # Paths which include space contain %20
                # Text/12%20Knowing.xhtml
                # Text/Cultivation%20And%20Fruition.xhtml

                href_xhtml = urllib.parse.unquote(x.href)
                # remove anchors, e.g. part02.xhtml#pt02
                p = re.sub(r'\#.*$', '', href_xhtml)
                assert(self.ebook_opf_dir is not None)
                html_path = self.ebook_opf_dir.joinpath(p)

                items.append(ChapterItem(x.title, html_path))

            elif isinstance(x, Tuple) or isinstance(x, List):
                a = [_parse_toc(y) for y in x]
                items.extend(list(chain.from_iterable(a)))

            return items

        self.ebook_toc_items = _parse_toc(ebook.toc)

        for i in self.ebook_toc_items:
            s = "✓" if i.suttas_linked else " "
            linked = QStandardItem(s)
            root_node.appendRow([linked, i])

        self.toc_tree_view.resizeColumnToContents(0)
        self.toc_tree_view.resizeColumnToContents(1)

    def _handle_toc_tree_clicked(self, val: QModelIndex):
        if self.ebook_opf_dir is None:
            return

        # Use this instead of self.toc_tree_model.itemFromIndex(val) to always
        # get the chapter item, not the linked checkmark column.
        item = self.ebook_toc_items[val.row()]

        if item is not None and item.path is not None:

            self.reading_state.sutta_tabs.setTabText(0, item.title)
            self.reading_state.sutta_tab.set_qwe_html_file(item.path)

    def _show_sutta_by_url(self, url: QUrl):
        if url.host() != QueryType.suttas:
            return False

        uid = re.sub(r"^/", "", url.path())

        self.sutta_state._show_sutta_by_uid(uid, sutta_quote_from_url(url))

    def _setup_reading_panel(self):
        self.reading_state = SuttaSearchWindowState(
            app_data = self._app_data,
            parent_window = self,
            searchbar_layout = None,
            sutta_tabs_layout = self.reading_panel_layout,
            tabs_layout = None,
            focus_input = False,
            enable_language_filter = False,
            enable_search_extras = False,
            enable_sidebar = False,
            enable_find_panel = False,
            show_query_results_in_active_tab = False,
            custom_create_context_menu_fn = self._create_reading_context_menu,
        )

        self.reading_state.show_url_action_fn = self._show_sutta_by_url

        self.reading_state.sutta_tabs.setTabText(0, "Chapter")

    def _setup_sutta_panel(self):
        self.sutta_state = SuttaSearchWindowState(
            app_data = self._app_data,
            parent_window = self,
            searchbar_layout = None,
            sutta_tabs_layout = self.sutta_panel_layout,
            tabs_layout = None,
            focus_input = False,
            enable_language_filter = False,
            enable_search_extras = False,
            enable_sidebar = False,
            enable_find_panel = False,
            show_query_results_in_active_tab = False)

    def _create_reading_context_menu(self, menu: QMenu):
        s = self.reading_state

        self.qwe_copy_selection = QAction("Copy Selection")
        # NOTE: don't bind Ctrl-C, will be ambiguous to the window menu action
        self.qwe_copy_selection.triggered.connect(partial(s._handle_copy))
        menu.addAction(self.qwe_copy_selection)

        self.qwe_lookup_menu = QMenu("Lookup Selection")
        menu.addMenu(self.qwe_lookup_menu)

        self.qwe_lookup_in_suttas = QAction("In Suttas")
        self.qwe_lookup_in_suttas.triggered.connect(partial(s._lookup_selection_in_new_sutta_window))
        self.qwe_lookup_menu.addAction(self.qwe_lookup_in_suttas)

        self.qwe_lookup_in_dictionary = QAction("In Dictionary")
        self.qwe_lookup_in_dictionary.triggered.connect(partial(s._lookup_selection_in_dictionary))
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
            a.triggered.connect(partial(s._open_gpt_prompt_with_params, db_id))
            self.gpt_prompts_actions.append(a)
            self.gpt_prompts_menu.addAction(a)

        for i in prompts:
            _add_action_to_menu(i)

        tab = s._get_active_tab()

        s.qwe_devtools = QAction("Show Inspector")
        s.qwe_devtools.setCheckable(True)
        s.qwe_devtools.setChecked(tab.devtools_open)
        s.qwe_devtools.triggered.connect(partial(s._toggle_devtools_inspector))
        menu.addAction(s.qwe_devtools)

    def start_loading_animation(self):
        pass

    def stop_loading_animation(self):
        pass

    def _update_vert_splitter_widths(self):
        left_toc = 1000 if self.toc_panel_visible else 0
        middle_reading = 2000
        right_suttas = 2000 if self.sutta_panel_visible else 0

        self.vert_splitter.setSizes([left_toc, middle_reading, right_suttas])

    def _toggle_toc_panel(self):
        self.toc_panel_visible = not self.toc_panel_visible
        self._update_vert_splitter_widths()

    def _toggle_sutta_panel(self):
        self.sutta_panel_visible = not self.sutta_panel_visible
        self._update_vert_splitter_widths()

    def _handle_selection_changed(self, selected: QItemSelection, _: QItemSelection):
        indexes = selected.indexes()
        if len(indexes) > 0:
            self._handle_toc_tree_clicked(indexes[0])

    def _handle_open_ebook(self):
        file_path, _ = QFileDialog \
            .getOpenFileName(self,
                            "Open ebooks...",
                            "",
                            "Epub Files (*.epub)")

        if len(file_path) == 0:
            return

        ebook_path = Path(file_path)

        if ebook_path.suffix.lower() == ".mobi":
            self._show_warning("Can't open Mobi files currently.")
            return

            # epub_path = EBOOK_UNZIP_DIR.joinpath(ebook_path.name).with_suffix(".epub")

            # ebook_convert_path = self._app_data.app_settings['path_to_ebook_convert']
            # if ebook_convert_path is None:
            #     raise Exception("<p>ebook-convert path is None</p>")

            # try:
            #     convert_mobi_to_epub(Path(ebook_convert_path), ebook_path, epub_path)

            # except Exception as e:
            #     self._show_warning(f"<p>Error:</p><p>{e}</p>")

            # ebook_path = epub_path

        try:
            s = re.sub(r'[^a-zA-Z0-9-_]', '_', ebook_path.with_suffix("").name)
            self.ebook_unzip_dir = EBOOK_UNZIP_DIR.joinpath(s)
            if self.ebook_unzip_dir.exists():
                shutil.rmtree(self.ebook_unzip_dir)

            self.ebook_unzip_dir.mkdir()

            zf = zipfile.ZipFile(ebook_path)
            zf.extractall(self.ebook_unzip_dir)

            reader = epub.EpubReader(file_path, None)

            # FIXME epub converted from mobi errors out below with "Bad Zip
            # File", but the above, zipfile was able to extract the epub
            # contents already.

            ebook = reader.load()
            reader.process()

            if reader.opf_dir is None:
                return
            self.ebook_opf_dir = self.ebook_unzip_dir.joinpath(reader.opf_dir)

            # Reload has to come before the extra css so that
            # self.ebook_toc_items is populated.
            self._reload_toc_tree(ebook)
            self._add_extra_css_js_to_ebook(self.ebook_toc_items)

            # Select the first item when opening the ebook
            idx = self.toc_tree_model.index(0, 0)
            self.toc_tree_view.selectionModel() \
                              .select(idx,
                                      QItemSelectionModel.SelectionFlag.ClearAndSelect | \
                                      QItemSelectionModel.SelectionFlag.Rows)

            self._handle_toc_tree_clicked(idx)

        except Exception as e:
            self._show_warning(f"<p>Error:</p><p>{e}</p>")

    def _add_extra_css_js_to_ebook(self, toc_items: List[ChapterItem]):
        if self.ebook_opf_dir is None:
            return

        files = [i.path for i in toc_items if i.path]

        # files = list(glob.glob(str(self.ebook_opf_dir.joinpath("**/*.xhtml")), recursive=True))
        # files.extend(glob.glob(str(self.ebook_opf_dir.joinpath("**/*.html")), recursive=True))

        api_url = self._app_data.api_url

        css = EBOOK_EXTRA_CSS

        if api_url is not None:
            css = css.replace("http://localhost:8000", api_url)

        # NOTE:
        #
        # <script>{EBOOK_EXTRA_JS}</script>
        #
        # Triggers browser error:
        #
        # error on line 277 at column 24: xmlParseEntityRef: no name
        #
        # Somehow the browser is interpreting JS && ('and') as an xml entity.

        head_extra = f"""
<style>{css}</style>
<script> const API_URL = '{api_url}'; </script>
<script src="qrc:///qtwebchannel/qwebchannel.js"></script>
<script src="{api_url}/assets/js/ebook_extra.js"></script>
        """

        for p in files:
            with open(p, 'r', encoding='utf-8') as f:
                content = f.read()

            content = re.sub('<style(.*?)</style>', '', content)
            # <link xmlns="http://www.w3.org/1999/xhtml" href="styles/epub3.css" rel="stylesheet" type="text/css" />
            # <link href="../styles/stylesheet.css" rel="stylesheet" type="text/css">
            content = re.sub(r'<link(.*?)rel=[\'"]stylesheet[\'"](.*?)>', '', content)

            content = content.replace("</head>", f"{head_extra}</head>")

            with open(p, 'w', encoding='utf-8') as f:
                f.write(content)

        def _to_item(x: ChapterItem) -> Optional[LinkWorkItem]:
            if self.ebook_opf_dir is None or x.path is None:
                return None

            return LinkWorkItem(
                path = self.ebook_opf_dir.joinpath(x.path),
                title = x.title,
                content = None,
            )

        a = [_to_item(i) for i in toc_items if i.path]
        items = [i for i in a if i]
        self.sutta_links_worker = SuttaLinksWorker(items)

        self.sutta_links_worker.signals.finished.connect(partial(self._links_finished))
        self.sutta_links_worker.signals.done_chapter_path.connect(partial(self._links_done_chapter))

        self.thread_pool.start(self.sutta_links_worker)

    def _links_finished(self):
        self.toc_tree_model.removeColumn(0)
        self.toc_tree_model.layoutChanged.emit()

    def _links_done_chapter(self, path: Path):
        for idx, i in enumerate(self.ebook_toc_items):
            if i.path == path:
                self.ebook_toc_items[idx].suttas_linked = True
                linked = QStandardItem("✓")
                self.toc_tree_model.setItem(idx, 0, linked)
                break

        self.toc_tree_model.layoutChanged.emit()
        self.toc_tree_view.resizeColumnToContents(0)

    def _show_warning(self, msg: str, title = "Warning"):
        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle(title)
        box.setText(msg)
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()

    def _handle_close(self):
        self.close()

    def closeEvent(self, event: QCloseEvent):
        if self.ebook_unzip_dir is not None and self.ebook_unzip_dir.exists():
            shutil.rmtree(self.ebook_unzip_dir)

        if self.queue_id in APP_QUEUES.keys():
            del APP_QUEUES[self.queue_id]

        msg = ApiMessage(queue_id = 'app_windows',
                         action = ApiAction.remove_closed_window_from_list,
                         data = self.queue_id)
        s = json.dumps(msg)
        APP_QUEUES['app_windows'].put_nowait(s)

        event.accept()

    def _connect_signals(self):
        self.action_close_window \
            .triggered.connect(partial(self._handle_close))

        self.action_open.triggered.connect(partial(self._handle_open_ebook))

        self._open_btn.clicked.connect(partial(self._handle_open_ebook))

        self.toggle_toc_panel_btn.clicked.connect(partial(self._toggle_toc_panel))
        self.toggle_sutta_panel_btn.clicked.connect(partial(self._toggle_sutta_panel))

        self.toc_tree_view.selectionModel().selectionChanged.connect(partial(self._handle_selection_changed))

        self.action_Show_Related_Suttas \
            .triggered.connect(partial(self.sutta_state._handle_show_related_suttas))

class SuttaLinksWorkerSignals(QObject):
    done_chapter_path = pyqtSignal(Path)
    finished = pyqtSignal()

class LinkWorkItem(TypedDict):
    path: Path
    title: str
    content: Optional[str]

class SuttaLinksWorker(QRunnable):
    signals: SuttaLinksWorkerSignals

    def __init__(self, chapter_items: List[LinkWorkItem]):
        super().__init__()

        self.signals = SuttaLinksWorkerSignals()

        self.chapter_init_items = chapter_items

        self.will_emit_finished = True

    def add_links_to_content(self, db_session: Session, item: LinkWorkItem) -> Optional[LinkWorkItem]:
        if item['content'] is None:
            return None

        linked_content = add_sutta_links(db_session, item['content'])
        return LinkWorkItem(
            path = item['path'],
            title = item['title'],
            content = linked_content,
        )

    @pyqtSlot()
    def run(self):
        try:
            chapter_read_contents: List[LinkWorkItem] = []

            for i in self.chapter_init_items:
                with open(i['path'], 'r', encoding='utf-8') as f:
                    content = f.read()

                chapter_read_contents.append(LinkWorkItem(
                    path = i['path'],
                    title = i['title'],
                    content = content,
                ))

            db_eng, db_conn, db_session = get_db_engine_connection_session()

            for i in chapter_read_contents:
                w = self.add_links_to_content(db_session, i)
                if w is None:
                    continue

                if w['content'] is None:
                    continue

                with open(w['path'], 'w', encoding='utf-8') as f:
                    f.write(w['content'])
                    self.signals.done_chapter_path.emit(w['path'])

            db_conn.close()
            db_session.close()
            db_eng.dispose()

            if self.will_emit_finished:
                self.signals.finished.emit()

        except Exception as e:
            logger.error(e)
