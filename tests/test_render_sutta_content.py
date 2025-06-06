"""Test HTML Rendering for Suttas
"""

from simsapa import SIMSAPA_API_DEFAULT_PORT
from simsapa.app.db import appdata_models as Am
from simsapa.app.app_data import AppData
from simsapa.app.export_helpers import render_sutta_content

app_data = AppData(api_port=SIMSAPA_API_DEFAULT_PORT)

def test_html_for_pali():
    sutta = app_data.db_session.query(Am.Sutta) \
                               .filter(Am.Sutta.uid == "mn2/pli/ms") \
                               .first()
    assert(sutta is not None)

    html = render_sutta_content(app_data, sutta, None)

    assert("""<div class="suttacentral bilara-text">""" in html)

    assert("""<header><ul><li class='division'><span data-tmpl-key='mn2:0.1'>Majjhima Nikāya 2 </span></li></ul>""" in html)

    assert("""<p><span data-tmpl-key='mn2:2.1'>“sabbāsavasaṁvarapariyāyaṁ vo, bhikkhave, desessāmi. </span>""" in html)

def test_line_by_line_with_variants():
    sutta = app_data.db_session.query(Am.Sutta) \
                               .filter(Am.Sutta.uid == "sn1.61/en/sujato") \
                               .first()
    assert(sutta is not None)

    html = render_sutta_content(app_data, sutta, None)

    # with open("sn1.61_en_sujato.html", "w", encoding="utf-8") as f:
    #     f.write(html)

    expected_html = ""

    with open("tests/data/sn1.61_en_sujato.html", "r", encoding="utf-8") as f:
        expected_html = f.read()

    assert(html == expected_html)
