"""Test Search: DPD Lookup
"""

from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.search.helpers import dpd_lookup

# The headwords are sorted for consistent test results.
QUERY_TEXT_TEST_CASES = {
    "20400/dpd": {"words": ["kammika 1"]},
    "20400": {"words": ["kammika 1"]},
    "assa": {"words": ["assa 1.1", "assa 1.2", "assa 2.1", "assa 3.1", "assa 4.1", "assa 4.2", "assa 5.1", "assati 2.1", "assā 1.1", "assā 1.2", "assā 2.1", "ima 1.1"]},
    "Idha": {"words": ["idha 1", "idha 2", "idha 3"]},
    "kammapatta": {"words": ["apatta 1.1", "apatta 2.1", "abhāva 1", "abhāva 2", "abhāva 3", "kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 6", "kamma 7", "kamma 8", "patta 1.1", "patta 2.1", "patta 2.2", "patta 2.3", "patta 2.4", "patta 2.5", "patta 3.1", "patta 3.2", "patti 1.1", "patti 1.2", "patti 2.1", "pattī 1.1", "bhāva 1", "bhāva 2", "bhāva 3", "bhāva 4"]},
    "kammapattā": {"words": ["apatta 1.1", "apatta 2.1", "kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 6", "kamma 7", "kamma 8", "kammī", "patta 1.1", "patta 2.1", "patta 2.2", "patta 2.3", "patta 2.4", "patta 2.5", "patta 3.1", "patta 3.2"]},
    "kamma": {"words": ["kamma 1", "kamma 2", "kamma 3", "kamma 4", "kamma 5", "kamma 6", "kamma 7", "kamma 8"]},
    "kammikassa": {"words": ["kammika 1", "kammika 2"]},
    "kammī": {"words": ["kammī"]},
    "ka": {"words": ["√kā", "ka 1.1", "ka 1.2", "ka 2.1", "ka 3.1", "ka 4.1", "kā 1.1", "kā 2.1"]},
    "Kīdisī": {"words": ["kīdisa"]},
    "natavedisaṁ": {"words": ["edisa", "tava 1", "tava 2", "tvaṁ 1", "na 1", "na 2", "na 3", "na 4"]},
    "passasāmī'ti": {"words": ["iti", "passasati"]},
    "passasāmī’ti": {"words": ["iti", "passasati"]},
    "samadhi": {"words": ["samādhi 1", "samādhi 2"]},
    "samādhi": {"words": ["samādhi 1", "samādhi 2"]},
    "ṭhitomhī'ti": {"words": ["atthi 1.1", "amha 2.1", "amhā", "amhi", "āsi 2.1", "iti", "ima 1.1", "ṭhita 1", "ṭhita 2", "ṭhita 3", "ṭhita 4", "ṭhita 6", "ṭhita 7"]},
    "ṭhitomhī’ti": {"words": ["atthi 1.1", "amha 2.1", "amhā", "amhi", "āsi 2.1", "iti", "ima 1.1", "ṭhita 1", "ṭhita 2", "ṭhita 3", "ṭhita 4", "ṭhita 6", "ṭhita 7"]},
    "upacara samadhi": {"words": ["upacārasamādhi"]},
    "upacāra samādhi": {"words": ["upacārasamādhi"]},
    "upacārasamādhi": {"words": ["upacārasamādhi"]},
    "upacara": {"words": ["upacarati 1", "upacarati 2", "upacari", "upacāra 1", "upacāra 2", "upacāra 3", "upacāra 4", "upacāra 5", "upacāra 6"]},
    "upacāra": {"words": ["upacāra 1", "upacāra 2", "upacāra 3", "upacāra 4", "upacāra 5", "upacāra 6"]},
    "vacchagotta": {"words": ["vacchagotta 1", "vacchagotta 2"]},
}

def test_dpd_lookup():
    _, _, db_session = get_db_engine_connection_session()

    for query_text, v in QUERY_TEXT_TEST_CASES.items():
        results = dpd_lookup(db_session, query_text, do_pali_sort=True)

        headwords = [i["title"] for i in results]

        assert "\n".join(headwords) == "\n".join(v["words"])
