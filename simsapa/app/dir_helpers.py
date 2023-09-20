import os, shutil
from pathlib import Path
from simsapa import (APP_DB_PATH, ASSETS_DIR, COURSES_DIR, DESKTOP_FILE_PATH, EBOOK_UNZIP_DIR, GRAPHS_DIR,
                     HTML_RESOURCES_APPDATA_DIR, HTML_RESOURCES_USERDATA_DIR, INDEX_DIR, IS_LINUX,
                     SIMSAPA_DIR, USER_DB_PATH, USER_HOME_DIR, ICONS_DIR)

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

def create_or_update_linux_desktop_icon_file():
    if not IS_LINUX or DESKTOP_FILE_PATH is None:
        return

    if 'APPIMAGE' not in os.environ:
        return

    appimage_path = Path(os.environ['APPIMAGE'])

    if DESKTOP_FILE_PATH.exists():
        with open(DESKTOP_FILE_PATH, mode='r', encoding='utf-8') as f:
            s = f.read()

        if str(appimage_path) in s:
            return

    user_icon_path = USER_HOME_DIR.joinpath(".local/share/icons/simsapa.png")

    if not user_icon_path.exists():
        if not user_icon_path.parent.exists():
            user_icon_path.parent.mkdir(parents=True)

        asset_icon_path = ICONS_DIR.joinpath("appicons/simsapa.png")
        shutil.copy(str(asset_icon_path), user_icon_path)

    if not DESKTOP_FILE_PATH.parent.exists():
        DESKTOP_FILE_PATH.parent.mkdir(parents=True)

    s = """
[Desktop Entry]
Encoding=UTF-8
Name=Simsapa
Icon=simsapa
Terminal=false
Type=Application
Path=%s
Exec=env QTWEBENGINE_DISABLE_SANDBOX=1 %s
    """ % (appimage_path.parent, appimage_path)

    desktop_entry = s.strip()

    with open(DESKTOP_FILE_PATH, mode='w', encoding='utf-8') as f:
        f.write(desktop_entry)
