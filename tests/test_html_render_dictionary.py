"""Test HTML Rendering for Dictionary words: DictWord, DpdHeadwords, DpdRoots
"""

from simsapa.app.db import appdata_models as Am
from simsapa.app.db import dpd_models as Dpd

from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.search.dictionary_queries import DictionaryQueries

_, _, db_session = get_db_engine_connection_session()
api_url = 'http://localhost:4848'
dictionary_queries = DictionaryQueries(db_session, api_url)

def test_html_for_dict_words():
    w = db_session.query(Am.DictWord) \
                  .filter(Am.DictWord.uid == "ahosi-kamma/pts") \
                  .first()
    assert(w is not None)

    html = dictionary_queries.words_to_html_page([w,])

    assert("""<div class="word-title flex-item">
        <h1>ahosi-kamma</h1>
    </div>""" in html)

    assert("""ahosikakamma is said to be a kamma inhibited by a more powerful one""" in html)

def test_html_for_pali_words():
    w = db_session.query(Dpd.DpdHeadwords) \
                  .filter(Dpd.DpdHeadwords.uid == "20400/dpd") \
                  .first()
    assert(w is not None)

    html = dictionary_queries.words_to_html_page([w,])

    assert("""<div class="word-title flex-item">
        <h1>kammika 1</h1>
    </div>""" in html)

    assert("""<b>working in; doing; occupied with</b>""" in html)

    # When a single word is rendered, the declension table should be open (no "hidden" class).
    assert("""<div id="declension_kammika_1" class="content ">""" in html)

def test_html_for_pali_roots():
    w = db_session.query(Dpd.DpdRoots) \
                  .filter(Dpd.DpdRoots.uid == "√kā/dpd") \
                  .first()
    assert(w is not None)

    html = dictionary_queries.words_to_html_page([w,])

    assert("""<div class="word-title flex-item">
        <h1>√kā</h1>
    </div>""" in html)

    assert("""root. √kā･3 ya (makes sound) 1""" in html)
