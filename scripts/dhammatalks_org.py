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
from simsapa.app.helpers import consistent_niggahita, compact_rich_text
from simsapa.app.lookup import DHP_CHAPTERS_TO_RANGE
from simsapa.app.types import QueryType

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

HTML_DIR = bootstrap_assets_dir.joinpath('dhammatalks-org/www.dhammatalks.org/suttas/')

RE_SUTTA_HTML_NAME = re.compile(r'(DN|MN|SN|AN|Ch|iti|khp|StNp|thag|thig|ud)[\d_]+.html')

def ref_notation_convert(ref: str) -> str:
    ref = ref.replace('_', '.').lower()
    ref = ref.replace('.html', '').lower()
    ref = ref.replace('stnp', 'snp')
    ref = re.sub(r'khp(\d)', r'kp\1', ref)

    # remove leading zeros, dn02
    ref = re.sub(r'([a-z])0+', r'\1', ref)

    if ref.startswith('ch'):
        m = re.findall(r'ch(\d+)', ref)
        ch_num = int(m[0])
        r = DHP_CHAPTERS_TO_RANGE[ch_num]

        ref = f"dhp{r[0]}-{r[1]}"

    return ref

def href_sutta_html_to_ssp(href: str) -> str:
    m = re.findall(r'#.+', href)
    if len(m) > 0:
        anchor = m[0]
    else:
        anchor = ''

    ref = re.sub(r'^.*/([^/]+)$', r'\1', href)

    ref = ref_notation_convert(ref)

    ssp_uid = f"ssp://{QueryType.suttas.value}/{ref}/en/thanissaro{anchor}"

    return ssp_uid

def parse_sutta(p: Path) -> Am.Sutta:
    html_text = open(p, 'r', encoding='utf-8').read()

    soup = BeautifulSoup(html_text, 'html.parser')
    h = soup.find(id = 'sutta')
    if h is not None:
        # Replace sutta links with internal ssp://
        links = soup.select('#sutta a')
        for link in links:
            href = link.get('href', None)
            if href is not None:
                if href.__class__ == list:
                    href = href[0]

                if re.search(RE_SUTTA_HTML_NAME, href): # type: ignore
                    ssp_href = href_sutta_html_to_ssp(href) # type: ignore
                    link['href'] = ssp_href

        content_html = h.decode_contents() # type: ignore
    else:
        logger.error("No #sutta in %s" % p)
        sys.exit(1)

    # <title>DN 1 &nbsp;Brahmajāla Sutta | The Brahmā Net</title>
    # <title>DN 33 Saṅgīti Sutta | The Discourse for Reciting Together</title>
    # <title>AN 6:20 &nbsp;Maraṇassati Sutta | Mindfulness of Death (2)</title>
    m = re.findall(r'<title>(.+)</title>', html_text)
    s = m[0]

    if '/Ud/' in f"{p}":
        # 2 Appāyuka Sutta | Short-lived
        m = re.findall(r'^.*\|(.+)', s)

    elif '/KN/' in f"{p}":
        # Sn 5:4 &#160;Mettagū’s Questions
        # Khp 6 &#160;Ratana Sutta — Treasures
        m = re.findall(r'^.*&#160;(.+)', s)

    else:
        # AN 6:20
        m = re.findall(r'^\w+ +[\d:]+[\W](.+)\|', s)

    # Dhp XVII : Anger
    if len(m) == 0:
        m = re.findall(r'^[^:]+:(.+)', s)

    # Dhp I &nbsp; Pairs
    if len(m) == 0:
        m = re.findall(r'^.*&nbsp;(.+)', s)

    if len(m) == 0:
        m = re.findall(r'^.*&\#160;(.+)', s)

    # 82 Itivuttaka
    if len(m) == 0:
        m = re.findall(r'^\d+ *(.+)', s)

    title = m[0].strip()
    title = title.replace('&nbsp;', '')
    title = title.replace('&amp;', 'and')
    title = consistent_niggahita(title)

    if '/Ud/' in f"{p}":
        # 2 Appāyuka Sutta | Short-lived
        m = re.findall(r'\d+ +(.+)\|', s)
    else:
        m = re.findall(r'\| *(.+)$', s)

    if len(m) == 0:
        title_pali = ''
    else:
        title_pali = consistent_niggahita(m[0])

    ref = re.sub(r'([^0-9])0*', r'\1', p.stem).lower()

    ref = ref_notation_convert(ref)

    lang = "en"
    author = "thanissaro"
    uid = f"{ref}/{lang}/{author}"

    # logger.info(f"{ref} -- {title} -- {title_pali}")

    content_html = '<div class="dhammatalks_org">' + consistent_niggahita(content_html) + '</div>'

    sutta = Am.Sutta(
        source_uid = author,
        title = title,
        title_ascii = title,
        title_pali = title_pali,
        uid = uid,
        sutta_ref = helpers.uid_to_ref(ref),
        nikaya = helpers.uid_to_nikaya(ref),
        language = lang,
        content_html = content_html,
        content_plain = compact_rich_text(content_html),
        created_at = func.now(),
    )

    return sutta

def get_suttas(limit: Optional[int] = None) -> List[Am.Sutta]:

    suttas: List[Am.Sutta] = []

    paths = []
    for folder in ['DN', 'MN', 'SN', 'AN']:
        paths.extend(glob.glob(f"{HTML_DIR.joinpath(folder).joinpath('*.html')}"))

    for folder in ['Dhp', 'Iti', 'Khp', 'StNp', 'Thag', 'Thig', 'Ud']:
        paths.extend(glob.glob(f"{HTML_DIR.joinpath('KN').joinpath(folder).joinpath('*.html')}"))

    if limit:
        n = limit if len(paths) >= limit else len(paths)
        paths = paths[0:n]

    for p in paths:
        p = Path(p)
        if not re.search(RE_SUTTA_HTML_NAME, p.name):
            continue

        sutta = parse_sutta(p)

        suttas.append(sutta)

    return suttas

def populate_suttas_from_dhammatalks_org(appdata_db: Session, limit: Optional[int] = None):
    logger.info("=== populate_suttas_from_dhammatalks_org() ===")

    suttas = get_suttas(limit)

    try:
        for i in suttas:
            appdata_db.add(i)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

def main():
    logger.info("Extract suttas from dhammatalks.org mirror")

    suttas = get_suttas()

    logger.info(f"Count: {len(suttas)}")

if __name__ == "__main__":
    main()
