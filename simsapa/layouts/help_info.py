import subprocess

from PyQt6.QtCore import QUrl, Qt
from functools import partial
from PyQt6.QtGui import QDesktopServices, QIcon, QPixmap
from PyQt6.QtWidgets import QFrame, QLabel, QMainWindow, QMessageBox, QPushButton, QScrollArea, QVBoxLayout, QWidget
from simsapa import IS_SWAY, SIMSAPA_DIR, SIMSAPA_PACKAGE_DIR

from simsapa.layouts.gui_helpers import get_app_version, get_sys_version
from simsapa.layouts.gui_types import QExpanding

def setup_info_button(layout):
    icon = QIcon()
    icon.addPixmap(QPixmap(":/info"))
    btn = QPushButton()
    btn.setFixedSize(40, 40)
    btn.setToolTip("Search query terms")
    btn.setIcon(icon)

    layout.addWidget(btn)

    btn.clicked.connect(partial(show_search_info))

SEARCH_INFO_MSG = """
<h2 id="search-modes-in-brief">Search Modes in Brief</h2>
<p>See also: <a href="https://simsapa.github.io/features/search-queries/">Search Queries (simsapa.github.io)</a><p>
<p>Sutta references are matched first. Typing 'mn8', 'sn 56.11', 'iti92' will list those suttas.</p>
<p><strong>Fulltext Match:</strong> it searches the content for keywords using the query expressions, non-accented letters matching accented ones. (I.e. it makes a query in the tantivy fulltext index and assigns scores to the results.)</p>
<p>Fulltext search matches words in full, not in part, e.g. 'bodhi' will not match 'bodhisatta', but words are stemmed and will match declensions, e.g. 'bodhiṁ / bodhiyā'.</p>
<p><strong>Title / Headword Match:</strong> it searches only the titles of suttas or the headwords of dictionary words. (SQL queries)</p>
<h2 id="fulltext-match-queries">Fulltext Match Queries</h2>
<p>Powered by the <a href="https://github.com/quickwit-oss/tantivy">tantivy</a> fulltext search engine. Read more in the <a href="https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html">QueryParser</a> docs.</p>
<p>The words in a query term are related as OR by default. <strong>kamma vipāka</strong> searches for entries which SHOULD include <strong>kamma</strong> OR <strong>vipāka</strong>, but not MUST include.</p>
<p>Prefixing the word with the '+' sign means a term must be included, the '-' signs means it must be excluded.</p>
<p><strong>bhikkhu +kamma -vipaka</strong> means should include 'bhikkhu', must include 'kamma', must exclude 'vipaka'.</p>
<p>The texts are indexed with Pāli, English, etc. grammar stemmers, so declension forms will also match in the appropriate language.</p>
<ul>
<li><strong>dukkha</strong> will match <strong>duddkaṁ / dukkhā / dukkhāni / dukkhena</strong> etc.,</li>
<li><strong>bhikkhu kamma vipaka</strong> will match <strong>bhikkhave kammānaṁ vipāko</strong>,</li>
<li><strong>monk receives robes</strong> will match <strong>monks receiving robes</strong>.</li>
</ul>
<p>Latin terms are expanded to include diacritics, <strong>patipada</strong> will match <strong>paṭipadā</strong>.</p>
<p>A pharse query is expressed with quote marks: <strong>"paṭhamena jhānena"</strong>.</p>
<p>The query can match parts of the document:</p>
<ul>
<li><strong>title:sticks cessation</strong> - match 'sticks' in the title, 'cessation' in the content</li>
<li><strong>word:kamma +work</strong> - match 'kamma' in the headword, must include 'work' in the content</li>
<li><strong>uid:pj4</strong> - the uid should include 'pj4'</li>
<li><strong>upekkhindriyaṁ -source:cst4</strong> - match 'upekkhindriyaṁ' in the content, exclude all cst4 documents</li>
<li><strong>calmness +source:thanissaro</strong> - match 'calmness' in the content, only in documents by Bh. Thanissaro</li>
<li><strong>+"buddhas of the past" +source:bodhi</strong> - must include the phrase 'buddhas of the past', only in documents by Bh. Bodhi</li>
</ul>
<h4 id="regex-search-icon">Regex search (.* icon)</h4>
<p>This option will parse the query as a regex pattern, but limited to globbing expressions, such as: <code>.* .+ a* a+</code></p>
<p>The <code>.</code> (dot) matches any single character, <code>*</code> (asterisk) means 'zero or more' or the previous character, <code>+</code> (plus) means 'one or more'.</p>
<ul>
<li><strong>a*vitak.*</strong> - the word start with zero or more of 'a', followed by 'vitak', followed by zero or more characters</li>
<li><strong>.*vitak.*</strong> - match any word containing 'vitak'</li>
<li><strong>vitak.*</strong> - match starting with 'vitak'</li>
</ul>
<h4 id="fuzzy-search-icon">Fuzzy search (~ icon)</h4>
<p>This option allows matching words which may differ from the query by N number of characters (i.e. the <a href="https://devopedia.org/levenshtein-distance">Levenshtein Distance</a>).</p>
<p>Fuzzy search is not availble together with regex patterns.</p>
"""

class SearchInfoWindow(QMainWindow):
    def __init__(self):
        super().__init__()

        self._setup_ui()

    def _setup_ui(self):
        self.setWindowTitle("Search Info")
        self.setWindowFlags(Qt.WindowType.WindowStaysOnTopHint)
        self.resize(600, 700)

        if IS_SWAY:
            cmd = """swaymsg 'for_window [title="Search Info"] floating enable'"""
            subprocess.Popen(cmd, shell=True)

        self._central_widget = QWidget(self)
        self.setCentralWidget(self._central_widget)

        self._layout = QVBoxLayout()
        self._layout.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._central_widget.setLayout(self._layout)

        self.content_page = QLabel()
        self.content_page.setWordWrap(True)
        self.content_page.setOpenExternalLinks(True)
        self.content_page.setSizePolicy(QExpanding, QExpanding)

        self.content_page.setText(SEARCH_INFO_MSG)

        self._scroll_area = QScrollArea()
        self._scroll_area.setWidgetResizable(True)
        self._scroll_area.setSizePolicy(QExpanding, QExpanding)
        self._scroll_area.setFrameShape(QFrame.Shape.NoFrame)

        self._scroll_area.setWidget(self.content_page)

        self._layout.addWidget(self._scroll_area)

        self._close_button = QPushButton("Close")
        self._close_button.clicked.connect(partial(self._handle_close))

        self._layout.addWidget(self._close_button)

    def _handle_close(self):
        self.close()

def show_search_info():
    w = SearchInfoWindow()
    w.show()
    w.raise_()
    w.activateWindow()

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
