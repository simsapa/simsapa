from enum import Enum
from typing import List, Optional, Tuple, TypedDict
import os, sys, re, platform
import json, semver
import xmltodict

import psutil
import markdown
import tomlkit

from PyQt6.QtCore import PYQT_VERSION_STR, QT_VERSION_STR

from simsapa.app.db_session import get_db_engine_connection_session, get_dpd_db_version
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa import (APP_DB_PATH, DPD_DB_PATH, DPD_RELEASES_REPO_URL, RELEASES_FALLBACK_JSON, SIMSAPA_APP_VERSION, SIMSAPA_PACKAGE_DIR,
                     SIMSAPA_RELEASES_BASE_URL, IS_MAC, logger)
from simsapa.app.helpers import is_valid_date
from simsapa.app.types import SearchMode, AllSearchModeNameToType, SearchParams
from simsapa.layouts.gui_types import AppWindowInterface, DictionarySearchWindowInterface, EbookReaderWindowInterface, SearchBarInterface, SuttaSearchWindowInterface, SuttaStudyWindowInterface


class Version(TypedDict):
    major: int
    minor: int
    patch: int
    alpha: Optional[int]

class ReleasesReqestParams(TypedDict):
    channel: str
    app_version: str
    system: str
    machine: str
    cpu_max: str
    cpu_cores: str
    mem_total: str
    screen: str
    no_stats: bool

def to_version(ver: str) -> Version:
    # v0.1.7-alpha.5
    m = re.match(r'^v*([0-9]+)\.([0-9]+)\.([0-9]+)', ver)
    if m is None:
        raise Exception('invalid version string')

    major = int(m.group(1))
    minor = int(m.group(2))
    patch = int(m.group(3))

    m = re.match(r'.*-alpha\.([0-9]+)$', ver)
    if m is None:
        alpha = None
    else:
        alpha = int(m.group(1))

    return Version(
        major=major,
        minor=minor,
        patch=patch,
        alpha=alpha,
    )

def get_app_dev_version() -> Optional[str]:

    p = SIMSAPA_PACKAGE_DIR.joinpath('..').joinpath('pyproject.toml')
    if not p.exists(): # type: ignore
        return None

    with open(p) as pyproject: # type: ignore
        s = pyproject.read()

    try:
        t = tomlkit.parse(s)
        v = t['tool']['poetry']['version'] # type: ignore
        ver = f"{v}"
    except Exception as e:
        logger.error(e)
        ver = None

    return ver

def get_app_version() -> Optional[str]:
    # Dev version when running from local folder
    ver = get_app_dev_version()
    if ver is not None:
        return ver

    # If not dev, return installed version
    ver = SIMSAPA_APP_VERSION
    if len(ver) == 0:
        return None

    # convert PEP440 alpha version string to semver compatible string
    # 0.1.7a5 -> 0.1.7-alpha.5
    ver = re.sub(r'\.(\d+)+a(\d+)$', r'.\1-alpha.\2', ver)

    return ver

def get_db_version() -> Optional[str]:
    if APP_DB_PATH.exists():
        db_eng, db_conn, db_session = get_db_engine_connection_session()
        res = db_session \
            .query(Am.AppSetting.value) \
            .filter(Am.AppSetting.key == 'db_version') \
            .first()
        db_conn.close()
        db_session.close()
        db_eng.dispose()
    else:
        return None

    if res is None:
        # A database exists but retreiving the db_version failed.
        # Probably a very old db when version was not yet stored.
        return None

    ver = str(res[0])

    # 'v' prefix is invalid semver string
    # v0.1.7-alpha.1 -> 0.1.7-alpha.1
    ver = re.sub(r'^v', '', ver)

    return ver

def get_sys_version() -> str:
    return f"Python {sys.version}, Qt {QT_VERSION_STR}, PyQt {PYQT_VERSION_STR}"

class UpdateInfo(TypedDict):
    version: str
    message: str
    visit_url: Optional[str]

class EntryType(str, Enum):
    Application = 'application'
    Assets = 'assets'
    Dpd = 'dpd'

class ReleaseEntry(TypedDict):
    version_tag: str
    github_repo: str
    suttas_lang: List[str]
    date: str
    title: str
    description: str

class DpdEntry(TypedDict):
    releases_feed_url: str

class ReleaseSection(TypedDict):
    releases: List[ReleaseEntry]

