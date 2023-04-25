import sys
import shutil
from typing import Callable, Dict, List, Optional, TypedDict, Union
import re
from bs4 import BeautifulSoup

from whoosh import writing
from whoosh.highlight import SCORE, HtmlFormatter
from whoosh.index import FileIndex, create_in, open_dir, exists_in
from whoosh.fields import SchemaClass, NUMERIC, TEXT, ID
from whoosh.qparser import QueryParser, FuzzyTermPlugin
from whoosh.searching import Results, Hit
from whoosh.analysis import CharsetFilter, StemmingAnalyzer
from whoosh.support.charset import accent_map

from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from simsapa import DbSchemaName, logger
from simsapa.app.helpers import compact_rich_text, compact_plain_text
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa import INDEX_DIR

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]

# Add an accent-folding filter to the stemming analyzer
folding_analyzer = StemmingAnalyzer() | CharsetFilter(accent_map)

# MN44; MN 118; AN 4.10; Sn 4:2; Dhp 182; Thag 1207; Vism 152
# Must not match part of the path in a url, <a class="link" href="ssp://suttas/mn44/en/sujato">
RE_ALL_BOOK_SUTTA_REF = re.compile(r'(?<!/)\b(DN|MN|SN|AN|Pv|Vv|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]*(\d[\d\.:]*)\b', re.IGNORECASE)
# Vin.iii.40; AN.i.78; D iii 264; SN i 190; M. III. 203.
RE_ALL_PTS_VOL_SUTTA_REF = re.compile(r'(?<!/)\b(D|DN|M|MN|S|SN|A|AN|Pv|Vv|Vin|Vism|iti|kp|khp|snp|th|thag|thig|ud|uda|dhp)[ \.]+([ivxIVX]+)[ \.]+(\d[\d\.]*)\b', re.IGNORECASE)

class SuttasIndexSchema(SchemaClass):
    index_key = ID(stored = True, unique = True)
    db_id = NUMERIC(stored = True)
    schema_name = TEXT(stored = True)
    uid = ID(stored = True)
    language = ID(stored = True)
    source_uid = ID(stored = True)
    title = TEXT(stored = True, analyzer = folding_analyzer)
    title_pali = TEXT(stored = True, analyzer = folding_analyzer)
    title_trans = TEXT(stored = True, analyzer = folding_analyzer)
    content = TEXT(stored = True, analyzer = folding_analyzer)
    ref = ID(stored = True)

class DictWordsIndexSchema(SchemaClass):
    index_key = ID(stored = True, unique = True)
    db_id = NUMERIC(stored = True)
    schema_name = TEXT(stored = True)
    uid = ID(stored = True)
    source_uid = ID(stored = True)
    word = TEXT(stored = True, analyzer = folding_analyzer)
    synonyms = TEXT(stored = True, analyzer = folding_analyzer)
    content = TEXT(stored = True, analyzer = folding_analyzer)

# TODO same as simsapa.app.types.Labels, but declared here to avoid cirular import
class Labels(TypedDict):
    appdata: List[str]
    userdata: List[str]

class SearchResult(TypedDict):
    # database id
    db_id: int
    # database schema name (appdata or userdata)
    schema_name: str
    # database table name (e.g. suttas or dict_words)
    table_name: str
    uid: Optional[str]
    source_uid: Optional[str]
    title: str
    ref: Optional[str]
    author: Optional[str]
    # highlighted snippet
    snippet: str
    # page number in a document
    page_number: Optional[int]
    score: Optional[float]
    rank: Optional[int]

def sutta_hit_to_search_result(x: Hit, snippet: str) -> SearchResult:
    return SearchResult(
        db_id = x['db_id'],
        schema_name = x['schema_name'],
        table_name = 'suttas',
        uid = x['uid'],
        source_uid = x['source_uid'],
        title = x['title'] if 'title' in x.keys() else '',
        ref = x['ref'] if 'ref' in x.keys() else '',
        author = None,
        snippet = snippet,
        page_number = None,
        score = x.score,
        rank = x.rank,
    )

def sutta_to_search_result(x: USutta, snippet: str) -> SearchResult:
    return SearchResult(
        db_id = int(str(x.id)),
        schema_name = x.metadata.schema,
        table_name = 'suttas',
        uid = str(x.uid),
        source_uid = str(x.source_uid),
        title = str(x.title) if x.title else '',
        ref = str(x.sutta_ref) if x.sutta_ref else '',
        author = None,
        snippet = snippet,
        page_number = None,
        score = None,
        rank = None,
    )

