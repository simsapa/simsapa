#!/usr/bin/env python3

import os
import sys
import logging as _logging
from pathlib import Path
import re
import json
from typing import List
from dotenv import load_dotenv
from collections import namedtuple

from sqlalchemy import create_engine, null
from sqlalchemy.orm import sessionmaker
from sqlalchemy.orm.session import Session
from sqlalchemy.sql import func
# from sqlalchemy.dialects.sqlite import insert

from pyArango.connection import Connection
from pyArango.database import DBHandle

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
sc_data_dir = bootstrap_assets_dir.joinpath("sc-data")

for p in [bootstrap_assets_dir, sc_data_dir]:
    if not p.exists():
        print(f"Missing folder: {p}")
        sys.exit(1)

def get_appdata_db(db_path: Path) -> Session:
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
        sys.exit(1)

    return db_session

def get_suttacentral_db() -> DBHandle:
    conn = Connection(
        arangoURL="http://localhost:8529",
        username="root",
        password="test",
    )

    # db = conn.createDatabase(name="suttacentral")
    db: DBHandle = conn["suttacentral"]

    return db

def html_text_uid(x) -> str:
    '''dn1/en/bodhi'''
    return f"{x['uid']}/{x['lang']}/{x['author_uid']}"

def bilara_text_uid(x) -> str:
    '''dn1/pli/ms'''

    a = x['muids'].copy()
    for k in ['translation', 'root', 'reference', 'variant', 'comment', x['lang']]:
        if k in a:
            a.pop(a.index(k))

    if len(a) == 1:
        author = a[0]
    elif len(a) == 0 and '/pli/ms/' in x['file_path']:
        author = 'ms'
    elif len(a) == 0 and '/pli/vri/' in x['file_path']:
        author = 'vri'
    else:
        print(f"Can't find author for bilara text, _id: {x['_id']}, muids: {x['muids']}, {x['file_path']}")
        author = 'unknown'

    return f"{x['uid']}/{x['lang']}/{author}"

def uid_to_ref(uid: str) -> str:
    '''sn12.23 to SN 12.23'''

    # Add a space after the letters, i.e. the collection abbrev
    uid = re.sub(r'^([a-z]+)([0-9])', r'\1 \2', uid)

    # handle all-upcase collections
    subs = [('dn ', 'DN '),
            ('mn ', 'DN '),
            ('sn ', 'SN '),
            ('an ', 'AN ')]
    for sub_from, sub_to in subs:
        uid = uid.replace(sub_from, sub_to)

    # titlecase the rest, upcase the first letter
    uid = uid[0].upper() + uid[1:]

    return uid

def html_text_to_sutta(x, title: str) -> Am.Sutta:
    return Am.Sutta(
        source_info = x['_id'],
        # The All-embracing Net of Views
        title = title,
        # dn1/en/bodhi
        uid = html_text_uid(x),
        # SN 12.23
        sutta_ref = uid_to_ref(x['uid']),
        # en
        language = x['lang'],
        # <!DOCTYPE html>\n<html>\n<head>...
        content_html = x['text'],
        created_at = func.now(),
    )

def bilara_text_to_sutta(x, title: str) -> Am.Sutta:
    a = json.loads(x['text'])
    text = "\n\n".join(a.values())

    # test if text contains html tags
    m = re.match(r'<\w+', text)
    if m is not None:
        content_html = text
        content_plain = null()
    else:
        content_html = null()
        content_plain = text

    return Am.Sutta(
        source_info = x['_id'],
        title = title,
        # mn123/pli/ms
        uid = bilara_text_uid(x),
        # SN 12.23
        sutta_ref = uid_to_ref(x['uid']),
        # pli
        language = x['lang'],
        content_plain = content_plain,
        content_html = content_html,
        created_at = func.now(),
    )

