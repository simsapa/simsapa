#!/usr/bin/env python3

import os
import sys
import glob
import re
from pathlib import Path
from typing import List, Optional, Tuple
from dotenv import load_dotenv
from bs4 import BeautifulSoup

from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from simsapa.app.db import appdata_models as Am
from simsapa import logger

import helpers
from simsapa.app.helpers import consistent_nasal_m, compact_rich_text

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

HTML_DIR = bootstrap_assets_dir.joinpath('dhammapada-tipitaka-net/www.tipitaka.net/tipitaka/dhp/')

def parse_chapter(p: Path) -> Tuple[int, str, str]:
    html_text = open(p, 'r', encoding='latin1').read()

    # Mirrored from www.tipitaka.net/tipitaka/dhp/verseload.php?verse=002 by HTTrack
    # Mirrored from www.tipitaka.net/tipitaka/dhp/verseload.php?verse=416b by HTTrack
    m = re.findall(r'verseload.php\?verse=(\d+)\w* by HTTrack', html_text)
    dhp_num = int(m[0])

    soup = BeautifulSoup(html_text, 'html.parser')

    # Extract title text and add an id to anchor link to
    h = soup.select('.main > p:first-child > strong')
    if len(h) != 0:
        title_id = f"title_{dhp_num}"
        h[0]['id'] = title_id
        title = h[0].decode_contents()
        title = title.replace('\n', ' ').replace('<br>', ' ').replace('<br/>', ' ')
    else:
        title = f"Dhammapada Verse {dhp_num}"
        title_id = ""

    # Extract main text
    h = soup.select('.main > blockquote')
    if len(h) != 0:
        content_html = h[0].decode_contents()
        if title_id == "":
            title_id = f"title_{dhp_num}"
            content_html = f'<a id="{title_id}"></a>' + content_html
    else:
        logger.error("No main blockquote in %s" % p)
        sys.exit(1)

    title_li = f'<li><a href="#{title_id}">{title}</a></li>'

    return (dhp_num, content_html, title_li)

def parse_sutta(ref: str, content_html: str) -> Am.Sutta:
    title = consistent_nasal_m("Dhammapada")
    title_pali = title

    lang = "en"
    # Translated by Daw Mya Tin, M.A.
    author = "daw"
    uid = f"{ref}/{lang}/{author}"

    logger.info(f"{ref} -- {title}")

    content_html = '<div class="tipitaka_net">' + consistent_nasal_m(content_html) + '</div>'

    sutta = Am.Sutta(
        source_uid = author,
        title = title,
        title_pali = title_pali,
        uid = uid,
        sutta_ref = helpers.uid_to_ref(ref),
        language = lang,
        content_html = content_html,
        content_plain = compact_rich_text(content_html),
        created_at = func.now(),
    )

    return sutta

def get_suttas(limit: Optional[int] = None) -> List[Am.Sutta]:

    suttas: List[Am.Sutta] = []

    num_to_html: dict[int, str] = {}
    num_to_li: dict[int, str] = {}
    chapters: dict[str, str] = {}
    toc_links: dict[str, str] = {}

    paths = glob.glob(f"{HTML_DIR.joinpath('verseload*.html')}")

    if limit:
        n = limit if len(paths) >= limit else len(paths)
        paths = paths[0:n]

    for p in paths:
        p = Path(p)

        dhp_num, content_html, title_li = parse_chapter(p)
        num_to_html[dhp_num] = content_html
        num_to_li[dhp_num] = title_li

    sorted_keys = list(num_to_html.keys())
    sorted_keys.sort()

    for dhp_num in sorted_keys:
        ref = helpers.dhp_chapter_ref_for_verse_num(dhp_num)
        if ref is None:
            logger.error(f"Can't get chapter ref: {dhp_num}")
            continue

        if ref not in chapters:
            chapters[ref] = ''

        if ref not in toc_links:
            toc_links[ref] = ''

        chapters[ref] += num_to_html[dhp_num]
        toc_links[ref] += num_to_li[dhp_num]

    for ref, html in chapters.items():
        html = '<ul class="toc">' + toc_links[ref] + '</ul>' + html
        suttas.append(parse_sutta(ref, html))

    return suttas

def populate_suttas_from_dhammapada_tipitaka_net(appdata_db: Session, limit: Optional[int] = None):
    logger.info("=== populate_suttas_from_dhammapada_tipitaka_net() ===")

    suttas = get_suttas(limit)

    try:
        for i in suttas:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

def main():
    logger.info(f"Parsing suttas from {HTML_DIR}")

    suttas = get_suttas()

    logger.info(f"Count: {len(suttas)}")

if __name__ == "__main__":
    main()
