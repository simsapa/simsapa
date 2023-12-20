import re
from typing import Callable, List, Optional, TypedDict, Dict
from binascii import crc32
from PyQt6.QtCore import QObject, QRunnable, pyqtSignal, pyqtSlot

from sqlalchemy import or_
from sqlalchemy.orm.session import Session

from simsapa import SIMSAPA_PACKAGE_DIR, DbSchemaName, DetailsTab, logger, QueryType
from simsapa.app.search.helpers import SearchResult
from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.dict_link_helpers import add_word_links_to_bold
from simsapa.app.types import SearchParams, UDictWord, DictionaryQueriesInterface
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.layouts.html_content import page_tmpl

from simsapa.app.dpd_render import DPD_PALI_WORD_TEMPLATES, pali_word_dpd_html

class ResultHtml(TypedDict):
    body: str
    css: str
    js: str

class DictionaryQueries(DictionaryQueriesInterface):
    db_session: Session
    api_url: Optional[str] = None
    completion_cache: List[str] = []
    dictionary_font_size = 18

    def __init__(self, db_session: Session, api_url: Optional[str]):
        self.db_session = db_session
        self.api_url = api_url

    def is_complete_uid(self, uid: str) -> bool:
        # Check if uid contains a /, i.e. if it specifies the dictionary
        # (dhammacakkhu/dpd).
        return ("/" in uid.strip("/"))

    def get_words_by_uid(self, uid: str) -> List[UDictWord]:
        results: List[UDictWord] = []

        if self.is_complete_uid(uid):

            res = self.db_session \
                .query(Am.DictWord) \
                .filter(Am.DictWord.uid == uid) \
                .all()
            results.extend(res)

            res = self.db_session \
                .query(Um.DictWord) \
                .filter(Um.DictWord.uid == uid) \
                .all()
            results.extend(res)

            res = self.db_session \
                .query(Dpd.PaliWord) \
                .filter(Dpd.PaliWord.uid == uid) \
                .all()
            results.extend(res)

        else:

            res = self.db_session \
                .query(Am.DictWord) \
                .filter(Am.DictWord.uid.like(f"{uid}/%")) \
                .all()
            results.extend(res)

            res = self.db_session \
                .query(Um.DictWord) \
                .filter(Um.DictWord.uid.like(f"{uid}/%")) \
                .all()
            results.extend(res)

            res = self.db_session \
                .query(Dpd.PaliWord) \
                .filter(Dpd.PaliWord.uid.like(f"{uid}/%")) \
                .all()
            results.extend(res)

        return results

    def dict_word_from_result(self, x: SearchResult) -> Optional[UDictWord]:
        if x['schema_name'] == DbSchemaName.AppData.value:
            word = self.db_session \
                                 .query(Am.DictWord) \
                                 .filter(Am.DictWord.uid == x['uid']) \
                                 .first()

        elif x['schema_name'] == DbSchemaName.UserData.value:
            word = self.db_session \
                                 .query(Um.DictWord) \
                                 .filter(Um.DictWord.uid == x['uid']) \
                                 .first()

        elif x['schema_name'] == DbSchemaName.Dpd.value:
            word = self.db_session \
                                 .query(Dpd.PaliWord) \
                                 .filter(Dpd.PaliWord.id == x['db_id']) \
                                 .first()

        else:
            raise Exception(f"Unknown schema_name: {x['schema_name']}")

        return word

    def words_to_html_page(self,
                           words: List[UDictWord],
                           css_extra: Optional[str] = None,
                           js_extra: Optional[str] = None,
                           html_title: Optional[str] = None) -> str:

        # Avoid multiple copies of the same content with a crc32 checksum.
        #
        # Use ordered structures (i.e. not a dict) to preserve the order of the
        # words[] argument.
        #
        # Below, the different kinds are kept in a dict, but values of each kind
        # are appended to a list.

        parts: Dict[str, List[str]] = dict()
        # The keys are the same as in ResultHtml type.
        parts['body'] = []
        parts['css'] = []
        parts['js'] = []

        sums: Dict[str, List[int]] = dict()
        sums['body'] = []
        sums['css'] = []
        sums['js'] = []

        if len(words) == 1:
            open_details = [DetailsTab.Inflections]
        else:
            open_details = []

        for w in words:
            word_html = self.get_word_html(w, open_details)

            if w.source_uid == "cpd":
                word_html['body'] = add_word_links_to_bold(word_html['body'])

            for k in word_html.keys():
                sum = crc32(bytes(word_html[k], 'utf-8'))
                if sum not in sums[k]:
                    parts[k].append(word_html[k])
                    sums[k].append(sum)

        css = f"html {{ font-size: {self.dictionary_font_size}px; }}"
        if css_extra:
            css_extra += css
        else:
            css_extra = css

        if js_extra:
            js = js_extra
        else:
            js = ""

        js_head = js + "\n\n".join(parts['js'])

        body = "\n\n".join(parts['body'])
        if html_title:
            body = html_title + body

        page_html = self.render_html_page(
            body = body,
            css_head = "\n\n".join(parts['css']),
            css_extra = css_extra,
            js_head = js_head)

        return page_html

    def render_html_page(self,
                          body: str,
                          css_head: str = '',
                          css_extra: Optional[str] = None,
                          js_head: str = '',
                          js_body: str = '') -> str:
        try:
            with open(SIMSAPA_PACKAGE_DIR.joinpath('assets/css/dictionary.css'), 'r') as f: # type: ignore
                css = f.read()
                if self.api_url is not None:
                    css = css.replace("http://localhost:8000", self.api_url)
        except Exception as e:
            logger.error(f"Can't read dictionary.css: {e}")
            css = ""

        css_head = re.sub(r'font-family[^;]+;', '', css_head)
        css_head += css

        if css_extra is not None:
            css_head += css_extra

        html = str(page_tmpl.substitute(content=body,
                                        css_head=css_head,
                                        js_head=js_head,
                                        js_body=js_body,
                                        api_url=self.api_url))

        return html

    def esq(self, s: str) -> str:
        """escape single quote"""
        return s.replace("'", "\\'")

    def word_heading(self, w: UDictWord) -> str:
        el_id_key = f"{w.metadata.schema}_{w.id}"
        transient_id = f"transient-messages_{el_id_key}"

        tmpl = DPD_PALI_WORD_TEMPLATES.dpd_word_heading_simsapa_templ

        html = str(tmpl.render(
            w = w,
            transient_id = transient_id,
            el_id_key = el_id_key,
            esq = self.esq,
        ))

        return html

    def get_word_html(self, word: UDictWord, open_details: List[DetailsTab] = []) -> ResultHtml:
        from bs4 import BeautifulSoup

        if word.metadata.schema == DbSchemaName.Dpd:

            res = pali_word_dpd_html(word, open_details)

            definition = res['definition_html']

        elif word.definition_html is not None and word.definition_html != '':
            definition = str(word.definition_html)

        elif word.definition_plain is not None and word.definition_plain != '':
            style = '<style>pre { font-family: serif; }</style>'
            text = str(word.definition_plain)

            # Wordnet uses curly braces syntax for links: {Lions' beard}
            matches = re.findall(r'(\{(.+?)\})', text, flags = re.DOTALL)
            for m in matches:
                name = m[0].replace('{', '').replace('}', '')
                name = re.sub(r'[\n ]+', ' ', name)
                url = f"ssp://{QueryType.words.value}/{name.replace(' ', '%20')}"
                text = text.replace(m[0], f'<a href="{url}">{name}</a>')

            definition = style + '<pre>' + text + '</pre>'

        else:
            definition = '<p>No definition.</p>'

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

        if '<body' in definition or '<BODY' in definition:
            # Definition is a complete HTML page with a <body> block
            body_re = re.compile(r'<body[^>]*>(.*?)</body>', flags = re.DOTALL | re.IGNORECASE)
            match = body_re.search(definition)

            if match:
                body = match.group(1)
            else:
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

        body = '<div class="word-block">' + self.word_heading(word) + body + '</div>'

        return ResultHtml(
            body = body,
            css = css,
            js = js,
        )

