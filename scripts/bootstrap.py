#!/usr/bin/env python3

import os, shutil, glob
from datetime import datetime
from pathlib import Path
from typing import Optional
from dotenv import load_dotenv
import subprocess

from scripts import suttacentral
from scripts import helpers
from scripts.helpers import NoSuttasException

from simsapa import SIMSAPA_APP_VERSION, DbSchemaName, logger
from simsapa.app.app_data import AppData
from simsapa.app.dir_helpers import create_app_dirs
from simsapa.app.search.tantivy_index import TantivySearchIndexes
from simsapa.app.db_session import get_db_engine_connection_session

START_TIME = datetime.now()

ISO_DATE = START_TIME.strftime("%Y-%m-%d")

s = os.getenv('BOOTSTRAP_LIMIT')
if s is None or s == "":
    BOOTSTRAP_LIMIT = None
else:
    BOOTSTRAP_LIMIT = int(s)

SIMSAPA_DIR = Path.home().joinpath(".local/share/simsapa")
ASSETS_DIR = SIMSAPA_DIR.joinpath("assets")
BOOTSTRAP_ASSETS_DIR = Path("../bootstrap-assets-resources")
DIST_DIR = BOOTSTRAP_ASSETS_DIR.joinpath("dist")
SC_DATA_DIR = BOOTSTRAP_ASSETS_DIR.joinpath("sc-data")
RELEASE_DIR = Path(f"../releases/{ISO_DATE}-dev")

DOT_ENV = f"""
BOOTSTRAP_LIMIT={BOOTSTRAP_LIMIT if BOOTSTRAP_LIMIT is not None else ""}
SIMSAPA_DIR={SIMSAPA_DIR}
BOOTSTRAP_ASSETS_DIR={BOOTSTRAP_ASSETS_DIR}
USE_TEST_DATA=false
DISABLE_LOG=false
ENABLE_PRINT_LOG=true
START_NEW_LOG=false
ENABLE_WIP_FEATURES=false
SAVE_STATS=false
RELEASE_CHANNEL=development
"""

with open(".env", "w", encoding="utf-8") as f:
    f.write(DOT_ENV)

load_dotenv()

def clean_and_create_folders():
    logger.info("=== clean_and_create_folders() ===")

    for p in [ASSETS_DIR, RELEASE_DIR, DIST_DIR]:
        if p.exists():
            shutil.rmtree(p)
        p.mkdir()

    create_app_dirs()

    p = SIMSAPA_DIR.joinpath("unzipped_stardict")
    if p.exists():
        shutil.rmtree(p)

    for i in glob.glob(os.path.join(SIMSAPA_DIR, '*.tar.bz2')):
        Path(i).unlink()

def import_user_data(app_data: AppData):
    logger.info("=== import_user_data() ===")

    # FIXME import_pali_course()
    # p = BOOTSTRAP_ASSETS_DIR.joinpath("courses/dhammapada-word-by-word/dhammapada-word-by-word.toml")
    # name = app_data.import_pali_course(str(p))
    # logger.info(f"Imported Pali Course: {name}")

    p = BOOTSTRAP_ASSETS_DIR.joinpath("bookmarks/bookmarks.csv")
    n_bookmarks = app_data.import_bookmarks(str(p))
    logger.info(f"Imported {n_bookmarks} bookmarks.")

    p = BOOTSTRAP_ASSETS_DIR.joinpath("prompts/prompts.csv")
    n_prompts = app_data.import_prompts(str(p))
    logger.info(f"Imported {n_prompts} prompts.")

def reindex():
    logger.info("=== reindex() ===")

    # NOTE: Have to index in a shell to make sure tantivy temp files are
    # cleaned up before tar starts compressing.
    #
    # This avoids errors such as:
    #
    # tar: index/dict_words/pli/.tmp9BNL9g: File removed before we read it
    # tar: index/dict_words/pli: file changed as we read it

    cmd = """ ./run.py index reindex """
    subprocess.run(cmd,
                   shell=True,
                   capture_output=True,
                   cwd=Path(".")) \
              .check_returncode()

def index_suttas_lang(lang: str):
    logger.info(f"=== index_suttas_lang() {lang} ===")

    db_eng, db_conn, db_session = get_db_engine_connection_session()

    search_indexes = TantivySearchIndexes(db_session)
    search_indexes.index_all_suttas_lang(lang)

    db_conn.close()
    db_session.close()
    db_eng.dispose()

