from pathlib import Path
import re
import subprocess
import json
from typing import List, Optional, Tuple

from PyQt6.QtCore import QUrl

from sqlalchemy.orm.session import Session
from ebooklib import epub
from simsapa.app.lookup import RE_ALL_BOOK_SUTTA_REF, RE_ALL_PTS_VOL_SUTTA_REF

from simsapa import QueryType, SuttaQuote
from simsapa.layouts.html_content import html_page
from simsapa.app.helpers import bilara_content_json_to_html, bilara_line_by_line_html, normalize_sutta_ref
from simsapa.app.helpers import strip_html
from simsapa.app.types import USutta
from simsapa.app.app_data import AppData
from simsapa.app.db import appdata_models as Am
# from simsapa.app.db import userdata_models as Um

def sutta_content_plain(sutta: USutta, join_short_lines: int = 80) -> str:
    if sutta.content_json is not None and sutta.content_json != '':
        # {'an4.10:0.1': 'Numbered Discourses 4.10 ', 'an4.10:0.2': '1. At Bhaṇḍa Village ', ... }
        lines = json.loads(str(sutta.content_json)).values()
        content = "\n\n".join(lines)

    elif sutta.content_html is not None and sutta.content_html != '':
        html = str(sutta.content_html)
        html = re.sub(r'<style(.*?)</style>', '', html, flags=re.DOTALL)
        html = re.sub(r'<footer(.*?)</footer>', '', html, flags=re.DOTALL)
        # <a class="ref sc" href="#sc2" id="sc2">SC 2</a>
        html = re.sub(r'<a class="ref sc"[^>]+>(.*?)</a>', '', html)
        content = strip_html(html)

    elif sutta.content_plain is not None and sutta.content_plain != '':
        content = str(sutta.content_plain)

    else:
        content = 'No content.'

    content = content.strip()
    content = re.sub(r'\s+$', '', content)
    content = re.sub(r'\n\n\n+', r'\n\n', content)

    max = join_short_lines
    if max > 0:
        re_line = re.compile(f'^(.{{1,{max}}})\n')

        # Join short lines to reduce token count.
        content = re_line.sub(r'\1', content)

    return content

def save_sutta_as_html(app_data: AppData, output_path: Path, sutta: USutta):
    s = render_sutta_content(app_data, sutta)
    html = sanitized_sutta_html_for_export(s)
    with open(output_path, 'w') as f:
        f.write(html)

def save_suttas_as_epub(app_data: AppData,
                        output_path: Path,
                        suttas: List[USutta],
                        title: str,
                        author: Optional[str] = None,
                        language: Optional[str] = None):

    book = epub.EpubBook()

    book.set_identifier(output_path.name)
    book.set_title(title)

    # NOTE: Always set language and author.
    # Missing language is an error in epubcheck, and the Kindle Library rejects the epub.

    if language:
        book.set_language(language)
    else:
        book.set_language('en')

    if author:
        book.add_author(author)
    else:
        book.add_author('Unknown')

    toc = []

    for i in suttas:
        uid = i.uid.replace("/", "_")
        chapter = epub.EpubHtml(title = f"{i.sutta_ref} {i.title}", file_name = f"sutta_{uid}.xhtml")

        if i.language is not None:
            chapter.lang = i.language

        html = render_sutta_content(app_data, i)
        chapter.content = sanitized_sutta_html_for_export(html)
        book.add_item(chapter)
        toc.append(chapter)

    book.toc = toc

    book.add_item(epub.EpubNcx(uid='ncx'))
    book.add_item(epub.EpubNav(uid='nav'))

    book.spine = ['nav',]
    book.spine.extend(toc)

    epub.write_epub(output_path, book)

def save_suttas_as_mobi(app_data: AppData,
                        output_path: Path,
                        suttas: List[USutta],
                        title: str,
                        author: Optional[str] = None,
                        language: Optional[str] = None):

    epub_path = output_path.with_suffix(".epub")

    save_suttas_as_epub(app_data, epub_path, suttas, title, author, language)

    ebook_convert_path = app_data.app_settings['path_to_ebook_convert']
    if ebook_convert_path is None:
        raise Exception("<p>ebook-convert path is None</p>")

    convert_epub_to_mobi(Path(ebook_convert_path), epub_path, output_path)

def save_html_as_epub(output_path: Path, sanitized_html: str, title: str, author: str, language: str):
    html = sanitized_html
    book = epub.EpubBook()

    book.set_identifier(output_path.name)
    book.set_title(title)
    book.set_language(language)

    book.add_author(author)

    chapter = epub.EpubHtml(title=title, file_name='sutta.xhtml', lang=language)
    chapter.content = html

    book.add_item(chapter)

    book.toc = (chapter,) # type: ignore

    book.add_item(epub.EpubNcx(uid='ncx'))
    book.add_item(epub.EpubNav(uid='nav'))

    book.spine = ['nav', chapter]

    epub.write_epub(output_path, book)

