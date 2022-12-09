#!/usr/bin/env python3

import os
import sys
import glob
import re
from pathlib import Path
from typing import List, Optional
from dotenv import load_dotenv
from bs4 import BeautifulSoup

from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from simsapa.app.db import appdata_models as Am
from simsapa import logger

import helpers

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

HTML_DIR = bootstrap_assets_dir.joinpath('dhammapada-munindo/html/')


def parse_sutta(p: Path) -> Am.Sutta:
    html_text = open(p, 'r', encoding='utf-8').read()

    soup = BeautifulSoup(html_text, 'html.parser')
    h = soup.find(name = 'h1')
    if h is not None:
        title = h.decode_contents() # type: ignore
    else:
        logger.error("No h1 in %s" % p)
        sys.exit(1)

    m = re.findall(r'dhp-(\d+)', p.stem)
    ch_num = int(m[0])
    r = helpers.DHP_CHAPTERS_TO_RANGE[ch_num]

    ref = f"dhp{r[0]}-{r[1]}"

    lang = "en"
    author = "munindo"
    source_uid = author
    uid = f"{ref}/{lang}/{source_uid}"

    logger.info(f"{ref} -- {title}")

    content_html = '<div class="dhammapada_munindo">' + html_text + '</div>'

    sutta = Am.Sutta(
        source_uid = source_uid,
        title = title,
        title_pali = '',
        uid = uid,
        sutta_ref = helpers.uid_to_ref(ref),
        language = lang,
        content_html = content_html,
        created_at = func.now(),
    )

    return sutta

def get_suttas(limit: Optional[int] = None) -> List[Am.Sutta]:

    suttas: List[Am.Sutta] = []

    paths = glob.glob(f"{HTML_DIR.joinpath('dhp-*.html')}")

    if limit:
        n = limit if len(paths) >= limit else len(paths)
        paths = paths[0:n]

    for p in paths:
        p = Path(p)
        if not re.search(r'^dhp-\d+.html', p.name):
            continue

        sutta = parse_sutta(p)

        suttas.append(sutta)

    return suttas

def populate_suttas_from_dhammapada_munindo(appdata_db: Session, limit: Optional[int] = None):
    logger.info("=== populate_suttas_from_dhammapada_munindo() ===")

    suttas = get_suttas(limit)

    try:
        for i in suttas:
            appdata_db.add(i)
            appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

def main():
    logger.info(f"Parsing suttas from {HTML_DIR}", start_new=True)

    suttas = get_suttas()

    logger.info(f"Count: {len(suttas)}")

if __name__ == "__main__":
    main()
