#!/usr/bin/env python3

import os, sys, glob
from pathlib import Path
from typing import List, Optional
import bs4
from bs4 import BeautifulSoup

from sqlalchemy.orm.session import Session

from simsapa import logger, DbSchemaName, DictTypeName
from simsapa.app.db import appdata_models as Am

import helpers

from dotenv import load_dotenv
load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

BOOTSTRAP_ASSETS_DIR = Path(s)

HTML_DIR = BOOTSTRAP_ASSETS_DIR.joinpath('dppn-palikanon-com-fixed/www.palikanon.com/english/pali_names/')

def get_name_from_list_item(item: bs4.Tag) -> Optional[str]:
    # The name can be:
    # <li><b>name</b> ... </li>
    # <li><b><a>name</a></b> ... </li>
    # <li><a><b>name</b></a> ... </li>

    # Sometimes the content is li > p > a so don't use li > a
    name_tag = item.select_one('li a > b')
    if name_tag is None:
        name_tag = item.select_one('li b > a')

    if name_tag is not None:
        # The list item is a link to a long definition block. Those pages
        # are parsed separately.
        return None

    else:
        name_tag = item.select_one('li b')

    if name_tag is None:
        raise Exception(f"Can't find name in list item: {item.decode_contents()}")

    name = name_tag.decode_contents()

    return name

def parse_page(p: Path, dppn_dict: Am.Dictionary) -> List[Am.DictWord]:
    html_text = open(p, 'r', encoding='utf-8').read()

    words: List[Am.DictWord] = []

    soup = BeautifulSoup(html_text, 'html.parser')

    if is_long_definition_page(soup):
        words.append(helpers.get_long_definition_blocks(soup, dppn_dict))

    else:
        words.extend(helpers.get_short_definitions(soup, dppn_dict, get_name_from_list_item))

    return words

def is_long_definition_page(soup: BeautifulSoup) -> bool:
    header_items = soup.select('body > ul > li > h2')
    if len(header_items) == 0:
        header_items = soup.select('body > h2')

    # If the page contains h2, the html page contains long definition blocks.

    # Otherwise the page is a list of short definitions and links to long definitions.

    return (len(header_items) > 0)

def populate_dppn_from_palikanon_com(appdata_db: Session, limit: Optional[int] = None):
    logger.info("populate_dppn_from_palikanon_com()")

    dppn_dict = appdata_db \
        .query(Am.Dictionary) \
        .filter(Am.Dictionary.label == "DPPN") \
        .first()

    if dppn_dict is None:
        dppn_dict = Am.Dictionary(
            label = "DPPN",
            title = "Dictionary of Pāli Proper Names",
            dict_type = DictTypeName.Custom.value,
        )
        appdata_db.add(dppn_dict)
        appdata_db.commit()

    appdata_db.query(Am.DictWord).filter(Am.DictWord.dictionary_id == dppn_dict.id).delete()
    appdata_db.commit()

    words: List[Am.DictWord] = []

    paths = []
    for folder in os.scandir(HTML_DIR):
        if not Path(folder).is_dir():
            continue
        if folder == "images":
            continue

        p = f"{Path(folder).joinpath('*.html')}"
        paths.extend(glob.glob(p))

    if limit:
        n = limit if len(paths) >= limit else len(paths)
        paths = paths[0:n]

    for p in paths:
        dict_entries = parse_page(p, dppn_dict)
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

    populate_dppn_from_palikanon_com(appdata_db)

if __name__ == "__main__":
    main()
