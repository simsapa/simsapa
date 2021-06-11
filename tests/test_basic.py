import os
from pathlib import Path
import json

from sqlalchemy import func
from sqlalchemy.orm import joinedload  # type: ignore

from simsapa.app.types import AppData
from simsapa.app.db import appdata_models as am
from simsapa.app.db import userdata_models as um

import unittest


def get_app_data():
    tests_data_dir = Path(os.path.dirname(__file__)).absolute().joinpath('data')
    app_db_path = tests_data_dir.joinpath('appdata.sqlite3')
    user_db_path = tests_data_dir.joinpath('userdata.sqlite3')

    app_data = AppData(app_db_path=app_db_path, user_db_path=user_db_path)

    return app_data


class BasicDatabaseTestSuite(unittest.TestCase):
    """Basic database test cases."""

    def test_count_appdata_suttas(self):
        app_data = get_app_data()
        count = app_data.db_session.query(func.count(am.Sutta.id)).scalar()

        self.assertEqual(count, 4)

    def test_count_userdata_dict_words(self):
        app_data = get_app_data()
        count = app_data.db_session.query(func.count(um.DictWord.id)).scalar()

        self.assertEqual(count, 1)

    def test_dict_word_examples(self):
        app_data = get_app_data()

        results = app_data.db_session \
                          .query(am.DictWord) \
                          .options(joinedload(am.DictWord.examples)) \
                          .all()

        text = results[0].examples[0].text_html

        self.assertEqual(text, "Dhammo gambhīro duradhigamā bhogā")

    def test_dn1_memos(self):
        app_data = get_app_data()

        results = app_data.db_session \
                          .query(um.MemoAssociation.memo_id) \
                          .filter(
                              um.MemoAssociation.associated_table == 'appdata.suttas',
                              um.MemoAssociation.associated_id == 1) \
                          .all()

        memo_ids = list(map(lambda x: x[0], results))

        memos_data = app_data.db_session \
                             .query(um.Memo) \
                             .filter(um.Memo.id.in_(memo_ids)) \
                             .all()

        def get_front(x):
            d = json.loads(x.fields_json)
            return d['Front']

        memos_front = ' '.join(map(get_front, memos_data))

        self.assertEqual(memos_front, 'Who is criticizing the Buddha? Who is praising the Buddha?')


class PitakaGroupsTestSuite(unittest.TestCase):
    """Sutta groups test cases."""

    def test_pitaka_group(self):
        app_data = get_app_data()

        count = app_data.db_session \
                        .query(am.Sutta) \
                        .filter(am.Sutta.group_path.like('/sutta-pitaka/digha-nikaya/%')) \
                        .count()

        self.assertEqual(count, 2)


if __name__ == '__main__':
    unittest.main()
