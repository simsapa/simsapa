from pathlib import Path
import re
import subprocess
import json

from ebooklib import epub

from simsapa.app.helpers import strip_html
from simsapa.app.types import USutta

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

def save_html_as_mobi(ebook_convert_path: Path, output_path: Path, sanitized_html: str, title: str, author: str, language: str):
    # Test if we can call ebook-covert
    res = subprocess.run([ebook_convert_path, '--version'], capture_output=True)
    if res.returncode != 0 or 'calibre' not in res.stdout.decode():
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

    epub_path = output_path.with_suffix(".mobi")
    save_html_as_epub(output_path = epub_path,
                      sanitized_html = sanitized_html,
                      title = title,
                      author = author,
                      language = language)

    res = subprocess.run([ebook_convert_path, epub_path, output_path], capture_output=True)

    epub_path.unlink()

    if res.returncode != 0:
        raise Exception(f"<p>ebook-convert returned with status {res.returncode}:</p><p>{res.stderr.decode()}</p><p>{res.stderr.decode()}</p>")

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