class ReleasesInfo(TypedDict):
    application: ReleaseSection
    assets: ReleaseSection
    dpd: ReleaseSection

class VersionFeedInfo(TypedDict):
    ids_tags: List[Tuple[str, str]]
    feed_content: str

def is_version_stable(ver: str):
    return not ('.dev' in ver or '.rc' in ver)

def is_dpd_version_compatible(remote_ver: str) -> bool:
    # Currently the DPD version is the release date, e.g 2023-11-27
    # Check that version tag is in this format.
    return is_valid_date(remote_ver)

def get_dpd_releases(compatible_only = True, greater_only = True) -> List[ReleaseEntry]:
    import requests
    logger.info(f"get_dpd_releases() compatible_only = {compatible_only}, greater_only = {greater_only}")

    local_ver: Optional[str] = None

    if DPD_DB_PATH.exists():
        try:
            local_ver = get_dpd_db_version()
        except Exception as e:
            logger.error(e)

        if local_ver is None:
            logger.error("Cannot determine local dpd_db version.")
            local_ver = "1970-01-01"

    else:
        local_ver = "1970-01-01"

    local_version = int(local_ver.replace("-", ""))

    url = f"{DPD_RELEASES_REPO_URL}/releases.atom"
    try:
        r = requests.get(url)
        if r.ok:
            feed_data = r.text
            feed_xml = xmltodict.parse(feed_data)
        else:
            raise Exception(f"Response: {r.status_code}")
    except Exception as e:
        raise e

    releases: List[ReleaseEntry] = []

    # feed.entry is not a list when there is only one release in the repo.
    if isinstance(feed_xml['feed']['entry'], list):
        entry_items = feed_xml['feed']['entry']
    else:
        entry_items = [feed_xml['feed']['entry']]

    for item in entry_items:

        #  tag:github.com,2008:Repository/469025679/v0.2.0-alpha.1
        #  tag:github.com,2008:Repository/469025679/v0.1.8-alpha.1
        #  tag:github.com,2008:Repository/455816765/2023-11-27
        m = re.match(r'tag:github.com,2008:Repository/[0-9]+/([^<]+)', item['id'])
        if m is None:
            continue

        remote_ver = m.group(1)

        if compatible_only and not is_dpd_version_compatible(remote_ver):
            continue

        description =  item['content']['#text']

        """
        <p><a href="http://creativecommons.org/licenses/by-nc/4.0/" rel="nofollow"><img alt="Creative Commons License" src="https://camo.githubusercontent.com/a273c84704b3424ee6a393f65c6ad765e44059e65aae2f82da91ddc1168156
        7b/68747470733a2f2f692e6372656174697665636f6d6d6f6e732e6f72672f6c2f62792d6e632f342e302f38387833312e706e67" data-canonical-src="https://i.creativecommons.org/l/by-nc/4.0/88x31.png" style="max-width: 100%;"></a><br>
        </p>
        """
        # Remove the <img>, will not render in a QMessageBox, only in a QWebEnginePage.
        description = re.sub(r"""<p><a href="http://creativecommons.org/licenses/by-nc/4.0/" rel="nofollow"><img alt="Creative Commons License" [^>]+></a><br>\s*</p>""", "", description)

        entry = ReleaseEntry(
            version_tag = remote_ver,
            # Expects only the user/repo part
            github_repo = DPD_RELEASES_REPO_URL.replace("https://github.com/", ""),
            suttas_lang = [],
            date = item['updated'],
            title = item['title'],
            description = description,
        )

        releases.append(entry)

    def _is_greater(item: ReleaseEntry) -> bool:
        # NOTE: this has been checked to be an ISO date, e.g 2023-11-27
        remote_version = int(item['version_tag'].replace("-", ""))
        return (remote_version > local_version)

    if greater_only:
        releases = [i for i in releases if _is_greater(i)]

    return releases

def get_release_channel() -> str:
    """Determine the release channel to use, either 'main' or 'development'. The
    env var RELEASE_CHANNEL takes precedence over the value of
    userdata.app_settings:key='release_channel'."""

    channel = 'main'

    s = os.getenv('RELEASE_CHANNEL')

    if s == 'development':
        channel = s

    elif s is None or s == '':
        if APP_DB_PATH.exists():
            db_eng, db_conn, db_session = get_db_engine_connection_session()
            res = db_session \
                .query(Um.AppSetting.value) \
                .filter(Um.AppSetting.key == 'release_channel') \
                .first()
            db_conn.close()
            db_session.close()
            db_eng.dispose()

            if res is not None:
                channel = res[0]

    return channel

