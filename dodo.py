import platform, shutil, sqlite3
from pathlib import Path

from simsapa import APP_DB_PATH, ASSETS_DIR

from doit.tools import title_with_actions

DOIT_CONFIG = {
    'default_tasks': ['update_build_open'],
    'verbosity': 2,
}

IS_LINUX = (platform.system() == 'Linux')
IS_WINDOWS = (platform.system() == 'Windows')
IS_MAC = (platform.system() == 'Darwin')

def task_update_build_open():
    """Update the repo, install libs, build the app, open the dist folder."""

    if IS_MAC:
        open_cmd = 'open ./dist'
    elif IS_LINUX:
        open_cmd = 'xdg-open ./dist'
    elif IS_WINDOWS:
        open_cmd = 'start ./dist'
    else:
        open_cmd = ''

    return {
        'actions': [
            'git checkout %(branch)s',
            'git pull origin %(branch)s',
            'poetry install',
            'poetry run doit build',
            open_cmd,
        ],
        'params': [{'name': 'branch', 'long': 'branch', 'default': 'main', 'help': 'Specify the git branch.'}],
        'title': title_with_actions,
    }

def task_build():
    """Build Simsapa for the current platform."""

    if IS_LINUX:
        task = {
            'actions': [],
            'file_dep': ['dist/Simsapa Dhamma Reader.AppImage'],
            'clean': True,
        }

    elif IS_WINDOWS:
        task = {
            'actions': [],
            'file_dep': ['dist/Simsapa Dhamma Reader/Simsapa Dhamma Reader.exe'],
            'clean': True,
        }

    elif IS_MAC:
        task = {
            'actions': [],
            'file_dep': ['dist/Simsapa Dhamma Reader.dmg'],
            'clean': True,
        }

    else:
        print("The build platform is not one of: Linux, Windows, MacOS.")
        return False

    return task

def task_build_linux():
    """Build Simsapa as AppImage on Linux."""

    return {
        'actions': [""" echo "Hey Linux!" """],
        'targets': ['dist/Simsapa Dhamma Reader.AppImage'],
        'clean': True,
    }

def task_build_windows():
    """Build Simsapa as .exe on Windows."""

    return {
        'actions': [""" echo "Hey Windows!" """],
        'targets': ['dist/Simsapa Dhamma Reader/Simsapa Dhamma Reader.exe'],
        'clean': True,
    }

def task_build_macos_app():
    """Build Simsapa as .app on MacOS."""

    # Mac M1 chips: arm64
    # Mac Intel chips: x86_64
    machine = platform.machine()

    pyinstaller_cmd = """
pyinstaller run.py \
    --name "Simsapa Dhamma Reader" \
    --onedir \
    --windowed \
    --clean \
    --noupx \
    -i "simsapa/assets/icons/appicons/simsapa.ico" \
    --add-data "simsapa/assets:simsapa/assets" \
    --add-data "simsapa/alembic:simsapa/alembic" \
    --add-data "simsapa/alembic.ini:simsapa/alembic.ini" \
    --target-architecture %s \
    --osx-bundle-identifier 'com.profound-labs.dhamma.simsapa' \
    --hidden-import=tiktoken_ext \
    --hidden-import=tiktoken_ext.openai_public
    """ % machine

    return {
        'actions': [pyinstaller_cmd],
        'targets': ['dist/Simsapa Dhamma Reader.app'],
        'clean': True,
    }

def task_build_macos_dmg():
    """Build .dmg from .app"""

    return {
        'actions': ['./scripts/build_dmg.sh'],
        'file_dep': ['dist/Simsapa Dhamma Reader.app'],
        'targets': ['dist/Simsapa Dhamma Reader.dmg'],
        'clean': True,
    }

def task_remove_local_db():
    """Delete the local database and index files."""
    def _task():
        shutil.rmtree(ASSETS_DIR)
    return {'actions': [_task]}

def task_set_development_channel_local_db():
    """Set release_channel to 'development' in local db."""
    def _task():
        update_release_channel(APP_DB_PATH, 'development')
    return {'actions': [_task]}

def task_set_main_channel_local_db():
    """Set release_channel to 'main' in local db."""
    def _task():
        update_release_channel(APP_DB_PATH, 'main')
    return {'actions': [_task]}

def update_release_channel(db_path: Path, release_channel: str):
    if not db_path.exists():
        print(f"File does not exist: {db_path}")
        return False

    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    cursor.execute("SELECT * FROM app_settings WHERE key = 'release_channel'")
    r = cursor.fetchone()

    if r is None:
        cursor.execute("INSERT INTO app_settings (key, value) VALUES ('release_channel', ?);",
                       (release_channel,))
    else:
        cursor.execute("UPDATE app_settings SET value = ? WHERE key = 'release_channel';",
                       (release_channel,))

    conn.commit()
    conn.close()
