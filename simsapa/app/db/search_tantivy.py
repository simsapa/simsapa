import shutil
from typing import Dict, List, Optional, Union, Any, Tuple
import re
from bs4 import BeautifulSoup

import tantivy

# from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from simsapa import DbSchemaName, logger, INDEX_DIR
from simsapa.app.helpers import compact_rich_text, compact_plain_text
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db.search_helpers import Labels, SearchResult, RE_ALL_BOOK_SUTTA_REF, search_compact_plain_snippet, search_oneline

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord]

# A Score(f32) or an Order(u64)
TantivyFruit = Union[float, int]

TantivyHit = Tuple[TantivyFruit, tantivy.DocAddress]

def tantivy_sutta_doc_to_search_result(x: tantivy.Document,
                                       snippet: str,
                                       score: Optional[float] = None,
                                       rank: Optional[int] = None) -> SearchResult:
    return SearchResult(
        db_id = x['db_id'][0],
        schema_name = x['schema_name'][0],
        table_name = 'suttas',
        uid = x['uid'][0],
        source_uid = x['source_uid'][0],
        title = x['title'][0] if 'title' in x.keys() else '',
        ref = x['ref'][0] if 'ref' in x.keys() else '',
        author = None,
        snippet = snippet,
        page_number = None,
        score = score,
        rank = rank,
    )

def tantivy_dict_word_doc_to_search_result(x: tantivy.Document,
                                           snippet: str,
                                           score: Optional[float] = None,
                                           rank: Optional[int] = None) -> SearchResult:
    return SearchResult(
        db_id = x['db_id'][0],
        schema_name = x['schema_name'][0],
        table_name = 'dict_words',
        uid = x['uid'][0],
        source_uid = x['source_uid'][0],
        title = x['word'][0] if 'word' in x.keys() else '',
        ref = None,
        author = None,
        snippet = snippet,
        page_number = None,
        score = score,
        rank = rank,
    )

def suttas_index_schema(tokenizer_name: str = 'en_stem_fold') -> tantivy.Schema:
    builder = tantivy.SchemaBuilder()
    builder.add_text_field("index_key", stored=True)
    builder.add_integer_field("db_id", stored=True)
    builder.add_text_field("schema_name", stored=True)
    builder.add_text_field("uid", stored=True)
    builder.add_text_field("language", stored=True)
    builder.add_text_field("source_uid", stored=True)
    builder.add_text_field("ref", stored=True)
    builder.add_text_field("title", stored=True, tokenizer_name=tokenizer_name)
    builder.add_text_field("title_pali", stored=True)
    builder.add_text_field("title_trans", stored=True, tokenizer_name=tokenizer_name)
    builder.add_text_field("content", stored=True, tokenizer_name=tokenizer_name)

    schema = builder.build()

    return schema

def dict_words_index_schema(tokenizer_name: str = 'en_stem_fold') -> tantivy.Schema:
    builder = tantivy.SchemaBuilder()
    builder.add_text_field("index_key", stored=True)
    builder.add_integer_field("db_id", stored=True)
    builder.add_text_field("schema_name", stored=True)
    builder.add_text_field("uid", stored=True)
    builder.add_text_field("language", stored=True)
    builder.add_text_field("source_uid", stored=True)
    builder.add_text_field("word", stored=True, tokenizer_name=tokenizer_name)
    builder.add_text_field("synonyms", stored=True, tokenizer_name=tokenizer_name)
    builder.add_text_field("content", stored=True, tokenizer_name=tokenizer_name)

    schema = builder.build()

    return schema

