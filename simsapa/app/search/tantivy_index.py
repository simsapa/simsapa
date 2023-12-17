import shutil
from typing import Dict, List, Optional, Union, Tuple
import math

import tantivy

from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from simsapa import DICT_WORDS_INDEX_DIR, INDEX_WRITER_MEMORY_MB, LOG_PERCENT_PROGRESS, SUTTAS_INDEX_DIR, DbSchemaName, logger
from simsapa.app.helpers import compact_rich_text, compact_plain_text, consistent_niggahita, query_text_to_uid_field_query
from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um
from simsapa.app.db import dpd_models as Dpd
from simsapa.app.search.helpers import SearchResult, get_dict_word_languages, get_sutta_languages, is_index_empty, search_compact_plain_snippet, search_oneline
from simsapa.app.types import SearchArea, SearchParams

from simsapa.app.dpd_render import pali_word_index_plaintext

USutta = Union[Am.Sutta, Um.Sutta]
UDictWord = Union[Am.DictWord, Um.DictWord, Dpd.PaliWord]

# A Score(f32) or an Order(u64)
TantivyFruit = Union[float, int]

TantivyHit = Tuple[TantivyFruit, tantivy.DocAddress]

LANG_TO_STEMMER = {
    "ar": "ar_stem_fold",
    "da": "da_stem_fold",
    "de": "de_stem_fold",
    "el": "el_stem_fold",
    "en": "en_stem_fold",
    "es": "es_stem_fold",
    "fi": "fi_stem_fold",
    "fr": "fr_stem_fold",
    "hu": "hu_stem_fold",
    "it": "it_stem_fold",
    "nl": "nl_stem_fold",
    "no": "no_stem_fold",
    "pli": "pli_stem_fold",
    "pt": "pt_stem_fold",
    "ro": "ro_stem_fold",
    "ru": "ru_stem_fold",
    # Use Pali stemming for Sanskrit.
    "san": "pli_stem_fold",
    "sv": "sv_stem_fold",
    "ta": "ta_stem_fold",
    "tr": "tr_stem_fold",
}

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
        nikaya = x['nikaya'] if 'nikaya' in x.keys() else '',
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
        nikaya = None,
        author = None,
        snippet = snippet,
        page_number = None,
        score = score,
        rank = rank,
    )

def suttas_index_schema(content_tokenizer_name: str = "en_stem_fold") -> tantivy.Schema:
    tk = content_tokenizer_name
    builder = tantivy.SchemaBuilder()
    builder.add_integer_field("db_id",    stored=True)
    builder.add_text_field("index_key",   stored=True, tokenizer_name="raw")
    builder.add_text_field("schema_name", stored=True, tokenizer_name="raw")
    builder.add_text_field("uid",         stored=True, tokenizer_name="simple_fold")
    builder.add_text_field("language",    stored=True, tokenizer_name="raw")
    builder.add_text_field("source_uid",  stored=True, tokenizer_name="raw")
    builder.add_text_field("ref",         stored=True, tokenizer_name="simple_fold")
    builder.add_text_field("nikaya",      stored=True, tokenizer_name="raw")
    builder.add_text_field("title",       stored=True, tokenizer_name=tk)
    builder.add_text_field("title_pali",  stored=True, tokenizer_name="pli_stem_fold")
    builder.add_text_field("title_trans", stored=True, tokenizer_name=tk)
    builder.add_text_field("content",     stored=True, tokenizer_name=tk)

    schema = builder.build()

    return schema

def dict_words_index_schema(content_tokenizer_name: str = "en_stem_fold") -> tantivy.Schema:
    tk = content_tokenizer_name
    builder = tantivy.SchemaBuilder()
    builder.add_integer_field("db_id",    stored=True)
    builder.add_text_field("index_key",   stored=True, tokenizer_name="raw")
    builder.add_text_field("schema_name", stored=True, tokenizer_name="raw")
    builder.add_text_field("uid",         stored=True, tokenizer_name="simple_fold")
    builder.add_text_field("language",    stored=True, tokenizer_name="raw")
    builder.add_text_field("source_uid",  stored=True, tokenizer_name="raw")
    builder.add_text_field("dict_type",   stored=True, tokenizer_name="raw")
    builder.add_text_field("word",        stored=True, tokenizer_name="simple_fold")
    builder.add_text_field("synonyms",    stored=True, tokenizer_name="simple_fold")
    builder.add_text_field("content",     stored=True, tokenizer_name=tk)

    schema = builder.build()

    return schema

