import logging as _logging
from typing import Callable, List, Optional, TypedDict
from whoosh.highlight import SCORE, HtmlFormatter

from whoosh.index import FileIndex, create_in, open_dir
from whoosh.fields import SchemaClass, NUMERIC, TEXT, ID
from whoosh.qparser import QueryParser, FuzzyTermPlugin
from whoosh.searching import Searcher, Results, Hit
from whoosh.analysis import CharsetFilter, StemmingAnalyzer
from whoosh.support.charset import accent_map

from simsapa.app.helpers import compactPlainText, compactRichText
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import appdata_models as Um
from simsapa import INDEX_DIR

logger = _logging.getLogger(__name__)

# Add an accent-folding filter to the stemming analyzer
folding_analyzer = StemmingAnalyzer() | CharsetFilter(accent_map)

class SuttasIndexSchema(SchemaClass):
    id = NUMERIC(stored = True, unique = True)
    schema_name = TEXT(stored = True)
    title = TEXT(stored = True, analyzer = folding_analyzer)
    title_pali = TEXT(stored = True, analyzer = folding_analyzer)
    content = TEXT(stored = True, analyzer = folding_analyzer)
    ref = ID(stored = True)

class DictWordsIndexSchema(SchemaClass):
    id = NUMERIC(stored = True, unique = True)
    schema_name = TEXT(stored = True)
    word = TEXT(stored = True, analyzer = folding_analyzer)
    synonyms = TEXT(stored = True, analyzer = folding_analyzer)
    content = TEXT(stored = True, analyzer = folding_analyzer)

class SearchResult(TypedDict):
    # database id
    id: int
    # database schema name (appdata or userdata)
    schema_name: str
    # database table name
    table_name: str
    # result title
    title: str
    # highlighted snippet
    snippet: str
    # page number in a document
    page_number: Optional[int]

def sutta_hit_to_search_result(x: Hit, snippet: str) -> SearchResult:
    return SearchResult(
        id = x['id'],
        schema_name = x['schema_name'],
        table_name = 'suttas',
        title = x['title'],
        snippet = snippet,
        page_number = None,
    )

def dict_word_hit_to_search_result(x: Hit, snippet: str) -> SearchResult:
    return SearchResult(
        id = x['id'],
        schema_name = x['schema_name'],
        table_name = 'dict_words',
        title = x['word'],
        snippet = snippet,
        page_number = None,
    )

class SearchQuery:
    def __init__(self, ix: FileIndex, page_len: int, hit_to_result_fn: Callable):
        self.ix = ix
        self.searcher = ix.searcher()
        self.results: Results
        self.hits: int = 0
        self.page_len = page_len
        self.hit_to_result_fn = hit_to_result_fn

    def _result_with_snippet_highlight(self, x: Hit):
        style = "<style>span.match { background-color: yellow; }</style>"
        snippet = style + x.highlights(fieldname='content', top=5)

        return self.hit_to_result_fn(x, snippet)

    def _search_field(self, field_name: str, query: str) -> Results:
        parser = QueryParser(field_name, self.ix.schema)
        parser.add_plugin(FuzzyTermPlugin())
        q = parser.parse(query)

        # NOTE: limit=None is fast enough (~1700 hits for 'dhamma'), doesn't
        # hang when doing incremental search while typing.
        #
        # The highlighting _is_ slow, that has to be only done on the
        # displayed results page.

        return self.searcher.search(q, limit=None, terms=True)

    def highlight_results_page(self, page_num: int) -> List[SearchResult]:
        page_start = page_num * self.page_len
        page_end = page_start + self.page_len
        return list(map(self._result_with_snippet_highlight, self.results[page_start:page_end]))

    def new_query(self, query: str) -> List[SearchResult]:
        self.results = self._search_field('content', query)
        # NOTE: r.estimated_min_length() errors on some searches
        self.hits = len(self.results)

        if self.hits == 0:
            return []

        # may slow down highlighting with many results
        # r.fragmenter.charlimit = None

        self.results.fragmenter.maxchars = 200
        self.results.fragmenter.surround = 20
        # FIRST (the default) Show fragments in the order they appear in the document.
        # SCORE Show highest scoring fragments first.
        self.results.order = SCORE
        self.results.formatter = HtmlFormatter(tagname='span', classname='match')

        # NOTE: highlighting the matched fragments is slow, so only do
        # highlighting on the first page of results.

        return self.highlight_results_page(0)

