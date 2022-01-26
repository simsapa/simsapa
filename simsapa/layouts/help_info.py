from PyQt5.QtCore import QUrl
from functools import partial
from PyQt5.QtGui import QDesktopServices, QIcon, QPixmap
from PyQt5.QtWidgets import QMessageBox, QPushButton

from simsapa.app.helpers import get_app_version

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
    box.setIcon(QMessageBox.Information)
    msg = """
<p>Search query terms are related as AND by default, <b>kamma vipāka</b> searches for entries containing <b>kamma</b> AND <b>vipāka</b>.</p>
<p>Latin terms are expanded to include diacritics, <b>patipada</b> will match <b>paṭipadā</b>.</p>
<p>Add * or ? to match inexact terms. * matches a series of letters, ? matches a single letter.</p>
<p>Add ~ to match fuzzy terms: <b>citta~</b> or <b>citta~2/2</b> (term~pre/post).</p>
<p>Use <b>title:kamma*</b> to search only in the title, for a term containing 'kamma...'.</p>

<p>Example sutta queries:</p>

<p>
kamma vipāka<br>
title:vaccha*<br>
title:'fire sticks'<br>
ref:'SN 12.21'<br>
uid:an10.1/*
</p>

<p>Example dictionary queries:</p>

<p>
bhavana<br>
word:kamma*<br>
synonyms:dharma
</p>

<p>
Read more about queries at
<a href="https://whoosh.readthedocs.io/en/latest/querylang.html">whoosh.readthedocs.io</a>
</p>
"""
    box.setText(msg)
    box.setWindowTitle("Search query info")
    box.setStandardButtons(QMessageBox.Ok)

    box.exec()

def show_about(parent=None):
    box = QMessageBox(parent)
    box.setIcon(QMessageBox.Information)

    version_par = ''

    ver = get_app_version()
    if ver is not None:
        version_par = f"<p>Version {ver}</p>"

    msg = f"""
<h1>Simsapa Dhamma Reader</h1>
{version_par}
<p>
<a href="https://github.com/simsapa/simsapa">github.com/simsapa/simsapa</a>
</p>
"""
    box.setText(msg)
    box.setWindowTitle("About Simsapa Dhamma Reader")
    box.setStandardButtons(QMessageBox.Ok)

    box.exec()

def open_simsapa_website():
    QDesktopServices.openUrl(QUrl("https://github.com/simsapa/simsapa"))