#!/usr/bin/env python3

import os
import sys
import logging as _logging
from pathlib import Path
from typing import List
from dotenv import load_dotenv
from collections import namedtuple

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.sql import func
# from sqlalchemy.dialects.sqlite import insert

from simsapa.app.db import appdata_models as Am

from simsapa.app.helpers import find_or_create_db
from simsapa.app.stardict import import_stardict_into_db_as_new, parse_ifo, parse_stardict_zip

logger = _logging.getLogger(__name__)

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    print("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

if not bootstrap_assets_dir.exists():
    print(f"Does not exists: {bootstrap_assets_dir}")
    sys.exit(1)

def get_new_db_conn(db_path: Path):
    # remove previously generated db
    if db_path.exists():
        db_path.unlink()

    find_or_create_db(db_path, 'appdata')

    try:
        # Create an in-memory database
        engine = create_engine("sqlite+pysqlite://", echo=False)

        db_conn = engine.connect()

        # Attach appdata and userdata
        db_conn.execute(f"ATTACH DATABASE '{db_path}' AS appdata;")

        Session = sessionmaker(engine)
        Session.configure(bind=engine)
        db_session = Session()
    except Exception as e:
        logger.error("Can't connect to database.")
        print(e)
        exit(1)

    return (db_conn, db_session)

def get_legacy_db_conn(db_path: Path):
    try:
        # Create an in-memory database
        engine = create_engine(f"sqlite+pysqlite:///{db_path}", echo=False)

        db_conn = engine.connect()

        Session = sessionmaker(engine)
        Session.configure(bind=engine)
        db_session = Session()
    except Exception as e:
        logger.error("Can't connect to database.")
        print(e)
        exit(1)

    return (db_conn, db_session)

def populate_suttas_from_legacy(new_db_session, legacy_db_session):
    a = new_db_session.query(Am.Sutta).all()

    print("Adding Suttas from root_texts")

    a = legacy_db_session.execute("SELECT * from root_texts;")

    # Convert the results into namedtuple
    # https://stackoverflow.com/a/22084672/195141

    RootText = namedtuple('RootText', a.keys())
    records = [RootText(*r) for r in a.fetchall()]

    def root_text_to_sutta(x: RootText) -> Am.Sutta:
        return Am.Sutta(
            title = x.title,
            uid = x.uid,
            sutta_ref = x.acronym,
            language = x.content_language,
            content_plain = x.content_plain,
            content_html = x.content_html,
            created_at = func.now(),
        )

    suttas: List[Am.Sutta] = list(map(root_text_to_sutta, records))

    try:
        # TODO: bulk insert errors out
        """
        (builtins.TypeError) SQLite DateTime type only accepts Python datetime and date objects as input.
        [SQL: INSERT INTO appdata.suttas (id, uid, group_path, group_index, sutta_ref, sutta_ref_pts, language, order_index, title, title_pali, title_trans, description, content_plain, content_html, source_info, source_language, message, copyright, license, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)]
        [parameters: [{}]]
        """

        # stmt = insert(Am.Sutta).values(suttas)
        # new_db_session.execute(stmt)

        # NOTE: this is slow but works
        for i in suttas:
            new_db_session.add(i)
            new_db_session.commit()
    except Exception as e:
        print(e)
        logger.error(e)
        exit(1)

    print("Adding Suttas from traslated_texts")

    a = legacy_db_session.execute("SELECT * from translated_texts;")

    TranslatedText = namedtuple('TranslatedText', a.keys())
    records = [TranslatedText(*r) for r in a.fetchall()]

    def translated_text_to_sutta(x: TranslatedText) -> Am.Sutta:
        return Am.Sutta(
            title = x.title,
            title_pali = x.root_title,
            uid = x.uid,
            sutta_ref = x.acronym,
            language = x.content_language,
            content_plain = x.content_plain,
            content_html = x.content_html,
            created_at = func.now(),
        )

    suttas: List[Am.Sutta] = list(map(translated_text_to_sutta, records))

    try:
        for i in suttas:
            new_db_session.add(i)
            new_db_session.commit()
    except Exception as e:
        print(e)
        logger.error(e)
        exit(1)

def populate_dict_words_from_stardict(new_db_session, stardict_base_path: Path):
    for d in stardict_base_path.glob("*.zip"):
        print(d)
        paths = parse_stardict_zip(Path(d))
        ifo = parse_ifo(paths)
        print(f"Importing {ifo['bookname']} ...")
        import_stardict_into_db_as_new(new_db_session, 'appdata', paths, 10000)

def main():
    legacy_db_path = bootstrap_assets_dir.joinpath("db").joinpath("appdata-legacy.sqlite3")
    new_db_path = bootstrap_assets_dir.joinpath("dist").joinpath("appdata.sqlite3")
    stardict_base_path = bootstrap_assets_dir.joinpath("dict")

    _, new_db_session = get_new_db_conn(new_db_path)

    _, legacy_db_session = get_legacy_db_conn(legacy_db_path)

    populate_suttas_from_legacy(new_db_session, legacy_db_session)

    populate_dict_words_from_stardict(new_db_session, stardict_base_path)

if __name__ == "__main__":
    main()
