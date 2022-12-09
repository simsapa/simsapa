#!/usr/bin/env bash

START_TIME=$(date --iso-8601=seconds)

# Activate poetry venv
# DIR=$(poetry env list --full-path)
# source "$DIR/bin/activate"

# Ensure latest local simsapa is in the venv, this is what ./scripts/*.py will import.
# poetry install

echo "=== Clean and Create Folders ==="

mkdir -p ~/.local/share/simsapa/assets/

rm -r ~/.local/share/simsapa/unzipped_stardict

rm ~/.local/share/simsapa/*.tar.bz2

rm ../releases/*.tar.bz2

rm ../bootstrap-assets-resources/dist/*

./scripts/remove_log.py

echo "=== Bootstrap Appdata DB ==="

./scripts/bootstrap_appdata_db.py

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

echo "=== Create sanskrit-appdata.tar.bz2 ==="

cd ~/.local/share/simsapa/assets/

tar cjf sanskrit-appdata.tar.bz2 appdata.sqlite3

cd -

mv ~/.local/share/simsapa/assets/sanskrit-appdata.tar.bz2 ../releases/

echo "=== Create sanskrit-index.tar.bz2 ==="

cd ~/.local/share/simsapa/assets/

tar cjf sanskrit-index.tar.bz2 index/

cd -

mv ~/.local/share/simsapa/assets/sanskrit-index.tar.bz2 ../releases/

END_TIME=$(date --iso-8601=seconds)

src="from dateutil import parser
started = parser.parse('$START_TIME')
finished = parser.parse('$END_TIME')
d = finished - started
print(d)"

echo "======"
echo "Bootstrap started: $START_TIME"
echo "Bootstrap ended:   $END_TIME"
echo "Duration:          $(echo "$src" | python3)"
