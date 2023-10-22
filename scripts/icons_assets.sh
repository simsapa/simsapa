#!/usr/bin/env bash

# /bin/pyrcc5 contains:
#
# !/bin/sh
# exec /usr/bin/python3 -m PyQt5.pyrcc_main ${1+"$@"}

# pyrcc5 requires python3.10+

/usr/bin/python3 -m PyQt5.pyrcc_main -o simsapa/assets/icons_rc.py simsapa/assets/icons.qrc

sed -i 's/from PyQt5 import/from PyQt6 import/' simsapa/assets/icons_rc.py