def convert_epub_to_mobi(ebook_convert_path: Path, epub_path: Path, mobi_path: Path, remove_source = False):
    # Test if we can call ebook-covert
    res = subprocess.run([ebook_convert_path, '--version'], capture_output=True)
    if res.returncode != 0 or 'calibre' not in res.stdout.decode():
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

    res = subprocess.run([ebook_convert_path, epub_path, mobi_path], capture_output=True)

    if remove_source:
        epub_path.unlink()

    if res.returncode != 0:
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

def convert_mobi_to_epub(ebook_convert_path: Path, mobi_path: Path, epub_path: Path, remove_source = False):
    # Test if we can call ebook-covert
    res = subprocess.run([ebook_convert_path, '--version'], capture_output=True)
    if res.returncode != 0 or 'calibre' not in res.stdout.decode():
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

    res = subprocess.run([ebook_convert_path, mobi_path, epub_path], capture_output=True)

    if remove_source:
        mobi_path.unlink()

    if res.returncode != 0:
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

def save_html_as_mobi(ebook_convert_path: Path, output_path: Path, sanitized_html: str, title: str, author: str, language: str):
    epub_path = output_path.with_suffix(".epub")
    save_html_as_epub(output_path = epub_path,
                      sanitized_html = sanitized_html,
                      title = title,
                      author = author,
                      language = language)

    convert_epub_to_mobi(ebook_convert_path, epub_path, output_path)

    if epub_path.exists():
        epub_path.unlink()

def sanitized_sutta_html_for_export(tab_html: str) -> str:
    clean_html = tab_html
    clean_html = re.sub(r'<script(.*?)</script>', '', clean_html, flags=re.DOTALL)
    # <a class="ref sc" href="#sc2" id="sc2">SC 2</a>
    clean_html = re.sub(r'<a class="ref sc"[^>]+>(.*?)</a>', '', clean_html)

    # convert ssp://suttas/... links back to suttacentral
    clean_html = clean_html.replace('ssp://suttas/', 'https://suttacentral.net/')

    # Remove bg/fg color
    """
    html, body {
        height: 100%;
        color: #1a1a1a;
        background-color: #FAE6B2;
    }
    """
    clean_html = re.sub(r'html, body [{](.*?)[}]',
                        'html, body { height: 100%; }',
                        clean_html,
                        flags=re.DOTALL)

    # Remove inline svg icons
    clean_html = re.sub(r'<svg(.*?)</svg>', '', clean_html, flags=re.DOTALL)

    return clean_html

def render_sutta_content(app_data: AppData, sutta: USutta, sutta_quote: Optional[SuttaQuote] = None) -> str:
    if sutta.content_json is not None and sutta.content_json != '':
        line_by_line = app_data.app_settings.get('show_translation_and_pali_line_by_line', True)

        pali_sutta = app_data.get_pali_for_translated(sutta)

        if line_by_line and pali_sutta:
            translated_json = app_data.sutta_to_segments_json(sutta, use_template=False)
            pali_json = app_data.sutta_to_segments_json(pali_sutta, use_template=False)
            tmpl_json = json.loads(str(sutta.content_json_tmpl))
            content = bilara_line_by_line_html(translated_json, pali_json, tmpl_json)

        else:
            translated_json = app_data.sutta_to_segments_json(sutta, use_template=True)
            content = bilara_content_json_to_html(translated_json)

    elif sutta.content_html is not None and sutta.content_html != '':
        content = str(sutta.content_html)

    elif sutta.content_plain is not None and sutta.content_plain != '':
        content = '<pre>' + str(sutta.content_plain) + '</pre>'

    else:
        content = 'No content.'

    font_size = app_data.app_settings.get('sutta_font_size', 22)
    max_width = app_data.app_settings.get('sutta_max_width', 75)

    css_extra = f"html {{ font-size: {font_size}px; }} body {{ max-width: {max_width}ex; }}"

    js_extra = f" const SUTTA_UID = '{sutta.uid}';"

    is_on = app_data.app_settings.get('show_bookmarks', True)
    if is_on:
        js_extra += " const SHOW_BOOKMARKS = true;"
    else:
        js_extra += " const SHOW_BOOKMARKS = false;"

    if sutta_quote:
        text = sutta_quote['quote'].replace('"', '\\"')
        # selection_range = sutta_quote['selection_range'] if sutta_quote['selection_range'] is not None else 0
        # NOTE: highlight_and_scroll_to() doesn't take selection_range argument at the moment.
        js_extra += """
        document.addEventListener("DOMContentLoaded", function(event) { highlight_and_scroll_to("%s"); });
        const SHOW_QUOTE = "%s";
        """ % (text, text)

    html = html_page(content, app_data.api_url, css_extra, js_extra)

    return html

