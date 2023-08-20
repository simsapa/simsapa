"""
Type stub for tantivy-py.

https://github.com/Kapiche/tantivy-py

https://github.com/quickwit-oss/tantivy

https://docs.rs/tantivy/latest/tantivy/index.html

https://tantivy-search.github.io/examples/basic_search.html

https://github.com/microsoft/pyright/blob/main/docs/type-stubs.md
"""

from typing import List, Dict, Optional, Tuple, Union

# from tantivy import *

Field = int

class FieldEntry:
    name: str

class Schema:
    def __init__(self): ...

    def field_names(self) -> List[str]: ...

class Document:
    def __init__(self, **kwargs): ...

    def __getitem__(self, key): ...

    def keys(self) -> List[str]: ...

class SchemaBuilder:
    fields: List[FieldEntry]
    fields_map: Dict[str, Field]

    def __init__(self): ...

    def add_text_field(self,
                       field_name_str: str,
                       stored: bool,
                       tokenizer_name: Optional[str] = None) -> Field: ...

    def add_integer_field(self,
                          field_name_str: str,
                          stored: bool) -> Field: ...

    def build(self) -> Schema: ...

class Query:
    def __init__(self): ...

DocAddress = int
Score = float
Order = int

Fruit = Union[float, int]

class SearchResult:
    hits: List[Tuple[Fruit, DocAddress]] = []
    count: Optional[int] = None

    def __init__(self): ...

class Searcher:
    def __init__(self): ...

    def search(self,
               query: Query,
               top_n: Optional[int] = None,
               order_by_field: Optional[str] = None) -> SearchResult: ...

    def doc(self, doc_address: DocAddress) -> Document: ...

class IndexWriter:
    def __init__(self): ...

    def add_document(self, doc: Document): ...

    def commit(self): ...

class Index:
    schema: Schema

    def __init__(self,
                 schema: Schema,
                 path: Optional[str] = None,
                 reuse: Optional[bool] = None): ...

    def writer(self,
               heap_size: int = 3_000_000,
               num_threads: int = 0) -> IndexWriter: ...

    def reload(self): ...

    def searcher(self) -> Searcher: ...

    def parse_query(self,
                    query: str,
                    fields: Optional[List[str]] = None) -> Query: ...

class Snippet:
    def __init__(self): ...

    def to_html(self) -> str: ...

class SnippetGenerator:
    def __init__(self): ...

    @classmethod
    def create(cls,
               searcher: Searcher,
               query: Query,
               schema: Schema,
               field: str) -> SnippetGenerator: ...

    def snippet_from_doc(self, doc: Document) -> Snippet: ...

    def set_max_num_chars(self, max_num_chars: int): ...