class TantivySearchQuery:
    def __init__(self, ix: tantivy.Index, page_len: int):
        self.ix = ix
        self.searcher = ix.searcher()
        self.all_results: tantivy.SearchResult
        # FIXME rename to count
        self.hits: int = 0
        self.page_len = page_len

        # FIXME quick fix to replace whoosh ix.indexname
        self.index_name = 'suttas'

        self.suttas_schema = suttas_index_schema()

    def _plain_snippet_from_doc(self, doc: tantivy.Document) -> str:
        if 'content' not in doc.keys():
            return ''

        s = doc['content'][0:500]

        if 'title' in doc.keys() and 'ref' in doc.keys():
            s = search_compact_plain_snippet(s, doc['title'], doc['ref'])
        else:
            s = search_compact_plain_snippet(s)
        s = search_oneline(s).strip()
        snippet = s[0:250] + ' ...'

        return snippet

    def _result_with_snippet(self, hit: TantivyHit, highlight: bool = True) -> SearchResult:
        (n, doc_address) = hit
        doc = self.searcher.doc(doc_address)

        if highlight:
            snippet = self.snippet_generator.snippet_from_doc(doc)
            snippet_text = snippet.to_html() \
                                  .replace("<b>", "<span class='match'>") \
                                  .replace("</b>", "</span>")

        else:
            snippet_text = self._plain_snippet_from_doc(doc)

        score = None
        rank = None
        if isinstance(n, float):
            score = n
        elif isinstance(n, int):
            rank = n

        if 'title' in doc.keys():
            return tantivy_sutta_doc_to_search_result(doc, snippet_text, score, rank)

        else:
            return tantivy_dict_word_doc_to_search_result(doc, snippet_text, score, rank)

    def _result_with_snippet_highlight(self, hit: TantivyHit) -> SearchResult:
        return self._result_with_snippet(hit, highlight=True)

    def _result_with_snippet_no_highlight(self, hit: TantivyHit) -> SearchResult:
        return self._result_with_snippet(hit, highlight=False)

    def _search_fields(self, query_text: str, fields: List[str]) -> tantivy.SearchResult:
        self.ix.reload()

        parsed_query = self.ix.parse_query(query_text, fields)

        self.snippet_generator = tantivy.SnippetGenerator \
                                        .create(self.searcher,
                                                parsed_query,
                                                self.suttas_schema,
                                                'content')

        self.snippet_generator.set_max_num_chars(200)

        return self.searcher.search(parsed_query)

    def highlighted_results_page(self, page_num: int) -> List[SearchResult]:
        logger.info(f"app.db.search_tantivy:: highlighted_results_page({page_num})")
        page_start = page_num * self.page_len
        page_end = page_start + self.page_len
        return list(map(self._result_with_snippet_highlight, self.all_results.hits[page_start:page_end]))

    def new_query(self,
                  query_text: str,
                  disabled_labels: Optional[Labels] = None,
                  only_lang: Optional[str] = None,
                  only_source: Optional[str] = None):
        logger.info("TantivySearchQuery::new_query()")

        if 'uid:' not in query_text:
            # Replace user input sutta refs such as 'SN 56.11' with query language
            matches = re.finditer(RE_ALL_BOOK_SUTTA_REF, query_text)
            for m in matches:
                nikaya = m.group(1).lower()
                number = m.group(2)
                query_text = query_text.replace(m.group(0), f"uid:{nikaya}{number}")

        self.all_results = self._search_fields(query_text, ['content'])

        # FIXME filter for only_lang, only_source, disabled_labels

        if self.all_results.count is None:
            self.hits = 0
        else:
            self.hits = self.all_results.count

    def get_all_results(self, highlight: bool = False) -> List[SearchResult]:
        if highlight:
            return list(map(self._result_with_snippet_highlight, self.all_results.hits))
        else:
            return list(map(self._result_with_snippet_no_highlight, self.all_results.hits))


class TantivySearchIndexed:
    suttas_index: tantivy.Index
    dict_words_index: tantivy.Index
    suttas_lang_index: Dict[str, tantivy.Index] = dict()

    def __init__(self):
        self.open_all()

    def has_empty_index(self) -> bool:
        # FIXME
        return False

    def index_all(self, db_session: Session, only_if_empty: bool = False):
        logger.info("Indexing suttas ...")

        suttas: List[USutta] = db_session.query(Am.Sutta).all()

        self.index_suttas(self.suttas_index, DbSchemaName.AppData.value, suttas)

        suttas: List[USutta] = db_session.query(Um.Sutta).all()

        self.index_suttas(self.suttas_index, DbSchemaName.UserData.value, suttas)

    def index_all_suttas_lang(self, db_session: Session, lang: str, only_if_empty: bool = False):
        pass

    def get_suttas_lang_index_names(self) -> set[str]:
        return set([])

    def open_all(self):
        if not INDEX_DIR.exists():
            INDEX_DIR.mkdir()

        self.suttas_index = tantivy.Index(
            suttas_index_schema('en_stem_fold'),
            path=str(INDEX_DIR),
            reuse=True,
        )

    def clear_all(self):
        # FIXME
        pass

    def close_all(self):
        # FIXME
        pass

    def create_all(self, remove_if_exists: bool = True):
        if remove_if_exists and INDEX_DIR.exists():
            shutil.rmtree(INDEX_DIR)

        if not INDEX_DIR.exists():
            INDEX_DIR.mkdir()

        self.open_all()

    def open_or_create_index(self, index_name: str, index_schema: Any) -> Optional[tantivy.Index]:
        if not INDEX_DIR.exists():
            INDEX_DIR.mkdir(exist_ok=True)

        suttas_index = tantivy.Index(
            suttas_index_schema('en_stem_fold'),
            path=str(INDEX_DIR),
            reuse=True,
        )

        # FIXME

        return suttas_index

    def index_suttas(self, ix: tantivy.Index, schema_name: str, suttas: List[USutta]):
        logger.info(f"index_suttas() len: {len(suttas)}")

        writer = ix.writer(256*1024*1024)

        total = len(suttas)
        for idx, i in enumerate(suttas):
            percent = idx/(total/100)
            logger.info(f"Indexing {percent:.2f}% {idx}/{total}: {i.uid}")

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

            writer.add_document(tantivy.Document(
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
            ))

        logger.info("writer.commit()")
        writer.commit()

    def index_suttas_lang(self, schema_name: str, lang: str, suttas: List[USutta]):
        # FIXME
        pass

    def index_dict_words(self, schema_name: str, words: List[UDictWord]):
        # FIXME
        pass