def dict_word_hit_to_search_result(x: Hit, snippet: str) -> SearchResult:
    return SearchResult(
        db_id = x['db_id'],
        schema_name = x['schema_name'],
        table_name = 'dict_words',
        uid = x['uid'],
        source_uid = x['source_uid'],
        title = x['word'] if 'word' in x.keys() else '',
        ref = None,
        author = None,
        snippet = snippet,
        page_number = None,
        score = x.score,
        rank = x.rank,
    )

def dict_word_to_search_result(x: UDictWord, snippet: str) -> SearchResult:
    return SearchResult(
        db_id = int(str(x.id)),
        schema_name = x.metadata.schema,
        table_name = 'dict_words',
        uid = str(x.uid),
        source_uid = str(x.source_uid),
        title = str(x.word),
        ref = None,
        author = None,
        snippet = snippet,
        page_number = None,
        score = None,
        rank = None,
    )

class SearchQuery:
    def __init__(self, ix: FileIndex, page_len: int, hit_to_result_fn: Callable):
        self.ix = ix
        self.searcher = ix.searcher()
        self.all_results: Results
        self.filtered: List[Hit] = []
        self.hits: int = 0
        self.page_len = page_len
        self.hit_to_result_fn = hit_to_result_fn

    def _compact_plain_snippet(self,
                               content: str,
                               title: Optional[str] = None,
                               ref: Optional[str] = None) -> str:

        s = content

        # AN 3.119 Kammantasutta
        # as added to the content when indexing
        if ref is not None and title is not None:
            s = s.replace(f"{ref} {title}", '')

        # 163–182. (- dash and -- en-dash)
        s = re.sub(r'[0-9\.–-]+', '', s)

        # ... Book of the Sixes 5.123.
        s = re.sub(r'Book of the [\w ]+[0-9\.]+', '', s)

        # Connected Discourses on ... 12.55.
        s = re.sub(r'\w+ Discourses on [\w ]+[0-9\.]+', '', s)

        # ...vagga
        s = re.sub(r'[\w -]+vagga', '', s)

        # ... Nikāya 123.
        s = re.sub(r'[\w]+ Nikāya +[0-9\.]*', '', s)

        # SC 1, (SuttaCentral ref link text)
        s = re.sub('SC [0-9]+', '', s)

        # Remove the title from the content, but only the first instance, so as
        # not to remove a common word (e.g.'kamma') from the entire text.
        if title is not None:
            s = s.replace(title, '', 1)

        return s

    def _oneline(self, content: str) -> str:
        s = content
        # Clean up whitespace so that all text is one line
        s = s.replace("\n", ' ')
        # replace multiple spaces to one
        s = re.sub(r'  +', ' ', s)

        return s

    def _plain_snippet_from_hit(self, x: Hit) -> str:
            s = x['content'][0:500]
            if 'title' in x.keys() and 'ref' in x.keys():
                s = self._compact_plain_snippet(s, x['title'], x['ref'])
            else:
                s = self._compact_plain_snippet(s)
            s = self._oneline(s).strip()
            snippet = s[0:250] + ' ...'

            return snippet

    def _result_with_snippet_highlight(self, x: Hit):
        fragments = x.highlights(fieldname='content', top=5)

        if len(fragments) > 0:
            snippet = fragments
        else:
            snippet = self._plain_snippet_from_hit(x)

        return self.hit_to_result_fn(x, snippet)

    def _result_with_content_no_highlight(self, x: Hit):
        snippet = self._plain_snippet_from_hit(x)
        return self.hit_to_result_fn(x, snippet)

    def _search_field(self, field_name: str, query: str) -> Results:
        parser = QueryParser(field_name, self.ix.schema)
        parser.add_plugin(FuzzyTermPlugin())
        q = parser.parse(query)

        # NOTE: limit=None is fast enough (~1700 hits for 'dhamma'), doesn't
        # hang when search_as_you_type setting is enabled.
        #
        # The highlighting _is_ slow, that has to be only done on the
        # displayed results page.

        return self.searcher.search(q, limit=None)

    def highlighted_results_page(self, page_num: int) -> List[SearchResult]:
        logger.info(f"app.db.search:: highlighted_results_page({page_num})")
        page_start = page_num * self.page_len
        page_end = page_start + self.page_len
        return list(map(self._result_with_snippet_highlight, self.filtered[page_start:page_end]))

    def new_query(self,
                  query: str,
                  disabled_labels: Optional[Labels] = None,
                  only_lang: Optional[str] = None,
                  only_source: Optional[str] = None):
        logger.info("SearchQuery::new_query()")

        if 'uid:' not in query:
            # Replace user input sutta refs such as 'SN 56.11' with query language
            matches = re.finditer(RE_ALL_BOOK_SUTTA_REF, query)
            for m in matches:
                nikaya = m.group(1).lower()
                number = m.group(2)
                query = query.replace(m.group(0), f"uid:{nikaya}{number}/* ")

        results = self._search_field(field_name = 'content', query = query)

        for k in ['word', 'title', 'title_pali', 'title_trans']:
            q = query.strip('*')
            q = f"*{q}*"

            if k in self.ix.schema._fields.keys():
                res = self._search_field(field_name = k, query = q)
                results.upgrade_and_extend(res)

        self.all_results = results

        def _not_in_disabled(x: Hit):
            if disabled_labels is not None:
                for schema in disabled_labels.keys():
                    for label in disabled_labels[schema]:
                        if x['schema_name'] == schema and x['uid'].endswith(f'/{label.lower()}'):
                            return False
                return True
            else:
                return True

        self.filtered = list(self.all_results)

        if only_lang == "Language":
            only_lang = None
        elif only_lang:
            only_lang = only_lang.lower()

        if only_source == "Source" or only_source == "Dictionaries":
            only_source = None
        elif only_source:
            only_source = only_source.lower()

        if only_lang is not None:
            self.filtered = list(filter(lambda x: x['language'].lower() == only_lang, self.filtered))

        if only_source is not None:
            self.filtered = list(filter(lambda x: x['source_uid'].lower() == only_source, self.filtered))

        if disabled_labels is not None:
            self.filtered = list(filter(_not_in_disabled, self.filtered))

        # NOTE: r.estimated_min_length() errors on some searches
        self.hits = len(self.filtered)

        if self.hits == 0:
            return

        # may slow down highlighting with many results
        # r.fragmenter.charlimit = None

        self.all_results.fragmenter.maxchars = 200
        self.all_results.fragmenter.surround = 20
        # FIRST (the default) Show fragments in the order they appear in the document.
        # SCORE Show highest scoring fragments first.
        self.all_results.order = SCORE
        self.all_results.formatter = HtmlFormatter(tagname='span', classname='match')

    def get_all_results(self, highlight: bool = False) -> List[SearchResult]:
        if highlight:
            return list(map(self._result_with_snippet_highlight, self.all_results))
        else:
            return list(map(self._result_with_content_no_highlight, self.all_results))


