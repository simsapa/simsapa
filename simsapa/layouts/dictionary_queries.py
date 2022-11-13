import re
from typing import List, Optional, TypedDict
from binascii import crc32
from bs4 import BeautifulSoup

from sqlalchemy import or_

from simsapa import SIMSAPA_PACKAGE_DIR, DbSchemaName, logger
from simsapa.app.db.search import SearchResult
from ..app.types import AppData, UDictWord
from ..app.db import appdata_models as Am
from ..app.db import userdata_models as Um
from .html_content import page_tmpl

class ResultHtml(TypedDict):
    body: str
    css: str
    js: str

class DictionaryQueries:
    def __init__(self, app_data: AppData):
        self._app_data = app_data

    def get_words_by_uid(self, uid: str) -> List[UDictWord]:
        results: List[UDictWord] = []

        res = self._app_data.db_session \
            .query(Am.DictWord) \
            .filter(Am.DictWord.uid == uid) \
            .all()
        results.extend(res)

        res = self._app_data.db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.uid == uid) \
            .all()
        results.extend(res)

        return results

    def dict_word_from_result(self, x: SearchResult) -> Optional[UDictWord]:
        if x['schema_name'] == DbSchemaName.AppData.value:
            word = self._app_data.db_session \
                                 .query(Am.DictWord) \
                                 .filter(Am.DictWord.id == x['db_id']) \
                                 .first()
        else:
            word = self._app_data.db_session \
                                 .query(Um.DictWord) \
                                 .filter(Um.DictWord.id == x['db_id']) \
                                 .first()
        return word

    def word_exact_matches(self, query: str) -> List[UDictWord]:
        res: List[UDictWord] = []

        r = self._app_data.db_session \
                          .query(Am.DictWord) \
                          .filter(or_(
                              Am.DictWord.word.like(f"{query}%"),
                              Am.DictWord.synonyms.like(f"%{query}%"),
                          )) \
                          .all()
        res.extend(r)

        r = self._app_data.db_session \
                          .query(Um.DictWord) \
                          .filter(or_(
                              Um.DictWord.word.like(f"{query}%"),
                              Um.DictWord.synonyms.like(f"%{query}%"),
                          )) \
                          .all()
        res.extend(r)

        ch = "." * len(query)
        # for 'dhamma' also match 'dhammā'
        # for 'akata' also match 'akaṭa'
        # for 'kondanna' also match 'koṇḍañña'
        p_query = re.compile(f"^{ch}$")
        # for 'dhamma' also match 'dhamma 1'
        p_num = re.compile(f"^{ch}[ 0-9]+$")

        def _is_match(x: UDictWord):
            return re.match(p_query, str(x.word)) or re.match(p_num, str(x.word))

        res = list(filter(_is_match, res))

        return res

    def words_to_html_page(self, words: List[UDictWord], css_extra: Optional[str] = None) -> str:
        # avoid multiple copies of the same content with a crc32 checksum
        page_body: dict[int, str] = {}
        page_css: dict[int, str] = {}
        page_js: dict[int, str] = {}

        for w in words:
            word_html = self.get_word_html(w)

            body_sum = crc32(bytes(word_html['body'], 'utf-8'))
            if body_sum not in page_body.keys():
                page_body[body_sum] = word_html['body']

            css_sum = crc32(bytes(word_html['css'], 'utf-8'))
            if css_sum not in page_css.keys():
                page_css[css_sum] = word_html['css']

            js_sum = crc32(bytes(word_html['js'], 'utf-8'))
            if js_sum not in page_js.keys():
                page_js[js_sum] = word_html['js']

        font_size = self._app_data.app_settings.get('dictionary_font_size', 18)
        css = f"html {{ font-size: {font_size}px; }}"
        if css_extra:
            css_extra += css
        else:
            css_extra = css

        page_html = self.render_html_page(
            body = "\n\n".join(page_body.values()),
            css_head = "\n\n".join(page_css.values()),
            css_extra = css_extra,
            js_head = "\n\n".join(page_js.values()))

        return page_html

    def render_html_page(self,
                          body: str,
                          css_head: str = '',
                          css_extra: Optional[str] = None,
                          js_head: str = '',
                          js_body: str = '') -> str:
        try:
            with open(SIMSAPA_PACKAGE_DIR.joinpath('assets/css/dictionary.css'), 'r') as f:
                css = f.read()
                if self._app_data.api_url is not None:
                    css = css.replace("http://localhost:8000", self._app_data.api_url)
        except Exception as e:
            logger.error("Can't read dictionary.css")
            css = ""

        css_head = re.sub(r'font-family[^;]+;', '', css_head)
        css_head += css

        if css_extra is not None:
            css_head += css_extra

        html = str(page_tmpl.render(content=body,
                                    css_head=css_head,
                                    js_head=js_head,
                                    js_body=js_body,
                                    api_url=self._app_data.api_url))

        return html

    def word_heading(self, w: UDictWord) -> str:
        return f"""
        <div class="word-heading">
            <h1>{w.word}</h1>
            <div class="uid">{w.uid}</div>
        </div>
        <div class="clear"></div>
        """

    def get_word_html(self, word: UDictWord) -> ResultHtml:
        if word.definition_html is not None and word.definition_html != '':
            definition = str(word.definition_html)

        elif word.definition_plain is not None and word.definition_plain != '':
            style = '<style>pre { font-family: serif; }</style>'
            text = str(word.definition_plain)

            # Wordnet uses curly braces syntax for links: {Lions' beard}
            matches = re.findall(r'(\{(.+?)\})', text, flags = re.DOTALL)
            for m in matches:
                name = m[0].replace('{', '').replace('}', '')
                name = re.sub(r'[\n ]+', ' ', name)
                url = "bword://localhost/" + name.replace(' ', '%20')
                text = text.replace(m[0], f'<a href="{url}">{name}</a>')

            definition = style + '<pre>' + text + '</pre>'
        else:
            definition = '<p>No definition.</p>'

        # Ensure localhost in bword:// urls, otherwise they are invalid and lookup content is empty
        # First remove possibly correct cases, to then replace all cases
        definition = definition \
            .replace('bword://localhost/', 'bword://') \
            .replace('bword://', 'bword://localhost/')

        # We'll remove CSS and JS from 'definition' before assigning it to 'body'
        body = ""
        css = ""
        js = ""

        soup = BeautifulSoup(str(definition), 'html.parser')

        if '<style' in definition:
            h = soup.find_all(name = 'style')
            for i in h:
                css += i.decode_contents()
                definition = definition.replace(css, '')

        if '<script' in definition:
            h = soup.find_all(name = 'script')
            for i in h:
                js += i.decode_contents()
                definition = definition.replace(js, '')

        if '<html' in definition or '<HTML' in definition:
            # Definition is a complete HTML page with a <body> block
            h = soup.find(name = 'body')
            if h is not None:
                body = h.decode_contents() # type: ignore
            else:
                logger.warn("Missing <body> from html page in %s" % word.uid)
                body = definition
        else:
            # Definition is a HTML fragment block, CSS and JS already removed
            body = definition

        def example_format(example):
            return "<div>" + example.text_html + "</div><div>" + example.translation_html + "</div>"

        examples = "".join(list(map(example_format, word.examples))) # type: ignore

        # Wrap definition body in a class
        body = "<div class=\"word-definition\">%s</div>" % body

        # Add examples to body
        if len(examples) > 0:
            body += "<div class=\"word-examples\">%s</div>" % examples

        body = self.word_heading(word) + body

        return ResultHtml(
            body = body,
            css = css,
            js = js,
        )

    def autocomplete_hits(self, query: str) -> set[str]:
        a = set(filter(lambda x: x.lower().startswith(query.lower()), self._app_data.completion_cache['dict_words']))
        return a
