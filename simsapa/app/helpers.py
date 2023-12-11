from pathlib import Path
from typing import Dict, Optional, TypedDict
import re, socket
import html, json

from simsapa import logger
from simsapa.app.lookup import (RE_ALL_BOOK_SUTTA_REF, RE_ALL_PTS_VOL_SUTTA_REF, DHP_CHAPTERS_TO_RANGE,
                                SNP_UID_TO_RANGE, THAG_UID_TO_RANGE, THIG_UID_TO_RANGE)

class SuttaRange(TypedDict):
    # sn30.7-16
    group: str # sn30
    start: Optional[int] # 7
    end: Optional[int] # 16

def download_file(url: str, folder_path: Path) -> Path:
    import requests

    logger.info(f"download_file() : {url}, {folder_path}")
    file_name = url.split('/')[-1]
    file_path = folder_path.joinpath(file_name)

    with requests.get(url, stream=True) as r:
        r.raise_for_status()
        with open(file_path, 'wb') as f:
            for chunk in r.iter_content(chunk_size=8192):
                f.write(chunk)

    return file_path

def is_book_sutta_ref(ref: str) -> bool:
    return (re.match(RE_ALL_BOOK_SUTTA_REF, ref) is not None)

def is_pts_sutta_ref(ref: str) -> bool:
    return (re.match(RE_ALL_PTS_VOL_SUTTA_REF, ref) is not None)

def query_text_to_uid_field_query(query_text: str) -> str:
    # Replace user input sutta refs such as 'SN 56.11' with query language
    matches = re.finditer(RE_ALL_BOOK_SUTTA_REF, query_text)
    for m in matches:
        nikaya = m.group(1).lower()
        number = m.group(2)
        query_text = query_text.replace(m.group(0), f"uid:{nikaya}{number}")

    return query_text

def sutta_range_from_ref(ref: str) -> Optional[SuttaRange]:
    # logger.info(f"sutta_range_from_ref(): {ref}")

    """
    sn30.7-16/pli/ms -> SuttaRange(group: 'sn30', start: 7, end: 16)
    sn30.1/pli/ms -> SuttaRange(group: 'sn30', start: 1, end: 1)
    dn1-5/bodhi/en -> SuttaRange(group: 'dn', start: 1, end: 5)
    dn12/bodhi/en -> SuttaRange(group: 'dn', start: 12, end: 12)
    dn2-a -> -> SuttaRange(group: 'dn-a', start: 2, end: 2)
    pli-tv-pvr10
    """

    """
    Problematic:

    ---
    _id: text_extra_info/21419
    uid: sn22.57_a
    acronym: SN 22.57(*) + AN 2.19(*)
    volpage: PTS SN iii 61–63 + AN i 58
    ---
    """

    # logger.info(ref)

    if '/' in ref:
        ref = ref.split('/')[0]

    ref = ref.replace('--', '-')

    # sn22.57_a -> sn22.57
    ref = re.sub(r'_a$', '', ref)

    # an2.19_an3.29 -> an2.19
    # an3.29_sn22.57 -> an3.29
    ref = re.sub(r'_[as]n.*$', '', ref)

    # snp1.2(33-34) -> snp1.2
    if ref == "snp1.2(33-34)":
        ref = "snp1.2"

    # Atthakata
    if ref.endswith('-a'):
        # dn2-a -> dn-a2
        ref = re.sub(r'([a-z-]+)([0-9-]+)-a', r'\1-a\2', ref)

    if not re.search('[0-9]', ref):
        return SuttaRange(
            group = ref,
            start = None,
            end = None,
        )

    if '.' in ref:
        a = ref.split('.')
        group = a[0]
        numeric = a[1]

    else:
        m = re.match(r'([a-z-]+)([0-9-]+)', ref)
        if not m:
            logger.warn(f"Cannot determine range for {ref}")
            return None

        group = m.group(1)
        numeric = m.group(2)

    try:
        if '-' in numeric:
            a = numeric.split('-')
            start = int(a[0])
            end = int(a[1])
        else:
            start = int(numeric)
            end = start
    except Exception as e:
        logger.warn(f"Cannot determine range for {ref}: {e}")
        return None

    res = SuttaRange(
        group = group,
        start = start,
        end = end,
    )

    # logger.info(res)

    return res


def normalize_sutta_ref(ref: str) -> str:
    ref = ref.lower()

    ref = re.sub(r'ud *(\d)', r'uda \1', ref)
    ref = re.sub(r'khp *(\d)', r'kp \1', ref)
    ref = re.sub(r'th *(\d)', r'thag \1', ref)

    ref = re.sub(r'[\. ]*([ivx]+)[\. ]*', r' \1 ', ref)
    ref = re.sub(r'^d ', 'dn ', ref)
    ref = re.sub(r'^m ', 'mn ', ref)
    ref = re.sub(r'^s ', 'sn ', ref)
    ref = re.sub(r'^a ', 'an ', ref)

    return ref.strip()


def normalize_sutta_uid(uid: str) -> str:
    uid = normalize_sutta_ref(uid).replace(' ', '')
    return uid


def consistent_nasal_m(text: Optional[str] = None) -> str:
    if text is None:
        return ''

    # Use only ṁ, both in content and query strings.
    #
    # CST4 uses ṁ
    # SuttaCentral MS uses ṁ
    # Aj Thanissaro's BMC uses ṁ
    # Uncommon Wisdom uses ṁ
    #
    # PTS books use ṃ
    # Digital Pali Reader MS uses ṃ
    # Bodhirasa DPD uses ṃ
    # Bhikkhu Bodhi uses ṃ
    # Forest Sangha Pubs uses ṃ
    # Buddhadhamma uses ṃ

    return text.replace('ṃ', 'ṁ')

