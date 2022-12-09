from importlib import metadata
from pathlib import Path
import shutil
from typing import Dict, List, Optional, TypedDict
from bleach.css_sanitizer import CSSSanitizer
from bs4 import BeautifulSoup
import json
import requests
import feedparser
import semver
import sys
import re
import bleach

import tomlkit

from PyQt6.QtWidgets import QMainWindow, QMessageBox
from PyQt6.QtCore import PYQT_VERSION_STR, QT_VERSION_STR

from simsapa.app.db_helpers import get_db_engine_connection_session
from simsapa.app.db import appdata_models as Am

from simsapa import ASSETS_DIR, COURSES_DIR, GRAPHS_DIR, SIMSAPA_DIR, SIMSAPA_PACKAGE_DIR, logger

def create_app_dirs():
    if not SIMSAPA_DIR.exists():
        SIMSAPA_DIR.mkdir(parents=True, exist_ok=True)

    if not ASSETS_DIR.exists():
        ASSETS_DIR.mkdir(parents=True, exist_ok=True)

    if not GRAPHS_DIR.exists():
        GRAPHS_DIR.mkdir(parents=True, exist_ok=True)

    if not COURSES_DIR.exists():
        COURSES_DIR.mkdir(parents=True, exist_ok=True)

def ensure_empty_graphs_cache():
    if GRAPHS_DIR.exists():
        shutil.rmtree(GRAPHS_DIR)

    GRAPHS_DIR.mkdir(parents=True, exist_ok=True)

def download_file(url: str, folder_path: Path) -> Path:
    logger.info(f"download_file() : {url}, {folder_path}")
    file_name = url.split('/')[-1]
    file_path = folder_path.joinpath(file_name)

    with requests.get(url, stream=True) as r:
        r.raise_for_status()
        with open(file_path, 'wb') as f:
            for chunk in r.iter_content(chunk_size=8192):
                f.write(chunk)

    return file_path

class Version(TypedDict):
    major: int
    minor: int
    patch: int
    alpha: Optional[int]

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
    if not p.exists():
        return None

    with open(p) as pyproject:
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
    ver = metadata.version('simsapa')
    if len(ver) == 0:
        return None

    # convert PEP440 alpha version string to semver compatible string
    # 0.1.7a5 -> 0.1.7-alpha.5
    ver = re.sub(r'\.(\d+)+a(\d+)$', r'.\1-alpha.\2', ver)

    return ver

def get_db_version() -> Optional[str]:
    _, _, db_session = get_db_engine_connection_session()

    res = db_session \
        .query(Am.AppSetting.value) \
        .filter(Am.AppSetting.key == 'db_version') \
        .first()

    db_session.close()

    if res is None:
        # An old version number before db_version was stored.
        ver = "v0.1.8-alpha.1"
    else:
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

def get_app_update_info() -> Optional[UpdateInfo]:
    logger.info("get_app_update_info()")

    # Test if connection to github is working.
    try:
        requests.head("https://github.com/", timeout=5)
    except Exception as e:
        logger.error("No Connection: Update info unavailable: %s" % e)
        return None

    try:
        d = feedparser.parse("https://github.com/simsapa/simsapa/releases.atom")

        def _id_to_version(id: str):
            return re.sub(r'.*/([^/]+)$', r'\1', id).replace('v', '')

        def _is_version_stable(ver: str):
            return not ('.dev' in ver or '.rc' in ver)

        def _is_entry_version_stable(x):
            ver = _id_to_version(x.id)
            return _is_version_stable(ver)

        # filter entries with .dev or .rc version tags
        stable_entries = list(filter(_is_entry_version_stable, d.entries))

        if len(stable_entries) == 0:
            return None

        entry = stable_entries[0]

        # <id>tag:github.com,2008:Repository/364995446/v0.1.6</id>
        remote_version = _id_to_version(entry.id)
        content = entry.content[0]

        app_version_str = get_app_version()
        if app_version_str is None:
            return None

        # if remote version is not greater, do nothing
        if semver.compare(remote_version, app_version_str) != 1:
            return None

        message = f"<h1>An application update is available</h1>"
        message += f"<h3>Current: {app_version_str}</h3>"
        message += f"<h3>Available: {remote_version}</h3>"
        message += f"<p>Download from the <a href='{entry.link}'>Relases page</a></p>"
        message += f"<div>{content.value}</div>"

        return UpdateInfo(
            version = remote_version,
            message = message,
            visit_url = entry.link,
        )
    except Exception as e:
        logger.error(e)
        return None

def _id_to_version(id: str):
    return re.sub(r'.*/([^/]+)$', r'\1', id).replace('v', '')

def _is_version_stable(ver: str):
    return not ('.dev' in ver or '.rc' in ver)

def _is_entry_version_stable(x):
    ver = _id_to_version(x.id)
    return _is_version_stable(ver)

