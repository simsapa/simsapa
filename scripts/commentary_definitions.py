#!/usr/bin/env python3

import os, sys, csv, re
from pathlib import Path
from typing import Dict, List, Optional
from dotenv import load_dotenv

from sqlalchemy.orm.session import Session, make_transient

from simsapa import logger, DbSchemaName, DictTypeName
from simsapa.app.db import appdata_models as Am

from scripts import helpers
from simsapa.app.helpers import compact_rich_text, pali_to_ascii, word_uid

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

BOOTSTRAP_ASSETS_DIR = Path(s)

CSV_PATH = BOOTSTRAP_ASSETS_DIR.joinpath('commentary-definitions/definitions.csv')

SQLITE_PATH = BOOTSTRAP_ASSETS_DIR.joinpath('commentary-definitions/definitions.sqlite')

def find_or_create_dict(appdata_db: Session) -> Am.Dictionary:
    comm_dict = appdata_db \
        .query(Am.Dictionary) \
        .filter(Am.Dictionary.label == "COMM") \
        .first()

    if comm_dict is None:
        comm_dict = Am.Dictionary(
            label = "COMM",
            title = "Pāli commentary definitions of terms in Vinaya, late Khuddaka Nikāya, Aṭṭhakathā, Ṭīkā and Aññā.",
            dict_type = DictTypeName.Custom.value,
        )
        appdata_db.add(comm_dict)
        appdata_db.commit()

    return comm_dict

def populate_from_csv_to_appdata(appdata_db: Session, limit: Optional[int] = None):
    logger.info("populate_commentary_definitions()")

    comm_dict = find_or_create_dict(appdata_db)

    appdata_db.query(Am.DictWord).filter(Am.DictWord.dictionary_id == comm_dict.id).delete()
    appdata_db.commit()

    word_occur_count: Dict[str, int] = dict()

    words: List[Am.DictWord] = []

    # CSV fields:
    #
    # file_name
    # ref_code
    # nikaya
    # book
    # title
    # subhead
    # bold
    # bold_end
    # commentary

    def _row_to_word(r: Dict[str, str]) -> Am.DictWord:
        # Separate 'ti from the word, avoid joining it when ' is removed
        w = r['bold'].replace("'ti", " ti")
        # Only remove punctuation from the end, commentary words might include comma ',' or compounds might include ' quote mark.
        word_text = re.sub(r"[\.,;:\(\)'\"]+$", "", w)

        if word_text in word_occur_count.keys():
            word_occur_count[word_text] += 1
        else:
            word_occur_count[word_text] = 1

        word_uniq = word_text + " " + str(word_occur_count[word_text])

        word_source = " ".join([r['nikaya'], r['book'], r['title'], r['subhead']])

        # 'commentary' includes the word text already.
        definition_html = f"""
        <div>
            <p>({r['ref_code']}) {r['commentary']}</p>
            <p style="font-size: 0.8em; font-style: italic;">{word_source} ({r['file_name']})</p>
        </div>
        """

        word = Am.DictWord(
            dictionary_id = comm_dict.id,
            uid = word_uid(word_uniq, comm_dict.label.lower()),
            word = word_text,
            word_ascii = pali_to_ascii(word_text),
            language = "pli",
            source_uid = comm_dict.label.lower(),
            definition_html = definition_html,
            definition_plain = compact_rich_text(definition_html),
        )

        return word

    with open(CSV_PATH, 'r') as f:
        reader = csv.DictReader(f, delimiter="\t")
        words: List[Am.DictWord] = list(map(_row_to_word, reader))

    if limit:
        words = words[0:limit]

    logger.info(f"Adding {len(words)} words.")

    all_uids = [i._tuple()[0] for i in appdata_db.query(Am.DictWord.uid).all()]

    try:
        total = len(words)
        for idx, i in enumerate(words):
            percent = idx/(total/100)
            logger.info(f"Adding {percent:.2f}% {idx}/{total}: {i.uid}")

            helpers.check_and_fix_dict_word_uid(all_uids, i)
            appdata_db.add(i)

        appdata_db.commit()
    except Exception as e:
        logger.error(e)

def export_from_appdata_to_sqlite(appdata_db: Session):
    comm_db = helpers.get_simsapa_db(SQLITE_PATH, DbSchemaName.AppData, remove_if_exists = True)

    comm_dict = find_or_create_dict(comm_db)

    appdata_words = appdata_db.query(Am.DictWord).filter(Am.DictWord.source_uid == "comm").all()

    try:
        for i in appdata_words:
            appdata_db.expunge(i)
            make_transient(i)
            # Necessary to reset id, otherwise will not get a new id.
            i.id = None # type: ignore
            i.dictionary_id = comm_dict.id

            comm_db.add(i)

        comm_db.commit()
    except Exception as e:
        logger.error(f"Import problem: {e}")
        exit(1)

def populate_from_sqlite_to_appdata(appdata_db: Session, limit: Optional[int] = None):
    comm_db = helpers.get_simsapa_db(SQLITE_PATH, DbSchemaName.AppData, remove_if_exists = False)

    appdata_dict = find_or_create_dict(comm_db)

    appdata_db.query(Am.DictWord).filter(Am.DictWord.dictionary_id == appdata_dict.id).delete()
    appdata_db.commit()

    comm_words = comm_db.query(Am.DictWord).filter(Am.DictWord.source_uid == "comm").all()

    if limit:
        comm_words = comm_words[0:limit]

    try:
        for i in comm_words:
            comm_db.expunge(i)
            make_transient(i)
            # Necessary to reset id, otherwise will not get a new id.
            i.id = None # type: ignore
            i.dictionary_id = appdata_dict.id

            appdata_db.add(i)

        appdata_db.commit()
    except Exception as e:
        logger.error(f"Import problem: {e}")
        exit(1)

def main():
    appdata_db_path = Path("/home/gambhiro/.local/share/simsapa/assets/appdata.sqlite3")

    appdata_db = helpers.get_simsapa_db(appdata_db_path, DbSchemaName.AppData, remove_if_exists = False)

    populate_from_csv_to_appdata(appdata_db)

    export_from_appdata_to_sqlite(appdata_db)

    # populate_from_sqlite_to_appdata(appdata_db)

if __name__ == "__main__":
    main()
