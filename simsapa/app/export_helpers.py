from pathlib import Path
import re
import subprocess
import json
from typing import List, Optional

from ebooklib import epub

from simsapa.layouts.html_content import html_page
from simsapa.app.helpers import bilara_content_json_to_html, bilara_line_by_line_html
from simsapa.app.helpers import strip_html
from simsapa.app.types import AppData, SuttaQuote, USutta

def sutta_content_plain(sutta: USutta, join_short_lines: int = 80) -> str:
    if sutta.content_json is not None and sutta.content_json != '':
        lines = json.loads(str(sutta.content_json))
        content = "\n\n".join(lines)

    elif sutta.content_html is not None and sutta.content_html != '':
        html = str(sutta.content_html)
        html = re.sub(r'<style(.*?)</style>', '', html, flags=re.DOTALL)
        html = re.sub(r'<footer(.*?)</footer>', '', html, flags=re.DOTALL)
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

    if language:
        book.set_language(language)

    if author:
        book.add_author(author)

    toc = []

    for i in suttas:
        chapter = epub.EpubHtml(
            title = f"{i.sutta_ref} {i.title}",
            file_name = f"sutta_{i.uid}.xhtml")

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

def convert_epub_to_mobi(ebook_convert_path: Path, epub_path: Path, mobi_path: Path):
    # Test if we can call ebook-covert
    res = subprocess.run([ebook_convert_path, '--version'], capture_output=True)
    if res.returncode != 0 or 'calibre' not in res.stdout.decode():
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

    res = subprocess.run([ebook_convert_path, epub_path, mobi_path], capture_output=True)

    epub_path.unlink()

    if res.returncode != 0:
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

def save_html_as_mobi(ebook_convert_path: Path, output_path: Path, sanitized_html: str, title: str, author: str, language: str):
    epub_path = output_path.with_suffix(".mobi")
    save_html_as_epub(output_path = epub_path,
                      sanitized_html = sanitized_html,
                      title = title,
                      author = author,
                      language = language)

    convert_epub_to_mobi(ebook_convert_path, epub_path, output_path)

def sanitized_sutta_html_for_export(tab_html: str) -> str:
    clean_html = tab_html
    clean_html = re.sub(r'<script(.*?)</script>', '', clean_html, flags=re.DOTALL)

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

    js_extra = f"const SUTTA_UID = '{sutta.uid}';";

    is_on = app_data.app_settings.get('show_bookmarks', True)
    if is_on:
        js_extra += "const SHOW_BOOKMARKS = true;";
    else:
        js_extra += "const SHOW_BOOKMARKS = false;";

    if sutta_quote:
        text = sutta_quote['quote'].replace('"', '\\"')
        selection_range = sutta_quote['selection_range'] if sutta_quote['selection_range'] is not None else 0
        js_extra += """document.addEventListener("DOMContentLoaded", function(event) { highlight_and_scroll_to("%s", "%s"); });""" % (text, selection_range)

    html = html_page(content, app_data.api_url, css_extra, js_extra)

    return html
