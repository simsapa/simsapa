import os
from pathlib import Path
from typing import Tuple

from sqlalchemy import create_engine
from sqlalchemy.engine import Engine
from sqlalchemy.engine.base import Connection
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import Session
from sqlalchemy_utils import database_exists, create_database

from alembic import command
from alembic.config import Config
from alembic.script import ScriptDirectory
from alembic.runtime.migration import MigrationContext

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import APP_DB_PATH, USER_DB_PATH, DbSchemaName, logger
from simsapa import ALEMBIC_INI, ALEMBIC_DIR

def find_or_create_db(db_path: Path, schema_name: str):
    # Create an in-memory database
    engine = create_engine("sqlite+pysqlite://", echo=False)

    if isinstance(engine, Engine):
        db_conn = engine.connect()
        db_url = f"sqlite+pysqlite:///{db_path}"

        alembic_cfg = Config(f"{ALEMBIC_INI}")
        alembic_cfg.set_main_option('script_location', f"{ALEMBIC_DIR}")
        alembic_cfg.set_main_option('sqlalchemy.url', db_url)

        if not database_exists(db_url):
            logger.info(f"Cannot find {db_url}, creating it")
            # On a new install, create database and all tables with the recent schema.
            create_database(db_url)
            db_conn.execute(f"ATTACH DATABASE '{db_path}' AS '{schema_name}';")
            if schema_name == DbSchemaName.UserData.value:
                Um.metadata.create_all(bind=engine)
            else:
                Am.metadata.create_all(bind=engine)

            # generate the Alembic version table, "stamping" it with the most recent rev:
            command.stamp(alembic_cfg, "head")

        elif not is_db_revision_at_head(alembic_cfg, engine):
            logger.info(f"{db_url} is stale, running migrations")

            if db_conn is not None:
                alembic_cfg.attributes['connection'] = db_conn
                try:
                    command.upgrade(alembic_cfg, "head")
                except Exception as e:
                    logger.error("Failed to run migrations: %s" % e)
                    exit(1)

        db_conn.close()
    else:
        logger.error("Can't create in-memory database")

def get_db_engine_connection_session(include_userdata: bool = True) -> Tuple[Engine, Connection, Session]:
    app_db_path = APP_DB_PATH
    user_db_path = USER_DB_PATH

    if not os.path.isfile(app_db_path):
        logger.error(f"Database file doesn't exist: {app_db_path}")
        exit(1)

    if include_userdata and not os.path.isfile(user_db_path):
        logger.error(f"Database file doesn't exist: {user_db_path}")
        exit(1)

    try:
        # Create an in-memory database
        db_eng = create_engine("sqlite+pysqlite://", echo=False)

        db_conn = db_eng.connect()

        # Attach appdata and userdata
        db_conn.execute(f"ATTACH DATABASE '{app_db_path}' AS appdata;")
        if include_userdata:
            db_conn.execute(f"ATTACH DATABASE '{user_db_path}' AS userdata;")

        Session = sessionmaker(db_eng)
        Session.configure(bind=db_eng)
        db_session = Session()

    except Exception as e:
        logger.error(f"Can't connect to database: {e}")
        exit(1)

    return (db_eng, db_conn, db_session)

def is_db_revision_at_head(alembic_cfg: Config, e: Engine) -> bool:
    directory = ScriptDirectory.from_config(alembic_cfg)
    with e.begin() as db_conn:
        context = MigrationContext.configure(db_conn)
        return set(context.get_current_heads()) == set(directory.get_heads())
