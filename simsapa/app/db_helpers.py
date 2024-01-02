import sys, json
from pathlib import Path
from typing import Tuple, Set, Dict
from datetime import date

import sqlite3
import contextlib

from PyQt6.QtWidgets import QMessageBox

from sqlalchemy import create_engine
from sqlalchemy.orm.session import Session
from sqlalchemy.sql import text
from sqlalchemy.engine import Engine

from alembic import command
from alembic.config import Config
from alembic.script import ScriptDirectory
from alembic.runtime.migration import MigrationContext

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.db_session import get_db_session_with_schema
from simsapa.app.helpers import pali_to_ascii, word_uid

from simsapa.dpd_db.tools.sandhi_contraction import make_sandhi_contraction_dict
from simsapa.dpd_db.tools.sandhi_contraction import SandhiContractions
from simsapa.dpd_db.exporter.helpers import cf_set_gen, make_roots_count_dict

from simsapa import DbSchemaName, DictTypeName, logger, ALEMBIC_INI, ALEMBIC_DIR

def upgrade_db(db_path: Path, _: str):
    # NOTE: argument not used: schema_name: str

    db_url = f"sqlite+pysqlite:///{db_path}"
    engine = create_engine(db_url, echo=False)

    if isinstance(engine, Engine):
        db_conn = engine.connect()

        alembic_cfg = Config(f"{ALEMBIC_INI}")
        alembic_cfg.set_main_option('script_location', f"{ALEMBIC_DIR}")
        alembic_cfg.set_main_option('sqlalchemy.url', db_url)

        if not is_db_revision_at_head(alembic_cfg, engine):
            logger.info(f"{db_url} is stale, running migrations")

            if db_conn is not None:
                alembic_cfg.attributes['connection'] = db_conn
                try:
                    command.upgrade(alembic_cfg, "head")
                except Exception as e:
                    msg = "Failed to run migrations: %s" % e
                    logger.error(msg)
                    db_conn.close()

                    box = QMessageBox()
                    box.setIcon(QMessageBox.Icon.Warning)
                    box.setWindowTitle("Warning")
                    box.setText("<p>" + msg + "</p><p>Start the application anyway?</p>")
                    box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

                    reply = box.exec()
                    if reply == QMessageBox.StandardButton.No:
                        sys.exit(1)

        db_conn.close()
        engine.dispose()
    else:
        logger.error("Can't create in-memory database")

def find_or_create_db(db_path: Path, schema: DbSchemaName):
    from sqlalchemy_utils import database_exists, create_database

    db_url = f"sqlite+pysqlite:///{db_path}"
    # engine = create_engine(db_url, echo=False)

    alembic_cfg = Config(f"{ALEMBIC_INI}")
    alembic_cfg.set_main_option('script_location', f"{ALEMBIC_DIR}")
    alembic_cfg.set_main_option('sqlalchemy.url', db_url)

    if not database_exists(db_url):
        logger.info(f"Cannot find {db_url}, creating it")
        # On a new install, create database and all tables with the recent schema.
        create_database(db_url)

        # Create an in-memory database
        engine = create_engine("sqlite+pysqlite://", echo=False)
        db_conn = engine.connect()

        db_conn.execute(text(f"ATTACH DATABASE '{db_path}' AS '{schema.value}';"))
        if schema == DbSchemaName.UserData:
            Um.metadata.create_all(bind=engine)
        elif schema == DbSchemaName.AppData:
            Am.metadata.create_all(bind=engine)
        else:
            raise Exception("Only appdata and userdata can be re-created.")

        # generate the Alembic version table, "stamping" it with the most recent rev:
        command.stamp(alembic_cfg, "head")

        db_conn.close()
        engine.dispose()

    else:
        engine = create_engine(db_url, echo=False)
        db_conn = engine.connect()

        if not is_db_revision_at_head(alembic_cfg, engine):
            logger.info(f"{db_url} is stale, running migrations")

            if db_conn is not None:
                alembic_cfg.attributes['connection'] = db_conn
                try:
                    command.upgrade(alembic_cfg, "head")
                except Exception as e:
                    msg = "Failed to run migrations: %s" % e
                    logger.error(msg)
                    db_conn.close()

                    box = QMessageBox()
                    box.setIcon(QMessageBox.Icon.Warning)
                    box.setWindowTitle("Warning")
                    box.setText("<p>" + msg + "</p><p>Start the application anyway?</p>")
                    box.setStandardButtons(QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)

                    reply = box.exec()
                    if reply == QMessageBox.StandardButton.No:
                        sys.exit(1)

        db_conn.close()
        engine.dispose()

def is_db_revision_at_head(alembic_cfg: Config, e: Engine) -> bool:
    directory = ScriptDirectory.from_config(alembic_cfg)
    with e.begin() as db_conn:
        context = MigrationContext.configure(db_conn)
        return set(context.get_current_heads()) == set(directory.get_heads())

def find_or_create_dpd_dictionary(appdata_db_session: Session) -> Am.Dictionary:
    # Find or create DPD Dictionary record in appdata
    dpd_dict = appdata_db_session \
        .query(Am.Dictionary) \
        .filter(Am.Dictionary.label == "DPD") \
        .first()

    if dpd_dict is None:
        dpd_dict = Am.Dictionary(
            label = "DPD",
            title = "Digital Pāḷi Dictionary",
            dict_type = DictTypeName.Sql.value,
        )
        appdata_db_session.add(dpd_dict)
        appdata_db_session.commit()

    return dpd_dict

