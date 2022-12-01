import json
import re
import shutil
from functools import partial
from pathlib import Path
from typing import List, Optional, TypedDict

from PyQt6 import QtWidgets
from PyQt6 import QtCore
from PyQt6.QtCore import QItemSelection, QItemSelectionModel, QModelIndex, QSize, pyqtSignal
from PyQt6.QtGui import QAction, QStandardItem, QStandardItemModel

import tomlkit
import markdown

from sqlalchemy import func, null

from PyQt6.QtWidgets import (QFileDialog, QHBoxLayout, QLabel, QMenu, QMenuBar, QMessageBox, QPushButton, QSpacerItem, QSplitter, QTreeView, QVBoxLayout, QWidget)
from tomlkit.toml_document import TOMLDocument

from simsapa import COURSES_DIR, DbSchemaName, logger

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import AppData, AppWindowInterface, PaliChallengeType, PaliCourseGroup, PaliItem, PaliListItem, PaliListModel, QExpanding, QMinimum, UChallengeCourse, UChallengeGroup


class TomlCourseChallenge(TypedDict):
    challenge_type: str
    explanation_md: str
    question: str
    answer: str
    audio: str
    gfx: str


class TomlCourseGroup(TypedDict):
    name: str
    description: str
    sort_index: int
    challenges: List[TomlCourseChallenge]


class ListItem(QStandardItem):
    name: str
    data: PaliListItem

    def __init__(self, name: str, db_model: PaliListModel, db_schema: DbSchemaName, db_id: int):
        super().__init__()

        self.name = name
        self.data = PaliListItem(
            db_model=db_model,
            db_schema=db_schema,
            db_id=db_id,
        )

        self.setEditable(False)
        self.setText(self.name)