class TantivySearchQuery:
    ix: tantivy.Index
    searcher: tantivy.Searcher
    page_len: int
    hits_count: Optional[int] = None

    # The un-sanitized, un-extended original user input.
    # Only consistent_niggahita(query_text.strip()) applied.
    query_text_orig: Optional[str] = None

    search_params: SearchParams

    snippet_generator: Optional[tantivy.SnippetGenerator] = None
    parsed_query: Optional[tantivy.Query] = None

    def __init__(self, ix: tantivy.Index, params: SearchParams):
        self.ix = ix
        self.searcher = ix.searcher()
        self.page_len = params['page_len'] or 20
        self.search_params = params

    def is_sutta_index(self) -> bool:
        return ('title' in self.ix.schema.field_names())

    def is_dict_word_index(self) -> bool:
        return ('word' in self.ix.schema.field_names())

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

        if highlight and self.snippet_generator is not None:
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

    def boost_score_by_headword(self, headword: str, dict_words: List[SearchResult]) -> List[SearchResult]:
        logger.info(f"boost_score_by_headword() headword: {headword}")

        boosted_results = []
        starts_with_word = []

        for i in dict_words:
            if i['title'] == headword:

                if i['score'] is None:
                    i['score'] = 0.0
                i['score'] += 1000.0

                boosted_results.append(i)

            elif i['title'].startswith(headword):

                if i['score'] is None:
                    i['score'] = 0.0
                i['score'] += 100.0

                starts_with_word.append(i)

            else:
                boosted_results.append(i)

        starts_with_word.sort(key = lambda x: x['title'], reverse = True)
        for idx, i in enumerate(starts_with_word):
            i['score'] += idx*10.0

        boosted_results.extend(starts_with_word)

        return boosted_results

    def highlighted_results_page(self, page_num: int) -> List[SearchResult]:
        logger.info(f"TantivySearchQuery::highlighted_results_page({page_num})")

        if page_num < 0:
            page_num = 0

        if self.parsed_query is None:
            return []

        p = self.search_params

        if p['source'] is not None and (p['enable_regex'] or p['fuzzy_distance'] > 0):

            total_hits_count = 0

            filtered_page_start_offset = 0
            # Take more results to save iterations.
            filtered_page_len = self.page_len*10
            filtered_page_num = 0

            results = []
            filtered_results = []

            while True:

                tantivy_results = self.searcher.search(
                    query = self.parsed_query,
                    limit = filtered_page_len,
                    count = True,
                    order_by_field = None,
                    offset = filtered_page_start_offset,
                )
                # It will be the same total count at every iteration.
                total_hits_count = tantivy_results.count
                if total_hits_count is None or total_hits_count == 0:
                    break

                # Filter the results

                r = list(map(self._result_with_snippet_highlight, tantivy_results.hits))

                if p['source_include']:
                    filtered_results.extend([i for i in r if i['source_uid'] == p['source']])
                else:
                    filtered_results.extend([i for i in r if i['source_uid'] != p['source']])


                # Stop if the filtered results fill the requested results page.
                # Filtered results are filled progressively up to the requested results page.
                # E.g. results page 2 is filled when filtered results have 2*page_len items.
                #
                # Unless there are no more items to filter, i.e. no next page.
                if len(filtered_results) >= page_num * self.page_len \
                   or filtered_page_start_offset >= total_hits_count:

                    # The results are slice which is requested results page.
                    res_start = page_num * self.page_len
                    res_end = (page_num+1) * self.page_len

                    results = filtered_results[res_start:res_end]

                    if filtered_page_len >= total_hits_count:
                        self.hits_count = len(filtered_results)

                    else:
                        # Set hits_count to None because we don't know the filtered count
                        # unless we filter all the results pages.
                        self.hits_count = None

                    break

                else:
                    filtered_page_num += 1
                    filtered_page_start_offset += filtered_page_num * filtered_page_len

        else:
            page_start_offset = page_num * self.page_len

            tantivy_results = self.searcher.search(
                query = self.parsed_query,
                limit = self.page_len,
                count = True,
                # Ordering only works for u64 integer fields.
                order_by_field = None,
                offset = page_start_offset,
            )

            self.hits_count = tantivy_results.count

            results = list(map(self._result_with_snippet_highlight, tantivy_results.hits))

        if self.is_sutta_index():
            return results

        else:
            # Raise dictionary headword matches:
            # - exact matches: kamma, kamma 01, kamma 02
            # - starts with the word: kammaṁ, kammena

            # If the original user input contains query terms, leave the scoring
            # as returned by tantivy.

            if self.query_text_orig is None:
                raise Exception("query_text_orig is None")

            has_terms = any([c in self.query_text_orig for c in ['+', '-', ':']])
            if has_terms:
                return results

            boosted_results = self.boost_score_by_headword(self.query_text_orig, results)

            return boosted_results

    def new_query(self,
                  query_text: str,
                  source: Optional[str] = None,
                  source_include = True,
                  enable_regex = False,
                  fuzzy_distance = 0):
        logger.info("TantivySearchQuery::new_query()")

        self.ix.reload()

        self.query_text_orig = consistent_niggahita(query_text.strip())

        query_string = sanitize_user_input(self.query_text_orig)

        if 'uid:' not in query_string \
            and 'ref:' not in query_string:
            query_string = query_text_to_uid_field_query(query_string)

        # At this point query_string is either:
        # - the user entered a sutta ref, now converted to a uid:text expression
        # - the user entered a query expression
        # - the user entered several words, e.g. a quote
        # - the user entered a single word

        # Determine if the query was a single word without a query expression.
        marks = [' ', '"', "'", '+', ':']
        a = [i for i in marks if i in query_string]
        is_query_single_word = (len(a) == 0)

        # If it is not a regex or fuzzy query, add source filtering with an
        # expression. It is much faster, and easier to paginate, when tantivy
        # returns the already filtered top-n results, then filtering a longer
        # list in Python.
        if source is not None \
           and not enable_regex \
           and not fuzzy_distance > 0 \
           and (source != "Sources" or source != "Dictionaries"):

            sign = '+' if source_include else '-'
            query_string += f" {sign}source_uid:{source.lower()}"

        if self.is_sutta_index():
            # Only search in content. Title search skews results, i.e. 'buddha'
            # will return English title results first, with Pali results
            # 'buddhassa' not visible on first page.
            #
            # Also, Pali titles are long words, and since the tokenizer matches
            # complete words, it will not match the beginning of titles, e.g.
            # 'silavant' for 'sīlavantasuttaṁ'.
            #
            # If the query started with a single word, convert it to a 'must' expression.
            if is_query_single_word \
               and not enable_regex \
               and not fuzzy_distance > 0:
                query_string = f"+{query_string}"

            logger.info(f"query_string: {query_string}")

            try:
                if enable_regex:
                    self.parsed_query = self.ix.parse_regex_query(query_string, 'content')

                elif fuzzy_distance > 0:
                    self.parsed_query = self.ix.parse_fuzzy_query(query_string, 'content', fuzzy_distance)

                else:
                    self.parsed_query = self.ix.parse_query(query_string, ['content'])

            except Exception as e:
                logger.error(f"TantivySearchQuery: {e}")
                return


        elif self.is_dict_word_index():
            if is_query_single_word \
               and not enable_regex \
               and not fuzzy_distance > 0:
                query_string = f"+{query_string} word:{self.query_text_orig}"

            # A single word query with a dictionary filter now looks like:
            # +vitakkaya +source_uid:dpd word:vitakkaya

            logger.info(f"query_string: {query_string}")

            try:
                if enable_regex:
                    self.parsed_query = self.ix.parse_regex_query(query_string, 'word')

                elif fuzzy_distance > 0:
                    self.parsed_query = self.ix.parse_fuzzy_query(query_string, 'word', fuzzy_distance)

                else:
                    self.parsed_query = self.ix.parse_query(query_string, ['content', 'word'])

            except Exception as e:
                logger.error(f"TantivySearchQuery: {e}")
                return

        else:
            try:
                self.parsed_query = self.ix.parse_query(query_string, ['content'])

            except Exception as e:
                logger.error(f"TantivySearchQuery: {e}")
                return

        self.snippet_generator = tantivy.SnippetGenerator \
                                        .create(self.searcher,
                                                self.parsed_query,
                                                self.ix.schema,
                                                'content')

        self.snippet_generator.set_max_num_chars(200)

    def get_hits_count(self) -> Optional[int]:
        # Request one page, so that hits_count gets set.
        self.highlighted_results_page(0)
        return self.hits_count

    def get_all_results(self) -> List[SearchResult]:
        # If we already queried, and there were no hits.
        hits_count = self.get_hits_count()
        if hits_count is not None and hits_count == 0:
            return []

        # hits_count None does not mean no results. In the case of filtered
        # regex results, we may not know the total hits count.

        if self.hits_count is None:
            page_num = 0
            res = []

            while True:
                r = self.highlighted_results_page(page_num)
                res.extend(r)
                page_num += 1
                if len(r) == 0:
                    break

        else:
            page_num = 0
            total_pages = math.ceil(self.hits_count / self.page_len)

            res = []

            # Example: for a 108 results with page_len 20, there will be 6
            # total_pages, indexed as page_num 0 to 5.
            while page_num < total_pages:
                res.extend(self.highlighted_results_page(page_num))
                page_num += 1

        return res

