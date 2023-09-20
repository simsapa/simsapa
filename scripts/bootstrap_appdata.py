#!/usr/bin/env python3

import os
import sys
from pathlib import Path
import tomlkit
from typing import Optional
from dotenv import load_dotenv

from sqlalchemy.orm.session import Session

from simsapa import DbSchemaName, logger

from simsapa.app.types import SearchArea
from simsapa.app.db import appdata_models as Am
from simsapa.app.dir_helpers import create_app_dirs
from simsapa.app.completion_lists import get_and_save_completions

from simsapa.app.stardict import parse_stardict_zip
from simsapa.app.db.stardict import import_stardict_as_new

import helpers
import suttacentral
import nyanatiloka
import cst4
import dhammatalks_org
import dhammapada_munindo
import dhammapada_tipitaka_net
import nyanadipa
import multi_refs
import create_links

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

BOOTSTRAP_ASSETS_DIR = Path(s)
SC_DATA_DIR = BOOTSTRAP_ASSETS_DIR.joinpath("sc-data")

for p in [BOOTSTRAP_ASSETS_DIR, SC_DATA_DIR]:
    if not p.exists():
        logger.error(f"Missing folder: {p}")
        sys.exit(1)

s = os.getenv('BOOTSTRAP_LIMIT')
if s is None or s == "":
    BOOTSTRAP_LIMIT = None
else:
    BOOTSTRAP_LIMIT = int(s)


def populate_dict_words_from_stardict(appdata_db: Session,
                                      stardict_base_path: Path,
                                      ignore_synonyms = False,
                                      limit: Optional[int] = None):
    logger.info("=== populate_dict_words_from_stardict() ===")

    # Folder structure: stardict_base_path/lang/en/DPD.zip

    languages = [str(p.name) for p in stardict_base_path.joinpath('lang').iterdir()]

    for lang in languages:

        for d in stardict_base_path.joinpath("lang").joinpath(lang).glob("*.zip"):
            logger.info(d)
            # use label as the ZIP file name without the .zip extension
            label = os.path.basename(d).replace('.zip', '')
            paths = parse_stardict_zip(Path(d))

            import_stardict_as_new(appdata_db,
                                   DbSchemaName.AppData.value,
                                   None,
                                   paths,
                                   lang,
                                   label,
                                   10000,
                                   ignore_synonyms,
                                   limit)


def insert_db_version(appdata_db: Session):
    p = Path('pyproject.toml')
    if not p.exists():
        logger.error("pyproject.toml not found")
        sys.exit(1)

    with open(p) as f:
        s = f.read()

    try:
        t = tomlkit.parse(s)
        v = t['simsapa']['db_version'] # type: ignore
        ver = f"{v}"
    except Exception as e:
        logger.error(e)
        sys.exit(1)

    item = Am.AppSetting(
        key = "db_version",
        value = ver,
    )

    try:
        appdata_db.add(item)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        sys.exit(1)


def main():
    create_app_dirs()

    appdata_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db = helpers.get_simsapa_db(appdata_db_path, DbSchemaName.AppData, remove_if_exists = True)

    limit = BOOTSTRAP_LIMIT

    sc_db = suttacentral.get_suttacentral_db()

    stardict_base_path = BOOTSTRAP_ASSETS_DIR.joinpath("dict")

    insert_db_version(appdata_db)

    nyanatiloka.populate_nyanatiloka_dict_words_from_legacy(appdata_db, BOOTSTRAP_ASSETS_DIR, limit)

    for lang in ['en', 'pli']:
        suttacentral.populate_suttas_from_suttacentral(appdata_db, DbSchemaName.AppData, sc_db, SC_DATA_DIR, lang, limit)

    cst4.populate_suttas_from_cst4(appdata_db, limit)

    dhammatalks_org.populate_suttas_from_dhammatalks_org(appdata_db, limit)

    dhammapada_munindo.populate_suttas_from_dhammapada_munindo(appdata_db, limit)

    dhammapada_tipitaka_net.populate_suttas_from_dhammapada_tipitaka_net(appdata_db, limit)

    nyanadipa.populate_suttas_from_nyanadipa(appdata_db, limit)

    # === All suttas are added above ===

    suttacentral.add_sc_multi_refs(appdata_db, sc_db)

    multi_refs.populate_sutta_multi_refs(appdata_db, limit)

    # FIXME improve synonym parsing
    populate_dict_words_from_stardict(appdata_db, stardict_base_path, ignore_synonyms=True, limit=limit)

    # === All dict words are added above ===

    get_and_save_completions(appdata_db, SearchArea.Suttas, save_to_schema = DbSchemaName.AppData, load_only_from_appdata=True)
    get_and_save_completions(appdata_db, SearchArea.DictWords, save_to_schema = DbSchemaName.AppData, load_only_from_appdata=True)

    # Create db links from ssp:// links after all suttas have been added.
    create_links.populate_links(appdata_db)

if __name__ == "__main__":
    main()
