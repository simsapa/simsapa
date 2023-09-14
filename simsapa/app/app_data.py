import csv, re, json, os, shutil
import os.path
from pathlib import Path
from functools import partial
from typing import Dict, List, Optional, Tuple

from deepmerge import always_merger
import tomlkit
from tomlkit.toml_document import TOMLDocument

from sqlalchemy import create_engine
from sqlalchemy.engine import Engine
from sqlalchemy.engine.base import Connection
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import Session
from sqlalchemy.sql import func, text
from sqlalchemy import and_

from PyQt6.QtCore import QSize, QThreadPool, QTimer
from PyQt6.QtGui import QClipboard

from simsapa import COURSES_DIR, DbSchemaName, get_is_gui, logger, APP_DB_PATH, USER_DB_PATH, ASSETS_DIR, INDEX_DIR
from simsapa.app.actions_manager import ActionsManager
from simsapa.app.completion_lists import WordSublists
from simsapa.app.db_session import get_db_session_with_schema
from simsapa.app.helpers import bilara_text_to_segments
from simsapa.app.search.tantivy_index import TantivySearchIndexes

from simsapa.app.types import SearchArea, USutta, UDictWord, UBookmark

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa.layouts.gui_types import AppSettings, default_app_settings, PaliGroupStats, PaliChallengeType, PaliItem, TomlCourseGroup

