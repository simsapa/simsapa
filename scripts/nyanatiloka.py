#!/usr/bin/env python3

import os, sys, glob, re
from pathlib import Path
from typing import List, Optional
import bs4
from bs4 import BeautifulSoup
from dotenv import load_dotenv

from sqlalchemy.orm.session import Session

from simsapa import logger, DbSchemaName, DictTypeName
from simsapa.app.db import appdata_models as Am

from simsapa.app.helpers import strip_html

from scripts import helpers

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

BOOTSTRAP_ASSETS_DIR = Path(s)

HTML_DIR = BOOTSTRAP_ASSETS_DIR.joinpath('nyanatiloka-palikanon-com-fixed/www.palikanon.com/english/wtb/')

def get_term_from_list_item(item: bs4.Tag) -> Optional[str]:
    # The term is the text until the first ':' ';' '='
    # <li>beings, The 9 worlds of: <i><a href="s_t/sattaavaasa_n.htm">sattāvāsa</a></i>. </li>
    # <li>bodily action (wholesome or unwholesome); s. <a href="g_m/karma.htm"> karma</a>, <a href="g_m/karma_formations.htm">karma formations</a> - Right b.a. = <i> sammā-kammanta;</i> s. <i> <a href="g_m/magga.htm">magga</a>. </i> </li>
    # <li>sambodhi = <i><a href="b_f/bodhi.htm">bodhi</a></i>.  </li>
    #
    # Or it is a link to a long definition page, which we ignore because it will be parsed serparately.
    # <li><a href="b_f/bhava.htm">bhava</a></li>

    item_html = item.decode_contents()
    item_text = strip_html(item_html)

    term_link = item.select_one('li > a')
    if term_link is not None:
        term_link.decompose()
        s = item.decode_contents().strip()
        if s == "":
            # The list item is a link to a long definition block. Those pages
            # are parsed separately.
            return None

    a = re.split("[:;=]", item_text)
    if len(a) == 1:
        raise Exception(f"Can't recognize the term in list item: {item.decode_contents()}")

    term = a[0]

    if len(term) < 3:
        raise Exception(f"Term seems too short: {term}, from item: {item_html}")

    return term

def parse_page(p: Path, nyanat_dict: Am.Dictionary) -> List[Am.DictWord]:
    # logger.info(f"parse_page: {p}")
    html_text = open(p, 'r', encoding='utf-8').read()

    words: List[Am.DictWord] = []

    soup = BeautifulSoup(html_text, 'html.parser')

    if is_long_definition_page(soup):
        words.append(helpers.get_long_definition_blocks(soup, nyanat_dict))

    else:
        words.extend(helpers.get_short_definitions(soup, nyanat_dict, get_term_from_list_item))

    return words

def is_long_definition_page(soup: BeautifulSoup) -> bool:
    header_items = soup.select('body > ul > li > h2')
    if len(header_items) == 0:
        header_items = soup.select('body > h2')

    # If the list contains h2, the html page contains long definition blocks.

    # Otherwise the page is a list of short definitions and links to long definitions.

    return (len(header_items) > 0)

def populate_nyanatiloka_from_palikanon_com(appdata_db: Session, limit: Optional[int] = None):
    logger.info("populate_nyanatiloka_from_palikanon_com()")

    nyanat_dict = appdata_db \
        .query(Am.Dictionary) \
        .filter(Am.Dictionary.label == "NYANAT") \
        .first()

    if nyanat_dict is None:
        nyanat_dict = Am.Dictionary(
            label = "NYANAT",
            title = "Manual of Buddhist Terms and Doctrines by Ven. Nyanatiloka",
            dict_type = DictTypeName.Custom.value,
        )
        appdata_db.add(nyanat_dict)
        appdata_db.commit()

    appdata_db.query(Am.DictWord).filter(Am.DictWord.dictionary_id == nyanat_dict.id).delete()
    appdata_db.commit()

    words: List[Am.DictWord] = []

    paths = []

    p = f"{Path(HTML_DIR).joinpath('*.html')}"
    paths.extend(glob.glob(p))

    for folder in os.scandir(HTML_DIR):
        if not Path(folder).is_dir():
            continue

        p = f"{Path(folder).joinpath('*.html')}"
        paths.extend(glob.glob(p))

    if limit:
        n = limit if len(paths) >= limit else len(paths)
        paths = paths[0:n]

    for p in paths:
        if str(p).endswith("dic_idx.html"):
            continue

        dict_entries = parse_page(p, nyanat_dict)
        words.extend(dict_entries)

    logger.info(f"Adding {len(words)} words.")

    try:
        for i in words:
            helpers.check_and_fix_dict_word_uid(appdata_db, i)
            appdata_db.add(i)
            # Necessary to commit each word in order to check for uid
            # constraint.
            appdata_db.commit()
    except Exception as e:
        logger.error(e)

def main():
    # appdata_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db_path = Path("/home/gambhiro/.local/share/simsapa/assets/appdata.sqlite3")

    appdata_db = helpers.get_simsapa_db(appdata_db_path, DbSchemaName.AppData, remove_if_exists = False)

    populate_nyanatiloka_from_palikanon_com(appdata_db)

if __name__ == "__main__":
    main()
