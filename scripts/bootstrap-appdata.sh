#!/usr/bin/env bash

echo "=== Bootstrap Appdata DB ==="

./scripts/bootstrap-appdata-db.py

echo "=== Create appdata.tar.bz2 ==="

cd ../bootstrap-assets-resources/dist/

tar cjf appdata.tar.bz2 appdata.sqlite3

mv appdata.tar.bz2 ../../releases/

cd -

echo "=== Copy Appdata DB to user folder ==="

cp ../bootstrap-assets-resources/dist/appdata.sqlite3 ~/.local/share/simsapa/assets/appdata.sqlite3

echo "=== Reindex ==="

./run.py index reindex

echo "=== Create index.tar.bz2 ==="

cd ~/.local/share/simsapa/assets/

tar cjf index.tar.bz2 index/

cd -

mv ~/.local/share/simsapa/assets/index.tar.bz2 ../releases/

echo "=== Bootstrap Sanskrit Texts DB and Import to Appdata ==="

./scripts/sanskrit_texts.py

echo "=== Create sanskrit-texts.tar.bz2 ==="

cd ../bootstrap-assets-resources/dist/

tar cjf sanskrit-texts.tar.bz2 sanskrit-texts.sqlite3

mv sanskrit-texts.tar.bz2 ../../releases/

cd -

echo "=== Copy Appdata DB to user folder ==="

cp ../bootstrap-assets-resources/dist/appdata.sqlite3 ~/.local/share/simsapa/assets/appdata.sqlite3

echo "=== Reindex ==="

./run.py index reindex

