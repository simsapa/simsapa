import logging as _logging
from pathlib import Path
import requests

import re
import bleach

from sqlalchemy import create_engine
from sqlalchemy.engine import Engine
from sqlalchemy_utils import database_exists, create_database
from alembic import command
from alembic.config import Config
from alembic.script import ScriptDirectory
from alembic.runtime.migration import MigrationContext

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import ALEMBIC_INI, ALEMBIC_DIR

logger = _logging.getLogger(__name__)

def download_file(url: str, folder_path: Path) -> Path:
    file_name = url.split('/')[-1]
    file_path = folder_path.joinpath(file_name)

    with requests.get(url, stream=True) as r:
        r.raise_for_status()
        with open(file_path, 'wb') as f:
            for chunk in r.iter_content(chunk_size=8192):
                f.write(chunk)

    return file_path

def compactPlainText(text: str) -> str:
    # NOTE: Don't remove new lines here, useful for matching beginning of lines when setting snippets.
    # Replace multiple spaces to one.
    text = re.sub(r"  +", ' ', text)

    return text

def compactRichText(text: str) -> str:
    text = bleach.clean(text, tags=[], styles=[], strip=True)
    text = compactPlainText(text)

    return text

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
            if schema_name == 'userdata':
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
                    # NOTE: logger.error() is not printed for some reason.
                    print("ERROR - Failed to run migrations.")
                    print(e)
                    exit(1)
    else:
        logger.error("Can't create in-memory database")

def is_db_revision_at_head(alembic_cfg: Config, e: Engine) -> bool:
    directory = ScriptDirectory.from_config(alembic_cfg)
    with e.begin() as db_conn:
        context = MigrationContext.configure(db_conn)
        return set(context.get_current_heads()) == set(directory.get_heads())