class AppData:

    app_settings: AppSettings
    screen_size: Optional[QSize] = None
    db_eng: Engine
    db_conn: Connection
    db_session: Session
    # Keys are db schema, and course group id
    pali_groups_stats: Dict[DbSchemaName, Dict[int, PaliGroupStats]] = dict()
    search_indexes: Optional[TantivySearchIndexes] = None

    _sutta_titles_completion_cache: WordSublists = dict()
    _dict_words_completion_cache: WordSublists = dict()

    def __init__(self,
                 actions_manager: Optional[ActionsManager] = None,
                 app_clipboard: Optional[QClipboard] = None,
                 app_db_path: Optional[Path] = None,
                 user_db_path: Optional[Path] = None,
                 api_port: Optional[int] = None):
        logger.profile("AppData::__init__(): start")

        self.clipboard: Optional[QClipboard] = app_clipboard

        self.actions_manager = actions_manager

        # Remove indexes marked to be deleted in a previous session. Can't
        # safely clear and remove them after the application has opened them.
        self._remove_marked_indexes()

        if app_db_path is None:
            self._app_db_path = self._find_app_data_or_exit()

        if user_db_path is None:
            self._user_db_path = USER_DB_PATH
        else:
            self._user_db_path = user_db_path

        # Make sure the user_db exists before getting the db_session handle.
        self._check_db(self._user_db_path, DbSchemaName.UserData)

        self.db_eng, self.db_conn, self.db_session = self._get_db_engine_connection_session(self._app_db_path, self._user_db_path)
        self._read_app_settings()

        self.graph_gen_pool = QThreadPool()

        self.api_url: Optional[str] = None

        if api_port:
            self.api_url = f'http://localhost:{api_port}'

        self.sutta_to_open: Optional[USutta] = None
        self.dict_word_to_open: Optional[UDictWord] = None

        self._init_completion_cache()

        logger.info(f"IS_GUI: {get_is_gui()}")
        if get_is_gui():
            # Wait 0.5s, then run slowish initialize tasks, e.g. search indexes, db check, upgrade and init, etc.
            # By that time the first window will be opened and will not delay app.exec().
            #
            # QTimer can only be used when running Qt. Otherwise it errors:
            #
            # QObject::startTimer: Timers can only be used with threads started with QThread
            #
            # Separate threads also don't work becuase the db_session cannot cross thread boundaries.

            self.init_timer = QTimer()
            self.init_timer.setSingleShot(True)
            self.init_timer.timeout.connect(partial(self._init_tasks))
            self.init_timer.start(500)

        else:
            self._init_tasks()

        logger.profile("AppData::__init__(): end")

    def _init_tasks(self):
        self._init_search_indexes()
        self._find_cli_paths()
        self._read_pali_groups_stats()
        self._ensure_user_memo_deck()

    def _init_search_indexes(self):
        logger.profile("_init_search_indexes()")
        self.search_indexes = TantivySearchIndexes(self.db_session)
        self._check_empty_index(self.search_indexes)

    def _init_completion_cache(self):
        from simsapa.app.completion_lists import get_and_save_completions

        sutta_titles = get_and_save_completions(self.db_session, SearchArea.Suttas)
        dict_words = get_and_save_completions(self.db_session, SearchArea.DictWords)

        logger.info(f"AppData::_init_completion_cache(): sutta_titles: {len(sutta_titles)}, dict_words: {len(dict_words)}")

        self._sutta_titles_completion_cache = sutta_titles
        self._dict_words_completion_cache = dict_words

    def _check_empty_index(self, search_indexes: TantivySearchIndexes):
        if search_indexes.has_empty_index():
            from simsapa.layouts.create_search_index import CreateSearchIndexWindow
            w = CreateSearchIndexWindow()
            w.show()
            # FIXME handle empty index action
            # logger.info(f"open_simsapa: {w.open_simsapa}")
            # logger.info(f"app status: {status}")
            # if not w.open_simsapa:
            #     logger.info("Exiting.")
            #     sys.exit(status)

    def _check_db(self, db_path: Path, schema: DbSchemaName):
        """
        Checks if db at db_path exists. If not, creates it.

        This check avoids loading the db_helpers module if not necessary.
        """

        if not db_path.exists():
            from simsapa.app.db_helpers import find_or_create_db
            find_or_create_db(db_path, schema.value)

    def get_search_indexes(self) -> Optional[TantivySearchIndexes]:
        return self.search_indexes

    def _remove_marked_indexes(self):
        p = ASSETS_DIR.joinpath('indexes_to_remove.txt')
        if not p.exists():
            return

        with open(p, 'r') as f:
            s = f.read()
        p.unlink()

        if s == "":
            return

        langs = s.split(',')
        for lang in langs:
            p = INDEX_DIR.joinpath(f'suttas_lang_{lang}_WRITELOCK')
            p.unlink()

            for p in INDEX_DIR.glob(f'suttas_lang_{lang}_*.seg'):
                p.unlink()

            for p in INDEX_DIR.glob(f'_suttas_lang_{lang}_*.toc'):
                p.unlink()

    def _get_db_engine_connection_session(self, app_db_path, user_db_path) -> Tuple[Engine, Connection, Session]:
        if not os.path.isfile(app_db_path):
            logger.error(f"Database file doesn't exist: {app_db_path}")
            exit(1)

        # FIXME avoid loading alembic just for getting a db session
        # upgrade_db(app_db_path, DbSchemaName.AppData.value)

        if not os.path.isfile(user_db_path):
            logger.error(f"Database file doesn't exist: {user_db_path}")
            exit(1)

        # FIXME avoid loading alembic just for getting a db session
        # upgrade_db(user_db_path, DbSchemaName.UserData.value)

        try:
            # Create an in-memory database
            db_eng = create_engine("sqlite+pysqlite://", echo=False)

            db_conn = db_eng.connect()

            # Attach appdata and userdata
            db_conn.execute(text(f"ATTACH DATABASE '{app_db_path}' AS appdata;"))
            db_conn.execute(text(f"ATTACH DATABASE '{user_db_path}' AS userdata;"))

            Session = sessionmaker(db_eng)
            Session.configure(bind=db_eng)
            db_session = Session()

        except Exception as e:
            logger.error(f"Can't connect to database: {e}")
            exit(1)

        return (db_eng, db_conn, db_session)

    def _read_app_settings(self):
        x = self.db_session \
                .query(Um.AppSetting) \
                .filter(Um.AppSetting.key == 'app_settings') \
                .first()

        if x is not None and x.value is not None:
            default_settings = default_app_settings()
            stored_settings = json.loads(x.value)

            # Ensure that keys are not missing by updating the default value.
            s = always_merger.merge(default_settings, stored_settings)
            self.app_settings = s

        else:
            self.app_settings = default_app_settings()

        # Always save it back, default keys might have been updated.
        self._save_app_settings()

    def _save_app_settings(self):
        x = self.db_session \
                .query(Um.AppSetting) \
                .filter(Um.AppSetting.key == 'app_settings') \
                .first()

        try:
            if x is not None:
                x.value = json.dumps(self.app_settings)
                self.db_session.commit()
            else:
                x = Um.AppSetting(
                    key = 'app_settings',
                    value = json.dumps(self.app_settings),
                    created_at = func.now(),
                )
                self.db_session.add(x)
                self.db_session.commit()
        except Exception as e:
            logger.error(e)

    def _find_cli_paths(self):
        s = self.app_settings

        if not s['path_to_curl']:
            p = shutil.which('curl')
            if p:
                s['path_to_curl'] = str(p)

        if not s['path_to_scp']:
            p = shutil.which('scp')
            if p:
                s['path_to_scp'] = str(p)

        if not s['path_to_ebook_convert']:
            p = shutil.which('ebook-convert')
            if p:
                s['path_to_ebook_convert'] = str(p)

        self.app_settings = s
        self._save_app_settings()

    def _read_pali_groups_stats(self):
        schemas = [DbSchemaName.AppData, DbSchemaName.UserData]

        for sc in schemas:
            key = f"{sc}_pali_groups_stats"
            r = self.db_session \
                    .query(Um.AppSetting) \
                    .filter(Um.AppSetting.key == key) \
                    .first()

            if r is not None and r.value is not None:
                self.pali_groups_stats[sc] = json.loads(r.value)
            else:
                self.pali_groups_stats[sc] = dict()
                self._save_pali_groups_stats(sc)

    def _save_pali_groups_stats(self, schema: DbSchemaName):
        key = f"{schema}_pali_groups_stats"
        r = self.db_session \
                .query(Um.AppSetting) \
                .filter(Um.AppSetting.key == key) \
                .first()

        try:
            if r is not None:
                r.value = json.dumps(self.pali_groups_stats[schema])
                self.db_session.commit()
            else:
                r = Um.AppSetting(
                    key = key,
                    value = json.dumps(self.pali_groups_stats[schema]),
                    created_at = func.now(),
                )
                self.db_session.add(r)
                self.db_session.commit()
        except Exception as e:
            logger.error(e)

    def _ensure_user_memo_deck(self):
        deck = self.db_session.query(Um.Deck).first()
        if deck is None:
            deck = Um.Deck(name = "Simsapa")
            self.db_session.add(deck)
            self.db_session.commit()

    def clipboard_setText(self, text):
        if self.clipboard is not None:
            self.clipboard.clear()
            self.clipboard.setText(text)

    def clipboard_getText(self) -> Optional[str]:
        if self.clipboard is not None:
            return self.clipboard.text()
        else:
            return None

    def _find_app_data_or_exit(self):
        if not APP_DB_PATH.exists():
            logger.error("Cannot find appdata.sqlite3")
            exit(1)
        else:
            return APP_DB_PATH

    def import_bookmarks(self, file_path: str) -> int:
        p = Path(file_path)
        if not p.exists():
            return 0

        rows = []

        with open(file_path) as f:
            reader = csv.DictReader(f)
            for row in reader:
                rows.append(row)

        def _to_bookmark(x: Dict[str, str]) -> UBookmark:
            return Um.Bookmark(
                name          = x['name']          if x['name']          != 'None' else None,
                quote         = x['quote']         if x['quote']         != 'None' else None,
                selection_range = x['selection_range'] if x['selection_range'] != 'None' else None,
                sutta_id      = int(x['sutta_id']) if x['sutta_id']      != 'None' else None,
                sutta_uid     = x['sutta_uid']     if x['sutta_uid']     != 'None' else None,
                sutta_schema  = x['sutta_schema']  if x['sutta_schema']  != 'None' else None,
                sutta_ref     = x['sutta_ref']     if x['sutta_ref']     != 'None' else None,
                sutta_title   = x['sutta_title']   if x['sutta_title']   != 'None' else None,
                comment_text  = x['comment_text']  if x['comment_text']  != 'None' else None,
                comment_attr_json = x['comment_attr_json'] if x['comment_attr_json'] != 'None' else None,
                read_only     = x['read_only']     if x['read_only']     != 'None' else None,
            )

        bookmarks = list(map(_to_bookmark, rows))

        try:
            for i in bookmarks:
                self.db_session.add(i)
            self.db_session.commit()
        except Exception as e:
            logger.error(e)
            return 0

        return len(bookmarks)

    def import_suttas_to_userdata(self, db_path: str) -> int:
        import_db_eng, import_db_conn, import_db_session = get_db_session_with_schema(Path(db_path), DbSchemaName.UserData)

        import_suttas = import_db_session.query(Um.Sutta).all()

        if len(import_suttas) == 0:
            import_db_conn.close()
            import_db_session.close()
            import_db_eng.dispose()
            return 0

        for i in import_suttas:
            sutta = Um.Sutta(
                uid = i.uid,
                group_path = i.group_path,
                group_index = i.group_index,
                sutta_ref = i.sutta_ref,
                language = i.language,
                order_index = i.order_index,

                sutta_range_group = i.sutta_range_group,
                sutta_range_start = i.sutta_range_start,
                sutta_range_end = i.sutta_range_end,

                title = i.title,
                title_pali = i.title_pali,
                title_trans = i.title_trans,
                description = i.description,
                content_plain = i.content_plain,
                content_html = i.content_html,
                content_json = i.content_json,
                content_json_tmpl = i.content_json_tmpl,

                source_uid = i.source_uid,
                source_info = i.source_info,
                source_language = i.source_language,
                message = i.message,
                copyright = i.copyright,
                license = i.license,
            )

            author_uid = i.source_uid

            author = self.db_session \
                         .query(Um.Author) \
                         .filter(Um.Author.uid == author_uid) \
                         .first()

            if author is None:
                author = Um.Author(uid = author_uid)

            self.db_session.add(author)
            sutta.author = author

            self.db_session.add(sutta)

        self.db_session.commit()

        n = len(import_suttas)

        import_db_conn.close()
        import_db_session.close()
        import_db_eng.dispose()

        return n

    def export_bookmarks(self, file_path: str) -> int:
        if not file_path.endswith(".csv"):
            file_path = f"{file_path}.csv"

        res = self.db_session \
                  .query(Um.Bookmark) \
                  .filter(Um.Bookmark.sutta_uid != '') \
                  .all()

        if not res:
            return 0

        def _to_row(x: UBookmark) -> Dict[str, str]:
            return {
                "name": str(x.name),
                "quote": str(x.quote),
                "selection_range": str(x.selection_range),
                "sutta_id": str(x.sutta_id),
                "sutta_uid": str(x.sutta_uid),
                "sutta_schema": str(x.sutta_schema),
                "sutta_ref": str(x.sutta_ref),
                "sutta_title": str(x.sutta_title),
                "comment_text": str(x.comment_text),
                "comment_attr_json": str(x.comment_attr_json),
                "read_only": str(x.read_only),
            }

        a = list(map(_to_row, res))
        rows = sorted(a, key=lambda x: x['name'])

        try:
            with open(file_path, 'w') as f:
                w = csv.DictWriter(f, fieldnames=rows[0].keys())
                w.writeheader()
                for r in rows:
                    w.writerow(r)
        except Exception as e:
            logger.error(e)
            return 0

        return len(rows)

    def import_prompts(self, file_path: str) -> int:
        p = Path(file_path)
        if not p.exists():
            return 0

        rows = []

        with open(file_path) as f:
            reader = csv.DictReader(f)
            for row in reader:
                rows.append(row)

        def _to_prompt(x: Dict[str, str]) -> Um.GptPrompt:
            return Um.GptPrompt(
                name_path       = x['name_path']       if x['name_path']       != 'None' else None,
                messages_json   = x['messages_json']   if x['messages_json']   != 'None' else None,
                show_in_context = True                 if x['show_in_context'] == 'True' else False,
            )

        prompts = list(map(_to_prompt, rows))

        try:
            for i in prompts:
                # GPT Prompt name_path is unique, unlike the bookmark name.
                r = self.db_session.query(Um.GptPrompt).filter(Um.GptPrompt.name_path == i.name_path).first()
                if r is not None:
                    self.db_session.delete(r)
                    self.db_session.commit()

                self.db_session.add(i)

            self.db_session.commit()
        except Exception as e:
            logger.error(e)
            return 0

        return len(prompts)

    def export_prompts(self, file_path: str) -> int:
        if not file_path.endswith(".csv"):
            file_path = f"{file_path}.csv"

        res = self.db_session.query(Um.GptPrompt).all()

        if not res:
            return 0

        def _to_row(x: Um.GptPrompt) -> Dict[str, str]:
            return {
                "name_path": str(x.name_path),
                "messages_json": str(x.messages_json),
                "show_in_context": str(x.show_in_context),
            }

        a = list(map(_to_row, res))
        rows = sorted(a, key=lambda x: x['name_path'])

        try:
            with open(file_path, 'w') as f:
                w = csv.DictWriter(f, fieldnames=rows[0].keys())
                w.writeheader()
                for r in rows:
                    w.writerow(r)
        except Exception as e:
            logger.error(e)
            return 0

        return len(rows)

    def export_app_settings(self, file_path: str):
        if not file_path.endswith(".json"):
            file_path = f"{file_path}.json"

        try:
            with open(file_path, 'w') as f:
                s = json.dumps(self.app_settings)
                f.write(s)
        except Exception as e:
            logger.error(e)
            return 0

    def import_app_settings(self, file_path: str):
        p = Path(file_path)
        if not p.exists():
            return

        with open(p, 'r') as f:
            value = json.loads(f.read())

            d = default_app_settings()
            d.update(value)
            self.app_settings = d

            self._save_app_settings()

    def parse_toml(self, path: Path) -> Optional[TOMLDocument]:
        with open(path) as f:
            s = f.read()

        t = None
        try:
            t = tomlkit.parse(s)
        except Exception as e:
            msg = f"Can't parse TOML: {path}\n\n{e}"
            logger.error(msg)
            raise Exception(msg)

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


    def import_pali_course(self, file_path: str) -> Optional[str]:
        try:
            t = self.parse_toml(Path(file_path))
        except Exception as e:
            raise e

        if t is None:
            return

        courses_count = self.db_session \
                            .query(func.count(Um.ChallengeCourse.id)) \
                            .scalar()

        course_name = t.get('name') or 'Unknown'

        course_base = self._course_base_from_name(course_name)
        if not course_base.exists():
            course_base.mkdir(parents=True, exist_ok=True)

        shutil.copy(file_path, course_base)

        c = Um.ChallengeCourse(
            name = course_name,
            description = t['description'],
            course_dirname = course_base.name,
            sort_index = courses_count + 1,
        )

        self.db_session.add(c)
        self.db_session.commit()

        groups: List[TomlCourseGroup] = t.get('groups') or []

        for idx_i, i in enumerate(groups):
            g = Um.ChallengeGroup(
                name = i['name'],
                description = i['description'],
                sort_index = idx_i,
            )

            g.course = c

            self.db_session.add(g)
            self.db_session.commit()

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
                        self._copy_to_courses(Path(file_path), Path(j['gfx']), course_base)
                        # Challenge asset paths are relative to course dir
                        gfx = j['gfx']
                    else:
                        gfx = None

                    question = PaliItem(text = j['question'], audio = None, gfx = gfx, uuid = None)

                    if j.get('audio', False):
                        self._copy_to_courses(Path(file_path), Path(j['audio']), course_base)
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
                    self.db_session.add(ch)
                    self.db_session.commit()

        return course_name

    def get_pali_for_translated(self, sutta: USutta) -> Optional[USutta]:
        if sutta.language == 'pli':
            return None

        uid_ref = re.sub('^([^/]+)/.*', r'\1', str(sutta.uid))

        res: List[USutta] = []
        r = self.db_session \
            .query(Am.Sutta) \
            .filter(and_(
                Am.Sutta.uid != sutta.uid,
                Am.Sutta.language == 'pli',
                Am.Sutta.uid.like(f"{uid_ref}/%"),
            )) \
            .all()
        res.extend(r)

        r = self.db_session \
            .query(Um.Sutta) \
            .filter(and_(
                Um.Sutta.uid != sutta.uid,
                Um.Sutta.language == 'pli',
                Um.Sutta.uid.like(f"{uid_ref}/%"),
            )) \
            .all()
        res.extend(r)

        if len(res) > 0:
            return res[0]
        else:
            return None

    def sutta_to_segments_json(self,
                               sutta: USutta,
                               use_template: bool = True) -> Dict[str, str]:

        res = sutta.variant
        if res is None:
            variant = None
        else:
            variant = str(res.content_json)

        res = sutta.comment
        if res is None:
            comment = None
        else:
            comment = str(res.content_json)

        show_variants = self.app_settings.get('show_all_variant_readings', True)

        if use_template:
            tmpl = str(sutta.content_json_tmpl)
        else:
            tmpl = None

        segments_json = bilara_text_to_segments(
            str(sutta.content_json),
            tmpl,
            variant,
            comment,
            show_variants,
        )

        return segments_json