def text_all_escaped(text: str) -> str:
    # All letters to escape codes:
    # a -> 0x61 -> &#x61;
    # hello -> '&#x68;&#x65;&#x6c;&#x6c;&#x6f;'
    return "".join([hex(ord(s)).replace('0x', '&#x')+";" for s in list(text)])

def apply_escape(text: str) -> str:
    matches = re.finditer(r':ESCAPE_START:(.*?):ESCAPE_END:', text)
    already_replaced = []
    for m in matches:
        if m.group(0) in already_replaced:
            continue

        text = text.replace(m.group(0), text_all_escaped(m.group(1)))

        already_replaced.append(m.group(0))

    return text

def find_linkable_sutta_urls_in_text(db_session: Session, content: str) -> List[Tuple[QUrl, str]]:
    content = content \
        .replace("&nbsp;", " ") \
        .replace(u"\u00A0", " ")

    linkable: List[Tuple[QUrl, str]] = []

    matches = re.finditer(RE_ALL_BOOK_SUTTA_REF, content)
    already_found = []
    for ref in matches:
        if ref.group(0) in already_found:
            continue

        sutta_uid = f"{ref.group(1)}{ref.group(2)}".lower()

        url = QUrl(f"ssp://{QueryType.suttas.value}/{sutta_uid}")
        text = ref.group(0)
        linkable.append((url, text))

        already_found.append(text)

    matches = re.finditer(RE_ALL_PTS_VOL_SUTTA_REF, content)
    already_found = []
    for ref in matches:
        if ref.group(0) in already_found:
            continue
        pts_ref = normalize_sutta_ref(ref.group(0), for_ebooks=True)

        multi_ref = db_session \
            .query(Am.MultiRef) \
            .filter(Am.MultiRef.ref.like(f"%{pts_ref}%")) \
            .first()

        if multi_ref and len(multi_ref.suttas) > 0:

            sutta = multi_ref.suttas[0]

            url = QUrl(f"ssp://{QueryType.suttas.value}/{sutta.uid}")
            text = ref.group(0)
            linkable.append((url, text))

            already_found.append(text)

    return linkable

def add_href_sutta_links_in_text(db_session: Session,
                                 content: str,
                                 do_mark_escape = True,
                                 do_apply_escape = True) -> str:

    linkable = find_linkable_sutta_urls_in_text(db_session, content)
    linked_content = content
    already_replaced: List[str] = []

    for i in linkable:
        url, text = i
        if text in already_replaced:
            continue

        label = text
        if do_mark_escape:
            label = f":ESCAPE_START:{label}:ESCAPE_END:"
        if do_apply_escape:
            label = apply_escape(label)
        link = f'<a href="{url.toString()}">{label}</a>'

        linked_content = re.sub(text, link, linked_content)
        already_replaced.append(text)

    return linked_content

def add_sutta_links(db_session: Session, html_content: str) -> str:
    from bs4 import BeautifulSoup
    from bs4.element import Tag, ResultSet

    # Interferes with sutta ref linking if &nbsp; is used between the nikaya and section numbers.
    html_content = html_content \
        .replace("&nbsp;", " ") \
        .replace(u"\u00A0", " ")

    # Find all links and rewrite to ssp:// links if possible
    # - If the href is suttacentral, use the path as uid and rewrite as ssp://
    # - else, if the link text is a sutta ref, parse it and replace link href to a ssp://
    # - replace link text with escaped text block overlapping replacements later

    # NOTE: html.parser mangles href url params,
    # replacing &quot with "
    # kālaṁ&quote_scope=nikaya becomes kālaṁ"e_scope=nikaya
    soup = BeautifulSoup(html_content, 'lxml')

    links: ResultSet[Tag] = soup.find_all(name = 'a', href = True)
    for link in links:
        if 'ssp://' in str(link.get('href')):
            if link.string is not None:
                link.string = f":ESCAPE_START:{link.string}:ESCAPE_END:"

        elif 'suttacentral.net' in str(link.get('href')):

            sc_url = QUrl(link.attrs['href'])

            uid = re.sub(r'^/', '', sc_url.path())

            ssp_href = f"ssp://{QueryType.suttas.value}/{uid}"

            link['href'] = ssp_href

            if link.string is not None:
                link.string = f":ESCAPE_START:{link.string}:ESCAPE_END:"

        elif link.string is not None:
            linkable = find_linkable_sutta_urls_in_text(db_session, link.string)
            if len(linkable) > 0:
                url, _ = linkable[0]
                link['href'] = url.toString()
                link.string = f":ESCAPE_START:{link.string}:ESCAPE_END:"

    linked_content = apply_escape(soup.decode_contents())

    linked_content = add_href_sutta_links_in_text(db_session, linked_content)

    return linked_content
