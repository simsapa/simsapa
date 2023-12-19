"""Test Search: DPD Lookup
"""

from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.search.helpers import dpd_lookup

# The headwords are sorted.
QUERY_TEXT_TEST_CASES = {
    "kammapattā": {"headwords": ["apatta 1.1", "apatta 2.1", "kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 8", "kammī", "patta 1.1", "patta 2.1", "patta 2.2", "patta 2.3", "patta 2.4", "patta 2.5", "patta 3.1", "patta 3.2",]},
    "kammī": {"headwords": ["kammī"]},
    "kammikassa": {"headwords": ["kammika 1", "kammika 2"]},
    "20400": {"headwords": ["kammika 1"]},
    "20400/dpd": {"headwords": ["kammika 1"]},
    "Idha": {"headwords": ["idha 1", "idha 2", "idha 3"]},
    "Kīdisī": {"headwords": ["kīdisa"]},
    "passasāmī’ti": {"headwords": ["iti", "passasati"]},
    "passasāmī'ti": {"headwords": ["iti", "passasati"]},
    "ṭhitomhī’ti": {"headwords": ["atthi 1.1", "amha 2.1", "amhā", "amhi", "āsi 2.1", "iti", "ima 1.1", "ṭhita 1", "ṭhita 2", "ṭhita 3", "ṭhita 4", "ṭhita 6", "ṭhita 7"]},
    "ṭhitomhī'ti": {"headwords": ["atthi 1.1", "amha 2.1", "amhā", "amhi", "āsi 2.1", "iti", "ima 1.1", "ṭhita 1", "ṭhita 2", "ṭhita 3", "ṭhita 4", "ṭhita 6", "ṭhita 7"]},
}

def test_dpd_lookup():
    _, _, db_session = get_db_engine_connection_session()

    for query_text, v in QUERY_TEXT_TEST_CASES.items():
        results = dpd_lookup(db_session, query_text, do_pali_sort=True)

        headwords = [i["title"] for i in results]

        assert "; ".join(headwords) == "; ".join(v["headwords"])