def save_dpd_caches(dpd_db_session: Session) -> Tuple[Set[str], Dict[str, int], SandhiContractions]:
    """Save cf_set and sandhi_contractions"""

    dpd_cf_set: Set[str] = set()
    dpd_roots_count_dict: Dict[str, int] = dict()
    dpd_sandhi_contractions: SandhiContractions = dict()

    dpd_db_session.query(Dpd.DbInfo).filter(Dpd.DbInfo.key == "cf_set").delete()
    dpd_db_session.query(Dpd.DbInfo).filter(Dpd.DbInfo.key == "roots_count_dict").delete()
    dpd_db_session.query(Dpd.DbInfo).filter(Dpd.DbInfo.key == "sandhi_contractions").delete()
    dpd_db_session.commit()

    # === cf_set ===

    dpd_cf_set = cf_set_gen(dpd_db_session)
    dpd_db_session.add(Dpd.DbInfo(key="cf_set", value=json.dumps(list(dpd_cf_set))))
    dpd_db_session.commit()

    # === roots_count_dict ===

    dpd_roots_count_dict = make_roots_count_dict(dpd_db_session)
    dpd_db_session.add(Dpd.DbInfo(key="roots_count_dict", value=json.dumps(dpd_roots_count_dict)))
    dpd_db_session.commit()

    # === sandhi_contractions ===

    pali_words = dpd_db_session.query(Dpd.PaliWord).all()

    dpd_sandhi_contractions = make_sandhi_contraction_dict(pali_words)

    data = dict()
    for k, v in dpd_sandhi_contractions.items():
        data[k] = {
            'contractions': list(v['contractions']),
            'ids': v['ids'],
        }

    dpd_db_session.add(Dpd.DbInfo(key="sandhi_contractions",
                                  value=json.dumps(data)))
    dpd_db_session.commit()

    return (dpd_cf_set, dpd_roots_count_dict, dpd_sandhi_contractions)


def replace_all_niggahitas(db_conn: sqlite3.Connection):
    cursor = db_conn.cursor()

    # Get all table names
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table';")
    tables = cursor.fetchall()

    for table in tables:
        table = table[0]

        # Get all column names of the table
        cursor.execute(f"PRAGMA table_info({table});")
        columns = [column[1] for column in cursor.fetchall()]

        # For each column, replace 'ṃ' with 'ṁ'
        for column in columns:
            query = f"""
            UPDATE {table}
            SET `{column}` = REPLACE(`{column}`, 'ṃ', 'ṁ')
            WHERE `{column}` LIKE '%ṃ%';
            """
            cursor.execute(query)

    db_conn.commit()

def migrate_dpd(dpd_db_path: Path, dpd_dictionary_id: int) -> None:
    logger.info("migrate_dpd()")

    # Use an sqlite session and update the db schema to agree with the SQLAlchemy model.
    try:
        with contextlib.closing(sqlite3.connect(dpd_db_path)) as connection:
            with contextlib.closing(connection.cursor()) as cursor:
                logger.info("Add db_info table.")
                query = """
                CREATE TABLE
                    db_info(
                        id integer primary key autoincrement,
                        key VARCHAR UNIQUE NOT NULL,
                        value VARCHAR NOT NULL
                    );
                """

                cursor.execute(query)
                connection.commit()

                # logger.info("Add dpd_release_version.")
                query = """
                INSERT INTO db_info (key, value) VALUES (?, ?);
                """

                # Dpd uses current date as version number. Add the version as
                # today's date. When comparing new DPD releases, a more recent
                # date will mean we can download an update for the user.
                ver = date.today().strftime("%Y-%m-%d")

                cursor.execute(query, ("dpd_release_version", ver))
                connection.commit()

                # PaliWord.dictionary_id:
                # PaliRoot.dictionary_id:
                # should return DPD dict id

                query = """
                ALTER TABLE pali_words ADD COLUMN dictionary_id integer NOT NULL DEFAULT 0;
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN dictionary_id integer NOT NULL DEFAULT 0;
                """
                cursor.execute(query)

                connection.commit()

                query = """
                UPDATE pali_words SET dictionary_id = %s;
                """ % (dpd_dictionary_id)
                cursor.execute(query)

                query = """
                UPDATE pali_roots SET dictionary_id = %s;
                """ % (dpd_dictionary_id)
                cursor.execute(query)

                connection.commit()

                # PaliWord: uid, word_ascii, pali_clean

                query = """
                ALTER TABLE pali_words ADD COLUMN uid VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_words ADD COLUMN word_ascii VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_words ADD COLUMN pali_clean VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                # PaliRoot: uid, word_ascii, root_clean, root_no_sign

                query = """
                ALTER TABLE pali_roots ADD COLUMN uid VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN word_ascii VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN root_clean VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN root_no_sign VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                connection.commit()

                replace_all_niggahitas(connection)

    except Exception as e:
        print(str(e))
        sys.exit(2)

    # Now the DPD schema is up to date with the SQLAlchemy definition.

    _, _, dpd_db_session = get_db_session_with_schema(dpd_db_path, DbSchemaName.Dpd)

    for i in dpd_db_session.query(Dpd.PaliWord).all():
        i.uid = word_uid(str(i.id), 'dpd')
        i.pali_clean = i.calc_pali_clean
        # Use pali_clean for word_ascii to remove trailing numbers.
        i.word_ascii = pali_to_ascii(i.calc_pali_clean)

    dpd_db_session.commit()

    for i in dpd_db_session.query(Dpd.PaliRoot).all():
        i.uid = word_uid(i.root, 'dpd')
        i.root_clean = i.calc_root_clean
        i.root_no_sign = i.calc_root_no_sign
        # Use root_clean for word_ascii to remove trailing numbers.
        i.word_ascii = pali_to_ascii(i.calc_root_clean)

    dpd_db_session.commit()

    save_dpd_caches(dpd_db_session)

    dpd_db_session.close()
