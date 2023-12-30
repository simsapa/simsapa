"""Test Search: Combined
"""

from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.search.helpers import combined_search
from simsapa.app.search.tantivy_index import TantivySearchIndexes
from simsapa.layouts.gui_queries import GuiSearchQueries

# The headwords are sorted for consistent test results.
QUERY_TEXT_TEST_CASES = {
    "20400/dpd": {"words": ["kammika 1.dpd.20400/dpd"]},
    "20400": {"words": ["kammika 1.dpd.20400/dpd"]},
    # "Idha": {"words": ["idha 1", "idha 2", "idha 3"]},
    # "ka": {"headwords": ["√kā", "ka 1.1", "ka 1.2", "ka 2.1", "ka 3.1", "ka 4.1", "kā 1.1", "kā 2.1"]},
    # "kamma": {"headwords": ["kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 6", "kamma 7", "kamma 8"]},
    # "kammapatta": {"headwords": ["apatta 1.1", "apatta 2.1", "kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 8", "kammī", "patta 1.1", "patta 2.1", "patta 2.2", "patta 2.3", "patta 2.4", "patta 2.5", "patta 3.1", "patta 3.2",]},
    # "kammapattā": {"headwords": ["apatta 1.1", "apatta 2.1", "kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 8", "kammī", "patta 1.1", "patta 2.1", "patta 2.2", "patta 2.3", "patta 2.4", "patta 2.5", "patta 3.1", "patta 3.2",]},
    # "kammī": {"headwords": ["kammī"]},
    # "kammikassa": {"headwords": ["kammika 1", "kammika 2"]},
    # "Kīdisī": {"headwords": ["kīdisa"]},
    # "natavedisaṁ": {"headwords": ["na", "tava", "edisa"]},
    # "passasāmī'ti": {"headwords": ["iti", "passasati"]},
    # "passasāmī’ti": {"headwords": ["iti", "passasati"]},
    # "samadhi": {"headwords": ["samādhi 1", "samādhi 2"]},
    # "samādhi": {"headwords": ["samādhi 1", "samādhi 2"]},
    # "ṭhitomhī'ti": {"headwords": ["atthi 1.1", "amha 2.1", "amhā", "amhi", "āsi 2.1", "iti", "ima 1.1", "ṭhita 1", "ṭhita 2", "ṭhita 3", "ṭhita 4", "ṭhita 6", "ṭhita 7"]},
    # "ṭhitomhī’ti": {"headwords": ["atthi 1.1", "amha 2.1", "amhā", "amhi", "āsi 2.1", "iti", "ima 1.1", "ṭhita 1", "ṭhita 2", "ṭhita 3", "ṭhita 4", "ṭhita 6", "ṭhita 7"]},
    # "upacara": {"headwords": ["upacāra 1", "upacāra 2", "upacāra 3", "upacāra 4", "upacāra 5", "upacāra 6"]},
    # "upacāra": {"headwords": ["upacāra 1", "upacāra 2", "upacāra 3", "upacāra 4", "upacāra 5", "upacāra 6"]},
    # "upacārasamādhi": {"headwords": ["upacārasamādhi"]},
    # "upacāra samādhi": {"headwords": ["upacārasamādhi"]},
    # "upacara samadhi": {"headwords": ["upacārasamādhi"]},
}

def test_combined_search():
    _, _, db_session = get_db_engine_connection_session()

    search_indexes = TantivySearchIndexes(db_session)

    def get_search_indexes() -> TantivySearchIndexes:
        return search_indexes

    api_url = 'http://localhost:4848'

    queries = GuiSearchQueries(db_session,
                               get_search_indexes,
                               api_url)

    for query_text, v in QUERY_TEXT_TEST_CASES.items():
        api_results = combined_search(
            queries = queries,
            query_text = query_text,
            lang = 'en',
            page_num = 0,
            do_pali_sort = True,
        )

        headwords = [f"{i['title']}.{i['schema_name']}.{i['uid']}" for i in api_results['results']]

        assert "; ".join(headwords) == "; ".join(v["words"])
