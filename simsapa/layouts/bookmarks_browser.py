from pathlib import Path
import re
import subprocess
from PyQt6 import QtWidgets
from PyQt6 import QtCore
from PyQt6 import QtGui
from PyQt6.QtCore import QModelIndex, QSize, QUrl, QUrlQuery, Qt, QAbstractTableModel, pyqtSignal
from PyQt6.QtGui import QAction, QStandardItem, QStandardItemModel
from PyQt6.QtWidgets import (QAbstractItemView, QFileDialog, QHBoxLayout, QHeaderView, QLineEdit, QMenu, QMenuBar, QMessageBox, QPushButton, QSpacerItem, QSplitter, QTableView, QTreeView, QVBoxLayout, QWidget)

from functools import partial
from typing import Dict, List, Optional
from urllib.parse import quote_plus
import shutil

from sqlalchemy import and_, or_, not_

from simsapa import IS_SWAY, DbSchemaName, logger, QueryType

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa.app.app_data import AppData
from simsapa.app.export_helpers import save_suttas_as_epub, save_suttas_as_mobi
from simsapa.app.types import UBookmark, USutta

from simsapa.layouts.gui_types import BookmarksBrowserWindowInterface

from simsapa.layouts.parts.bookmark_dialog import BookmarkDialog, HasBookmarkDialog

# Keys with underscore prefix will not be shown in table columns.
SuttaModelColToIdx = {
    "Ref": 0,
    "Title": 1,
    "uid": 2,
    "Quote": 3,
    "Comment": 4,
    "_selection_range": 5,
    "_db_id": 6,
}

class SuttaModel(QAbstractTableModel):
    def __init__(self, data = []):
        super().__init__()
        self._data = data
        self._columns = list(filter(lambda x: not x.startswith("_"), SuttaModelColToIdx.keys()))

    def data(self, index: QModelIndex, role: Qt.ItemDataRole):
        if role == Qt.ItemDataRole.DisplayRole:
            if len(self._data) == 0:
                return list(map(lambda _: "", self._columns))
            else:
                return self._data[index.row()][index.column()]
        elif role == Qt.ItemDataRole.UserRole:
            return self._data

    def rowCount(self, _):
        return len(self._data)

    def columnCount(self, _):
        if len(self._data) == 0:
            return 0
        else:
            return len(self._columns)

    def headerData(self, section, orientation, role):
       if role == Qt.ItemDataRole.DisplayRole:
            if orientation == Qt.Orientation.Horizontal:
                return self._columns[section]

            if orientation == Qt.Orientation.Vertical:
                return str(section+1)


class BookmarkItem(QStandardItem):
    path: str
    name: str

    def __init__(self, path: str):
        super().__init__()

        self.setEditable(False)
        # self.setForeground(QColor(0, 255, 0))
        # self.setFont(QFont("Open Sans", 12))

        # Remove trailing / for display.
        path = re.sub(r'/+$', '', path)

        self.path = re.sub(r' */ *', '/', path)
        self.name = self.path.split("/")[-1]

        self.setText(self.name)


