#!/usr/bin/env python3

import os
import sys
from pathlib import Path
from typing import List
from dotenv import load_dotenv

from bs4 import BeautifulSoup

import tantivy

from simsapa import DbSchemaName, logger, INDEX_DIR
from simsapa.app.db import appdata_models as Am
from simsapa.app.helpers import create_app_dirs
from simsapa.app.helpers import compact_rich_text, compact_plain_text

from simsapa.app.db.search_tantivy import suttas_index_schema

import helpers
from simsapa.app.types import USutta

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

BOOTSTRAP_ASSETS_DIR = Path(s)
SC_DATA_DIR = BOOTSTRAP_ASSETS_DIR.joinpath("sc-data")

for p in [BOOTSTRAP_ASSETS_DIR, SC_DATA_DIR]:
    if not p.exists():
        logger.error(f"Missing folder: {p}")
        sys.exit(1)

s = os.getenv('BOOTSTRAP_LIMIT')
if s is None or s == "":
    BOOTSTRAP_LIMIT = None
else:
    BOOTSTRAP_LIMIT = int(s)

def search(ix: tantivy.Index, schema: tantivy.Schema, query_text: str):
    ix.reload()
    searcher = ix.searcher()

    query = ix.parse_query(query_text, ["title", "content"])

    snippet_generator = tantivy.SnippetGenerator.create(searcher, query, schema, 'content')
    # snippet_generator.set_max_num_chars(100)

    for (_, doc_address) in searcher.search(query).hits:
        doc = searcher.doc(doc_address)
        print(f"{doc['uid']} {doc['ref']} {doc['title']}")

        snippet = snippet_generator.snippet_from_doc(doc)

        print(snippet.to_html())

def main():
    create_app_dirs()

    appdata_db_path = BOOTSTRAP_ASSETS_DIR.joinpath("dist").joinpath("appdata.sqlite3")
    appdata_db = helpers.get_simsapa_db(appdata_db_path, DbSchemaName.AppData, remove_if_exists = False)

    index_schema = suttas_index_schema('en_stem_fold')
    ix = tantivy.Index(index_schema, path=str(INDEX_DIR), reuse=True)

    suttas: List[USutta] = []

    # limit = BOOTSTRAP_LIMIT
    # limit = 1000
    lang = 'en'

    suttas = appdata_db.query(Am.Sutta) \
                        .filter(Am.Sutta.language == lang) \
                        .all()
    if suttas is None:
        return

    # TODO option to re-create the index, otherwise use as is
    # index_suttas(ix, DbSchemaName.AppData.value, suttas)

    search(ix, index_schema, "searching for heartwood")

    # TODO detect bad query
    # ValueError: Field does not exists: 'cont'
    # search(ix, index_schema, "cont:heartwood")

if __name__ == "__main__":
    main()
