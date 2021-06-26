from simsapa.app.db import userdata_models as Um

from .support.helpers import get_app_data

import unittest


class DocumentTestSuite(unittest.TestCase):
    """Basic Document test cases."""

    def test_user_documents(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.Document) \
            .filter(Um.Document.filepath.like("%wordofbuddha.pdf")) \
            .all()

        self.assertEqual(len(results), 1)

    def test_document_has_tags(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.Document) \
            .filter(Um.Document.filepath.like("%wordofbuddha.pdf")) \
            .all()

        name = results[0].tags[0].name

        self.assertEqual(name, 'classic')

    def test_tag_has_documents(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.Tag) \
            .filter(Um.Tag.name == 'classic') \
            .all()

        title = results[0].documents[0].title

        self.assertEqual(title, 'Word of the Buddha')

    def test_document_has_annotations(self):
        app_data = get_app_data()

        results = app_data.db_session \
            .query(Um.AnnotationAssociation.annotation_id) \
            .filter(
                Um.AnnotationAssociation.associated_table == 'userdata.documents',
                Um.AnnotationAssociation.associated_id == 1) \
            .all()

        annotation_ids = list(map(lambda x: x[0], results))

        annotations_data = app_data.db_session \
            .query(Um.Annotation) \
            .filter(Um.Annotation.id.in_(annotation_ids)) \
            .all()

        text = annotations_data[0].text

        self.assertEqual(text, 'the deliverance from it')
