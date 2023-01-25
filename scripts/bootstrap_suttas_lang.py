#!/usr/bin/env python3

import os
import sys
from pathlib import Path
from dotenv import load_dotenv

from simsapa import logger
from simsapa.app.helpers import create_app_dirs

import helpers
import suttacentral

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

def main():
    if len(sys.argv) < 2:
        print("First argument: language")
        sys.exit(2)

    lang = sys.argv[1]
    name = f"suttas_lang_{lang}.sqlite3"

    create_app_dirs()

    db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath(name)
    db_session = helpers.get_appdata_db(db_path, remove_if_exists = True)

    limit = BOOTSTRAP_LIMIT

    sc_db = suttacentral.get_suttacentral_db()

    suttacentral.populate_suttas_from_suttacentral(db_session, sc_db, SC_DATA_DIR, lang, limit)

if __name__ == "__main__":
    main()
