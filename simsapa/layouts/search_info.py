from functools import partial
from PyQt5.QtGui import QIcon, QPixmap
from PyQt5.QtWidgets import QMessageBox, QPushButton

def setup_info_button(layout):
    icon = QIcon()
    icon.addPixmap(QPixmap(":/info"))
    btn = QPushButton()
    btn.setFixedSize(30, 30)
    btn.setIcon(icon)

    layout.addWidget(btn)

    btn.clicked.connect(partial(_show_search_info))

def _show_search_info():
    box = QMessageBox()
    box.setIcon(QMessageBox.Information)
    msg = """
<p>Search query terms are related as AND by default, <b>kamma vipāka</b> searches for entries containing <b>kamma</b> AND <b>vipāka</b>.</p>
<p>Latin terms are expanded to include diacritics, <b>patipada</b> will match <b>paṭipadā</b>.</p>
<p>Add * or ? to match inexact terms.</p>
<p>Add ~ to match fuzzy terms: <b>citta~</b> or <b>citta~2/2</b> (term~pre/post).</p>
<p>Use <b>title:kamma*</b> to search only in the title, for a term beginning with 'kamma...'.</p>

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