def pali_to_ascii(text: Optional[str] = None) -> str:
    if text is None:
        return ''

    # including √ (root sign) and replacing it with space, which gets stripped
    # if occurs at the beginning or end
    from_chars = "āīūṁṃṅñṭḍṇḷṛṣśĀĪŪṀṂṄÑṬḌṆḶṚṢŚ√"
    to_chars   = "aiummnntdnlrssAIUMMNNTDNLRSS "

    trans = str.maketrans(from_chars, to_chars)
    return text.translate(trans).strip()

def word_uid(word: str, dict_label: str) -> str:
    w = word.replace("'", "").replace('"', '').replace(' ', '-')
    uid = f"{w}/{dict_label}".lower()

    return uid

def expand_quote_to_pattern_str(text: str) -> str:
    s = text
    # Normalize quote marks to '
    s = s.replace('"', "'")
    # Quote mark should match all types, and may not be present
    s = s.replace("'", r'[\'"“”‘’]*')
    # Normalize spaces
    s = re.sub(r' +', " ", s)
    # Common spelling variations
    s = re.sub(r'[iī]', '[iī]', s)
    # Punctuation may not be present
    # Space may have punctuation in the text, but not in the link quote param
    s = re.sub(r'[ \.,;\?\!…—-]', r'[ \\n\'"“”‘’\\.,;\\?\\!…—-]*', s)

    return s


def expand_quote_to_pattern(text: str) -> re.Pattern:
    return re.compile(expand_quote_to_pattern_str(text))


def remove_punct(text: Optional[str] = None) -> str:
    if text is None:
        return ''

    # Replace punctuation marks with space. Removing them can join lines or words.
    text = re.sub(r'[\.,;\?\!“”‘’…—-]', '', text)

    # Newline and tab to space
    text = text.replace("\n", " ")
    text = text.replace("\t", " ")

    # Remove quote marks.
    #
    # 'ti is sometimes not punctuated with an apostrophe. Remove the ' both from
    # plain text content and from query strings.
    #
    # Sometimes people add quote marks in compounds: manopubbaṅ'gamā dhammā

    text = text.replace("'", '')
    text = text.replace('"', '')

    # Normalize double spaces to single
    text = re.sub(r'  +', ' ', text)

    return text


def compact_plain_text(text: str) -> str:
    # NOTE: Don't remove new lines here, useful for matching beginning of lines when setting snippets.
    # Replace multiple spaces to one.
    text = re.sub(r"  +", ' ', text)
    text = text.replace('{', '').replace('}', '')

    # Make lowercase and remove punctuation to help matching query strings.
    text = text.lower()
    text = remove_punct(text)
    text = consistent_nasal_m(text)
    text = text.strip()

    return text

def compact_rich_text(text: str) -> str:
    # All on one line
    text = text.replace("\n", " ")

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

    text = strip_html(text)

    text = compact_plain_text(text)

    return text


def strip_html(text: str) -> str:
    text = html.unescape(text)

    re_thumbs = re.compile("["
        u"\U0001f44d" # thumb up
        u"\U0001f44e" # thumb down
    "]+", flags=re.UNICODE)

    text = re_thumbs.sub(r'', text)

    text = re.sub(r'<!doctype html>', '', text, flags=re.IGNORECASE)
    text = re.sub(r'<head(.*?)</head>', '', text)
    text = re.sub(r'<style(.*?)</style>', '', text)
    text = re.sub(r'<script(.*?)</script>', '', text)
    text = re.sub(r'<!--(.*?)-->', '', text)
    text = re.sub(r'</*\w[^>]*>', '', text)

    text = re.sub(r'  +', ' ', text)

    return text

def latinize(text: str) -> str:
    accents = 'ā ī ū ṃ ṁ ṅ ñ ṭ ḍ ṇ ḷ ṛ ṣ ś'.split(' ')
    latin = 'a i u m m n n t d n l r s s'.split(' ')

    for idx, i in enumerate(accents):
        text = text.replace(i, latin[idx])

    return text

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

def html_get_sutta_page_body(html_page: str):
    from bs4 import BeautifulSoup
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

    # from bs4 import BeautifulSoup
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


def dhp_verse_to_chapter(verse_num: int) -> Optional[str]:
    for lim in DHP_CHAPTERS_TO_RANGE.values():
        a = lim[0]
        b = lim[1]
        if verse_num >= a and verse_num <= b:
            return f"dhp{a}-{b}"

    return None


def dhp_chapter_ref_for_verse_num(num: int) -> Optional[str]:
    for ch, verses in DHP_CHAPTERS_TO_RANGE.items():
        if num >= ch and num <= ch:
            return f"dhp{verses[0]}-{verses[1]}"

    return None


def thag_verse_to_uid(verse_num: int) -> Optional[str]:
    # v1 - v120 are thag1.x
    if verse_num <= 120:
        return f"thag1.{verse_num}"

    for uid, lim in THAG_UID_TO_RANGE.items():
        a = lim[0]
        b = lim[1]
        if verse_num >= a and verse_num <= b:
            return uid

    return None


def thig_verse_to_uid(verse_num: int) -> Optional[str]:
    # v1 - v18 are thig1.x
    if verse_num <= 18:
        return f"thig1.{verse_num}"

    for uid, lim in THIG_UID_TO_RANGE.items():
        a = lim[0]
        b = lim[1]
        if verse_num >= a and verse_num <= b:
            return uid

    return None


def snp_verse_to_uid(verse_num: int) -> Optional[str]:
    for uid, lim in SNP_UID_TO_RANGE.items():
        a = lim[0]
        b = lim[1]
        if verse_num >= a and verse_num <= b:
            return uid

    return None

def find_available_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('', 0))
    _, port = sock.getsockname()
    return port