class TantivySearchIndexes:
    suttas_lang_index: Dict[str, tantivy.Index] = dict()
    dict_words_lang_index: Dict[str, tantivy.Index] = dict()

    def __init__(self, db_session: Session, remove_if_exists: bool = False):
        self.db_session = db_session
        self.open_all(remove_if_exists)

    def test_correct_query_syntax(self, search_area: SearchArea, query_text: str):
        """
        Test if a query_text will parse without syntax errors. Raise the
        ValueError exception to be handled elsewhere.
        """
        logger.info(f"test_correct_query_syntax() search_area: {search_area} query_text: {query_text}")

        query_text = sanitize_user_input(query_text)

        if search_area == SearchArea.Suttas:
            languages = list(self.suttas_lang_index.keys())

        elif search_area == SearchArea.DictWords:
            languages = list(self.dict_words_lang_index.keys())

        else:
            return

        if len(languages) == 0:
            logger.error("No index languages")
            return

        lang = languages[0]

        if search_area == SearchArea.Suttas:
            ix = self.suttas_lang_index[lang]

        elif search_area == SearchArea.DictWords:
            ix = self.dict_words_lang_index[lang]

        else:
            return

        try:
            _ = ix.parse_query(query_text, ['content'])

        except ValueError as e:
            logger.error(f"Incorrect query syntax: {e}")
            raise e

        except Exception as e:
            logger.error(e)

    def has_empty_index(self) -> bool:
        """
        The general suttas and dict_words index must exist. Additional lang
        indexes don't exist if the user hasn't added them.

        If they exist, they must have greater than 0 documents.
        """

        for i in ['en', 'pli']:
            if i not in self.suttas_lang_index.keys():
                return True
            if is_index_empty(self.suttas_lang_index[i]):
                return True

        if 'en' not in self.dict_words_lang_index.keys():
            return True
        if is_index_empty(self.dict_words_lang_index['en']):
            return True

        # If an index exists, it must have greater than 0 documents.

        # FIXME i.exists() requires path param
        # for ix in [i for i in self.suttas_lang_index.values() if i.exists()]:
        #     if ix.searcher().num_docs == 0:
        #         return True

        # for ix in [i for i in self.dict_words_lang_index.values() if i.exists()]:
        #     if ix.searcher().num_docs == 0:
        #         return True

        return False

    def index_all(self, only_if_empty: bool = False):
        logger.info("index_all()")

        for lang in self.suttas_lang_index.keys():
            self.index_all_suttas_lang(lang, only_if_empty)

        for lang in self.dict_words_lang_index.keys():
            self.index_all_dict_words_lang(lang, only_if_empty)

    def index_all_suttas_lang(self, lang: str, only_if_empty: bool = False):
        logger.info(f"index_all_suttas_lang(): {lang}")
        if lang not in self.suttas_lang_index.keys():
            return

        ix = self.suttas_lang_index[lang]

        if (not only_if_empty) or (only_if_empty and is_index_empty(ix)):

            logger.info(f"Indexing {lang} suttas ...")

            suttas: List[USutta] = self.db_session \
                .query(Am.Sutta) \
                .filter(Am.Sutta.language == lang) \
                .all()
            self.index_suttas(ix, DbSchemaName.AppData.value, suttas)

            suttas: List[USutta] = self.db_session \
                .query(Um.Sutta) \
                .filter(Um.Sutta.language == lang) \
                .all()
            self.index_suttas(ix, DbSchemaName.UserData.value, suttas)

    def index_all_dict_words_lang(self, lang: str, only_if_empty: bool = False):
        logger.info(f"index_all_dict_words_lang(): {lang}")
        if lang not in self.dict_words_lang_index.keys():
            return

        ix = self.dict_words_lang_index[lang]

        if (not only_if_empty) or (only_if_empty and is_index_empty(ix)):
            logger.info(f"Indexing {lang} dict_words ...")

            words: List[UDictWord] = self.db_session \
                .query(Am.DictWord) \
                .filter(Am.DictWord.language == lang) \
                .all()
            self.index_dict_words(ix, DbSchemaName.AppData.value, words)

            words: List[UDictWord] = self.db_session \
                .query(Um.DictWord) \
                .filter(Um.DictWord.language == lang) \
                .all()
            self.index_dict_words(ix, DbSchemaName.UserData.value, words)

            if lang == "en":
                words: List[UDictWord] = self.db_session \
                    .query(Dpd.PaliWord) \
                    .all()
                self.index_dict_words(ix, DbSchemaName.Dpd.value, words)

    def open_all(self, remove_if_exists: bool = False):
        for p in [SUTTAS_INDEX_DIR, DICT_WORDS_INDEX_DIR]:
            if remove_if_exists and p.exists():
                shutil.rmtree(p)

            if not p.exists():
                p.mkdir(parents=True)

        sutta_languages = get_sutta_languages(self.db_session)

        for lang in sutta_languages:
            lang_index_path = SUTTAS_INDEX_DIR.joinpath(lang)
            if not lang_index_path.exists():
                lang_index_path.mkdir()

            # Index suttas with the stemmer appropriate for the language text,
            # so that native language queries provide better results in that language.
            if lang in LANG_TO_STEMMER.keys():
                stemmer = LANG_TO_STEMMER[lang]
            else:
                stemmer = "en_stem_fold"

            self.suttas_lang_index[lang] = tantivy.Index(
                suttas_index_schema(stemmer),
                path=str(lang_index_path),
                reuse=True,
            )

        dict_languages = get_dict_word_languages(self.db_session)

        for lang in dict_languages:
            lang_index_path = DICT_WORDS_INDEX_DIR.joinpath(lang)
            if not lang_index_path.exists():
                lang_index_path.mkdir()

            # Always index dictionaries with Pali stemmer, because Pali queries are the most likely.
            self.dict_words_lang_index[lang] = tantivy.Index(
                dict_words_index_schema("pli_stem_fold"),
                path=str(lang_index_path),
                reuse=True,
            )

    def clear_all(self):
        # FIXME
        pass

    def close_all(self):
        # FIXME
        pass

    def index_suttas(self, ix: tantivy.Index, db_schema_name: str, suttas: List[USutta]):
        from bs4 import BeautifulSoup
        logger.info(f"index_suttas() schema: {db_schema_name}, len: {len(suttas)}")

        try:
            writer = ix.writer(INDEX_WRITER_MEMORY_MB*1024*1024)

            total = len(suttas)
            for idx, i in enumerate(suttas):
                percent = idx/(total/100)
                if LOG_PERCENT_PROGRESS:
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

                nikaya = ""
                if i.nikaya is not None:
                    nikaya = i.nikaya

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
                    index_key = f"{db_schema_name}:suttas:{i.uid}",
                    db_id = i.id,
                    schema_name = db_schema_name,
                    uid = i.uid,
                    language = language,
                    source_uid = source_uid,
                    title = title,
                    title_pali = title_pali,
                    title_trans = title_trans,
                    content = content,
                    ref = sutta_ref,
                    nikaya = nikaya,
                ))
                i.indexed_at = func.now() # type: ignore

            self.db_session.commit()
            logger.info("writer.commit()")
            writer.commit()

        except Exception as e:
            logger.error(f"Can't index: {e}")

    def index_suttas_lang(self, db_schema_name: str, lang: str, suttas: List[USutta]):
        logger.info(f"index_suttas_lang() lang: {lang} len: {len(suttas)}")

        if lang in self.suttas_lang_index.keys():
            self.index_suttas(self.suttas_lang_index[lang], db_schema_name, suttas)
        else:
            logger.warn(f"Index is not in suttas_lang_index: {lang}")

    def index_dict_words(self, ix: tantivy.Index, db_schema_name: str, words: List[UDictWord]):
        logger.info(f"index_dict_words() len: {len(words)}")

        try:
            writer = ix.writer(INDEX_WRITER_MEMORY_MB*1024*1024)

            total = len(words)
            for idx, i in enumerate(words):
                percent = idx/(total/100)
                if LOG_PERCENT_PROGRESS:
                    logger.info(f"Indexing {percent:.2f}% {idx}/{total}: {i.uid}")

                if i.source_uid == "dpd":
                    text = pali_word_index_plaintext(i)
                    content = compact_plain_text(text)

                elif i.definition_html is not None and len(i.definition_html.strip()) > 0:
                    # Prefer the html content field if not empty
                    content = compact_rich_text(str(i.definition_html))

                elif i.definition_plain is not None:
                    content = compact_plain_text(str(i.definition_plain))

                else:
                    logger.warn(f"Skipping, no content in {i.word}")
                    continue

                language = ""
                if i.language is not None:
                    language = i.language

                source_uid = ""
                if i.source_uid is not None:
                    source_uid = i.source_uid

                if source_uid == "dpd":
                    dict_type = "sql"
                else:
                    dict_type = "stardict"

                # Add word and synonyms to content field so a single query will match
                if i.word is not None:
                    content = f"{i.word} {content}"

                synonyms = ""
                if i.synonyms is not None:
                    synonyms = i.synonyms
                    content = f"{content} {i.synonyms}"

                writer.add_document(tantivy.Document(
                    index_key = f"{db_schema_name}:dict_words:{i.uid}",
                    db_id = i.id,
                    schema_name = db_schema_name,
                    uid = i.uid,
                    language = language,
                    source_uid = source_uid,
                    dict_type = dict_type,
                    word = i.word,
                    synonyms = synonyms,
                    content = content,
                ))
                i.indexed_at = func.now() # type: ignore

            self.db_session.commit()
            logger.info("writer.commit()")
            writer.commit()

        except Exception as e:
            logger.error(f"Can't index: {e}")

def sanitize_user_input(query_text: str) -> str:
    # In the user input terms for source_uid are easier to type as 'source:'
    query_text = query_text.replace("source:", "source_uid:")

    return query_text
