"""Test Pāli Stemmer
"""

from simsapa.app.pali_stemmer import pali_stem

STEMMING_TEST_CASES = {
    "dukkhā": "dukkh",
    "dukkho": "dukkh",
    "dukkhena": "dukkh",
    "dukkhānaṁ": "dukkh",
    "dukkhasmiṁ": "dukkh",
    "samayaṁ": "samay",
    "satiṁ": "sat",
    "vineyya": "vineyya",
    "uddeso": "uddes",
    "bhūmīnaṁ": "bhūm",
    "kumārīnaṁ": "kumār",
    "kīdisī": "kīdis",
}

def test_pali_stemmer():
    for inflected_form, stem in STEMMING_TEST_CASES.items():
        print(inflected_form)
        assert stem == pali_stem(inflected_form)
