"""Test Search: ContainsMatch
"""

from datetime import datetime

from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.search.tantivy_index import TantivySearchIndexes
from simsapa.app.types import SearchMode, SearchParams, SearchArea
from simsapa.app.search.query_task import SearchQueryTask

def get_contains_params() -> SearchParams:
    return SearchParams (
        mode = SearchMode.ContainsMatch,
        page_len = None,
        lang = "en",
        lang_include = True,
        source = None,
        source_include = True,
        enable_regex = False,
        fuzzy_distance = 0,
    )

def test_sutta_search_contains_match():
    _, _, db_session = get_db_engine_connection_session()
    search_indexes = TantivySearchIndexes(db_session)

    params = get_contains_params()
    query = "satipaṭṭhāna"
    last_query_time = datetime.now()

    query_task = SearchQueryTask(
        "en",
        search_indexes.suttas_lang_index["en"],
        query,
        last_query_time,
        params,
        SearchArea.Suttas,
    )

    results = query_task.results_page(0)

    assert results[0]["uid"] == "mil5.3.7/en/tw_rhysdavids"
    assert results[0]["snippet"].startswith("... e with the rules of <span class='match'>satipaṭṭhāna</span>")
    assert results[0]["snippet"].endswith("law of property to carry on the traditions o ...")
