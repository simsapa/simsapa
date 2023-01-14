#!/usr/bin/env bash

START_TIME=$(date --iso-8601=seconds)

SIMSAPA_DIR="$HOME/.local/share/simsapa"
ASSETS_DIR="$SIMSAPA_DIR/assets"
BOOTSTRAP_ASSETS_DIR="$(pwd)/../bootstrap-assets-resources"
DIST_DIR="$(pwd)/../bootstrap-assets-resources/dist"

RELEASE_DIR="$(pwd)/../releases/$(date --iso-8601=date)-dev"

# Activate poetry venv
# DIR=$(poetry env list --full-path)
# source "$DIR/bin/activate"

# Ensure latest local simsapa is in the venv, this is what ./scripts/*.py will import.
# poetry install

echo "=== Clean and Create Folders ==="

if [ -e "$ASSETS_DIR" ]; then rm -r "$ASSETS_DIR"; fi

mkdir -p "$ASSETS_DIR"
mkdir -p "$RELEASE_DIR"

d="$SIMSAPA_DIR/unzipped_stardict"
if [ -e "$d" ]; then rm -r "$d"; fi

rm "$SIMSAPA_DIR"/*.tar.bz2

rm "$RELEASE_DIR"/*.tar.bz2

rm -r "$DIST_DIR"/courses
rm "$DIST_DIR"/*

dotenv="SIMSAPA_DIR=/home/yume/.local/share/simsapa
BOOTSTRAP_ASSETS_DIR=../bootstrap-assets-resources
USE_TEST_DATA=false
DISABLE_LOG=false
ENABLE_PRINT_LOG=true
ENABLE_WIP_FEATURES=false"

echo "$dotenv" > .env

echo "" > "$SIMSAPA_DIR/log.txt"

echo "=== Bootstrap Appdata DB ==="

./scripts/bootstrap_appdata_db.py

echo "=== Create appdata.tar.bz2 ==="

cd "$DIST_DIR" || exit

tar cjf appdata.tar.bz2 appdata.sqlite3

mv appdata.tar.bz2 "$RELEASE_DIR"

cd - || exit

echo "=== Copy Appdata DB to user folder ==="

cp "$DIST_DIR"/appdata.sqlite3 "$ASSETS_DIR"

echo "=== Import User Data ==="

./run.py import-pali-course "$BOOTSTRAP_ASSETS_DIR/courses/dhammapada-word-by-word/dhammapada-word-by-word.toml"

./run.py import-bookmarks "$BOOTSTRAP_ASSETS_DIR/bookmarks/bookmarks.csv"

echo "=== Copy Userdata to DIST folder ==="

cp "$ASSETS_DIR"/userdata.sqlite3 "$DIST_DIR"
cp -r "$ASSETS_DIR"/courses "$DIST_DIR"

echo "=== Create userdata.tar.bz2 ==="

cd "$DIST_DIR" || exit

tar cjf userdata.tar.bz2 userdata.sqlite3 courses/

mv userdata.tar.bz2 "$RELEASE_DIR"

cd - || exit

echo "=== Reindex ==="

./run.py index reindex

echo "=== Create index.tar.bz2 ==="

cd "$ASSETS_DIR" || exit

tar cjf index.tar.bz2 index/

cd - || exit

mv "$ASSETS_DIR"/index.tar.bz2 "$RELEASE_DIR"

echo "=== Bootstrap Sanskrit Texts DB and Import to Appdata ==="

./scripts/sanskrit_texts.py

echo "=== Create sanskrit-texts.tar.bz2 ==="

cd "$DIST_DIR" || exit

tar cjf sanskrit-texts.tar.bz2 sanskrit-texts.sqlite3

mv sanskrit-texts.tar.bz2 "$RELEASE_DIR"

cd - || exit

echo "=== Copy Appdata DB to user folder ==="

cp "$DIST_DIR"/appdata.sqlite3 "$ASSETS_DIR"

echo "=== Reindex ==="

./run.py index reindex

echo "=== Create sanskrit-appdata.tar.bz2 ==="

cd "$ASSETS_DIR" || exit

tar cjf sanskrit-appdata.tar.bz2 appdata.sqlite3

cd - || exit

mv "$ASSETS_DIR"/sanskrit-appdata.tar.bz2 "$RELEASE_DIR"

echo "=== Create sanskrit-index.tar.bz2 ==="

cd "$ASSETS_DIR" || exit

tar cjf sanskrit-index.tar.bz2 index/

cd - || exit

mv "$ASSETS_DIR"/sanskrit-index.tar.bz2 "$RELEASE_DIR"

echo "=== Copy log.txt ==="

cp "$SIMSAPA_DIR/log.txt" "$RELEASE_DIR"

echo "=== Clean up ==="

d="$SIMSAPA_DIR/unzipped_stardict"
if [ -e "$d" ]; then rm -r "$d"; fi

echo "=== Bootstrap DB finished ==="

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
