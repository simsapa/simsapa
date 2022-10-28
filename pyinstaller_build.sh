#!/usr/bin/env bash

#poetry export --without-hashes -o requirements.txt

if [ -d build ]; then rm -r build; fi
if [ -d dist ]; then rm -r dist; fi

pyinstaller run.py \
    --name simsapa \
    --onefile \
    -w \
    --add-data simsapa/assets:simsapa/assets \
    --add-data simsapa/alembic:simsapa/alembic \
    --add-data simsapa/alembic.ini:simsapa/alembic.ini
