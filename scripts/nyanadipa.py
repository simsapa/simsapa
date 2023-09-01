#!/usr/bin/env python3

import os
import sys
import glob
from pathlib import Path
from typing import List, Optional
from dotenv import load_dotenv
from bs4 import BeautifulSoup
import markdown

from sqlalchemy.orm.session import Session

from simsapa.app.db import appdata_models as Am
from simsapa import logger

import helpers
from simsapa.app.helpers import consistent_nasal_m, compact_rich_text, pali_to_ascii

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

MARKDOWN_DIR = bootstrap_assets_dir.joinpath('nyanadipa-translations/texts//')

def parse_sutta(p: Path) -> Am.Sutta:
    md_text = open(p, 'r', encoding='utf-8').read()

    html_text = markdown.markdown(
        text = md_text,
        extensions = ['footnotes', 'smarty'],
    )

    soup = BeautifulSoup(html_text, 'html.parser')
    h = soup.find(name = 'h1')
    if h is not None:
        title = h.decode_contents() # type: ignore
    else:
        logger.error("No h1 in %s" % p)
        sys.exit(1)

    ref = p.stem
    lang = "en"
    author = "nyanadipa"
    source_uid = author
    uid = f"{ref}/{lang}/{source_uid}"

    # logger.info(f"{ref} -- {title}")

    title = consistent_nasal_m(title)
    title_ascii = pali_to_ascii(title)

    content_html = consistent_nasal_m(html_text)

    sutta = Am.Sutta(
        source_uid = source_uid,
        title = title,
        title_ascii = title_ascii,
        title_pali = '',
        uid = uid,
        sutta_ref = helpers.uid_to_ref(ref),
        language = lang,
        content_html = content_html,
        content_plain = compact_rich_text(content_html),
    )

    return sutta

def get_suttas(limit: Optional[int] = None) -> List[Am.Sutta]:
    suttas: List[Am.Sutta] = []
    paths = glob.glob(f"{MARKDOWN_DIR.joinpath('*.md')}")

    if limit:
        n = limit if len(paths) >= limit else len(paths)
        paths = paths[0:n]

    for p in paths:
        p = Path(p)
        sutta = parse_sutta(p)
        suttas.append(sutta)

    return suttas

def populate_suttas_from_nyanadipa(appdata_db: Session, limit: Optional[int] = None):
    logger.info("=== populate_suttas_from_nyanadipa() ===")

    suttas = get_suttas(limit)

    try:
        for i in suttas:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

def main():
    logger.info(f"Parsing suttas from {MARKDOWN_DIR}")

    suttas = get_suttas()

    logger.info(f"Count: {len(suttas)}")

if __name__ == "__main__":
    main()
