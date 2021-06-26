from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from .support.helpers import get_app_data

import unittest


class LinkTestSuite(unittest.TestCase):
    """Basic Link test cases."""

    def test_links_sutta_to_sutta(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.Link) \
            .filter(
                Um.Link.from_table == 'appdata.suttas',
                Um.Link.to_table == 'appdata.suttas') \
            .all()

        label = results[0].label

        self.assertEqual(label, 'insight')

    def test_links_document_to_sutta(self):
        app_data = get_app_data()

        link = app_data.db_session \
            .query(Um.Link) \
            .filter(
                Um.Link.from_table == 'userdata.documents',
                Um.Link.from_id == 1,
                Um.Link.to_table == 'appdata.suttas',
                Um.Link.to_id == 1) \
            .first()

        sutta = app_data.db_session \
            .query(Am.Sutta) \
            .filter(Am.Sutta.id == link.to_id) \
            .first()

        self.assertEqual(sutta.sutta_ref, 'DN 1')