class FeedEntry(TypedDict):
    title: str
    version: str
    content: str

def get_feed_entries(url: str, stable_only: bool = True) -> List[FeedEntry]:
    try:
        d = feedparser.parse(url)
    except Exception as e:
        raise e

    if stable_only:
        # filter entries with .dev or .rc version tags
        a = list(filter(_is_entry_version_stable, d.entries))
    else:
        a = d.entries

    def _to_entry(x) -> FeedEntry:
        return FeedEntry(
            title=x.title,
            # <id>tag:github.com,2008:Repository/364995446/v0.1.6</id>
            version=_id_to_version(x.id),
            content=x.content[0].value,
        )

    entries: List[FeedEntry] = list(map(_to_entry, a))

    return entries

def filter_compatible_db_entries(feed_entries: List[FeedEntry]) -> List[FeedEntry]:
    s = get_app_version()
    if s is None:
        return []

    app_version = to_version(s)

    def _is_compat_entry(x: FeedEntry) -> bool:
        v = to_version(x['version'])
        return (v["major"] == app_version["major"] and v["minor"] == app_version["minor"])

    compat_entries = list(filter(_is_compat_entry, feed_entries))

    return compat_entries

def get_db_update_info() -> Optional[UpdateInfo]:
    logger.info("get_db_update_info()")

    # Test if connection to github is working.
    try:
        requests.head("https://github.com/", timeout=5)
    except Exception as e:
        logger.error("No Connection: Update info unavailable: %s" % e)
        return None

    try:
        stable_entries = get_feed_entries("https://github.com/simsapa/simsapa-assets/releases.atom")

        if len(stable_entries) == 0:
            return None

        db_version_str = get_db_version()
        if db_version_str is None:
            return None

        db_version = to_version(db_version_str)

        compat_entries = filter_compatible_db_entries(stable_entries)

        if len(compat_entries) == 0:
            return None

        entry = compat_entries[0]

        v = to_version(entry['version'])
        # if patch number is less or equal, do nothing
        if v['patch'] <= db_version['patch']:
            # if remote has no alpha version, do nothing
            if v['alpha'] is None:
                return None
            # if local does not have alpha version, that supersedes a remote alpha
            # v0.1.8 > v0.1.8-alpha.1
            if db_version['alpha'] is None:
                return None
            # if both local and remote has alpha version, but not greater, do nothing
            if v['alpha'] <= db_version['alpha']:
                return None

        # Either patch number or alpha number is greater.

        message = f"<h1>A database update is available</h1>"
        message += f"<h3>Current: {db_version_str}</h3>"
        message += f"<h3>Available: {entry['version']}</h3>"
        message += f"<div>{entry['content']}</div>"

        return UpdateInfo(
            version = entry['version'],
            message = message,
            visit_url = None,
        )
    except Exception as e:
        logger.error(e)
        return None

def compactPlainText(text: str) -> str:
    # NOTE: Don't remove new lines here, useful for matching beginning of lines when setting snippets.
    # Replace multiple spaces to one.
    text = re.sub(r"  +", ' ', text)
    text = text.replace('{', '').replace('}', '')

    return text

def compactRichText(text: str) -> str:
    # All on one line
    text = text.replace("\n", " ")
    # Some CSS is not removed by bleach when syntax is malformed
    text = re.sub(r'<style.*</style>', '', text)
    # No JS here
    text = re.sub(r'<script.*</script>', '', text)
    # escaped html tags
    text = re.sub(r'&lt;[^&]+&gt;', '', text)
    text = text.replace('&nbsp;', ' ')
    text = text.replace('&amp;', '&')
    # remove SuttaCentral ref links
    text = re.sub(r"<a class=.ref\b[^>]+>[^<]*</a>", '', text)

    text = text.replace("<br>", " ")
    text = text.replace("<br/>", " ")

    # Respect word boundaries for <b> <strong> <i> <em> so that dhamm<b>āya</b> becomes dhammāya, not dhamm āya.
    text = re.sub(r'(\w*)<(b|strong|i|em)([^>]*)>(\w*)', r'\1\4', text)
    # corresponding closing tags
    text = re.sub(r'(\w*)</*(b|strong|i|em)>(\w*)', r'\1\3', text)

    # Make sure there is space before and after other tags, so words don't get joined after removing tags.
    #
    # <td>dhammassa</td>
    # <td>dhammāya</td>
    #
    # should become
    #
    # dhammassa dhammāya

    text = text.replace('<', ' <')
    text = text.replace('</', ' </')
    text = text.replace('>', '> ')

    css_sanitizer = CSSSanitizer(allowed_css_properties=[])

    text = bleach.clean(text, tags=[], strip=True, css_sanitizer=css_sanitizer)
    text = compactPlainText(text)

    return text