def get_simsapa_releases_info(save_stats = True, screen_size = '') -> ReleasesInfo:
    logger.info("get_simsapa_releases_info()")

    channel = get_release_channel()

    logger.info(f"Channel: {channel}")

    # Don't save stats if env var asks not to.
    #
    # Env var SAVE_STATS=false overrides argument save_stats=True.
    s = os.getenv('SAVE_STATS')
    if s is not None and s.lower() == 'false':
        save_stats = False

    # Env var NO_STATS=true => save_stats=False
    s = os.getenv('NO_STATS')
    if s is not None and s.lower() == 'true':
        save_stats = False

    if IS_MAC:
        cpu_max = ''
    else:
        cpu_max = f"{psutil.cpu_freq().max:.2f}"

    params = ReleasesReqestParams(
        channel = channel,
        app_version = str(get_app_version()),
        system = platform.system(),
        machine = platform.machine(),
        cpu_max = cpu_max,
        cpu_cores = str(psutil.cpu_count(logical=True)),
        mem_total = str(psutil.virtual_memory().total),
        screen = screen_size,
        no_stats = (not save_stats),
    )

    try:
        import requests
        r = requests.post(f"{SIMSAPA_RELEASES_BASE_URL}/releases", json=params)
        if r.ok:
            data: ReleasesInfo = r.json()
        else:
            # Don't raise an exception, use a sane default value instead.
            # Network timeouts can cause the request to fail, and the user gets
            # confused, especially when they encounter it in the first asset
            # download window.
            data: ReleasesInfo = json.loads(RELEASES_FALLBACK_JSON)

    except Exception as e:
        logger.error(e)
        raise e

    return data

def get_latest_release(info: ReleasesInfo, entry_type: EntryType) -> Optional[ReleaseEntry]:
    if entry_type == EntryType.Application:
        releases = info['application']['releases']
        if len(releases) > 0:
            return releases[0]
        else:
            return None

    else:
        return get_latest_app_compatible_assets_release(info)

def get_latest_app_compatible_assets_release(info: ReleasesInfo) -> Optional[ReleaseEntry]:
    s = get_app_version()
    if s is None:
        return None
    assets_releases = info['assets']['releases']

    # Compare app version with the latest available db version.

    app_v = to_version(s)

    def _is_compat(x: ReleaseEntry) -> bool:
        db_v = to_version(x['version_tag'])
        return (db_v["major"] == app_v["major"] and db_v["minor"] == app_v["minor"])

    compat = list(filter(_is_compat, assets_releases))

    if len(compat) > 0:
        return compat[0]
    else:
        return None

def is_app_version_compatible_with_db_version(app: Version, db: Version) -> bool:
    # Major number difference implies major breaking changes.
    if app['major'] != db['major']:
        return False

    # Minor number difference implies db schema change.
    if app['minor'] != db['minor']:
        return False

    # Patch- or alpha number difference is OK, implies data content change.
    return True

def has_update(info: ReleasesInfo, entry_type: EntryType) -> Optional[UpdateInfo]:
    logger.info(f"has_update(): {entry_type}")

    if entry_type == EntryType.Application:
        s = get_app_version()
    else:
        s = get_db_version()

    if s is None:
        return None

    local = to_version(s)

    entry = get_latest_release(info, entry_type)

    if entry is None:
        return None

    remote = to_version(entry['version_tag'])

    # If remote version is not greater, do nothing.
    # Semver doesn't use 'v' prefix.
    sans_v = re.sub(r'^v', '', entry['version_tag'])
    if semver.compare(sans_v, s) != 1:
        logger.info(f'Not new:\nlocal:  {s} >=\nremote: {sans_v}')
        return None

    if entry_type == EntryType.Assets \
       and not is_app_version_compatible_with_db_version(local, remote):
            logger.info(f'Not compatible:\nlocal:  {local}\nremote: {remote}')
            return None

    visit_url = None

    message = "<h1>An update is available</h1>"
    message += f"<h3>Current: {s}</h3>"
    message += f"<h3>Available: {entry['version_tag']}</h3>"

    if entry_type == EntryType.Application:
        visit_url = f"https://github.com/{entry['github_repo']}/releases/tag/{entry['version_tag']}"
        message += f"<p>Download from the <a href='{visit_url}'>Releases page</a></p>"

    if len(entry['description'].strip()) > 0:
        html_description = markdown.markdown(text = entry['description'], extensions = ['smarty'])
    else:
        html_description = ""

    message += f"<div><p><b>{entry['title']}</b></p>{html_description}</div>"

    return UpdateInfo(
        version = entry['version_tag'],
        message = message,
        visit_url = visit_url,
    )

