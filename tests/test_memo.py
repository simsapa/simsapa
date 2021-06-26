import json

from simsapa.app.db import userdata_models as Um

from .support.helpers import get_app_data

import unittest


class MemoTestSuite(unittest.TestCase):
    """Basic Memo test cases."""

    def test_dn1_memo_front_back(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.MemoAssociation.memo_id) \
            .filter(
                Um.MemoAssociation.associated_table == 'appdata.suttas',
                Um.MemoAssociation.associated_id == 1) \
            .all()

        memo_ids = list(map(lambda x: x[0], results))

        memos_data = app_data.db_session \
            .query(Um.Memo) \
            .filter(Um.Memo.id.in_(memo_ids)) \
            .all()

        def get_front(x):
            d = json.loads(x.fields_json)
            return d['Front']

        memos_front = ' '.join(map(get_front, memos_data))

        self.assertEqual(memos_front, 'Who is criticizing the Buddha? Who is praising the Buddha?')

    def test_dn1_memo_deck(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.MemoAssociation.memo_id) \
            .filter(
                Um.MemoAssociation.associated_table == 'appdata.suttas',
                Um.MemoAssociation.associated_id == 1) \
            .all()

        memo_ids = list(map(lambda x: x[0], results))

        memos_data = app_data.db_session \
            .query(Um.Memo) \
            .filter(Um.Memo.id.in_(memo_ids)) \
            .all()

        deck = memos_data[0].deck

        self.assertEqual(deck.name, 'Simsapa')

    def test_dn1_memo_has_tags(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.MemoAssociation.memo_id) \
            .filter(
                Um.MemoAssociation.associated_table == 'appdata.suttas',
                Um.MemoAssociation.associated_id == 1) \
            .all()

        memo_ids = list(map(lambda x: x[0], results))

        memos_data = app_data.db_session \
            .query(Um.Memo) \
            .filter(Um.Memo.id.in_(memo_ids)) \
            .all()

        memo = memos_data[0]

        name = memo.tags[0].name

        self.assertEqual(name, 'Suppiya')

    def test_tag_has_memos(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.Tag) \
            .filter(Um.Tag.name == 'Suppiya') \
            .all()

        memo = results[0].memos[0]

        fields = json.loads(memo.fields_json)

        self.assertEqual(fields['Front'], "Who is criticizing the Buddha?")