class SearchIndexed:
    suttas_index: FileIndex
    dict_words_index: FileIndex
    suttas_lang_index: Dict[str, FileIndex] = dict()

    def __init__(self):
        self.open_all()

    def has_empty_index(self) -> bool:
        if self.suttas_index.is_empty() or self.dict_words_index.is_empty():
            return True
        a = [i for i in self.suttas_lang_index.values() if i.is_empty()]
        return (len(a) > 0)

    def index_all(self, db_session: Session, only_if_empty: bool = False):
        if (not only_if_empty) or (only_if_empty and self.suttas_index.is_empty()):
            general_langs = ['en', 'pli', 'san']
            logger.info(f"Indexing {', '.join(general_langs)} suttas ...")

            suttas: List[USutta] = db_session \
                .query(Am.Sutta) \
                .filter(Am.Sutta.language.in_(general_langs)) \
                .all()
            self.index_suttas(self.suttas_index, DbSchemaName.AppData.value, db_session, suttas)

            suttas: List[USutta] = db_session \
                .query(Um.Sutta) \
                .filter(Um.Sutta.language.in_(general_langs)) \
                .all()
            self.index_suttas(self.suttas_index, DbSchemaName.UserData.value, db_session, suttas)

            for index_name, ix in self.suttas_lang_index.items():
                lang = index_name.replace('suttas_lang_', '')

                logger.info(f"Indexing {lang} suttas ...")

                suttas: List[USutta] = db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.language == lang) \
                    .all()
                self.index_suttas(ix, DbSchemaName.AppData.value, db_session, suttas)

                suttas: List[USutta] = db_session \
                    .query(Um.Sutta) \
                    .filter(Um.Sutta.language == lang) \
                    .all()
                self.index_suttas(ix, DbSchemaName.UserData.value, db_session, suttas)

        if (not only_if_empty) or (only_if_empty and self.dict_words_index.is_empty()):
            logger.info("Indexing dict_words ...")

            words: List[UDictWord] = db_session.query(Am.DictWord).all()
            self.index_dict_words(DbSchemaName.AppData.value, db_session, words)

            words: List[UDictWord] = db_session.query(Um.DictWord).all()
            self.index_dict_words(DbSchemaName.UserData.value, db_session, words)

    def index_all_suttas_lang(self, db_session: Session, lang: str, only_if_empty: bool = False):
        lang_index = self.open_or_create_index(f'suttas_lang_{lang}', SuttasIndexSchema) # type: ignore
        if lang_index is None:
            return
        self.suttas_lang_index[lang] = lang_index

        if (not only_if_empty) or (only_if_empty and self.suttas_lang_index[lang].is_empty()):
            logger.info(f"Indexing suttas: {lang} in appdata ...")

            suttas: List[USutta] = db_session \
                .query(Am.Sutta) \
                .filter(Am.Sutta.language == lang) \
                .all()

            self.index_suttas_lang(DbSchemaName.AppData.value, lang, db_session, suttas)

            logger.info(f"Indexing suttas: {lang} in userdata ...")

            suttas: List[USutta] = db_session \
                .query(Um.Sutta) \
                .filter(Um.Sutta.language == lang) \
                .all()

            self.index_suttas_lang(DbSchemaName.UserData.value, lang, db_session, suttas)

    def get_suttas_lang_index_names(self) -> set[str]:
        """Find suttas_lang_(lang) indexes in the INDEX_DIR"""
        return set(map(lambda x: re.sub(r'^(suttas_lang_[a-z]+)_.*', r'\1', x.name),
                       INDEX_DIR.glob('suttas_lang_*.seg')))

    def open_all(self):
        self.suttas_index: FileIndex = self.open_or_create_index('suttas', SuttasIndexSchema) # type: ignore
        self.dict_words_index: FileIndex = self.open_or_create_index('dict_words', DictWordsIndexSchema) # type: ignore

        for i in self.get_suttas_lang_index_names():
            self.suttas_lang_index[i] = self.open_or_create_index(i, SuttasIndexSchema) # type: ignore

    def clear_all(self):
        w = self.suttas_index.writer()
        w.commit(mergetype=writing.CLEAR)

        w = self.dict_words_index.writer()
        w.commit(mergetype=writing.CLEAR)

        for i in self.suttas_lang_index.keys():
            w = self.suttas_lang_index[i].writer()
            w.commit(mergetype=writing.CLEAR)

    def close_all(self):
        self.suttas_index.close()
        self.dict_words_index.close()
        for i in self.suttas_lang_index.keys():
            self.suttas_lang_index[i].close()

    def create_all(self, remove_if_exists: bool = True):
        if remove_if_exists and INDEX_DIR.exists():
            shutil.rmtree(INDEX_DIR)
        self.open_all()

    def open_or_create_index(self, index_name: str, index_schema: SchemaClass) -> Optional[FileIndex]:
        if not INDEX_DIR.exists():
            INDEX_DIR.mkdir(exist_ok=True)

        if exists_in(dirname = INDEX_DIR, indexname = index_name):
            try:
                ix = open_dir(dirname = INDEX_DIR, indexname = index_name, schema = index_schema)
                return ix
            except Exception as e:
                logger.warn(f"Can't open the index: {index_name}, {e}")

        else:
            try:
                logger.info(f"Creating the index: {index_name}")
                ix = create_in(dirname = INDEX_DIR, indexname = index_name, schema = index_schema)
                return ix
            except Exception as e:
                logger.error(f"Can't create the index: {index_name}, {e}")
                sys.exit(1)

        return None

    def index_suttas(self, ix: FileIndex, schema_name: str, db_session: Session, suttas: List[USutta]):
        logger.info(f"index_suttas() len: {len(suttas)}")

        try:
            # NOTE: Only use multisegment=True when indexing from scratch.
            # Memory limit applies to each process individually.
            writer = ix.writer(procs=4, limitmb=256, multisegment=True)

            # total = len(suttas)
            for _, i in enumerate(suttas):
                # percent = idx/(total/100)
                # logger.info(f"Indexing {percent:.2f}% {idx}/{total}: {i.uid}")
                # Prefer the html content field if not empty.
                if i.content_html is not None and len(i.content_html.strip()) > 0:
                    # Remove content marked with 'noindex' class, such as footer material
                    soup = BeautifulSoup(str(i.content_html), 'html.parser')
                    h = soup.find_all(class_='noindex')
                    for x in h:
                        x.decompose()

                    content = compact_rich_text(str(soup))

                elif i.content_plain is not None:
                    content = compact_plain_text(str(i.content_plain))

                else:
                    logger.warn(f"Skipping, no content in {i.uid}")
                    continue

                language = ""
                if i.language is not None:
                    language = i.language

                source_uid = ""
                if i.source_uid is not None:
                    source_uid = i.source_uid

                sutta_ref = ""
                if i.sutta_ref is not None:
                    sutta_ref = i.sutta_ref

                title = ""
                if i.title is not None:
                    title = i.title

                title_pali = ""
                if i.title_pali is not None:
                    title_pali = i.title_pali

                title_trans = ""
                if i.title_trans is not None:
                    title_trans = i.title_trans

                # Add title and title_pali to content field so a single field query will match
                # Db fields can be None
                c = list(filter(lambda x: len(str(x)) > 0, [str(sutta_ref), str(title), str(title_pali)]))
                pre = " ".join(c)
                # logger.info(f"pre: {pre}")

                if len(pre) > 0:
                    content = f"{pre} {content}"

                writer.update_document(
                    index_key = f"{schema_name}:suttas:{i.uid}",
                    db_id = i.id,
                    schema_name = schema_name,
                    uid = i.uid,
                    language = language,
                    source_uid = source_uid,
                    title = title,
                    title_pali = title_pali,
                    title_trans = title_trans,
                    content = content,
                    ref = sutta_ref,
                )

                i.indexed_at = func.now() # type: ignore

                # logger.info(f"updated: {i.uid}")

            writer.commit()
            db_session.commit()

        except Exception as e:
            logger.error(f"Can't index: {e}")

    def index_suttas_lang(self, schema_name: str, lang: str, db_session: Session, suttas: List[USutta]):
        logger.info(f"index_suttas_lang() lang: {lang} len: {len(suttas)}")

        if lang in self.suttas_lang_index.keys():
            self.index_suttas(self.suttas_lang_index[lang], schema_name, db_session, suttas)
        else:
            logger.warn(f"Index is not in suttas_lang_index: {lang}")

    def index_dict_words(self, schema_name: str, db_session: Session, words: List[UDictWord]):
        logger.info(f"index_dict_words('{schema_name}')")
        ix = self.dict_words_index

        try:
            # NOTE: Only use multisegment=True when indexing from scratch.
            # Memory limit applies to each process individually.
            writer = ix.writer(procs=4, limitmb=256, multisegment=True)

            # total = len(words)
            for _, i in enumerate(words):
                # percent = idx/(total/100)
                # logger.info(f"Indexing {percent:.2f}% {idx}/{total}: {i.uid}")

                # Prefer the html content field if not empty
                if i.definition_html is not None and len(i.definition_html.strip()) > 0:
                    content = compact_rich_text(str(i.definition_html))

                elif i.definition_plain is not None:
                    content = compact_plain_text(str(i.definition_plain))

                else:
                    logger.warn(f"Skipping, no content in {i.word}")
                    continue

                # Add word and synonyms to content field so a single query will match
                if i.word is not None:
                    content = f"{i.word} {content}"

                if i.synonyms is not None:
                    content = f"{content} {i.synonyms}"

                writer.update_document(
                    index_key = f"{schema_name}:dict_words:{i.uid}",
                    db_id = i.id,
                    schema_name = schema_name,
                    uid = i.uid,
                    source_uid = i.source_uid,
                    word = i.word,
                    synonyms = i.synonyms,
                    content = content,
                )
                i.indexed_at = func.now() # type: ignore

            writer.commit()
            db_session.commit()

        except Exception as e:
            logger.error(f"Can't index: {e}")
