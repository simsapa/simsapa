#!/usr/bin/env python3

import os, sys, shutil, json
from pathlib import Path
from typing import Tuple, Set, Dict, List
from datetime import date

import sqlite3
import contextlib

from sqlalchemy.orm.session import Session

from simsapa import logger, DbSchemaName, DictTypeName
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.db_session import get_db_session_with_schema
from simsapa.app.helpers import pali_to_ascii, word_uid

from simsapa.dpd_db.tools.sandhi_contraction import make_sandhi_contraction_dict
from simsapa.dpd_db.tools.sandhi_contraction import SandhiContractions

from simsapa.dpd_db.exporter.helpers import cf_set_gen, make_roots_count_dict

import helpers

def save_dpd_caches(dpd_db_session: Session) -> Tuple[Set[str], Dict[str, int], SandhiContractions]:
    """Save cf_set and sandhi_contractions"""

    dpd_cf_set: Set[str] = set()
    dpd_roots_count_dict: Dict[str, int] = dict()
    dpd_sandhi_contractions: SandhiContractions = dict()

    dpd_db_session.query(Dpd.DbInfo).filter(Dpd.DbInfo.key == "cf_set").delete()
    dpd_db_session.query(Dpd.DbInfo).filter(Dpd.DbInfo.key == "roots_count_dict").delete()
    dpd_db_session.query(Dpd.DbInfo).filter(Dpd.DbInfo.key == "sandhi_contractions").delete()
    dpd_db_session.commit()

    # === cf_set ===

    dpd_cf_set = cf_set_gen(dpd_db_session)
    dpd_db_session.add(Dpd.DbInfo(key="cf_set", value=json.dumps(list(dpd_cf_set))))
    dpd_db_session.commit()

    # === roots_count_dict ===

    dpd_roots_count_dict = make_roots_count_dict(dpd_db_session)
    dpd_db_session.add(Dpd.DbInfo(key="roots_count_dict", value=json.dumps(dpd_roots_count_dict)))
    dpd_db_session.commit()

    # === sandhi_contractions ===

    pali_words = dpd_db_session.query(Dpd.PaliWord).all()

    dpd_sandhi_contractions = make_sandhi_contraction_dict(pali_words)

    data = dict()
    for k, v in dpd_sandhi_contractions.items():
        data[k] = {
            'contractions': list(v['contractions']),
            'ids': v['ids'],
        }

    dpd_db_session.add(Dpd.DbInfo(key="sandhi_contractions",
                                  value=json.dumps(data)))
    dpd_db_session.commit()

    return (dpd_cf_set, dpd_roots_count_dict, dpd_sandhi_contractions)

def dpd_deconstructor_html_to_list(html: str) -> List[List[str]]:
    """
    Convert:
    kamma + pattā<br>kamma + apattā<br>kammi + apattā
    ->
    [["kamma", "pattā"], ["kamma", "apattā"], ["kammi", "apattā"]]
    """
    return [[word.strip() for word in line.split("+")] for line in html.split("<br>")]

def replace_all_niggahitas(db_conn: sqlite3.Connection):
    cursor = db_conn.cursor()

    # Get all table names
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table';")
    tables = cursor.fetchall()

    for table in tables:
        table = table[0]

        # Get all column names of the table
        cursor.execute(f"PRAGMA table_info({table});")
        columns = [column[1] for column in cursor.fetchall()]

        # For each column, replace 'ṃ' with 'ṁ'
        for column in columns:
            query = f"""
            UPDATE {table}
            SET `{column}` = REPLACE(`{column}`, 'ṃ', 'ṁ')
            WHERE `{column}` LIKE '%ṃ%';
            """
            cursor.execute(query)

    db_conn.commit()

