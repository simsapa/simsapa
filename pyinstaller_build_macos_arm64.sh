#!/usr/bin/env bash

#poetry export --without-hashes -o requirements.txt

if [ -d build ]; then rm -r build; fi
if [ -d dist ]; then rm -r dist; fi

pyinstaller run.py \
    --name "Simsapa Dhamma Reader" \
    --onedir \
    -w \
    --clean \
    --noupx \
    -i "simsapa/assets/icons/appicons/simsapa.ico" \
    --add-data "simsapa/assets:simsapa/assets" \
    --add-data "simsapa/alembic:simsapa/alembic" \
    --add-data "simsapa/alembic.ini:simsapa/alembic.ini" \
    --target-architecture arm64 \
    --osx-bundle-identifier 'com.profound-labs.dhamma.simsapa' \
    --hidden-import=tiktoken_ext \
    --hidden-import=tiktoken_ext.openai_public

dmgbuild -s dmgbuild_settings.py "Simsapa Dhamma Reader" "./dist/Simsapa Dhamma Reader.dmg"