class CoursesBrowserWindow(AppWindowInterface):

    start_group = pyqtSignal(dict)

    current_item: Optional[ListItem] = None

    def __init__(self, app_data: AppData, parent=None) -> None:
        super().__init__(parent)
        logger.info("CoursesBrowserWindow()")

        self._app_data: AppData = app_data

        self._ui_setup()
        self._connect_signals()


    def _ui_setup(self):
        self.setWindowTitle("Pali Courses Browser")
        self.resize(850, 650)

        self._central_widget = QtWidgets.QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._central_widget.setLayout(self._layout)

        self.course_buttons_box = QHBoxLayout()
        self._layout.addLayout(self.course_buttons_box)

        self.splitter = QSplitter(self._central_widget)
        self.splitter.setOrientation(QtCore.Qt.Orientation.Horizontal)

        self._layout.addWidget(self.splitter)

        self.left_box_widget = QWidget(self.splitter)
        self.left_box_layout = QVBoxLayout(self.left_box_widget)
        self.left_box_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(500, 0, QExpanding, QMinimum)
        self.left_box_layout.addItem(spacer)

        self.right_box_widget = QWidget(self.splitter)
        self.right_box_layout = QVBoxLayout(self.right_box_widget)
        self.right_box_layout.setContentsMargins(0, 0, 0, 0)

        spacer = QSpacerItem(500, 0, QExpanding, QMinimum)
        self.right_box_layout.addItem(spacer)

        self.item_name = QLabel()
        self.item_name.setWordWrap(True)
        self.right_box_layout.addWidget(self.item_name)

        self.item_desc = QLabel()
        self.item_desc.setWordWrap(True)
        self.right_box_layout.addWidget(self.item_desc)

        spacer = QSpacerItem(500, 0, QMinimum, QExpanding)
        self.right_box_layout.addItem(spacer)

        self._setup_menubar()

        self._setup_course_buttons()
        self._setup_courses_tree()


    def _setup_menubar(self):
        self.menubar = QMenuBar()
        self.setMenuBar(self.menubar)

        self.menu_File = QMenu(self.menubar)
        self.menu_File.setTitle("&File")

        self.menubar.addAction(self.menu_File.menuAction())

        self.action_Close_Window = QAction("&Close Window")
        self.menu_File.addAction(self.action_Close_Window)

        self.action_Import_Toml = QAction("&Import a Course from TOML...")
        self.menu_File.addAction(self.action_Import_Toml)

        self.action_Start_Course = QAction("&Start Selected Course")
        self.action_Start_Course.setShortcut("Return")
        self.menu_File.addAction(self.action_Start_Course)


    def _setup_course_buttons(self):
        self.start_btn = QPushButton("Start")
        self.start_btn.setFixedSize(QSize(80, 40))
        self.course_buttons_box.addWidget(self.start_btn)

        self.reset_btn = QPushButton("Reset Progress")
        self.reset_btn.setFixedSize(QSize(120, 40))
        self.course_buttons_box.addWidget(self.reset_btn)

        spacer = QSpacerItem(0, 0, QExpanding, QMinimum)
        self.course_buttons_box.addItem(spacer)

        self.delete_btn = QPushButton("Delete")
        self.delete_btn.setFixedSize(QSize(80, 40))
        self.course_buttons_box.addWidget(self.delete_btn)


    def _setup_courses_tree(self):
        self.tree_view = QTreeView()
        self.left_box_layout.addWidget(self.tree_view)

        self.tree_view.setHeaderHidden(True)
        self.tree_view.setRootIsDecorated(True)

        self._init_tree_model()


    def _init_tree_model(self):
        self.tree_model = QStandardItemModel(0, 1, self)

        self.tree_view.setModel(self.tree_model)

        self._create_tree_items(self.tree_model)

        idx = self.tree_model.index(0, 0)
        self.tree_view.selectionModel() \
                      .select(idx,
                              QItemSelectionModel.SelectionFlag.ClearAndSelect | \
                              QItemSelectionModel.SelectionFlag.Rows)

        self._handle_tree_clicked(idx)

        self.tree_view.setFocus()

        self.tree_view.expandAll()


    def _create_tree_items(self, model: QStandardItemModel):
        res = []

        r = self._app_data.db_session \
                            .query(Um.ChallengeCourse) \
                            .all()
        res.extend(r)

        r = self._app_data.db_session \
                            .query(Am.ChallengeCourse) \
                            .all()
        res.extend(r)

        res = sorted(res, key=lambda x: x.sort_index)

        root_node = model.invisibleRootItem()

        for r in res:
            course = ListItem(r.name, PaliListModel.ChallengeCourse, r.metadata.schema, r.id)

            for g in r.groups:
                group = ListItem(g.name, PaliListModel.ChallengeGroup, r.metadata.schema, r.id)
                course.appendRow(group)

            root_node.appendRow(course)


    def _show_item_content(self, item: ListItem):
        r = self._find_course(item.data)
        if r is None:
            r = self._find_group(item.data)

        if r:
            html = markdown.markdown(str(r.description))

            self.item_name.setText(f"<h1>{r.name}</h1>")
            self.item_desc.setText(html)


    def _handle_tree_clicked(self, val: QModelIndex):
        item: ListItem = self.tree_model.itemFromIndex(val) # type: ignore
        self.current_item = item
        self._show_item_content(item)


    def _find_course(self, data: PaliListItem) -> Optional[UChallengeCourse]:
        a: Optional[UChallengeCourse] = None

        if data['db_model'] == PaliListModel.ChallengeCourse:

            if data['db_schema'] == DbSchemaName.AppData:
                a = self._app_data.db_session \
                    .query(Am.ChallengeCourse) \
                    .filter(Am.ChallengeCourse.id == data['db_id']) \
                    .first()


            else:
                a = self._app_data.db_session \
                    .query(Um.ChallengeCourse) \
                    .filter(Um.ChallengeCourse.id == data['db_id']) \
                    .first()

        return a


    def _find_group(self, data: PaliListItem) -> Optional[UChallengeGroup]:
        a: Optional[UChallengeGroup] = None

        if data['db_model'] == PaliListModel.ChallengeGroup:

            if data['db_schema'] == DbSchemaName.AppData:
                a = self._app_data.db_session \
                    .query(Am.ChallengeGroup) \
                    .filter(Am.ChallengeGroup.id == data['db_id']) \
                    .first()


            else:
                a = self._app_data.db_session \
                    .query(Um.ChallengeGroup) \
                    .filter(Um.ChallengeGroup.id == data['db_id']) \
                    .first()

        return a


    def _handle_start(self):
        if self.current_item is None:
            return

        a = self._find_course(self.current_item.data)
        if a is not None and a.groups is not None:
            g: Optional[UChallengeGroup] = a.groups[0] # type: ignore

        else:
            g = self._find_group(self.current_item.data)

        if g is not None:
            msg = PaliCourseGroup(
                db_schema=g.metadata.schema,
                db_id=int(str(g.id)),
            )

            self.start_group.emit(msg)


    def _start_selected(self, val: QModelIndex):
        item: ListItem = self.tree_model.itemFromIndex(val) # type: ignore
        self.current_item = item
        self._handle_start()


    def parse_toml(self, path: Path) -> Optional[TOMLDocument]:
        with open(path) as f:
            s = f.read()

        t = None
        try:
            t = tomlkit.parse(s)
        except Exception as e:
            msg = f"Can't parse TOML: {path}\n\n{e}"
            logger.error(msg)

            box = QMessageBox(self)
            box.setWindowTitle("Error")
            box.setIcon(QMessageBox.Icon.Warning)
            box.setText(msg)
            box.exec()

        return t

    def _course_base_from_name(self, course_name: str) -> Path:
        p = COURSES_DIR.joinpath(re.sub(r'[^0-9A-Za-z]', '_', course_name))
        return p

    def _copy_to_courses(self, toml_path: Path, asset_rel_path: Path, course_base: Path):
        toml_dir = toml_path.parent

        from_path = toml_dir.joinpath(asset_rel_path)

        to_path = course_base.joinpath(asset_rel_path)

        to_dir = to_path.parent
        if not to_dir.exists():
            to_dir.mkdir(parents=True, exist_ok=True)

        shutil.copy(from_path, to_path)

    def import_from_toml(self, toml_path: Path):
        t = self.parse_toml(toml_path)
        if t is None:
            return

        courses_count = self._app_data.db_session \
                                      .query(func.count(Um.ChallengeCourse.id)) \
                                      .scalar()

        course_name = t.get('name') or 'Unknown'

        course_base = self._course_base_from_name(course_name)
        if not course_base.exists():
            course_base.mkdir(parents=True, exist_ok=True)

        shutil.copy(toml_path, course_base)

        c = Um.ChallengeCourse(
            name = course_name,
            description = t['description'],
            course_dirname = course_base.name,
            sort_index = courses_count + 1,
        )

        self._app_data.db_session.add(c)
        self._app_data.db_session.commit()

        groups: List[TomlCourseGroup] = t.get('groups') or []

        for idx_i, i in enumerate(groups):
            g = Um.ChallengeGroup(
                name = i['name'],
                description = i['description'],
                sort_index = idx_i,
            )

            g.course = c

            self._app_data.db_session.add(g)
            self._app_data.db_session.commit()

            for idx_j, j in enumerate(i['challenges']):

                ch = None

                if j['challenge_type'] == PaliChallengeType.Explanation.value:

                    ch = Um.Challenge(
                        sort_index = idx_j,
                        challenge_type = j['challenge_type'],
                        explanation_md = j['explanation_md'],
                    )

                elif j['challenge_type'] == PaliChallengeType.Vocabulary.value or \
                     j['challenge_type'] == PaliChallengeType.TranslateFromEnglish.value or \
                     j['challenge_type'] == PaliChallengeType.TranslateFromPali.value:

                    if j.get('gfx', False):
                        self._copy_to_courses(toml_path, Path(j['gfx']), course_base)
                        # Challenge asset paths are relative to course dir
                        gfx = j['gfx']
                    else:
                        gfx = None

                    question = PaliItem(text = j['question'], audio = None, gfx = gfx, uuid = None)

                    if j.get('audio', False):
                        self._copy_to_courses(toml_path, Path(j['audio']), course_base)
                        audio = j['audio']
                    else:
                        audio = None

                    answers = [PaliItem(text = j['answer'], audio = audio, gfx = None, uuid = None)]

                    ch = Um.Challenge(
                        sort_index = idx_j,
                        challenge_type = j['challenge_type'],
                        question_json = json.dumps(question),
                        answers_json = json.dumps(answers),
                    )

                if ch is not None:
                    ch.course = c
                    ch.group = g
                    self._app_data.db_session.add(ch)
                    self._app_data.db_session.commit()


    def _reload_courses_tree(self):
        self.tree_model.clear()
        self._create_tree_items(self.tree_model)
        self.tree_model.layoutChanged.emit()
        self.tree_view.expandAll()


    def _handle_import_toml(self):
        file_path, _ = QFileDialog.getOpenFileName(
            None,
            "Open File...",
            "",
            "TOML Files (*.toml)")

        if len(file_path) != 0:
            self.import_from_toml(Path(file_path))
            self._reload_courses_tree()


    def _handle_reset(self):
        if self.current_item is None:
            return

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle("Reset Progress")
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

        course = self._find_course(self.current_item.data)

        if course is not None:
            box.setText(f"Reset progress in {course.name}?")
            if box.exec() != QMessageBox.StandardButton.Yes:
                return

            for i in course.challenges: # type: ignore
                i.level = null()
                i.studied_at = null()
                i.due_at = null()

            self._app_data.db_session.commit()

            self._reload_courses_tree()

            return

        group = self._find_group(self.current_item.data)

        if group is not None:
            box.setText(f"Reset progress in {group.name}?")
            if box.exec() != QMessageBox.StandardButton.Yes:
                return

            for i in group.challenges: # type: ignore
                i.level = null()
                i.studied_at = null()
                i.due_at = null()

            self._app_data.db_session.commit()

            self._reload_courses_tree()


    def _handle_delete(self):
        if self.current_item is None:
            return

        box = QMessageBox()
        box.setIcon(QMessageBox.Icon.Warning)
        box.setWindowTitle("Delete")
        box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

        course = self._find_course(self.current_item.data)

        if course is not None:
            box.setText(f"Delete {course.name}?")
            if box.exec() != QMessageBox.StandardButton.Yes:
                return

            for i in course.challenges: # type: ignore
                self._app_data.db_session.delete(i)

            for i in course.groups: # type: ignore
                self._app_data.db_session.delete(i)

            self._app_data.db_session.delete(course)
            self._app_data.db_session.commit()

            if course.course_dirname is not None:
                p = COURSES_DIR.joinpath(str(course.course_dirname))
                if p.exists():
                    shutil.rmtree(p)

            self._reload_courses_tree()

            return

        group = self._find_group(self.current_item.data)

        if group is not None:
            box.setText(f"Delete {group.name}?")
            if box.exec() != QMessageBox.StandardButton.Yes:
                return

            for i in group.challenges: # type: ignore
                self._app_data.db_session.delete(i)

            self._app_data.db_session.delete(group)
            self._app_data.db_session.commit()
            self._reload_courses_tree()


    def _handle_selection_changed(self, selected: QItemSelection, _: QItemSelection):
        self._handle_tree_clicked(selected.indexes()[0])


    def _connect_signals(self):
        self.tree_view.clicked.connect(partial(self._handle_tree_clicked))
        self.tree_view.doubleClicked.connect(partial(self._start_selected))
        self.tree_view.selectionModel().selectionChanged.connect(partial(self._handle_selection_changed))

        self.start_btn.clicked.connect(partial(self._handle_start))
        self.action_Start_Course.triggered.connect(partial(self._handle_start))

        self.reset_btn.clicked.connect(partial(self._handle_reset))

        self.delete_btn.clicked.connect(partial(self._handle_delete))

        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Import_Toml \
            .triggered.connect(partial(self._handle_import_toml))
