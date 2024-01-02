"""Basic Sutta test cases
"""

from sqlalchemy.sql import func, text

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import userdata_models as Um

from simsapa.app.graph import sutta_nodes_and_edges

from .support.helpers import get_app_data

def test_count_appdata_suttas():
    app_data = get_app_data()
    count = app_data.db_session.query(func.count(Am.Sutta.id)).scalar()

    assert count == 7

def test_pitaka_group():
    app_data = get_app_data()

    count = app_data.db_session \
                    .query(Am.Sutta) \
                    .filter(Am.Sutta.group_path.like('/sutta pitaka/digha nikaya/%')) \
                    .count()

    assert count == 3

def test_fts_sutta_search():
    app_data = get_app_data()

    query = "evaṃ me sutaṃ"
    res = app_data.db_session.execute(text(f"""
SELECT
    suttas.id,
    suttas.title,
    snippet(fts_suttas, 0, '<b class="highlight">', '</b>', ' ... ', 64) AS content_snippet,
    suttas.content_html
FROM fts_suttas
INNER JOIN suttas ON suttas.id = fts_suttas.rowid
WHERE fts_suttas MATCH '{query}'
ORDER BY rank;""")).all()

    assert '<b class="highlight">Evaṃ</b>' in res[0].content_snippet

def test_sutta_has_annotations():
    app_data = get_app_data()

    results = app_data.db_session \
        .query(Um.AnnotationAssociation.annotation_id) \
        .filter(
            Um.AnnotationAssociation.associated_table == 'appdata.suttas',
            Um.AnnotationAssociation.associated_id == 1) \
        .all()

    annotation_ids = list(map(lambda x: x[0], results))

    annotations_data = app_data.db_session \
        .query(Um.Annotation) \
        .filter(Um.Annotation.id.in_(annotation_ids)) \
        .all()

    text = annotations_data[0].text

    assert text == "ekaṃ samayaṃ bhagavā antarā ca rājagahaṃ antarā ca nāḷandaṃ"

def test_sutta_graph_is_the_same():
    app_data = get_app_data()

    dn1 = app_data.db_session.query(Am.Sutta).filter(Am.Sutta.uid == 'dn1').first()

    mn1 = app_data.db_session.query(Am.Sutta).filter(Am.Sutta.uid == 'mn1').first()

    (nodes, edges) = sutta_nodes_and_edges(app_data, dn1, 3)
    dn1_result = f"{nodes}\n{edges}"

    (nodes, edges) = sutta_nodes_and_edges(app_data, mn1, 3)
    mn1_result = f"{nodes}\n{edges}"

    assert dn1_result == mn1_result