def migrate_dpd(dpd_bootstrap_current_dir: Path, dpd_db_path: Path, dpd_dictionary_id: int) -> None:
    logger.info("migrate_dpd()")

    # Use an sqlite session and update the db schema to agree with the SQLAlchemy model.
    try:
        with contextlib.closing(sqlite3.connect(dpd_db_path)) as connection:
            with contextlib.closing(connection.cursor()) as cursor:
                logger.info("Add db_info table.")
                query = """
                CREATE TABLE
                    db_info(
                        id integer primary key autoincrement,
                        key VARCHAR UNIQUE NOT NULL,
                        value VARCHAR NOT NULL
                    );
                """

                cursor.execute(query)
                connection.commit()

                # logger.info("Add dpd_release_version.")
                query = """
                INSERT INTO db_info (key, value) VALUES (?, ?);
                """

                # Dpd uses current date as version number. Add the version as
                # today's date. When comparing new DPD releases, a more recent
                # date will mean we can download an update for the user.
                ver = date.today().strftime("%Y-%m-%d")

                cursor.execute(query, ("dpd_release_version", ver))
                connection.commit()

                # PaliWord.dictionary_id:
                # PaliRoot.dictionary_id:
                # should return DPD dict id

                query = """
                ALTER TABLE pali_words ADD COLUMN dictionary_id integer NOT NULL DEFAULT 0;
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN dictionary_id integer NOT NULL DEFAULT 0;
                """
                cursor.execute(query)

                connection.commit()

                query = """
                UPDATE pali_words SET dictionary_id = %s;
                """ % (dpd_dictionary_id)
                cursor.execute(query)

                query = """
                UPDATE pali_roots SET dictionary_id = %s;
                """ % (dpd_dictionary_id)
                cursor.execute(query)

                connection.commit()

                # PaliWord: uid, word_ascii, pali_clean

                query = """
                ALTER TABLE pali_words ADD COLUMN uid VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_words ADD COLUMN word_ascii VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_words ADD COLUMN pali_clean VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                # PaliRoot: uid, word_ascii, root_clean, root_no_sign

                query = """
                ALTER TABLE pali_roots ADD COLUMN uid VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN word_ascii VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN root_clean VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                query = """
                ALTER TABLE pali_roots ADD COLUMN root_no_sign VARCHAR NOT NULL DEFAULT '';
                """
                cursor.execute(query)

                connection.commit()

                # Create DPD tables for word lookup data

                query = """
                CREATE TABLE
                    dpd_deconstructor(
                        id integer primary key autoincrement,
                        word VARCHAR UNIQUE NOT NULL,
                        headwords_json VARCHAR NOT NULL DEFAULT ''
                    );
                """
                cursor.execute(query)

                query = """
                CREATE TABLE
                    dpd_ebts(
                        id integer primary key autoincrement,
                        word VARCHAR UNIQUE NOT NULL,
                        definition VARCHAR NOT NULL DEFAULT ''
                    );
                """
                cursor.execute(query)

                query = """
                CREATE TABLE
                    dpd_i2h(
                        id integer primary key autoincrement,
                        word VARCHAR UNIQUE NOT NULL,
                        headwords_tsv VARCHAR NOT NULL DEFAULT ''
                    );
                """
                cursor.execute(query)

                connection.commit()

                replace_all_niggahitas(connection)

    except Exception as e:
        print(str(e))
        sys.exit(2)

    # Now the DPD schema is up to date with the SQLAlchemy definition.

    _, _, dpd_db_session = get_db_session_with_schema(dpd_db_path, DbSchemaName.Dpd)

    for i in dpd_db_session.query(Dpd.PaliWord).all():
        i.uid = word_uid(str(i.id), 'dpd')
        i.pali_clean = i.calc_pali_clean
        # Use pali_clean for word_ascii to remove trailing numbers.
        i.word_ascii = pali_to_ascii(i.calc_pali_clean)

    dpd_db_session.commit()

    for i in dpd_db_session.query(Dpd.PaliRoot).all():
        i.uid = word_uid(i.root, 'dpd')
        i.root_clean = i.calc_root_clean
        i.root_no_sign = i.calc_root_no_sign
        # Use root_clean for word_ascii to remove trailing numbers.
        i.word_ascii = pali_to_ascii(i.calc_root_clean)

    dpd_db_session.commit()

    save_dpd_caches(dpd_db_session)

    # Save DPD lookup data

    with open(dpd_bootstrap_current_dir.joinpath("dpd_deconstructor.json"), 'r', encoding='utf-8') as f:
        dpd_deconstructor: Dict[str, str] = json.loads(f.read())

    for k, v in dpd_deconstructor.items():
        data = json.dumps(dpd_deconstructor_html_to_list(v))
        dpd_db_session.add(Dpd.DpdDeconstructor(word=k, headwords_json=data))

    with open(dpd_bootstrap_current_dir.joinpath("dpd_ebts.json"), 'r', encoding='utf-8') as f:
        dpd_ebts: Dict[str, str] = json.loads(f.read())

    for k, v in dpd_ebts.items():
        dpd_db_session.add(Dpd.DpdEbts(word=k, definition=v))

    with open(dpd_bootstrap_current_dir.joinpath("dpd_i2h.json"), 'r', encoding='utf-8') as f:
        dpd_i2h: Dict[str, List[str]] = json.loads(f.read())

    for k, v in dpd_i2h.items():
        dpd_db_session.add(Dpd.DpdI2h(word=k, headwords_tsv="\t".join(v)))

    dpd_db_session.commit()
    dpd_db_session.close()

def prepare_dpd_for_dist(appdata_db_session: Session, bootstrap_dir: Path):
    logger.info("prepare_dpd_for_dist()")

    # Find or create DPD Dictionary record in appdata
    dpd_dict = appdata_db_session \
        .query(Am.Dictionary) \
        .filter(Am.Dictionary.label == "DPD") \
        .first()

    if dpd_dict is None:
        dpd_dict = Am.Dictionary(
            label = "DPD",
            title = "Digital Pāḷi Dictionary",
            dict_type = DictTypeName.Sql.value,
        )
        appdata_db_session.add(dpd_dict)
        appdata_db_session.commit()

    # Copy DPD to dist before modifying (migating)
    src = bootstrap_dir \
        .joinpath("dpd-db-for-bootstrap") \
        .joinpath("current") \
        .joinpath("dpd.db")

    dest = bootstrap_dir \
        .joinpath("dist") \
        .joinpath("dpd.sqlite3")

    shutil.copy(src, dest)

    migrate_dpd(src.parent, dest, dpd_dict.id)

def main():
    from dotenv import load_dotenv
    load_dotenv()

    s = os.getenv('BOOTSTRAP_ASSETS_DIR')
    if s is None or s == "":
        logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
        sys.exit(1)

    BOOTSTRAP_ASSETS_DIR = Path(s)

    # appdata_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db_path = Path("/home/gambhiro/.local/share/simsapa/assets/appdata.sqlite3")

    appdata_db = helpers.get_simsapa_db(appdata_db_path, DbSchemaName.AppData, remove_if_exists = False)

    prepare_dpd_for_dist(appdata_db, BOOTSTRAP_ASSETS_DIR)

    # Copy to local folder for testing with the app.

    src = BOOTSTRAP_ASSETS_DIR \
        .joinpath("dist") \
        .joinpath("dpd.sqlite3")

    dest = Path("/home/gambhiro/.local/share/simsapa/assets/dpd.sqlite3")

    shutil.copy(src, dest)

if __name__ == "__main__":
    main()