def latinize(text: str) -> str:
    accents = 'ā ī ū ṃ ṁ ṅ ñ ṭ ḍ ṇ ḷ ṛ ṣ ś'.split(' ')
    latin = 'a i u m m n n t d n l r s s'.split(' ')

    for idx, i in enumerate(accents):
        text = text.replace(i, latin[idx])

    return text

def show_work_in_progress():
    d = QMessageBox()
    d.setWindowTitle("Work in Progress")
    d.setText("Work in Progress")
    d.exec()

def gretil_header_to_footer(body: str) -> str:
    m = re.findall(r'(<h2>Header</h2>(.+?))<h2>Text</h2>', body, flags = re.DOTALL)
    if len(m) > 0:
        header_text = m[0][0]
        main_text = re.sub(r'<h2>Header</h2>(.+?)<h2>Text</h2>', '', body, flags = re.DOTALL)

        footer_text = header_text.replace('<h2>Header</h2>', '<h2>Footer</h2>').replace('<hr>', '')

        main_text = main_text + '<footer class="noindex"><hr>' + footer_text + '</footer>'

    else:
        main_text = body

    return main_text

def make_active_window(view: QMainWindow):
    view.show() # bring window to top on OSX
    view.raise_() # bring window from minimized state on OSX
    view.activateWindow() # bring window to front/unminimize on Windows

def html_get_sutta_page_body(html_page: str):
    if '<html' in html_page or '<HTML' in html_page:
        soup = BeautifulSoup(html_page, 'html.parser')
        h = soup.find(name = 'body')
        if h is None:
            logger.error("HTML document is missing a <body>")
            body = html_page
        else:
            body = h.decode_contents() # type: ignore
    else:
        body = html_page

    return body


def bilara_html_post_process(body: str) -> str:
    # add .noindex to <footer> in suttacentral

    # soup = BeautifulSoup(body, 'html.parser')
    # h = soup.find(name = 'footer')
    # if h is not None:
    #     h['class'] = h.get('class', []) + ['noindex'] # type: ignore
    #
    # html = str(soup)

    html = body.replace('<footer>', '<footer class="noindex">')

    return html

def bilara_text_to_segments(
        content: str,
        tmpl: Optional[str],
        variant: Optional[str] = None,
        comment: Optional[str] = None,
        show_variant_readings: bool = False) -> Dict[str, str]:

    content_json = json.loads(content)
    if tmpl:
        tmpl_json = json.loads(tmpl)
    else:
        tmpl_json = None

    if variant:
        variant_json = json.loads(variant)
    else:
        variant_json = None

    if comment:
        comment_json = json.loads(comment)
    else:
        comment_json = None

    for i in content_json.keys():
        if variant_json:
            if i in variant_json.keys():
                txt = variant_json[i].strip()
                if len(txt) == 0:
                    continue

                classes = ['variant',]

                if not show_variant_readings:
                    classes.append('hide')

                s = """
                <span class='variant-wrap'>
                  <span class='mark'>⧫</span>
                  <span class='%s'>(%s)</span>
                </span>
                """ % (' '.join(classes), txt)

                content_json[i] += s

        if comment_json:
            if i in comment_json.keys():
                txt = comment_json[i].strip()
                if len(txt) == 0:
                    continue

                s = """
                <span class='comment-wrap'>
                  <span class='mark'>✱</span>
                  <span class='comment hide'>(%s)</span>
                </span>
                """ % txt

                content_json[i] += s

        if tmpl_json and i in tmpl_json.keys():
            content_json[i] = tmpl_json[i].replace('{}', content_json[i])

    return content_json

def bilara_content_json_to_html(content_json: Dict[str, str]) -> str:
    page = "\n\n".join(content_json.values())

    body = html_get_sutta_page_body(page)
    body = bilara_html_post_process(body)

    content_html = '<div class="suttacentral bilara-text">' + body + '</div>'

    return content_html

def bilara_line_by_line_html(translated_json: Dict[str, str],
                             pali_json: Dict[str, str],
                             tmpl_json: Dict[str, str]) -> str:

    content_json: Dict[str, str] = dict()

    for i in translated_json.keys():
        translated_segment = translated_json[i]
        if i in pali_json:
            pali_segment = pali_json[i]
        else:
            pali_segment = ""

        content_json[i] = """
        <span class='segment'>
          <span class='translated'>%s</span>
          <span class='pali'>%s</span>
        </span>
        """ % (translated_segment, pali_segment)

        if tmpl_json and i in tmpl_json.keys():
            content_json[i] = tmpl_json[i].replace('{}', content_json[i])

    return bilara_content_json_to_html(content_json)

def bilara_text_to_html(
        content: str,
        tmpl: str,
        variant: Optional[str] = None,
        comment: Optional[str] = None,
        show_variant_readings: bool = False) -> str:

    content_json = bilara_text_to_segments(
        content,
        tmpl,
        variant,
        comment,
        show_variant_readings,
    )

    return bilara_content_json_to_html(content_json)
