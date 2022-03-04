#!/usr/bin/env python3

import os
import sys
import glob
import re
from pathlib import Path
from typing import List
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

HTML_DIR = bootstrap_assets_dir.joinpath('dhammatalks-org/www.dhammatalks.org/suttas/')


def parse_sutta(p: Path) -> Am.Sutta:
    html_text = open(p, 'r', encoding='utf-8').read()

    soup = BeautifulSoup(html_text, 'html.parser')
    h = soup.find(id = 'sutta')
    if h is not None:
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

    if '/Ud/' in f"{p}":
        # 2 Appāyuka Sutta | Short-lived
        m = re.findall(r'\d+ +(.+)\|', s)
    else:
        m = re.findall(r'\| *(.+)$', s)

    if len(m) == 0:
        title_pali = ''
    else:
        title_pali = m[0]

    ref = re.sub(r'([^0-9])0*', r'\1', p.stem).lower()
    ref = ref.replace('_', '.')
    ref = ref.replace('stnp', 'snp')

    if ref.startswith('ch'):
        m = re.findall(r'ch(\d+)', ref)
        ch_num = int(m[0])
        r = helpers.DHP_CHAPTERS_TO_RANGE[ch_num]

        ref = f"dhp{r[0]}-{r[1]}"

    lang = "en"
    author = "than"
    uid = f"{ref}/{lang}/{author}"

    logger.info(f"{ref} -- {title} -- {title_pali}")

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

    paths = []
    for folder in ['DN', 'MN', 'SN', 'AN']:
        paths.extend(glob.glob(f"{HTML_DIR.joinpath(folder).joinpath('*.html')}"))

    for folder in ['Dhp', 'Iti', 'Khp', 'StNp', 'Thag', 'Thig', 'Ud']:
        paths.extend(glob.glob(f"{HTML_DIR.joinpath('KN').joinpath(folder).joinpath('*.html')}"))

    for p in paths:
        p = Path(p)
        if not re.search(r'^(DN|MN|SN|AN|Ch|iti|khp|StNp|thag|thig|ud)[\d_]+.html', p.name):
            continue

        sutta = parse_sutta(p)

        suttas.append(sutta)

    return suttas

def populate_suttas_from_dhammatalks_org(appdata_db: Session):

    suttas = get_suttas()

    try:
        for i in suttas:
            appdata_db.add(i)
            appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

def main():
    logger.info("Extract suttas from dhammatalks.org mirror", start_new=True)

    suttas = get_suttas()

    logger.info(f"Count: {len(suttas)}")

if __name__ == "__main__":
    main()
