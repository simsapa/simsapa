from importlib import metadata
import logging as _logging
from pathlib import Path
from typing import Optional, TypedDict
import requests
import feedparser
import semver

import re
import bleach

from sqlalchemy import create_engine
from sqlalchemy.engine import Engine
from sqlalchemy_utils import database_exists, create_database
from alembic import command
from alembic.config import Config
from alembic.script import ScriptDirectory
from alembic.runtime.migration import MigrationContext
import tomlkit

from .db import appdata_models as Am
from .db import userdata_models as Um

from simsapa import ALEMBIC_INI, ALEMBIC_DIR, IS_WINDOWS, SIMSAPA_PACKAGE_DIR

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

def get_app_dev_version() -> Optional[str]:

    p = SIMSAPA_PACKAGE_DIR.joinpath('..').joinpath('pyproject.toml')
    if not p.exists():
        return None

    with open(p) as pyproject:
        s = pyproject.read()

    try:
        t = tomlkit.parse(s)
        v = t['tool']['poetry']['version'] # type: ignore
        ver = f"{v}"
    except Exception as e:
        print(f"ERROR: {e}")
        ver = None

    return ver

def get_app_version() -> Optional[str]:
    # Dev version when running from local folder
    ver = get_app_dev_version()
    if ver is not None:
        return ver

    # If not dev, return installed version
    ver = metadata.version('simsapa')
    if len(ver) == 0:
        return None

    # convert PEP440 alpha version string to semver compatible string
    # 0.1.7a5 -> 0.1.7-alpha.5
    ver = re.sub(r'\.(\d+)+a(\d+)$', r'.\1-alpha.\2', ver)

    return ver

class UpdateInfo(TypedDict):
    version: str
    message: str

def get_update_info() -> Optional[UpdateInfo]:
    try:
        d = feedparser.parse("https://github.com/simsapa/simsapa/releases.atom")

        def _id_to_version(id: str):
            return re.sub(r'.*/([^/]+)$', r'\1', id).replace('v', '')

        def _is_version_stable(ver: str):
            return not ('.dev' in ver or '.rc' in ver)

        def _is_entry_version_stable(x):
            ver = _id_to_version(x.id)
            return _is_version_stable(ver)

        # filter entries with .dev or .rc version tags
        stable_entries = list(filter(_is_entry_version_stable, d.entries))

        if len(stable_entries) == 0:
            return None

        entry = stable_entries[0]

        # <id>tag:github.com,2008:Repository/364995446/v0.1.6</id>
        remote_version = _id_to_version(entry.id)
        content = entry.content[0]

        app_version = get_app_version()
        if app_version is None:
            return None

        # if remote version is not greater, do nothing
        if semver.compare(remote_version, app_version) != 1:
            return None

        message = f"<h1>An update is available</h1>"
        message += f"<h2>{entry.title}</h2>"
        message += f"<p>Download from the <a href='{entry.link}'>Relases page</a></p>"
        message += f"<div>{content.value}</div>"

        return UpdateInfo(
            version = remote_version,
            message = message,
        )
    except Exception as e:
        logger.error(e)
        print(f"ERROR: {e}")

        return None

def compactPlainText(text: str) -> str:
    # NOTE: Don't remove new lines here, useful for matching beginning of lines when setting snippets.
    # Replace multiple spaces to one.
    text = re.sub(r"  +", ' ', text)

    return text

def compactRichText(text: str) -> str:
    # Some CSS is not removed by bleach when syntax is malformed
    text = re.sub(r'<style>.*</style>', '', text, flags = re.DOTALL)
    text = text.replace('&nbsp;', ' ')
    text = text.replace('&amp;', '&')
    # remove SuttaCentral ref links
    text = re.sub(r"<a class=.ref\b[^>]+>[^<]*</a>", '', text)
    # make sure there is space before and after tags, so words don't get joined after removing tags
    text = text.replace('<', ' <')
    text = text.replace('</', ' </')
    text = text.replace('>', '> ')
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

