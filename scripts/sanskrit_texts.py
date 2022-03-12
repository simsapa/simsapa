#!/usr/bin/env python3

import os
import sys
import re
from pathlib import Path
import glob
import json
from typing import List, Optional
from dotenv import load_dotenv

from sqlalchemy.orm.session import Session
from sqlalchemy.sql import func

from simsapa import logger
from simsapa.app.db import appdata_models as Am

import helpers

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)
gretil_html_dir = bootstrap_assets_dir.joinpath("sanskrit/gretil/1_sanskr/tei/transformations/html")

for p in [bootstrap_assets_dir, gretil_html_dir]:
    if not p.exists():
        logger.error(f"Missing folder: {p}")
        sys.exit(1)


def get_gretil_suttas() -> List[Am.Sutta]:
    suttas: List[Am.Sutta] = []

    for html_path in glob.glob(f"{gretil_html_dir}/*.htm"):
        if 'pAda-index_sa-dharma.htm' in html_path:
            continue

        html_path = Path(html_path)

        html_text = open(html_path, 'r').read()

        m = re.findall(r"<title>(.+?)</title>", html_text)

        if len(m) == 0:
            title = html_path.stem
        else:
            title = re.sub(r' *(GRETIL)', '', m[0])

        ref = html_path.stem.replace('sa_', '')
        lang = "skr"
        author = "gretil"
        uid = f"{ref}/{lang}/{author}"

        content_html = re.sub(r'<h2>Header</h2>(.+?)<h2>Text</h2>', '', html_text)

        sutta = Am.Sutta(
            title = title,
            title_pali = "",
            uid = uid,
            sutta_ref = "",
            language = lang,
            content_html = content_html,
            created_at = func.now(),
        )

        suttas.append(sutta)

    return suttas


def populate_from_gretil(db_session: Session):
    suttas = get_gretil_suttas()

    logger.info(f"Adding GRETIL, count {len(suttas)} ...")

    try:
        for i in suttas:
            db_session.add(i)
            db_session.commit()
    except Exception as e:
        logger.error(e)
        exit(1)


def main():
    sanskrit_db_path = bootstrap_assets_dir.joinpath("dist").joinpath("sanskrit-texts.sqlite3")

    sanskrit_db = helpers.get_appdata_db(sanskrit_db_path)

    populate_from_gretil(sanskrit_db)


if __name__ == "__main__":
    main()
