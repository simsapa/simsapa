import os, sys
from pathlib import Path
from typing import Tuple, Optional
import sqlite3

from sqlalchemy import create_engine
from sqlalchemy.sql import text
from sqlalchemy.engine import Engine
from sqlalchemy.engine.base import Connection
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import Session

from simsapa import APP_DB_PATH, DPD_DB_PATH, USER_DB_PATH, logger, DbSchemaName

def get_db_version(db_path: Path) -> Optional[str]:
    con = sqlite3.connect(db_path)
    cur = con.cursor()

    res = cur.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='app_settings';")
    val = res.fetchone()
    # ('app_settings',)
    # or None
    if val is None:
        return None

    res = cur.execute("SELECT value FROM app_settings WHERE key='db_version';")
    val = res.fetchone()

    con.close()

    return val[0]

def get_dpd_db_version() -> Optional[str]:
    con = sqlite3.connect(DPD_DB_PATH)
    cur = con.cursor()

    res = cur.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='db_info';")
    val = res.fetchone()
    # ('db_info',)
    # or None
    if val is None:
        return None

    res = cur.execute("SELECT value FROM db_info WHERE key='dpd_release_version';")
    val = res.fetchone()

    con.close()

    return val[0]

def get_db_engine_connection_session(include_userdata: bool = True) -> Tuple[Engine, Connection, Session]:
    app_db_path = APP_DB_PATH
    user_db_path = USER_DB_PATH
    dpd_db_path = DPD_DB_PATH

    if not os.path.isfile(app_db_path):
        logger.error(f"Database file doesn't exist: {app_db_path}")
        sys.exit(1)

    if include_userdata and not os.path.isfile(user_db_path):
        logger.error(f"Database file doesn't exist: {user_db_path}")
        sys.exit(1)

    if not os.path.isfile(dpd_db_path):
        logger.error(f"Database file doesn't exist: {dpd_db_path}")

    try:
        # Create an in-memory database
        db_eng = create_engine("sqlite+pysqlite://", echo=False)

        db_conn = db_eng.connect()

        # Attach appdata and userdata
        db_conn.execute(text(f"ATTACH DATABASE '{app_db_path}' AS appdata;"))
        if include_userdata:
            db_conn.execute(text(f"ATTACH DATABASE '{user_db_path}' AS userdata;"))

        db_conn.execute(text(f"ATTACH DATABASE '{dpd_db_path}' AS dpd;"))

        Session = sessionmaker(db_eng)
        Session.configure(bind=db_eng)
        db_session = Session()

    except Exception as e:
        logger.error(f"Can't connect to database: {e}")
        sys.exit(1)

    return (db_eng, db_conn, db_session)

def get_db_session_with_schema(db_path: Path, schema: DbSchemaName) -> Tuple[Engine, Connection, Session]:
    if not os.path.isfile(db_path):
        logger.error(f"Database file doesn't exist: {db_path}")
        sys.exit(1)

    try:
        # Create an in-memory database
        db_eng = create_engine("sqlite+pysqlite://", echo=False)

        db_conn = db_eng.connect()

        db_conn.execute(text(f"ATTACH DATABASE '{db_path}' AS {schema.value};"))

        Session = sessionmaker(db_eng)
        Session.configure(bind=db_eng)
        db_session = Session()

    except Exception as e:
        print(f"Can't connect to database: {e}")
        sys.exit(1)

    return (db_eng, db_conn, db_session)

def get_dpd_db_session() -> Tuple[Engine, Connection, Session]:
    return get_db_session_with_schema(DPD_DB_PATH, DbSchemaName.Dpd)
