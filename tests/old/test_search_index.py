"""Basic search index test cases
"""

from simsapa import INDEX_DIR
from .support.helpers import get_app_data

def test_index_created_with_app_data():
    app_data = get_app_data()

    res = app_data.search_indexed.search_suttas_indexed('dhamma')

    assert len(res) > 0
