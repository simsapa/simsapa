import sys
from pathlib import Path
from PyQt6.QtWidgets import QMessageBox

from sqlalchemy import create_engine
from sqlalchemy.sql import text
from sqlalchemy.engine import Engine

from alembic import command
from alembic.config import Config
from alembic.script import ScriptDirectory
from alembic.runtime.migration import MigrationContext

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import DbSchemaName, logger, ALEMBIC_INI, ALEMBIC_DIR


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

def find_or_create_db(db_path: Path, schema_name: str):
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

        db_conn.execute(text(f"ATTACH DATABASE '{db_path}' AS '{schema_name}';"))
        if schema_name == DbSchemaName.UserData.value:
            Um.metadata.create_all(bind=engine)
        else:
            Am.metadata.create_all(bind=engine)

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
