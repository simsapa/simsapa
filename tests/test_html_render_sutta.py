"""Test HTML Rendering for Suttas
"""

from simsapa import SIMSAPA_API_DEFAULT_PORT
from simsapa.app.db import appdata_models as Am

from simsapa.app.app_data import AppData
from simsapa.app.export_helpers import render_sutta_content

app_data = AppData(actions_manager=None, app_clipboard=None, api_port=SIMSAPA_API_DEFAULT_PORT)

def test_html_for_suttas():
    sutta = app_data.db_session.query(Am.Sutta) \
                               .filter(Am.Sutta.uid == "mn2/pli/ms") \
                               .first()
    assert(sutta is not None)

    html = render_sutta_content(app_data, sutta, None)

    assert("""<div class="suttacentral bilara-text">""" in html)

    assert("""<header><ul><li class='division'><span data-tmpl-key='mn2:0.1'>Majjhima Nikāya 2 </span></li></ul>""" in html)

    assert("""<p><span data-tmpl-key='mn2:2.1'>“sabbāsavasaṁvarapariyāyaṁ vo, bhikkhave, desessāmi. </span>""" in html)