def bootstrap_suttas_lang(lang: str) -> Optional[Path]:
    logger.info(f"bootstrap_suttas_lang() {lang}")
    db_name = f"suttas_lang_{lang}.sqlite3"

    db_path = DIST_DIR.joinpath(db_name)
    db_session = helpers.get_simsapa_db(db_path, DbSchemaName.UserData, remove_if_exists = True)

    limit = BOOTSTRAP_LIMIT
    sc_db = suttacentral.get_suttacentral_db()

    try:
        suttacentral.populate_suttas_from_suttacentral(db_session, DbSchemaName.UserData, sc_db, SC_DATA_DIR, lang, limit)

    except NoSuttasException as e:
        logger.error(e)

        db_session.close()

        if db_path.exists():
            db_path.unlink()

        return None

    db_session.close()

    return db_path

def import_index_move_lang(app_data: AppData, lang: str, db_path: Path):
    n_suttas = app_data.import_suttas_to_userdata(str(db_path))
    logger.info(f"Imported {n_suttas} suttas.")

    # NOTE: Have to index in a shell to make sure tantivy temp files are
    # cleaned up before tar starts compressing.

    # When creating the index in this Python process, tar errors out:
    # index_suttas_lang(lang)

    # tar: index/suttas/af/.tmpo29Qza: File removed before we read it
    # tar: index/suttas/af: file changed as we read it
    # subprocess.CalledProcessError: Command ' tar cjf "suttas_lang_af.tar.bz2" "suttas_lang_af.sqlite3" index/suttas/af/ ' returned non-zero exit status 1.

    # This will continue to write to log.txt. Capture the output to suppress
    # printing, but no need to add it to log.txt
    cmd = f""" ./run.py index suttas-lang {lang} """
    subprocess.run(cmd,
                   shell=True,
                   capture_output=True,
                   cwd=Path(".")) \
              .check_returncode()

    shutil.copy(db_path, ASSETS_DIR)

    cmd = f""" tar cjf "{db_path.stem}.tar.bz2" "{db_path.name}" index/suttas/{lang}/ """
    subprocess.run(cmd, shell=True, cwd=ASSETS_DIR).check_returncode()
    shutil.move(ASSETS_DIR.joinpath(f"{db_path.stem}.tar.bz2"),
                RELEASE_DIR)

