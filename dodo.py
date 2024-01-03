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
        'params': [{'name': 'branch',
                    'long': 'branch',
                    'default': 'main',
                    'type': str,
                    'help': 'Specify the git branch.'}],
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

def task_bootstrap_db():
    return {
        'actions': [""" ./scripts/bootstrap_db.sh """],
    }

def task_simsapa_min_js():
    return {
        'actions': [""" npx webpack """],
        'targets': ["simsapa/assets/js/simsapa.min.js"],
        'clean': True,
    }

def task_requirements_txt():
    return {
        'actions': [""" poetry export --without-hashes -o requirements.txt """],
        'targets': ['requirements.txt'],
        'clean': True,
    }

def task_icons():
    return {
        'actions': [""" ./scripts/icons_assets.sh """],
    }

def ui_to_py():
    cmd = """
pyuic6 -o simsapa/assets/ui/sutta_search_window_ui.py simsapa/assets/ui/sutta_search_window.ui && \
pyuic6 -o simsapa/assets/ui/sutta_study_window_ui.py simsapa/assets/ui/sutta_study_window.ui && \
pyuic6 -o simsapa/assets/ui/dictionary_search_window_ui.py simsapa/assets/ui/dictionary_search_window.ui && \
pyuic6 -o simsapa/assets/ui/memos_browser_window_ui.py simsapa/assets/ui/memos_browser_window.ui && \
pyuic6 -o simsapa/assets/ui/links_browser_window_ui.py simsapa/assets/ui/links_browser_window.ui && \
pyuic6 -o simsapa/assets/ui/dictionaries_manager_window_ui.py simsapa/assets/ui/dictionaries_manager_window.ui && \
pyuic6 -o simsapa/assets/ui/document_reader_window_ui.py simsapa/assets/ui/document_reader_window.ui && \
pyuic6 -o simsapa/assets/ui/library_browser_window_ui.py simsapa/assets/ui/library_browser_window.ui && \
pyuic6 -o simsapa/assets/ui/import_stardict_dialog_ui.py simsapa/assets/ui/import_stardict_dialog.ui
    """

    return {
        'actions': [cmd],
    }

def task_sass_build():
    # Ruby 3.0 sass, version:
    # 1.49.7 compiled with dart2js 2.15.1
    return {
        'actions': [""" sass --no-source-map './simsapa/assets/sass/:./simsapa/assets/css/' """]
    }

def task_sass_watch():
    return {
        'actions': [""" sass --no-source-map --watch './simsapa/assets/sass/:./simsapa/assets/css/' """]
    }

def task_count_code():
    return {
        'actions': [""" tokei --type Python --exclude simsapa/assets/ --exclude simsapa/keyboard/ --exclude simsapa/app/lookup.py . | grep -vE '===|Total' """],
    }

def task_profile_time_chart_png():
    return {
        'actions': [""" gnuplot < scripts/profile_time_chart.gp """],
        'targets': ['profile_time_chart.png'],
        'clean': True,
    }
