#!/usr/bin/env python3

import os, sys, shutil
from pathlib import Path

from sqlalchemy.orm.session import Session

from simsapa import logger, DbSchemaName
from simsapa.app.db_helpers import find_or_create_dpd_dictionary, migrate_dpd

from scripts import helpers

def prepare_dpd_for_dist(appdata_db_session: Session, bootstrap_dir: Path):
    logger.info("prepare_dpd_for_dist()")

    dpd_dict = find_or_create_dpd_dictionary(appdata_db_session)

    # Copy DPD to dist before modifying (migating)
    src = bootstrap_dir \
        .joinpath("dpd-db-for-bootstrap") \
        .joinpath("current") \
        .joinpath("dpd.db")

    dest = bootstrap_dir \
        .joinpath("dist") \
        .joinpath("dpd.sqlite3")

    shutil.copy(src, dest)

    migrate_dpd(dest, dpd_dict.id)

def main():
    from dotenv import load_dotenv
    load_dotenv()

    s = os.getenv('BOOTSTRAP_ASSETS_DIR')
    if s is None or s == "":
        logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
        sys.exit(1)

    BOOTSTRAP_ASSETS_DIR = Path(s)

    # appdata_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db_path = Path("/home/gambhiro/.local/share/simsapa/assets/appdata.sqlite3")

    appdata_db = helpers.get_simsapa_db(appdata_db_path, DbSchemaName.AppData, remove_if_exists = False)

    prepare_dpd_for_dist(appdata_db, BOOTSTRAP_ASSETS_DIR)

    # Copy to local folder for testing with the app.

    src = BOOTSTRAP_ASSETS_DIR \
        .joinpath("dist") \
        .joinpath("dpd.sqlite3")

    dest = Path("/home/gambhiro/.local/share/simsapa/assets/dpd.sqlite3")

    shutil.copy(src, dest)

if __name__ == "__main__":
    main()
