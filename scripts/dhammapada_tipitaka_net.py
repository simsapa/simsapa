#!/usr/bin/env python3

import os
import sys
import glob
import re
from pathlib import Path
from typing import List, Tuple
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

HTML_DIR = bootstrap_assets_dir.joinpath('dhammapada-tipitaka-net/www.tipitaka.net/tipitaka/dhp/')

def parse_chapter(p: Path) -> Tuple[int, str]:
    html_text = open(p, 'r', encoding='latin1').read()

    soup = BeautifulSoup(html_text, 'html.parser')
    h = soup.select('.main > blockquote')
    if len(h) != 0:
        content_html = h[0].decode_contents() # type: ignore
    else:
        logger.error("No main blockquote in %s" % p)
        sys.exit(1)

    # Mirrored from www.tipitaka.net/tipitaka/dhp/verseload.php?verse=002 by HTTrack
    # Mirrored from www.tipitaka.net/tipitaka/dhp/verseload.php?verse=416b by HTTrack
    m = re.findall(r'verseload.php\?verse=(\d+)\w* by HTTrack', html_text)
    dhp_num = int(m[0])

    return (dhp_num, content_html)

def parse_sutta(ref: str, content_html: str) -> Am.Sutta:
    title = "Dhammapada"
    title_pali = title

    lang = "en"
    # Translated by Daw Mya Tin, M.A.
    author = "daw"
    uid = f"{ref}/{lang}/{author}"

    logger.info(f"{ref} -- {title}")

    sutta = Am.Sutta(
        title = title,
        title_pali = title_pali,
        uid = uid,
        sutta_ref = helpers.uid_to_ref(ref),
        language = lang,
        content_html = content_html,
        created_at = func.now(),
    )

    return sutta

def get_suttas() -> List[Am.Sutta]:

    suttas: List[Am.Sutta] = []

    chapters = {}

    for p in glob.glob(f"{HTML_DIR.joinpath('verseload*.html')}"):
        p = Path(p)

        dhp_num, content_html = parse_chapter(p)
        ref = helpers.dhp_chapter_ref_for_verse_num(dhp_num)
        if ref is None:
            logger.error(f"Can't get chapter ref: {dhp_num}")
            continue

        if ref not in chapters:
            chapters[ref] = ''

        chapters[ref] += content_html

    for ref, html in chapters.items():
        suttas.append(parse_sutta(ref, html))

    return suttas

def populate_suttas_from_dhammapada_tipitaka_net(appdata_db: Session):

    suttas = get_suttas()

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
