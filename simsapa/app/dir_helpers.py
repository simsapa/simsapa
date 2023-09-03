import shutil
from simsapa import (APP_DB_PATH, ASSETS_DIR, COURSES_DIR, EBOOK_UNZIP_DIR, GRAPHS_DIR,
                     HTML_RESOURCES_APPDATA_DIR, HTML_RESOURCES_USERDATA_DIR, INDEX_DIR,
                     SIMSAPA_DIR, USER_DB_PATH)

def create_app_dirs():
    for d in [SIMSAPA_DIR,
              ASSETS_DIR,
              GRAPHS_DIR,
              COURSES_DIR,
              EBOOK_UNZIP_DIR,
              HTML_RESOURCES_APPDATA_DIR,
              HTML_RESOURCES_USERDATA_DIR]:

        if not d.exists():
            d.mkdir(parents=True, exist_ok=True)

def ensure_empty_graphs_cache():
    if GRAPHS_DIR.exists():
        shutil.rmtree(GRAPHS_DIR)

    GRAPHS_DIR.mkdir(parents=True, exist_ok=True)

def check_delete_files():
    p = ASSETS_DIR.joinpath("delete_files_for_upgrade.txt")
    if not p.exists():
        return

    p.unlink()

    if APP_DB_PATH.exists():
        APP_DB_PATH.unlink()

    if USER_DB_PATH.exists():
        USER_DB_PATH.unlink()

    if INDEX_DIR.exists():
        shutil.rmtree(INDEX_DIR)

    if COURSES_DIR.exists():
        shutil.rmtree(COURSES_DIR)