class BookmarksBrowserWindow(BookmarksBrowserWindowInterface, HasBookmarkDialog):

    show_sutta_by_url = pyqtSignal(QUrl)
    current_bookmark_item: Optional[BookmarkItem] = None

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        logger.info("BookmarksBrowserWindow()")

        self._app_data: AppData = app_data

        # path to item with name
        self.tree_nodes: Dict[str, BookmarkItem] = dict()

        self.init_bookmark_dialog()

        self._setup_ui()
        self._connect_signals()

    def _setup_ui(self):
        self.setWindowTitle("Bookmarks Browser - Simsapa")
        self.resize(850, 650)

        if IS_SWAY:
            cmd = """swaymsg 'for_window [title="Bookmarks Browser"] floating enable'"""
            subprocess.Popen(cmd, shell=True)

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        self.splitter = QSplitter(self._central_widget)
        self.splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)

        self._layout.addWidget(self.splitter)

        self.left_box_widget = QWidget(self.splitter)
        self.left_box_layout = QVBoxLayout(self.left_box_widget)
        self.left_box_layout.setContentsMargins(0, 0, 0, 0)

        self.right_box_widget = QWidget(self.splitter)
        self.right_box_layout = QVBoxLayout(self.right_box_widget)
        self.right_box_layout.setContentsMargins(0, 0, 0, 0)

        self.tree_search_box = QHBoxLayout()
        self.left_box_layout.addLayout(self.tree_search_box)

        self.tree_buttons_box = QHBoxLayout()
        self.left_box_layout.addLayout(self.tree_buttons_box)

        self.table_search_box = QHBoxLayout()
        self.right_box_layout.addLayout(self.table_search_box)

        self.table_buttons_box = QHBoxLayout()
        self.right_box_layout.addLayout(self.table_buttons_box)

        self._setup_menubar()

        self._setup_tree_search()
        self._setup_tree_buttons()
        self._setup_table_search()
        self._setup_table_buttons()
        self._setup_tree_view()
        self._setup_suttas_table()

    def _setup_menubar(self):
        self.menubar = QMenuBar()
        self.setMenuBar(self.menubar)

        self.menu_file = QMenu("&File", self.menubar)
        self.menubar.addMenu(self.menu_file)

        self.action_close_window = QAction("&Close Window")
        self.menu_file.addAction(self.action_close_window)

        self.action_import_csv = QAction("&Import Bookmarks from CSV...")
        self.menu_file.addAction(self.action_import_csv)

        self.action_export_csv = QAction("&Export Bookmarks as CSV...")
        self.menu_file.addAction(self.action_export_csv)

        self.menu_bookmarks = QMenu("&Bookmarks", self.menubar)
        self.menubar.addMenu(self.menu_bookmarks)

        self.action_export_suttas_as_epub = QAction("&Export Suttas Under Selected Bookmark as Epub...")
        self.menu_bookmarks.addAction(self.action_export_suttas_as_epub)

        self.action_export_suttas_as_mobi = QAction("&Export Suttas Under Selected Bookmark as Mobi...")
        self.menu_bookmarks.addAction(self.action_export_suttas_as_mobi)

    def _setup_tree_search(self):
        self.tree_search_input = QLineEdit()
        self.tree_search_input.setPlaceholderText("Filter bookmark tree...")

        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Policy.Fixed, QtWidgets.QSizePolicy.Policy.Fixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.tree_search_input.sizePolicy().hasHeightForWidth())
        self.tree_search_input.setSizePolicy(sizePolicy)

        self.tree_search_input.setMinimumSize(QtCore.QSize(250, 35))
        self.tree_search_input.setClearButtonEnabled(True)

        self.tree_search_box.addWidget(self.tree_search_input)

        self.tree_search_input.setFocus()

        self.tree_search_button = QtWidgets.QPushButton()

        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Policy.Fixed, QtWidgets.QSizePolicy.Policy.Fixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.tree_search_button.sizePolicy().hasHeightForWidth())

        self.tree_search_button.setSizePolicy(sizePolicy)
        self.tree_search_button.setMinimumSize(QtCore.QSize(40, 40))

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.tree_search_button.setIcon(icon)
        self.tree_search_box.addWidget(self.tree_search_button)

        spacer = QSpacerItem(0, 0, QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Minimum)
        self.tree_search_box.addItem(spacer)

    def _setup_tree_buttons(self):
        self.toggle_collapse_btn = QPushButton("Expand/Collapse All")
        self.toggle_collapse_btn.setFixedSize(QSize(160, 40))
        self.tree_buttons_box.addWidget(self.toggle_collapse_btn)

        self.new_node_btn = QPushButton("New")
        self.new_node_btn.setFixedSize(QSize(80, 40))
        self.tree_buttons_box.addWidget(self.new_node_btn)

        self.edit_node_btn = QPushButton("Edit")
        self.edit_node_btn.setFixedSize(QSize(80, 40))
        self.tree_buttons_box.addWidget(self.edit_node_btn)

        spacer = QSpacerItem(20, 0, QtWidgets.QSizePolicy.Policy.Minimum, QtWidgets.QSizePolicy.Policy.Minimum)
        self.tree_buttons_box.addItem(spacer)

        self.delete_node_btn = QPushButton("Delete")
        self.delete_node_btn.setFixedSize(QSize(80, 40))
        self.tree_buttons_box.addWidget(self.delete_node_btn)

        spacer = QSpacerItem(0, 0, QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Minimum)
        self.tree_buttons_box.addItem(spacer)

    def _setup_table_search(self):
        self.table_search_input = QLineEdit()
        self.table_search_input.setPlaceholderText("Filter bookmarks...")

        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Policy.Fixed, QtWidgets.QSizePolicy.Policy.Fixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.table_search_input.sizePolicy().hasHeightForWidth())
        self.table_search_input.setSizePolicy(sizePolicy)

        self.table_search_input.setMinimumSize(QtCore.QSize(250, 35))
        self.table_search_input.setClearButtonEnabled(True)

        self.table_search_box.addWidget(self.table_search_input)

        self.table_search_button = QtWidgets.QPushButton()

        sizePolicy = QtWidgets.QSizePolicy(QtWidgets.QSizePolicy.Policy.Fixed, QtWidgets.QSizePolicy.Policy.Fixed)
        sizePolicy.setHorizontalStretch(0)
        sizePolicy.setVerticalStretch(0)
        sizePolicy.setHeightForWidth(self.table_search_button.sizePolicy().hasHeightForWidth())

        self.table_search_button.setSizePolicy(sizePolicy)
        self.table_search_button.setMinimumSize(QtCore.QSize(40, 40))

        icon = QtGui.QIcon()
        icon.addPixmap(QtGui.QPixmap(":/search"), QtGui.QIcon.Mode.Normal, QtGui.QIcon.State.Off)

        self.table_search_button.setIcon(icon)
        self.table_search_box.addWidget(self.table_search_button)

        spacer = QSpacerItem(0, 0, QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Minimum)
        self.table_search_box.addItem(spacer)

    def _setup_table_buttons(self):
        self.open_row_btn = QPushButton("Open")
        self.open_row_btn.setFixedSize(QSize(80, 40))
        self.table_buttons_box.addWidget(self.open_row_btn)

        self.edit_row_btn = QPushButton("Edit")
        self.edit_row_btn.setFixedSize(QSize(80, 40))
        self.table_buttons_box.addWidget(self.edit_row_btn)

        self.copy_link_btn = QPushButton("Copy Link")
        self.copy_link_btn.setFixedSize(QSize(80, 40))
        self.table_buttons_box.addWidget(self.copy_link_btn)

        spacer = QSpacerItem(20, 0, QtWidgets.QSizePolicy.Policy.Minimum, QtWidgets.QSizePolicy.Policy.Minimum)
        self.table_buttons_box.addItem(spacer)

        self.delete_row_btn = QPushButton("Delete")
        self.delete_row_btn.setFixedSize(QSize(80, 40))
        self.table_buttons_box.addWidget(self.delete_row_btn)

        spacer = QSpacerItem(0, 0, QtWidgets.QSizePolicy.Policy.Expanding, QtWidgets.QSizePolicy.Policy.Minimum)
        self.table_buttons_box.addItem(spacer)

    def _setup_tree_view(self):
        self.tree_view = QTreeView()
        self.left_box_layout.addWidget(self.tree_view)

        self.tree_view.setHeaderHidden(True)
        self.tree_view.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.tree_view.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
        self.tree_view.setRootIsDecorated(True)

        self._init_tree_model()

    def _init_tree_model(self):
        self.tree_model = QStandardItemModel(0, 1, self)

        self.tree_view.setModel(self.tree_model)

        self._create_tree_items(self.tree_model)

        self.tree_view.expandAll()

    def _setup_suttas_table(self):
        self.suttas_table = QTableView()
        self.right_box_layout.addWidget(self.suttas_table)

        self.suttas_table.setShowGrid(False)
        self.suttas_table.setWordWrap(False)
        self.suttas_table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)

        # MultiSelection allows multiple items to be selected with left-click,
        # and it becomes confusing what should be opened when the Open button or
        # double-click is used.
        self.suttas_table.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)

        horiz_header = self.suttas_table.horizontalHeader()
        if horiz_header is not None:
            horiz_header.setSectionResizeMode(QHeaderView.ResizeMode.Interactive)
            horiz_header.setStretchLastSection(True)

        self.suttas_table.setContextMenuPolicy(Qt.ContextMenuPolicy.ActionsContextMenu)

        copy_link_to_sutta = QAction("Copy Link to Sutta and Selection", self.suttas_table)
        copy_link_to_sutta.triggered.connect(partial(self._handle_copy_link_to_sutta))
        self.suttas_table.addAction(copy_link_to_sutta)

        self.suttas_model = SuttaModel()
        self.suttas_table.setModel(self.suttas_model)

    def _handle_copy_link_to_sutta(self):
        sel = self.suttas_table.selectedIndexes()
        if len(sel) == 0:
            return

        idx = sel[0]

        data = self.suttas_model._data

        uid = data[idx.row()][SuttaModelColToIdx['uid']]
        url = QUrl(f"ssp://{QueryType.suttas.value}/{uid}")

        query = QUrlQuery()

        quote = data[idx.row()][SuttaModelColToIdx['Quote']]
        if quote is not None and len(quote) > 0:
            query.addQueryItem('quote', quote_plus(quote))

        sel_range = data[idx.row()][SuttaModelColToIdx['_selection_range']]
        if sel_range is not None and len(sel_range) > 0:
            query.addQueryItem('sel', quote_plus(sel_range))

        url.setQuery(query)

        self._app_data.clipboard_setText(url.toString())

    def _add_nodes_from_names(self, model: QStandardItemModel, bookmark_names: List[str]):
        root_node = model.invisibleRootItem()
        if root_node is None:
            return

        bookmark_names.sort()

        for name in bookmark_names:
            name = re.sub(r'/+$', '', name)
            parts = list(map(lambda x: x.strip(), name.split("/")))

            for level, _ in enumerate(parts):

                path = "/".join(parts[0:level+1])

                if path not in self.tree_nodes.keys():
                    item = BookmarkItem(path)
                    self.tree_nodes[path] = item

                    if level > 0:
                        parent_path = "/".join(parts[0:level])
                        self.tree_nodes[parent_path].appendRow(item)

        for k in self.tree_nodes.keys():
            if "/" not in k:
                root_node.appendRow(self.tree_nodes[k])

    def reload_bookmarks(self, query: Optional[str] = None):
        self.tree_model.clear()
        self.tree_nodes = dict()
        self._create_tree_items(self.tree_model, query)

        self.tree_model.layoutChanged.emit()

        self.tree_view.expandAll()

    def reload_table(self, query: Optional[str] = None):
        if self.current_bookmark_item is None:
            return

        path = self.current_bookmark_item.path
        query = self.table_search_input.text()

        data = self._data_items_for_bookmark_path(path, query)

        self.suttas_model = SuttaModel(data)
        self.suttas_table.setModel(self.suttas_model)

        self.suttas_model.layoutChanged.emit()

    def _create_tree_items(self,
                           model: QStandardItemModel,
                           query: Optional[str] = None):
        # data = [
        #     "clinging/attachment/",
        #     "clinging/attachment/becoming/",
        #     "craving/attachment/",
        #     "craving/clinging/becoming/",
        #     "craving/clinging/tanha/",
        #     "solitude/emptiness/",
        #     "solitude/quiet/",
        # ]

        if query:
            r = self._app_data.db_session \
                              .query(Um.Bookmark.name) \
                              .filter(Um.Bookmark.name.like(f"%{query}%")) \
                              .all()
        else:
            r = self._app_data.db_session \
                              .query(Um.Bookmark.name) \
                              .all()

        names = list(map(lambda x: str(x[0]), r))

        self._add_nodes_from_names(model, names)

    def _bookmarks_for_path(self, path: str, query: Optional[str] = None) -> List[UBookmark]:
        # ensure trailing / for db value
        if not path.endswith("/"):
            path += '/'

        if query:
            res = self._app_data.db_session \
                                .query(Um.Bookmark) \
                                .filter(and_(
                                    not_(Um.Bookmark.sutta_uid == ''),
                                    Um.Bookmark.name.like(f"{path}%"),
                                    or_(
                                        Um.Bookmark.quote.like(f"%{query}%"),
                                        Um.Bookmark.sutta_ref.like(f"%{query}%"),
                                        Um.Bookmark.sutta_uid.like(f"%{query}%"),
                                        Um.Bookmark.sutta_title.like(f"%{query}%"),
                                    ),
                                )) \
                                .all()

        else:
            res = self._app_data.db_session \
                                .query(Um.Bookmark) \
                                .filter(and_(
                                    not_(Um.Bookmark.sutta_uid == ''),
                                    Um.Bookmark.name.like(f"{path}%"),
                                )) \
                                .all()

        return res

    def _data_items_for_bookmark_path(self, path: str, query: Optional[str] = None) -> List[List[str]]:
        bookmarks = self._bookmarks_for_path(path, query)

        if len(bookmarks) == 0:
            return []

        def _model_data_item(x: UBookmark) -> List[str]:
            # Return values ordered as in SuttaModelColToIdx
            return ["" if x.sutta_ref is None else str(x.sutta_ref),
                    "" if x.sutta_title is None else str(x.sutta_title),
                    "" if x.sutta_uid is None else str(x.sutta_uid),
                    "" if x.quote is None else str(x.quote),
                    "" if x.comment_text is None else str(x.comment_text),
                    "" if x.selection_range is None else str(x.selection_range),
                    str(x.id)]

        data = list(map(_model_data_item, bookmarks))

        return data

    def _suttas_for_path(self, path: str, query: Optional[str] = None) -> List[USutta]:
        bookmarks = self._bookmarks_for_path(path, query)
        suttas: List[USutta] = []

        for i in bookmarks:
            r: Optional[USutta] = None
            if i.sutta_schema == DbSchemaName.AppData.value:
                r = self._app_data.db_session \
                                  .query(Am.Sutta) \
                                  .filter(Am.Sutta.uid == str(i.sutta_uid)) \
                                  .first()

            elif i.sutta_schema == DbSchemaName.UserData.value:
                r = self._app_data.db_session \
                                  .query(Um.Sutta) \
                                  .filter(Um.Sutta.uid == str(i.sutta_uid)) \
                                  .first()

            else:
                raise Exception("Only appdata and userdata schema are allowed.")

            if r is not None:
                suttas.append(r)

        return suttas

    def _handle_tree_clicked(self, val: QModelIndex):
        item: BookmarkItem = self.tree_model.itemFromIndex(val) # type: ignore
        self.current_bookmark_item = item

        data = self._data_items_for_bookmark_path(item.path)

        self.suttas_model = SuttaModel(data)
        self.suttas_table.setModel(self.suttas_model)

    def _handle_sutta_open(self, val: QModelIndex):
        model = val.model()
        if model is None:
            return
        data = model.data(val, Qt.ItemDataRole.UserRole)
        uid = data[val.row()][SuttaModelColToIdx['uid']]
        url = QUrl(f"ssp://{QueryType.suttas.value}/{uid}")

        query = QUrlQuery()

        quote = data[val.row()][SuttaModelColToIdx['Quote']]
        if quote is not None and len(quote) > 0:
            query.addQueryItem('quote', quote_plus(quote))

        sel_range = data[val.row()][SuttaModelColToIdx['_selection_range']]
        if sel_range is not None and len(sel_range) > 0:
            query.addQueryItem('sel', sel_range)

        url.setQuery(query)

        self.show_sutta_by_url.emit(url)

    def _handle_row_open(self):
        a = self.suttas_table.selectedIndexes()
        if len(a) != 0:
            self._handle_sutta_open(a[0])

    def _handle_tree_query(self):
        query = self.tree_search_input.text()
        if len(query) == 0:
            self.reload_bookmarks()

        elif len(query) > 3:
            self.reload_bookmarks(query)

    def _handle_table_query(self):
        query = self.table_search_input.text()
        if len(query) == 0:
            self.reload_table()

        elif len(query) > 3:
            self.reload_table(query)

    def _handle_toggle_collapse(self):
        a = self.tree_model.index(0, 0)
        if self.tree_view.isExpanded(a):
            self.tree_view.collapseAll()
        else:
            self.tree_view.expandAll()

    def _create_new_bookmark_folder(self, values: dict):
        # 'Folder' meaning it doesn't store a location.
        bookmark_name = values['name']

        if len(bookmark_name) == 0:
            return

        try:
            self._create_or_update_bookmark(bookmark_name)

            self.reload_bookmarks()
            self.reload_table()

        except Exception as e:
            logger.error(e)

    def _handle_node_new(self):
        d = BookmarkDialog(self._app_data, show_quote=False, show_comment=False)
        d.accepted.connect(self._create_new_bookmark_folder)
        d.exec()

    def _edit_node(self, values: dict):
        old_name = re.sub(r'/+$', '', values['old_name']) + '/'
        new_name = re.sub(r'/+$', '', values['new_name']) + '/'

        res = self._app_data.db_session \
            .query(Um.Bookmark) \
            .filter(Um.Bookmark.name.like(f"{old_name}%")) \
            .all()

        for i in res:
            p = re.compile('^' + old_name)
            name = re.sub(p, new_name, i.name)
            i.name = name

        self._app_data.db_session.commit()

        self.reload_bookmarks()
        self.reload_table()

    def _handle_node_edit(self):
        a = self.tree_view.selectedIndexes()
        if not a:
            return

        idx = a[0]
        item: BookmarkItem = self.tree_model.itemFromIndex(idx) # type: ignore


        d = BookmarkDialog(self._app_data,
                           name=item.path,
                           show_quote=False,
                           show_comment=False,
                           creating_new=False)

        d.accepted.connect(self._edit_node)
        d.exec()

    def _handle_node_delete(self):
        a = self.tree_view.selectedIndexes()
        if not a:
            return

        # only one tree node is selected at a time
        idx = a[0]

        item: BookmarkItem = self.tree_model.itemFromIndex(idx) # type: ignore
        # append trailing / for db value
        path = item.path + "/"

        bookmarks = self._app_data.db_session \
            .query(Um.Bookmark) \
            .filter(Um.Bookmark.name.like(f"{path}%")) \
            .all()

        # bookmarks which point to a sutta, i.e. are not placeholder nodes
        bm = list(filter(lambda x: x.sutta_id is not None, bookmarks))
        n = len(bm)
        if n > 1:
            reply = self._ask_question(f"Delete {n} bookmarks?", "Delete Confirmation")
            if reply != QMessageBox.StandardButton.Yes:
                return

        for i in bookmarks:
            self._app_data.db_session.delete(i)

        self._app_data.db_session.commit()

        self.tree_model.removeRow(idx.row(), idx.parent())

        self.tree_model.layoutChanged.emit()
        self.tree_view.clearSelection()

        self.reload_table()

    def _show_info(self, msg: str, title = "Info"):
        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Information)
        box.setWindowTitle(title)
        box.setText(msg)
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()

    def _ask_question(self, msg: str, title = "Warning") -> int:
        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle(title)
        box.setText(msg)
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)
        return box.exec()

    def _show_warning(self, msg: str, title = "Warning"):
        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle(title)
        box.setText(msg)
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()

    def _edit_row(self, values: dict):
        if not values['db_id']:
            return

        item = self._app_data.db_session \
                            .query(Um.Bookmark) \
                            .filter(Um.Bookmark.id == values['db_id']) \
                            .first()

        if not item:
            return

        old_name = re.sub(r'/+$', '', values['old_name']) + '/'
        new_name = re.sub(r'/+$', '', values['new_name']) + '/'
        quote = values['quote']

        p = re.compile('^' + old_name)
        name = re.sub(p, new_name, item.name)

        item.name = name
        item.quote = quote
        item.selection_range = str(values['selection_range'])
        item.comment_text = str(values['comment_text'])

        self._app_data.db_session.commit()

        self.reload_bookmarks()
        self.reload_table()

    def _handle_row_edit(self):
        a = self.suttas_table.selectedIndexes()
        if not a:
            return

        # only edit the top selection
        idx = a[0]
        db_id = self.suttas_model._data[idx.row()][SuttaModelColToIdx['_db_id']]

        item = self._app_data.db_session \
                            .query(Um.Bookmark) \
                            .filter(Um.Bookmark.id == db_id) \
                            .first()

        if item is None:
            return

        d = BookmarkDialog(self._app_data,
                           name=str(item.name),
                           quote=str(item.quote) if item.quote is not None else '',
                           selection_range=str(item.selection_range) if item.selection_range is not None else '',
                           comment=str(item.comment_text) if item.comment_text is not None else '',
                           db_id=db_id,
                           creating_new=False)

        d.accepted.connect(self._edit_row)
        d.exec()

    def _handle_row_delete(self):
        a = self.suttas_table.selectedIndexes()
        if not a:
            return

        db_ids = set(map(lambda idx: self.suttas_model._data[idx.row()][SuttaModelColToIdx['_db_id']], a))

        n = len(db_ids)
        if n > 1:
            box = QMessageBox()
            box.setIcon(QMessageBox.Icon.Warning)
            box.setWindowTitle("Delete Confirmation")
            box.setText(f"Delete {n} bookmarks?")
            box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

            reply = box.exec()
            if reply != QMessageBox.StandardButton.Yes:
                return

        bookmarks = self._app_data.db_session \
            .query(Um.Bookmark) \
            .filter(Um.Bookmark.id.in_(db_ids)) \
            .all()

        for i in bookmarks:
            self._app_data.db_session.delete(i)

        self._app_data.db_session.commit()

        self.reload_bookmarks()
        self.reload_table()

    def _handle_import_csv(self):
        file_path, _ = QFileDialog \
            .getOpenFileName(self,
                            "Import from CSV...",
                            "",
                            "CSV Files (*.csv)")

        if len(file_path) == 0:
            return

        bookmarks = self._app_data.import_bookmarks(file_path)

        self.reload_bookmarks()
        self.reload_table()

        box = QMessageBox(self)
        box.setIcon(QMessageBox.Icon.Information)
        box.setText(f"Imported {bookmarks} bookmarks.")
        box.setWindowTitle("Import Completed")
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()


    def _handle_export_csv(self):
        file_path, _ = QFileDialog \
            .getSaveFileName(self,
                             "Export as CSV...",
                             "",
                             "CSV Files (*.csv)")

        if len(file_path) == 0:
            return

        bookmarks = self._app_data.export_bookmarks(file_path)

        box = QMessageBox(self)
        box.setIcon(QMessageBox.Icon.Information)
        box.setText(f"Exported {bookmarks} bookmarks.")
        box.setWindowTitle("Export Completed")
        box.setStandardButtons(QMessageBox.StandardButton.Ok)
        box.exec()

    def _handle_export_suttas_epub(self):
        if not self.current_bookmark_item:
            return

        file_path, _ = QFileDialog \
            .getSaveFileName(self,
                             "Export Suttas Under Selected Bookmark as Epub...",
                             "",
                             "Epub Files (*.epub)")

        if len(file_path) == 0:
            return

        if not file_path.lower().endswith(".epub"):
            file_path += ".epub"

        suttas = self._suttas_for_path(self.current_bookmark_item.path)

        try:
            save_suttas_as_epub(
                self._app_data,
                Path(file_path),
                suttas,
                title = f"Bookmark: {self.current_bookmark_item.path}")

        except Exception as e:
            self._show_warning(f"<p>Export error:</p><p>{e}</p>", "Export Error")
            return

        self._show_info("Export completed.")

    def _handle_export_suttas_mobi(self):
        if not self.current_bookmark_item:
            return

        s = self._app_data.app_settings
        ebook_convert = None
        if s['path_to_ebook_convert'] is None:
            p = shutil.which('ebook-convert')
            if p:
                s['path_to_ebook_convert'] = str(p)
                ebook_convert = str(p)
                self._app_data.app_settings = s
                self._app_data._save_app_settings()

        else:
            ebook_convert = s['path_to_ebook_convert']

        if ebook_convert is None:
            self._show_warning("<p>The ebook-convert tool was not found. For MOBI conversion, Calibre Ebook Reader's ebook-convert must be available in PATH.</p>")
            return

        file_path, _ = QFileDialog \
            .getSaveFileName(self,
                             "Export Suttas Under Selected Bookmark as Mobi...",
                             "",
                             "Mobi Files (*.mobi)")

        if len(file_path) == 0:
            return

        if not file_path.lower().endswith(".mobi"):
            file_path += ".mobi"

        suttas = self._suttas_for_path(self.current_bookmark_item.path)

        try:
            save_suttas_as_mobi(
                self._app_data,
                Path(file_path),
                suttas,
                title = f"Bookmark: {self.current_bookmark_item.path}")

        except Exception as e:
            self._show_warning(f"<p>Export error:</p><p>{e}</p>", "Export Error")
            return

        self._show_info("Export completed.")

    def _handle_close(self):
        self.close()

    def _connect_signals(self):
        self.tree_view.clicked.connect(self._handle_tree_clicked)

        self.suttas_table.doubleClicked.connect(self._handle_sutta_open)

        self.tree_search_input.textEdited.connect(partial(self._handle_tree_query))
        self.tree_search_input.returnPressed.connect(partial(self._handle_tree_query))

        self.tree_search_button.clicked.connect(partial(self._handle_tree_query))

        self.toggle_collapse_btn.clicked.connect(partial(self._handle_toggle_collapse))

        self.table_search_input.textEdited.connect(partial(self._handle_table_query))
        self.table_search_input.returnPressed.connect(partial(self._handle_table_query))

        self.table_search_button.clicked.connect(partial(self._handle_table_query))

        self.new_node_btn.clicked.connect(partial(self._handle_node_new))
        self.edit_node_btn.clicked.connect(partial(self._handle_node_edit))
        self.delete_node_btn.clicked.connect(partial(self._handle_node_delete))

        self.open_row_btn.clicked.connect(partial(self._handle_row_open))
        self.edit_row_btn.clicked.connect(partial(self._handle_row_edit))
        self.copy_link_btn.clicked.connect(partial(self._handle_copy_link_to_sutta))
        self.delete_row_btn.clicked.connect(partial(self._handle_row_delete))

        self.action_close_window.triggered.connect(partial(self._handle_close))

        self.action_import_csv.triggered.connect(partial(self._handle_import_csv))
        self.action_export_csv.triggered.connect(partial(self._handle_export_csv))

        self.action_export_suttas_as_epub.triggered.connect(partial(self._handle_export_suttas_epub))
        self.action_export_suttas_as_mobi.triggered.connect(partial(self._handle_export_suttas_mobi))
