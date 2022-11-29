import json
import re
import shutil
from functools import partial
from pathlib import Path
from typing import List, Optional, TypedDict

from PyQt6 import QtWidgets
from PyQt6 import QtCore
from PyQt6.QtCore import QModelIndex, QSize, pyqtSignal
from PyQt6.QtGui import QAction, QStandardItem, QStandardItemModel

import tomlkit
import markdown

from sqlalchemy import func

from PyQt6.QtWidgets import (QFileDialog, QHBoxLayout, QLabel, QMenu, QMenuBar, QMessageBox, QPushButton, QSpacerItem, QSplitter, QTreeView, QVBoxLayout, QWidget)
from tomlkit.toml_document import TOMLDocument

from simsapa import COURSES_DIR, DbSchemaName, logger

from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um

from ..app.types import AppData, AppWindowInterface, PaliChallengeType, PaliCourseGroup, PaliItem, PaliListItem, QExpanding, QMinimum, UChallengeCourse, UChallengeGroup


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

    def __init__(self, name: str, db_model: str, db_schema: str, db_id: int):
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

        self.action_Import_Toml = QAction("&Import from TOML...")
        self.menu_File.addAction(self.action_Import_Toml)


    def _setup_course_buttons(self):
        self.start_btn = QPushButton("Start")
        self.start_btn.setFixedSize(QSize(80, 40))
        self.course_buttons_box.addWidget(self.start_btn)

        spacer = QSpacerItem(0, 0, QExpanding, QMinimum)
        self.course_buttons_box.addItem(spacer)


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
            course = ListItem(r.name, "ChallengeCourse", r.metadata.schema, r.id)

            for g in r.groups:
                group = ListItem(g.name, "ChallengeGroup", r.metadata.schema, r.id)
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

        if data['db_model'] == "ChallengeCourse":

            if data['db_schema'] == DbSchemaName.AppData.value:
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


    def _find_group(self, data: PaliListItem) -> Optional[UChallengeCourse]:
        a: Optional[UChallengeCourse] = None

        if data['db_model'] == "ChallengeGroup":

            if data['db_schema'] == DbSchemaName.AppData.value:
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
        if self.current_item is not None:
            a = self._find_course(self.current_item.data)
            if a is not None and a.groups is not None:
                g: UChallengeGroup = a.groups[0] # type: ignore

                msg = PaliCourseGroup(
                    db_schema=g.metadata.schema,
                    db_id=int(str(g.id)),
                )

                self.start_group.emit(msg)


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

        c = Um.ChallengeCourse(
            name = course_name,
            description = t['description'],
            sort_index = courses_count + 1,
        )

        self._app_data.db_session.add(c)
        self._app_data.db_session.commit()

        course_base = self._course_base_from_name(course_name)
        if not course_base.exists():
            course_base.mkdir(parents=True, exist_ok=True)

        shutil.copy(toml_path, course_base)

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
                        gfx = str(course_base.joinpath(j['gfx']))
                    else:
                        gfx = None

                    question = PaliItem(text = j['question'], audio = None, gfx = gfx, uuid = None)

                    if j.get('audio', False):
                        self._copy_to_courses(toml_path, Path(j['audio']), course_base)
                        audio = str(course_base.joinpath(j['audio']))
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


    def _connect_signals(self):
        self.tree_view.clicked.connect(self._handle_tree_clicked)

        self.start_btn.clicked.connect(self._handle_start)

        self.action_Close_Window \
            .triggered.connect(partial(self.close))

        self.action_Import_Toml \
            .triggered.connect(partial(self._handle_import_toml))
