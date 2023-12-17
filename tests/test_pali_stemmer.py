"""Test Pāli Stemmer
"""

from simsapa.app.pali_stemmer import pali_stem

STEMMING_TEST_CASES = {
    # "dukkhā": "dukkh",
    # "dukkho": "dukkh",
    # "dukkhena": "dukkh",
    # "dukkhānaṃ": "dukkh",
    "dukkhasmiṃ": "dukkh",
}

def test_pali_stemmer():
    for inflected_form, stem in STEMMING_TEST_CASES.items():
        assert stem == pali_stem(inflected_form)
