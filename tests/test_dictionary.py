from sqlalchemy import func
from sqlalchemy.orm import joinedload

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from .support.helpers import get_app_data

import unittest


class DictionaryTestSuite(unittest.TestCase):
    """Basic Dictionary test cases."""

    def test_count_userdata_dict_words(self):
        app_data = get_app_data()
        count = app_data.db_session.query(func.count(Um.DictWord.id)).scalar()

        self.assertEqual(count, 1)

    def test_dict_word_examples(self):
        app_data = get_app_data()

        results = app_data.db_session \
                          .query(Am.DictWord) \
                          .options(joinedload(Am.DictWord.examples)) \
                          .all()

        text = results[0].examples[0].text_html

        self.assertEqual(text, "Dhammo gambhīro duradhigamā bhogā")

    def test_dict_word_dictionary(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.DictWord) \
            .filter(Um.DictWord.word == 'dhamma') \
            .options(joinedload(Um.DictWord.examples)) \
            .all()

        dict = results[0].dictionary

        self.assertEqual(dict.title, "Personal Study")