class SearchIndexed:
    def __init__(self):
        self.suttas_index = self._open_or_create_index('suttas', SuttasIndexSchema)
        self.dict_words_index = self._open_or_create_index('dict_words', DictWordsIndexSchema)

    def index_if_empty(self, db_session):
        if self.suttas_index.is_empty():
            print("Indexing suttas ...")
            self.index_suttas(db_session, 'appdata')

        if self.dict_words_index.is_empty():
            print("Indexing dict_words ...")
            self.index_dict_words(db_session, 'appdata')

    def _open_or_create_index(self, index_name: str, index_schema: SchemaClass) -> FileIndex:
        if not INDEX_DIR.exists():
            INDEX_DIR.mkdir(exist_ok=True)

        try:
            ix = open_dir(dirname = INDEX_DIR, indexname = index_name, schema = index_schema)
            return ix
        except Exception as e:
            logger.info(f"Can't open the index: {index_name}")
            print(e)

        try:
            ix = create_in(dirname = INDEX_DIR, indexname = index_name, schema = index_schema)
        except Exception as e:
            logger.error(f"Can't create the index: {index_name}")
            print(e)
            exit(1)

        return ix

    def index_suttas(self, db_session, schema_name: str):
        ix = self.suttas_index

        if schema_name == 'appdata':
            a = db_session.query(Am.Sutta).all()
        else:
            a = db_session.query(Um.Sutta).all()

        try:
            # NOTE: Only use multisegment=True when indexing from scratch.
            # Memory limit applies to each process individually.
            writer = ix.writer(procs=4, limitmb=256, multisegment=True)

            for i in a:
                # Prefer the html content field if not empty
                if i.content_html is not None and len(i.content_html.strip()) > 0:
                    content = compactRichText(i.content_html)
                elif i.content_plain is None:
                    continue
                else:
                    content = compactPlainText(i.content_plain)

                # Add sutta ref to title so it can be matched
                title =  f"{i.sutta_ref} {i.title}"

                # Add title and title_pali to content field so a single field query will match
                content = f"{title} {i.title_pali} {content}"

                writer.add_document(
                    id = i.id,
                    schema_name = schema_name,
                    title = title,
                    title_pali = i.title_pali,
                    content = content,
                    ref = i.sutta_ref,
                )
            writer.commit()

        except Exception as e:
            logger.error("Can't index.")
            print(e)

    def index_dict_words(self, db_session, schema_name: str):
        ix = self.dict_words_index

        if schema_name == 'appdata':
            a = db_session.query(Am.DictWord).all()
        else:
            a = db_session.query(Um.DictWord).all()

        try:
            # NOTE: Only use multisegment=True when indexing from scratch.
            # Memory limit applies to each process individually.
            writer = ix.writer(procs=4, limitmb=256, multisegment=True)
            for i in a:
                # Prefer the html content field if not empty
                if i.definition_html is not None and len(i.definition_html.strip()) > 0:
                    content = compactRichText(i.definition_html)
                elif i.definition_plain is None:
                    continue
                else:
                    content = compactPlainText(i.definition_plain)

                # Add word and synonyms to content field so a single query will match
                content = f"{i.word} {i.synonyms} {content}"

                writer.add_document(
                    id = i.id,
                    schema_name = schema_name,
                    word = i.word,
                    synonyms = i.synonyms,
                    content = content
                )
            writer.commit()

        except Exception as e:
            logger.error("Can't index.")
            print(e)