def get_titles(db: DBHandle, language = 'en') -> dict[str, str]:
    if language == 'pli':
        get_names_aql = "LET docs = (FOR x IN names FILTER x.is_root == true RETURN x) RETURN docs"
        q = db.AQLQuery(get_names_aql)

    else:
        get_names_aql = "LET docs = (FOR x IN names FILTER x.lang == @language RETURN x) RETURN docs"
        q = db.AQLQuery(
            get_names_aql,
            bindVars={'language': language}
        )

    titles: dict[str, str] = {}
    for x in q.result[0]:
        uid = x['uid']
        if uid in titles.keys():
            print(f"WARN: title for {uid} exists")
        else:
            titles[uid] = x['name']

    return titles

def get_suttas(db: DBHandle, language = 'en') -> dict[str, Am.Sutta]:

    # NOTE: In suttacentral records an uid is not a unique record, it is the
    # sutta reference (dn12, an4.10). Some texts exist in two formats, both in
    # html_text and sc_bilara_texts.
    #
    # A record is unique to uid + language + author + format.
    #
    # Bilara format is newer and edited, we'll prefer that source to html_text.

    suttas: dict[str, Am.Sutta] = {}

    get_html_text_aql = '''
    LET docs = (
        FOR x IN html_text
            FILTER x.lang == @language
            RETURN x
    )
    RETURN docs
    '''

    get_bilara_text_aql = '''
    LET docs = (
        FOR x IN sc_bilara_texts
            FILTER x.lang == @language
            RETURN x
    )
    RETURN docs
    '''

    titles = get_titles(db, language)

    def _html_title(x, titles):
        return x['name']

    def _bilara_title(x, titles):
        uid = x['uid']
        if uid in titles:
            return titles[uid]
        else:
            return ''

    queries = (
        (get_html_text_aql, html_text_uid, html_text_to_sutta, _html_title),
        (get_bilara_text_aql, bilara_text_uid, bilara_text_to_sutta, _bilara_title),
    )

    total_results = 0
    ignored = 0
    known_dup = 0
    unknown_dup = 0

    for query, f_uid, f_to_sutta, f_title in queries:
        q = db.AQLQuery(
            query,
            bindVars={'language': language}
        )

        total_results += len(q.result[0])

        for r in q.result[0]:
            if 'muids' in r.keys() and 'comment' in r['muids']:
                # NOTE: ignoring comments
                ignored += 1
                continue

            convert_paths_to_content(r)
            uid = f_uid(r)
            title = f_title(r, titles)

            if uid not in suttas.keys():
                suttas[uid] = f_to_sutta(r, title)

            elif 'muids' in r.keys() and ('reference' in r['muids'] or 'variant' in r['muids']):
                # keeping only the 'root' version
                known_dup += 1
                continue

            elif 'muids' in r.keys() and 'root' in r['muids']:
                # keeping only the 'root' version
                known_dup += 1
                suttas[uid] = f_to_sutta(r, title)

            elif r['_id'].startswith('sc_bilara_texts/') and suttas[uid].source_info.startswith('html_text/'):
                # keeping the Bilara version
                known_dup += 1
                suttas[uid] = f_to_sutta(r, title)

            else:
                unknown_dup += 1
                print(f"WARN: Unknown duplicate uid: {uid}")
                print(r['_id'])
                print(r['muids'])
                print(suttas[uid].source_info)

    n = total_results - ignored - known_dup - unknown_dup
    if len(suttas) != n:
        print(f"WARN: Count is not adding up: {len(suttas)} != {n}, {total_results} - {ignored} - {known_dup} - {unknown_dup}.")

    # clear source_info, where we temp stored x['_id']
    for k, v in suttas.items():
        v.source_info = null() # type: ignore
        suttas[k] = v

    return suttas

def convert_paths_to_content(doc):
    conversions = (
        ('file_path', 'text', lambda f: f.read()),
        ('markup_path', 'markup', lambda f: f.read()),
        ('strings_path', 'strings', json.load),
    )

    for from_prop, to_prop, load_func in conversions:
        if (to_prop not in doc) and (from_prop in doc):
            file_path = doc[from_prop]
            if file_path is None:
                doc[to_prop] = None
            else:
                file_path = file_path.replace('/opt/sc/sc-flask/sc-data', f"{sc_data_dir}")
                p = Path(file_path)

                if not p.exists():
                    print(f"ERROR: File not found: {p}")
                    doc[to_prop] = None
                else:
                    with open(p) as f:
                        doc[to_prop] = load_func(f)

