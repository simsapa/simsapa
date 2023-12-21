#!/usr/bin/env python3

import os, sys, glob, re
from pathlib import Path
from typing import List, Optional
from bs4 import BeautifulSoup

from sqlalchemy.orm.session import Session

from simsapa import logger, DbSchemaName, DictTypeName
from simsapa.app.db import appdata_models as Am
from simsapa.app.helpers import compact_rich_text, pali_to_ascii, strip_html, word_uid

import helpers

from dotenv import load_dotenv
load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

BOOTSTRAP_ASSETS_DIR = Path(s)

HTML_DIR = BOOTSTRAP_ASSETS_DIR.joinpath('dppn-palikanon-com/www.palikanon.com/english/pali_names/')

def check_uid(db_session: Session, i: Am.DictWord):
    n = 0
    while True:
        r = db_session.query(Am.DictWord).filter(Am.DictWord.uid == i.uid).first()
        if r is not None:
            if n > 5:
                print(f"Tried updating the uid {n} times for word {i.word}. Exiting.")
                sys.exit(2)

            name, label = i.uid.split("/")
            n += 1
            i.uid = f"{name}-{n}/{label}"
        else:
            return

def get_long_definition_blocks(soup: BeautifulSoup, dppn_dict: Am.Dictionary) -> Am.DictWord:
    name_tag = soup.select_one('h2')
    assert(name_tag is not None)
    name = strip_html(name_tag.decode_contents())

    item = soup.select_one('body')
    assert(item is not None)
    item_html = item.decode_contents()

    # Remove the footer
    item_html = re.sub(r'<hr[^>]*>[\n ]*<p align="center">.*', '', item_html, flags=re.DOTALL)

    word = Am.DictWord(
        dictionary_id = dppn_dict.id,
        uid = word_uid(name, dppn_dict.label.lower()),
        word = name,
        word_ascii = pali_to_ascii(name),
        language = "en",
        source_uid = dppn_dict.label,
        definition_html = item_html,
        definition_plain = compact_rich_text(item_html),
    )

    return word

def get_short_definitions(soup: BeautifulSoup, dppn_dict: Am.Dictionary) -> List[Am.DictWord]:
    words: List[Am.DictWord] = []

    list_items = soup.select('body > ul > li')
    for item in list_items:

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
            continue

        else:
            name_tag = item.select_one('li b')

        item_html = item.decode_contents()

        if name_tag is None:
            continue

        assert(name_tag is not None)

        name = name_tag.decode_contents()

        w = Am.DictWord(
            dictionary_id = dppn_dict.id,
            uid = word_uid(name, dppn_dict.label.lower()),
            word = name,
            word_ascii = pali_to_ascii(name),
            language = "en",
            source_uid = dppn_dict.label,
            definition_html = item_html,
            definition_plain = compact_rich_text(item_html),
        )

        words.append(w)

    return words

def parse_name_page(p: Path, dppn_dict: Am.Dictionary) -> List[Am.DictWord]:
    html_text = open(p, 'r', encoding='utf-8').read()

    words: List[Am.DictWord] = []

    soup = BeautifulSoup(html_text, 'html.parser')

    header_items = soup.select('body > ul > li > h2')
    if len(header_items) == 0:
        header_items = soup.select('body > h2')

    if len(header_items) > 0:
        # If the list contains h2, the html page contains long definition blocks.
        words.append(get_long_definition_blocks(soup, dppn_dict))

    else:
        # Otherwise the page is a list of short definitions and links to long definitions.
        words.extend(get_short_definitions(soup, dppn_dict))

    return words


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
        dict_entries = parse_name_page(p, dppn_dict)
        words.extend(dict_entries)

    logger.info(f"Adding {len(words)} words.")

    try:
        for i in words:
            check_uid(appdata_db, i)
            appdata_db.add(i)
            # Necessary to commit each word in order to check for uid
            # constraint.
            appdata_db.commit()
    except Exception as e:
        print(f"Error: {e}")
        logger.error(e)

def main():
    # appdata_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db_path = Path("/home/gambhiro/.local/share/simsapa/assets/appdata.sqlite3")

    appdata_db = helpers.get_simsapa_db(appdata_db_path, DbSchemaName.AppData, remove_if_exists = False)

    populate_dppn_from_palikanon_com(appdata_db)

if __name__ == "__main__":
    main()
