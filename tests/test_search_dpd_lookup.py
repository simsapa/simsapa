"""Test Search: DPD Lookup
"""

from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.search.helpers import dpd_lookup

QUERY_TEXT_TEST_CASES = {
    "kammapattā": {"headwords": ["apatta 1.1", "apatta 2.1", "kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 8", "kammī", "patta 1.1", "patta 2.1", "patta 2.2", "patta 2.3", "patta 2.4", "patta 2.5", "patta 3.1", "patta 3.2",]},
    "kammī": {"headwords": ["kammī"]},
    "kammikassa": {"headwords": ["kammika 1", "kammika 2"]},
    "20400": {"headwords": ["kammika 1"]},
    "20400/dpd": {"headwords": ["kammika 1"]},
}

def test_dpd_lookup():
    _, _, db_session = get_db_engine_connection_session()

    for query_text, v in QUERY_TEXT_TEST_CASES.items():
        results = dpd_lookup(db_session, query_text)

        headwords = [i["title"] for i in results]

        assert "; ".join(headwords) == "; ".join(v["headwords"])
