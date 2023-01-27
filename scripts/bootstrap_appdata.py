#!/usr/bin/env python3

import os
import sys
from pathlib import Path
import tomlkit
from typing import List, Optional
from dotenv import load_dotenv
from collections import namedtuple

from sqlalchemy.orm.session import Session
from sqlalchemy.sql import func

from simsapa import DbSchemaName, logger
from simsapa.app.db import appdata_models as Am
from simsapa.app.helpers import consistent_nasal_m, create_app_dirs, compact_rich_text

from simsapa.app.stardict import parse_stardict_zip
from simsapa.app.db.stardict import import_stardict_as_new

import helpers
import suttacentral
import cst4
import dhammatalks_org
import dhammapada_munindo
import dhammapada_tipitaka_net
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


def populate_nyanatiloka_dict_words_from_legacy(appdata_db: Session, legacy_db: Session, limit: Optional[int] = None):
    logger.info("Adding Nyanatiloka DictWords from legacy dict_words")

    label = 'NYANAT'
    # create the dictionary
    dictionary = Am.Dictionary(
        label = label,
        title = "Nyanatiloka's Buddhist Dictionary",
        created_at = func.now(),
    )

    try:
        appdata_db.add(dictionary)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

    # get words and commit to appdata db

    # label is stored lowercase in legacy db
    if limit:
        a = legacy_db.execute(f"SELECT * from dict_words WHERE entry_source = '{label.lower()}' LIMIT {limit};") # type: ignore
    else:
        a = legacy_db.execute(f"SELECT * from dict_words WHERE entry_source = '{label.lower()}';") # type: ignore

    LegacyDictWord = namedtuple('LegacyDictWord', a.keys())
    records = [LegacyDictWord(*r) for r in a.fetchall()]

    def _legacy_to_dict_word(x: LegacyDictWord) -> Am.DictWord:
        # all-lowercase uid
        uid = f"{x.word}/{label}".lower()
        return Am.DictWord(
            dictionary_id = dictionary.id,
            word = consistent_nasal_m(x.word),
            uid = uid,
            source_uid = label,
            definition_plain = compact_rich_text(x.definition_plain),
            definition_html = consistent_nasal_m(x.definition_html),
            summary = consistent_nasal_m(x.summary),
            created_at = func.now(),
        )

    dict_words: List[Am.DictWord] = list(map(_legacy_to_dict_word, records))

    try:
        for i in dict_words:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

def populate_dict_words_from_stardict(appdata_db: Session,
                                      stardict_base_path: Path,
                                      ignore_synonyms = False,
                                      limit: Optional[int] = None):
    logger.info("=== populate_dict_words_from_stardict() ===")

    for d in stardict_base_path.glob("*.zip"):
        logger.info(d)
        # use label as the ZIP file name without the .zip extension
        label = os.path.basename(d).replace('.zip', '')
        paths = parse_stardict_zip(Path(d))

        import_stardict_as_new(appdata_db,
                               DbSchemaName.AppData.value,
                               None,
                               paths,
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

    legacy_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("legacy-db").joinpath("appdata-legacy.sqlite3")
    legacy_db = helpers.get_db_session(legacy_db_path)

    limit = BOOTSTRAP_LIMIT

    sc_db = suttacentral.get_suttacentral_db()

    stardict_base_path = BOOTSTRAP_ASSETS_DIR.joinpath("dict")

    insert_db_version(appdata_db)

    populate_nyanatiloka_dict_words_from_legacy(appdata_db, legacy_db, limit)

    for lang in ['en', 'pli']:
        suttacentral.populate_suttas_from_suttacentral(appdata_db, DbSchemaName.AppData, sc_db, SC_DATA_DIR, lang, limit)

    cst4.populate_suttas_from_cst4(appdata_db, limit)

    dhammatalks_org.populate_suttas_from_dhammatalks_org(appdata_db, limit)

    dhammapada_munindo.populate_suttas_from_dhammapada_munindo(appdata_db, limit)

    dhammapada_tipitaka_net.populate_suttas_from_dhammapada_tipitaka_net(appdata_db, limit)

    suttacentral.add_sc_multi_refs(appdata_db, sc_db)

    multi_refs.populate_sutta_multi_refs(appdata_db, limit)

    # FIXME improve synonym parsing
    populate_dict_words_from_stardict(appdata_db, stardict_base_path, ignore_synonyms=True, limit=limit)

    # Create db links from ssp:// links after all suttas have been added.
    create_links.populate_links(appdata_db)

if __name__ == "__main__":
    main()