def is_local_db_obsolete() -> Optional[UpdateInfo]:
    logger.info("is_local_db_obsolete()")

    app_s = get_app_version()
    db_s = get_db_version()

    if app_s is None or db_s is None:
        return None

    # If db version is not lesser, do nothing.
    if semver.compare(db_s, app_s) >= 0:
        logger.info(f'DB version {db_s} not lesser than app version {app_s}')
        return None

    # Only warn for major and minor version number differences, not for patch versions.

    app_s_part = re.sub(r'^(\d+\.\d+\.).*', r'\1', app_s)
    db_s_part = re.sub(r'^(\d+\.\d+\.).*', r'\1', db_s)

    if app_s_part == db_s_part:
        return None

    message = "<h1>The local database is older than the application</h1>"
    message += f"<h3>DB version: {db_s}</h3>"
    message += f"<h3>App version: {app_s}</h3>"
    message += "It is recommended to download a new database which is compatible with the app version."

    return UpdateInfo(
        version = app_s,
        message = message,
        visit_url = None,
    )

def get_search_params(w: SearchBarInterface) -> SearchParams:
    if hasattr(w, 'language_filter_dropdown'):
        idx = w.language_filter_dropdown.currentIndex()
        lang_value = w.language_filter_dropdown.itemText(idx)
        if lang_value == "Language":
            lang = None
        else:
            lang = lang_value
    else:
        lang = None

    if hasattr(w, 'language_include_btn'):
        lang_include = w.language_include_btn.isChecked()
    else:
        lang_include = False

    if hasattr(w, 'source_filter_dropdown'):
        idx = w.source_filter_dropdown.currentIndex()
        source_value = w.source_filter_dropdown.itemText(idx)
        if source_value == "Sources" or source_value == "Dictionaries":
            source = None
        else:
            source = source_value
    else:
        source = None

    if hasattr(w, 'source_include_btn'):
        source_include = w.source_include_btn.isChecked()
    else:
        source_include = False

    if hasattr(w, 'search_mode_dropdown'):
        idx = w.search_mode_dropdown.currentIndex()
        s = w.search_mode_dropdown.itemText(idx)
        mode = AllSearchModeNameToType[s]
    else:
        mode = SearchMode.FulltextMatch

    if hasattr(w, 'regex_checkbox'):
        enable_regex = w.regex_checkbox.isChecked()
    else:
        enable_regex = False

    if hasattr(w, 'fuzzy_spin'):
        fuzzy_distance = w.fuzzy_spin.value()
    else:
        fuzzy_distance = 0

    return SearchParams(
        mode = mode,
        page_len = w.page_len,
        lang = lang,
        lang_include = lang_include,
        source = source,
        source_include = source_include,
        enable_regex = enable_regex,
        fuzzy_distance = fuzzy_distance,
    )

def is_sutta_search_window(w: AppWindowInterface) -> bool:
    r = (str(type(w)) == "<class 'simsapa.layouts.sutta_search.SuttaSearchWindow'>" \
         and isinstance(w, SuttaSearchWindowInterface))
    return r

def is_sutta_study_window(w: AppWindowInterface) -> bool:
    r = (str(type(w)) == "<class 'simsapa.layouts.sutta_study.SuttaStudyWindow'>" \
         and isinstance(w, SuttaStudyWindowInterface))
    return r

def is_dictionary_search_window(w: AppWindowInterface) -> bool:
    r = (str(type(w)) == "<class 'simsapa.layouts.dictionary_search.DictionarySearchWindow'>" \
         and isinstance(w, DictionarySearchWindowInterface))
    return r

def is_ebook_reader_window(w: AppWindowInterface) -> bool:
    r = (str(type(w)) == "<class 'simsapa.layouts.ebook_reader.EbookReaderWindow'>" \
         and isinstance(w, EbookReaderWindowInterface))
    return r
