#!/usr/bin/env bash

set -e

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

d="$ASSETS_DIR"
if [ -e "$d" ]; then rm -r "$d"; fi

mkdir -p "$ASSETS_DIR"
mkdir -p "$RELEASE_DIR"

d="$SIMSAPA_DIR/unzipped_stardict"
if [ -e "$d" ]; then rm -r "$d"; fi

if stat -t "$SIMSAPA_DIR"/*.tar.bz2 >/dev/null 2>&1; then rm "$SIMSAPA_DIR"/*.tar.bz2; fi
if stat -t "$RELEASE_DIR"/*.tar.bz2 >/dev/null 2>&1; then rm "$RELEASE_DIR"/*.tar.bz2; fi

d="$DIST_DIR"/courses
if [ -e "$d" ]; then rm -r "$d"; fi

d="$DIST_DIR"/html_resources
if [ -e "$d" ]; then rm -r "$d"; fi

if stat -t "$DIST_DIR"/* >/dev/null 2>&1; then rm "$DIST_DIR"/*; fi

dotenv="SIMSAPA_DIR=$HOME/.local/share/simsapa
BOOTSTRAP_ASSETS_DIR=../bootstrap-assets-resources
USE_TEST_DATA=false
DISABLE_LOG=false
ENABLE_PRINT_LOG=true
START_NEW_LOG=false
ENABLE_WIP_FEATURES=false
SAVE_STATS=false
RELEASE_CHANNEL=development"

echo "$dotenv" > .env

echo "" > "$SIMSAPA_DIR/log.txt"

echo "=== Bootstrap Appdata DB ==="

./scripts/bootstrap_appdata.py

echo "=== Create appdata.tar.bz2 ==="

cd "$DIST_DIR" || exit

tar cjf appdata.tar.bz2 dpd.sqlite3 appdata.sqlite3

mv appdata.tar.bz2 "$RELEASE_DIR"

cd - || exit

echo "=== Copy Appdata DB to user folder ==="

cp "$DIST_DIR"/appdata.sqlite3 "$ASSETS_DIR"
cp "$DIST_DIR"/dpd.sqlite3 "$ASSETS_DIR"

echo "=== Import User Data ==="

# FIXME ./run.py import-pali-course "$BOOTSTRAP_ASSETS_DIR/courses/dhammapada-word-by-word/dhammapada-word-by-word.toml"

./run.py import-bookmarks "$BOOTSTRAP_ASSETS_DIR/bookmarks/bookmarks.csv"

./run.py import-prompts "$BOOTSTRAP_ASSETS_DIR/prompts/prompts.csv"

echo "=== Create userdata.tar.bz2 ==="

cp "$ASSETS_DIR"/userdata.sqlite3 "$DIST_DIR"
cp -r "$ASSETS_DIR"/courses "$DIST_DIR"
cp -r "$BOOTSTRAP_ASSETS_DIR"/html_resources/ "$DIST_DIR"

# Also copy to local assets dir for testing.
cp -r "$BOOTSTRAP_ASSETS_DIR"/html_resources/ "$ASSETS_DIR"

cd "$DIST_DIR" || exit

tar cjf userdata.tar.bz2 userdata.sqlite3 courses/ html_resources/

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

tar cjf sanskrit-appdata.tar.bz2 dpd.sqlite3 appdata.sqlite3

cd - || exit

mv "$ASSETS_DIR"/sanskrit-appdata.tar.bz2 "$RELEASE_DIR"

echo "=== Create sanskrit-index.tar.bz2 ==="

cd "$ASSETS_DIR" || exit

tar cjf sanskrit-index.tar.bz2 index/

cd - || exit

mv "$ASSETS_DIR"/sanskrit-index.tar.bz2 "$RELEASE_DIR"

echo "=== Bootstrap Languages from SuttaCentral ==="

# AQL:
#
# LET docs = (FOR x IN language
# FILTER x._key != 'en'
# && x._key != 'pli'
# && x._key != 'san'
# && x._key != 'hu'
# RETURN x._key)
# RETURN docs

for lang in "af" "ar" "au" "bn" "ca" "cs" "de" "es" "ev" "fa" "fi" "fr" "gu" "haw" "he" "hi" "hr" "id" "it" "jpn" "kan" "kho" "kln" "ko" "la" "lt" "lzh" "mr" "my" "nl" "no" "pgd" "pl" "pra" "pt" "ro" "ru" "si" "sk" "sl" "sld" "sr" "sv" "ta" "th" "uig" "vi" "vu" "xct" "xto" "zh"
do
    echo "=== $lang ==="
    name="suttas_lang_$lang"

    # Will exit with status 1 if there are 0 suttas for lang.
    ./scripts/bootstrap_suttas_lang.py $lang || true

    n=$(sqlite3 "$DIST_DIR/$name.sqlite3" "SELECT COUNT(*) FROM suttas;")
    if [ $n -eq 0 ]; then
        f="$DIST_DIR/$name.sqlite3"
        if [ -e "$f" ]; then rm "$f"; fi
    else
        ./run.py import-suttas-to-userdata "$DIST_DIR/$name.sqlite3"

        ./run.py index suttas-lang $lang

        cp "$DIST_DIR/$name.sqlite3" "$ASSETS_DIR"

        cd "$ASSETS_DIR" || exit
        tar cjf "$name.tar.bz2" "$name.sqlite3" index/suttas/"$lang"/
        mv "$ASSETS_DIR/$name.tar.bz2" "$RELEASE_DIR"
        cd - || exit
    fi
done

echo "=== Bootstrap Hungarian from Buddha Ujja ==="

lang="hu"
name="suttas_lang_$lang"

./scripts/buddha_ujja.py

./run.py import-suttas-to-userdata "$DIST_DIR/$name.sqlite3"

./run.py index suttas-lang $lang

cp "$DIST_DIR/$name.sqlite3" "$ASSETS_DIR"

cd "$ASSETS_DIR" || exit
tar cjf "$name.tar.bz2" "$name.sqlite3" index/suttas/"$lang"/
mv "$ASSETS_DIR/$name.tar.bz2" "$RELEASE_DIR"
cd - || exit

echo "=== Copy log.txt ==="

cp "$SIMSAPA_DIR/log.txt" "$RELEASE_DIR"

echo "=== Release Info ==="

cd "$DIST_DIR" || exit

suttas_lang=$(ls -1 suttas_lang_*.sqlite3 | sed 's/^suttas_lang_/\\"/' | perl -0777 -pe 's/\.sqlite3\n/\\", /g' | sed -e 's/^/[/; s/, *$/]/')
datetime=$(date +%FT%T)

release_info="[[assets.releases]]
date = \"$datetime\"
version_tag = \"v0.5.0-alpha.1\"
github_repo = \"simsapa/simsapa-assets\"
suttas_lang = $suttas_lang
title = \"Updates\"
description = \"\"
"

echo "$release_info"

echo "$release_info" > "release_info.toml"
cp "release_info.toml" "$RELEASE_DIR"
cd - || exit

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
