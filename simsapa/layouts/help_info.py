from PyQt6.QtCore import QUrl
from functools import partial
from PyQt6.QtGui import QDesktopServices, QIcon, QPixmap
from PyQt6.QtWidgets import QMessageBox, QPushButton
from simsapa import SIMSAPA_DIR, SIMSAPA_PACKAGE_DIR

from simsapa.layouts.gui_helpers import get_app_version, get_sys_version

def setup_info_button(layout, parent=None):
    icon = QIcon()
    icon.addPixmap(QPixmap(":/info"))
    btn = QPushButton()
    btn.setFixedSize(40, 40)
    btn.setToolTip("Search query terms")
    btn.setIcon(icon)

    layout.addWidget(btn)

    btn.clicked.connect(partial(show_search_info, parent))

def show_search_info(parent=None):
    box = QMessageBox(parent)
    box.setIcon(QMessageBox.Icon.Information)
    msg = """
<h3>Search Modes in Brief</h3>

<p>Sutta references are matched first. Typing 'mn8', 'sn 56.11', 'iti92' will list those suttas.</p>

<p><b>Fulltext Match:</b> it searches the content for keywords using the query expressions, non-accented letters matching accented ones. (I.e. it makes a query in the tantivy fulltext index and assigns scores to the results.)</p>

<p>Fulltext search is faster and more flexible than the Exact Match (SQL) search, but yields many partial matches.</p>

<p>Fulltext search matches words in full, not in part, e.g. 'bodhi' will not match 'bodhisatta', but words are stemmed and will match declensions, e.g. 'bodhiṁ / bodhiyā'.</p>

<p><b>Exact Match:</b> it searches the content for exact matches. (I.e. it makes SQL queries with <b>%query text%</b>.)

<p>Exact Match (via SQL) matches in part, i.e. if the query is exactly contained somewhere in the content. E.g. 'vitakka' will match 'kāma<b>vitakka</b>ṁ', but there is no stemming, and will not match 'vitakkeyya'.</p>

<p><b>Title / Headword Match:</b> it searches only the titles of suttas or the headwords of dictionary words. (SQL queries)

<h3>Fulltext Match Queries</h3>

<p>
Powered by the <a href="https://github.com/quickwit-oss/tantivy">tantivy</a> fulltext search engine.
Read more in the <a href="https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html">QueryParser</a> docs.
</p>

<p>The words in a query term are related as OR by default. <b>kamma vipāka</b> searches for entries which SHOULD include <b>kamma</b> OR <b>vipāka</b>, but not MUST include.</p>

<p>Prefixing the word with the '+' sign means a term must be included, the '-' signs means it must be excluded.</p>

<p><b>bhikkhu +kamma -vipaka</b> means should include 'bhikkhu', must include 'kamma', must exclude 'vipaka'.</p>

<p>The texts are indexed with Pāli, English, etc. grammar stemmers, so declension forms will also match in the appropriate language.</p>

<p><b>dukkha</b> will match <b>duddkaṁ / dukkhā / dukkhāni / dukkhena</b> etc.,<br>
<b>bhikkhu kamma vipaka</b> will match <b>bhikkhave kammānaṁ vipāko</b>,<br>
<b>monk receives robes</b> will match <b>monks receiving robes</b>.

<p>Latin terms are expanded to include diacritics, <b>patipada</b> will match <b>paṭipadā</b>.</p>

<p>A pharse query is expressed with quote marks: <b>"paṭhamena jhānena"</b></p>.

<p>The query can match parts of the document:<p>

<p>
<b>title:sticks cessation</b> - match 'sticks' in the title, 'cessation' in the content<br>
<b>word:kamma +work</b> - match 'kamma' in the headword, must include 'work' in the content<br>
<b>uid:pj4</b> - the uid should include 'pj4'<br>
<b>upekkhindriyaṁ -source:cst4</b> - match 'upekkhindriyaṁ' in the content, exclude all cst4 documents<br>
<b>calmness +source:thanissaro</b> - match 'calmness' in the content, only in documents by Bh. Thanissaro<br>
<b>+"buddhas of the past" +source:bodhi</b> - must include the phrase 'buddhas of the past', only in documents by Bh. Bodhi<br>
</p>

<h4>Regex search (.* icon)</h4>

<p>This option will parse the query as a regex pattern, but limited to globbing expressions, e.g. <b>.* .+ a* a+</b></p>

<p>The '.' matches any single character, '*' means 'zero or more' or the previous character, '+' means 'one or more'.</p>

<p>
<b>a*vitak.*</b> - the word start with zero or more of 'a', followed by 'vitak', followed by zero or more characters<br>
<b>.*vitak.*</b> - match any word containing 'vitak'<br>
<b>vitak.*</b> - match starting with 'vitak'<br>
</p>

<h4>Fuzzy search (~ icon)</h4>

<p>This option allows matching words which may differ from the query by N number of characters (i.e. the <a href="https://devopedia.org/levenshtein-distance">Levenshtein Distance</a>).</p>

<p>Fuzzy search is not availble together with regex patterns.</p>

<h3>Exact Match Queries</h3>

<p>Joining terms with <b>AND</b> creates filtered SQL queries.</p>

<p><b>kamma vipāka</b>: will match texts which contain the exact expression 'kamma vipāka' (including the space).</p>

<p><b>kamma AND vipāka</b>: will match texts which contain both 'kamma' and 'vipāka', anywhere in the text (including 'kammavipāka').</p>

"""
    box.setText(msg)
    box.setWindowTitle("Search query info")
    box.setStandardButtons(QMessageBox.StandardButton.Ok)

    box.exec()

def show_about(parent=None):
    box = QMessageBox(parent)
    box.setIcon(QMessageBox.Icon.Information)

    app_version_par = ''
    sys_version_par = ''

    ver = get_app_version()
    if ver is not None:
        app_version_par += f"<p>Version {ver}</p>"

    sys_version_par += f"<p>System: {get_sys_version()}</p>"

    dirs_par = f"<p>SIMSAPA_DIR: {SIMSAPA_DIR}<br> SIMSAPA_PACKAGE_DIR: {SIMSAPA_PACKAGE_DIR}</p>"

    msg = f"""
<h1>Simsapa Dhamma Reader</h1>
{app_version_par}
<p>
<a href="https://github.com/simsapa/simsapa">github.com/simsapa/simsapa</a>
</p>
{sys_version_par}
{dirs_par}
"""
    box.setText(msg)
    box.setWindowTitle("About Simsapa Dhamma Reader")
    box.setStandardButtons(QMessageBox.StandardButton.Ok)

    box.exec()

def open_simsapa_website():
    QDesktopServices.openUrl(QUrl("https://github.com/simsapa/simsapa"))