def main():
    with open(SIMSAPA_DIR.joinpath("log.txt"), "w", encoding="utf-8") as f:
        f.write("")

    clean_and_create_folders()

    from scripts import bootstrap_appdata
    bootstrap_appdata.main()

    logger.info("=== Create appdata.tar.bz2 ===")

    cmd = """ tar cjf appdata.tar.bz2 dpd.sqlite3 appdata.sqlite3 """
    subprocess.run(cmd, shell=True, cwd=DIST_DIR).check_returncode()

    shutil.move(DIST_DIR.joinpath("appdata.tar.bz2"), RELEASE_DIR)

    logger.info("=== Copy Appdata DB to user folder ===")

    shutil.copy(DIST_DIR.joinpath("appdata.sqlite3"), ASSETS_DIR)
    shutil.copy(DIST_DIR.joinpath("dpd.sqlite3"), ASSETS_DIR)

    app_data = AppData()

    import_user_data(app_data)

    logger.info("=== Create userdata.tar.bz2 ===")

    shutil.copy(ASSETS_DIR.joinpath("userdata.sqlite3"),
                DIST_DIR)

    shutil.copytree(ASSETS_DIR.joinpath("courses"),
                    DIST_DIR.joinpath("courses"),
                    dirs_exist_ok=True)

    shutil.copytree(BOOTSTRAP_ASSETS_DIR.joinpath("html_resources"),
                    DIST_DIR.joinpath("html_resources"),
                    dirs_exist_ok=True)

    # Also copy to local assets dir for testing.
    shutil.copytree(BOOTSTRAP_ASSETS_DIR.joinpath("html_resources"),
                    ASSETS_DIR.joinpath("html_resources"),
                    dirs_exist_ok=True)

    cmd = """ tar cjf userdata.tar.bz2 userdata.sqlite3 courses/ html_resources/ """
    subprocess.run(cmd, shell=True, cwd=DIST_DIR).check_returncode()
    shutil.move(DIST_DIR.joinpath("userdata.tar.bz2"),
                RELEASE_DIR)

    reindex()

    logger.info("=== Create index.tar.bz2 ===")

    cmd = """ tar cjf index.tar.bz2 index/ """
    subprocess.run(cmd, shell=True, cwd=ASSETS_DIR).check_returncode()
    shutil.move(ASSETS_DIR.joinpath("index.tar.bz2"),
                RELEASE_DIR)

    from scripts import sanskrit_texts
    sanskrit_texts.main()

    logger.info("=== Create sanskrit-texts.tar.bz2 ===")

    cmd = """ tar cjf sanskrit-texts.tar.bz2 sanskrit-texts.sqlite3 """
    subprocess.run(cmd, shell=True, cwd=DIST_DIR).check_returncode()
    shutil.move(DIST_DIR.joinpath("sanskrit-texts.tar.bz2"),
                RELEASE_DIR)

    logger.info("=== Copy Appdata DB to user folder ===")

    shutil.copy(DIST_DIR.joinpath("appdata.sqlite3"), ASSETS_DIR)

    reindex()

    logger.info("=== Create sanskrit-appdata.tar.bz2 ===")

    cmd = """ tar cjf sanskrit-appdata.tar.bz2 dpd.sqlite3 appdata.sqlite3 """
    subprocess.run(cmd, shell=True, cwd=ASSETS_DIR).check_returncode()
    shutil.move(ASSETS_DIR.joinpath("sanskrit-appdata.tar.bz2"),
                RELEASE_DIR)

    logger.info("=== Create sanskrit-index.tar.bz2 ===")

    cmd = """ tar cjf sanskrit-index.tar.bz2 index/ """
    subprocess.run(cmd, shell=True, cwd=ASSETS_DIR).check_returncode()
    shutil.move(ASSETS_DIR.joinpath("sanskrit-index.tar.bz2"),
                RELEASE_DIR)

    logger.info("=== Bootstrap Languages from SuttaCentral ===")

    """
AQL to produce the list of languages in SuttaCentral arango-db:

LET docs = (FOR x IN language
FILTER x._key != 'en'
&& x._key != 'pli'
&& x._key != 'san'
&& x._key != 'hu'
RETURN x._key)
RETURN docs
    """

    languages = ["af", "ar", "au", "bn", "ca", "cs", "de", "es", "ev", "fa",
                 "fi", "fr", "gu", "haw", "he", "hi", "hr", "id", "it", "jpn", "kan", "kho",
                 "kln", "ko", "la", "lt", "lzh", "mr", "my", "nl", "no", "pgd", "pl", "pra",
                 "pt", "ro", "ru", "si", "sk", "sl", "sld", "sr", "sv", "ta", "th", "uig",
                 "vi", "vu", "xct", "xto", "zh"]

    for lang in languages:
        logger.info(f"=== {lang} ===")

        # Returns None if there are 0 suttas for that language.
        db_path = bootstrap_suttas_lang(lang)
        if db_path is None:
            continue

        import_index_move_lang(app_data, lang, db_path)

    logger.info("=== Bootstrap Hungarian from Buddha Ujja ===")

    from scripts import buddha_ujja
    buddha_ujja.main()

    import_index_move_lang(app_data, "hu", DIST_DIR.joinpath("suttas_lang_hu.sqlite3"))

    logger.info("=== Copy log.txt ===")

    shutil.copy(SIMSAPA_DIR.joinpath("log.txt"), RELEASE_DIR)

    logger.info("=== Release Info ===")

    def _path_to_lang_quoted_str(p: Path) -> str:
        # '../bootstrap-assets-resources/dist/suttas_lang_hu.sqlite3' -> '"hu"'
        return '"' + p.stem.replace("suttas_lang_", "") + '"'

    paths = glob.glob(os.path.join(DIST_DIR, 'suttas_lang_*.sqlite3'))
    suttas_lang_list = [_path_to_lang_quoted_str(Path(i)) for i in paths]

    suttas_lang = ', '.join(suttas_lang_list)

    release_info = f"""
[[assets.releases]]
date = "{datetime.now().strftime('%FT%T')}"
version_tag = "v{SIMSAPA_APP_VERSION}"
github_repo = "simsapa/simsapa-assets"
suttas_lang = [{suttas_lang}]
title = "Updates"
description = ""
"""

    logger.info(release_info)

    with open(RELEASE_DIR.joinpath("release_info.toml"), "w", encoding="utf-8") as f:
        f.write(release_info)

    logger.info("=== Clean up ===")

    p = SIMSAPA_DIR.joinpath("unzipped_stardict")
    if p.exists():
        shutil.rmtree(p)

    logger.info("=== Bootstrap DB finished ===")

    end_time = datetime.now()

    msg = f"""
======
Bootstrap started: {START_TIME}
Bootstrap ended:   {end_time}
Duration:          {end_time - START_TIME}
"""

    logger.info(msg)

if __name__ == "__main__":
    main()
