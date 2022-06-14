#!/usr/bin/env python3

import os
import sys
import re
from pathlib import Path
import glob
from typing import List
from dotenv import load_dotenv
from bs4 import BeautifulSoup

from sqlalchemy.orm.session import Session
from sqlalchemy.sql import func
from sqlalchemy.orm.session import make_transient

from simsapa import logger
from simsapa.app.helpers import gretil_header_to_footer
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

        # Attempty to fix invalid HTML with html5bib
        # https://stackoverflow.com/questions/23394746/beautifulsoup-and-invalid-html-document
        # https://www.crummy.com/software/BeautifulSoup/bs4/doc/#installing-a-parser

        soup = BeautifulSoup(html_text, 'html5lib')
        h = soup.find(name = 'body')
        if h is not None:
            body = h.decode_contents() # type: ignore
        else:
            logger.error("Missing <body> from html page in %s" % html_path)
            continue

        body = str(body)

        h = soup.find(name = 'title')
        if h is not None:
            title = h.decode_contents() # type: ignore
            title = re.sub(r' *\(GRETIL\)', '', str(title))
        else:
            logger.error("Missing <title> from html page in %s" % html_path)
            title = html_path.stem.replace('sa_', '')

        title = title.strip()

        ref = html_path.stem.replace('sa_', '')
        lang = "skr"
        author = "gretil"
        uid = f"{ref}/{lang}/{author}"

        logger.info(f"{uid} -- {title}")

        main_text = gretil_header_to_footer(body)

        content_html = '<div class="gretil lang-skr">' + main_text + '</div>'

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


def populate_sanskrit_from_gretil(db_session: Session):
    suttas = get_gretil_suttas()

    logger.info(f"Adding GRETIL, count {len(suttas)} ...")

    try:
        for i in suttas:
            db_session.add(i)
            db_session.commit()
    except Exception as e:
        logger.error(e)
        exit(1)


def populate_from_sanskrit_to_appdata(sanskrit_db: Session, appdata_db: Session):
    res = sanskrit_db.query(Am.Sutta).all()

    logger.info(f"Importing to Appdata, {len(res)} suttas ...")

    try:
        for i in res:
            sanskrit_db.expunge(i)
            make_transient(i)
            i.id = None

            appdata_db.add(i)
            appdata_db.commit()
    except Exception as e:
        logger.error(f"Import problem: {e}")
        exit(1)


def main():
    sanskrit_db_path = bootstrap_assets_dir.joinpath("dist").joinpath("sanskrit-texts.sqlite3")
    sanskrit_db = helpers.get_appdata_db(sanskrit_db_path, remove_if_exists = True)

    populate_sanskrit_from_gretil(sanskrit_db)

    appdata_db_path = bootstrap_assets_dir.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db = helpers.get_appdata_db(appdata_db_path, remove_if_exists = False)

    populate_from_sanskrit_to_appdata(sanskrit_db, appdata_db)

if __name__ == "__main__":
    main()
