import re
from typing import List
from urllib.parse import urlencode

from PyQt6.QtCore import QUrl

from simsapa import logger
from simsapa.app.search.helpers import RE_ALL_BOOK_SUTTA_REF
from simsapa.app.helpers import strip_html
from simsapa.app.types import QueryType, QuoteScope

def add_sandhi_links(html_page: str) -> str:
    """
    <div class="sandhi">
        <a class="sandhi_feedback" href="" target="_blank"><abbr title=""></abbr></a>
        <p class="sandhi">
            viharatā + yasmā<br>
            viharatā + āyasmā<br>
            viharati + āyasmā<br>
            viharatu + āyasmā<br>
            viharatā + asmā
        </p>
    </div>
    """

    sandhi = re.findall(r'(<p class=[\'"]sandhi[\'"]>(.+?)</p>)', html_page)

    if len(sandhi) == 0:
        return html_page

    def _to_link(w: str) -> str:
        w = w.strip()
        url = QUrl(f"ssp://{QueryType.words.value}/{w}")
        return f'<a href="{url.toString()}">{w}</a>'

    def _to_linked_row(row: str) -> str:
        return " + ".join([_to_link(i) for i in row.split('+')])

    p_content = sandhi[0][1]
    sandhi_rows = [i.strip() for i in p_content.split('<br/>')]

    linked_content = "<br/>".join([_to_linked_row(i) for i in sandhi_rows])

    html_page = html_page.replace(sandhi[0][0], f'<p class="sandhi">{linked_content}</p>')

    return html_page

def add_grammar_links(html_page: str) -> str:
    """
    <tr>
    <td><b>noun</b></td>
    <td>nt loc sg</td>
    <td>of</td>
    <td>dhammacakkhu</td>
    </tr>
    """

    grammar_rows = re.findall(r'(<td>([^>]+)</td>\s*</tr>)', html_page)

    for m in grammar_rows:
        dict_word = m[1].lower().strip()
        url = QUrl(f"ssp://{QueryType.words.value}/{dict_word}")
        link = f'<a href="{url.toString()}">{m[1]}</a>'

        html_page = html_page.replace(m[0], f'<td>{link}</td></tr>')

    return html_page

def add_example_links(html_page: str) -> str:
    from bs4 import BeautifulSoup

    example_ids: List[str] = re.findall(r'id="(example__[^"]+)"', html_page)
    if len(example_ids) == 0:
        return html_page

    # NOTE: html.parser mangles href url params,
    # replacing &quot with "
    # kālaṁ&quote_scope=nikaya becomes kālaṁ"e_scope=nikaya
    soup = BeautifulSoup(html_page, 'lxml')
    for div_id in example_ids:
        div_el = soup.find(id = div_id)
        if div_el is None:
            logger.error(f"Can't find #{div_id}")
        else:
            example_content = div_el.decode() # type: ignore

            # FIXME: DPD dict. exmple text <p> tags are not closed before the sutta <p>.
            # FIXME: DPD dict. sutta refs format doesn't match.
            """
            <p>atthi nu kho bhante kiñci rūpaṃ yaṃ rūpaṃ niccaṃ dhuvaṃ sassataṃ avipariṇāma<b>dhammaṃ</b> sassatisamaṃ tath'eva ṭhassati<p class="sutta">SN 22.97 nakhasikhāsuttaṃ</p><p>gāth'ābhigītaṃ me abhojaneyyaṃ,<br/>sampassataṃ brāhmaṇa n'esa dhammo,<br/>gāth'ābhigītaṃ panudanti buddhā,<br/><b>dhamme</b> satī brāhmaṇa vutti'r'esā.<p class="sutta">SNP 4 kasibhāradvājasuttaṃ<br/>uragavaggo 4</p><p>Can you think of a better example? <a class="link" href="https://docs.google.com/forms/d/e/1FAIpQLSf9boBe7k5tCwq7LdWgBHHGIPVc4ROO5yjVDo1X5LDAxkmGWQ/viewform?usp=pp_url&amp;entry.438735500=dhamma 01&amp;entry.326955045=Example1&amp;entry.1433863141=GoldenDict 2022-11-08" target="_blank">Add it here.</a></p></p></p>
            """

            linked_content = example_content

            for m in re.findall(r'<p>(.*?)\s*(<p class="sutta">(.*?)</p>)', example_content, flags = re.DOTALL | re.MULTILINE):
                # Replace linebreaks with space, otherwise punctuation gets joined with the next line.
                s = m[0]
                s = s.replace("<br>", " ")
                s = s.replace("<br/>", " ")

                # Convert to plain text.
                quote = strip_html(s)

                # End the quote on a word boundary.
                if len(quote) > 100:
                    words = quote.split(" ")
                    q = ""
                    for i in words:
                        q += i + " "
                        if len(q) > 100:
                            break

                    quote = q.strip()

                text = m[2].strip()
                ref = re.search(RE_ALL_BOOK_SUTTA_REF, text)
                if not ref:
                    continue

                sutta_uid = f"{ref.group(1)}{ref.group(2)}".lower()

                url = QUrl(f"ssp://{QueryType.suttas.value}/{sutta_uid}")
                url.setQuery(urlencode({'q': quote, 'quote_scope': QuoteScope.Nikaya.value}))

                link = f'<a href="{url.toString()}">{m[2]}</a>'

                # count=1 so that two links to the same sutta doesn't get overwritten by the first
                linked_content = linked_content.replace(m[1], f'<p class="sutta">{link}</p>', 1)

            div_el.replace_with(BeautifulSoup(linked_content, 'lxml'))

    return soup.decode()

def add_epd_pali_words_links(html_page: str) -> str:
    if '<div class="epd">' not in html_page:
        return html_page

    epd_words: List[str] = re.findall(r'<b class="epd">([^<]+)</b>', html_page)
    if len(epd_words) == 0:
        return html_page

    for word in epd_words:
        url = QUrl(f"ssp://{QueryType.words.value}/{word}")
        link = f'<a href="{url.toString()}">{word}</a>'

        word_tag = f'<b class="epd">{word}</b>'
        linked_tag = f'<b class="epd">{link}</b>'

        html_page = html_page.replace(word_tag, linked_tag)

    return html_page

def add_word_links_to_bold(html_page: str) -> str:
    words: List[str] = re.findall(r'<b>([^<]+)</b>', html_page)
    if len(words) == 0:
        return html_page

    def _word_to_link(word: str) -> str:
        url = QUrl(f"ssp://{QueryType.words.value}/{word}")
        link = f'<a href="{url.toString()}">{word}</a>'
        return link

    for word in words:
        if '-' in word:
            parts = word.split('-')
        else:
            parts = [word]

        links = list(map(_word_to_link, parts))

        word_tag = f'<b>{word}</b>'
        linked_tag = f'<b>{"-".join(links)}</b>'

        html_page = html_page.replace(word_tag, linked_tag)

    return html_page