def get_legacy_db(db_path: Path) -> Session:
    try:
        # Create an in-memory database
        engine = create_engine(f"sqlite+pysqlite:///{db_path}", echo=False)

        # db_conn = engine.connect()

        Session = sessionmaker(engine)
        Session.configure(bind=engine)
        db_session = Session()
    except Exception as e:
        logger.error("Can't connect to database.")
        print(e)
        exit(1)

    return db_session

def populate_suttas_from_suttacentral(appdata_db: Session, sc_db: DBHandle):
    for lang in ['en', 'pli']:
        suttas = get_suttas(sc_db, lang)

        print(f"Adding {lang}, count {len(suttas)} ...")

        try:
            # TODO: bulk insert errors out
            # NOTE: this is slow but works
            for i in suttas.values():
                appdata_db.add(i)
                appdata_db.commit()
        except Exception as e:
            logger.error(e)
            print(e)
            exit(1)

def populate_nyanatiloka_dict_words_from_legacy(appdata_db: Session, legacy_db: Session):
    print("Adding Nyanatiloka DictWords from legacy dict_words")

    # create the dictionary
    dictionary = Am.Dictionary(
        label = 'NYANAT',
        title = "Nyanatiloka's Buddhist Dictionary",
        created_at = func.now(),
    )

    try:
        appdata_db.add(dictionary)
        appdata_db.commit()
    except Exception as e:
        logger.error(e)
        print(e)
        exit(1)

    # get words and commit to appdata db

    a = legacy_db.execute("SELECT * from dict_words WHERE entry_source = 'nyanat';") # type: ignore

    LegacyDictWord = namedtuple('LegacyDictWord', a.keys())
    records = [LegacyDictWord(*r) for r in a.fetchall()]

    def _legacy_to_dict_word(x: LegacyDictWord) -> Am.DictWord:
        return Am.DictWord(
            dictionary_id = dictionary.id,
            word = x.word,
            url_id = f"{x.word}-nyanat",
            definition_plain = x.definition_plain,
            definition_html = x.definition_html,
            summary = x.summary,
            created_at = func.now(),
        )

    dict_words: List[Am.DictWord] = list(map(_legacy_to_dict_word, records))

    try:
        for i in dict_words:
            appdata_db.add(i)
            appdata_db.commit()
    except Exception as e:
        logger.error(e)
        print(e)
        exit(1)

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
        logger.error(e)
        print(e)
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
        logger.error(e)
        print(e)
        exit(1)

def populate_dict_words_from_stardict(appdata_db: Session, stardict_base_path: Path):
    for d in stardict_base_path.glob("*.zip"):
        print(d)
        paths = parse_stardict_zip(Path(d))
        ifo = parse_ifo(paths)
        print(f"Importing {ifo['bookname']} ...")
        import_stardict_into_db_as_new(appdata_db, 'appdata', paths, 10000)

def main():
    appdata_db_path = bootstrap_assets_dir.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db = get_appdata_db(appdata_db_path)

    legacy_db_path = bootstrap_assets_dir.joinpath("db").joinpath("appdata-legacy.sqlite3")
    legacy_db = get_legacy_db(legacy_db_path)

    sc_db = get_suttacentral_db()

    stardict_base_path = bootstrap_assets_dir.joinpath("dict")

    # NOTE: Deprecated. Use the suttacentral db.
    # populate_suttas_from_legacy(appdata_db, legacy_db)

    populate_nyanatiloka_dict_words_from_legacy(appdata_db, legacy_db)

    populate_suttas_from_suttacentral(appdata_db, sc_db)

    populate_dict_words_from_stardict(appdata_db, stardict_base_path)

if __name__ == "__main__":
    main()