class ExactQueryResult(TypedDict):
    appdata_ids: List[int]
    userdata_ids: List[int]
    dpd_ids: List[int]
    add_recent: bool

class ExactQueryWorkerSignals(QObject):
    finished = pyqtSignal(dict)

class ExactQueryWorker(QRunnable):
    signals: ExactQueryWorkerSignals

    def __init__(self,
                 query: str,
                 finished_fn: Callable,
                 params: SearchParams,
                 add_recent: bool = True):

        super().__init__()
        self.signals = ExactQueryWorkerSignals()
        self.query = query
        self.only_lang = params['lang']
        self.only_source = params['source']
        self.add_recent = add_recent

        self.signals.finished.connect(finished_fn)

    @pyqtSlot()
    def run(self):
        logger.info("ExactQueryWorker::run()")
        res: List[UDictWord] = []
        try:
            db_eng, db_conn, db_session = get_db_engine_connection_session()

            r = db_session \
                .query(Am.DictWord) \
                .filter(or_(
                    Am.DictWord.word.like(f"{self.query}%"),
                    Am.DictWord.synonyms.like(f"%{self.query}%"),
                )) \
                .all()
            res.extend(r)

            r = db_session \
                .query(Um.DictWord) \
                .filter(or_(
                    Um.DictWord.word.like(f"{self.query}%"),
                    Um.DictWord.synonyms.like(f"%{self.query}%"),
                )) \
                .all()
            res.extend(r)

            db_conn.close()
            db_session.close()
            db_eng.dispose()

        except Exception as e:
            logger.error(f"DB query failed: {e}")

        try:
            def _only_lang(x: UDictWord):
                if self.only_lang is not None:
                    return (str(x.language) == self.only_lang)
                else:
                    return True

            def _only_in_source(x: UDictWord):
                if self.only_source is not None:
                    return str(x.uid).endswith(f'/{self.only_source.lower()}')
                else:
                    return True

            if self.only_lang is not None:
                res = list(filter(_only_lang, res))

            if self.only_source is not None:
                res = list(filter(_only_in_source, res))

            ch = "." * len(self.query)
            # for 'dhamma' also match 'dhammā'
            # for 'akata' also match 'akaṭa'
            # for 'kondanna' also match 'koṇḍañña'
            p_query = re.compile(f"^{ch}$")
            # for 'dhamma' also match 'dhamma 1'
            p_num = re.compile(f"^{ch}[ 0-9]+$")

            def _is_match(x: UDictWord):
                if x.word is None:
                    return False
                else:
                    return re.match(p_query, str(x.word)) or re.match(p_num, str(x.word))

            res = list(filter(_is_match, res))

            a = filter(lambda x: x.metadata.schema == DbSchemaName.AppData.value, res)
            appdata_ids = list(map(lambda x: x.id, a))

            a = filter(lambda x: x.metadata.schema == DbSchemaName.UserData.value, res)
            userdata_ids = list(map(lambda x: x.id, a))

            a = filter(lambda x: x.metadata.schema == DbSchemaName.Dpd.value, res)
            dpd_ids = list(map(lambda x: x.id, a))

            ret = ExactQueryResult(
                appdata_ids = appdata_ids,
                userdata_ids = userdata_ids,
                dpd_ids = dpd_ids,
                add_recent = self.add_recent,
            )

            self.signals.finished.emit(ret)

        except Exception as e:
            logger.error(e)
