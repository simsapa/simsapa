#!/usr/bin/env bash

echo "=== Bootstrap Appdata DB ==="

./scripts/bootstrap-appdata-db.py

echo "=== Copy Appdata DB to user share ==="

cp ../bootstrap-assets-resources/dist/appdata.sqlite3 ~/.local/share/simsapa/assets/appdata.sqlite3

echo "=== Reindex ==="

./run.py index reindex

echo "=== Create appdata.tar.bz2 ==="

cd ~/.local/share/simsapa/assets/

tar cjf appdata.tar.bz2 appdata.sqlite3

cd -

mv ~/.local/share/simsapa/assets/appdata.tar.bz2 ../releases/

echo "=== Create assets.tar.bz2 ==="

cd ~/.local/share/simsapa/assets/

tar cjf assets.tar.bz2 appdata.sqlite3 index/

cd -

mv ~/.local/share/simsapa/assets/assets.tar.bz2 ../releases/
