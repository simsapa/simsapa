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
START_NEW_LOG=false
ENABLE_WIP_FEATURES=false"

echo "$dotenv" > .env

echo "" > "$SIMSAPA_DIR/log.txt"

echo "=== Bootstrap Appdata DB ==="

./scripts/bootstrap_appdata.py

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

for lang in 'af' 'ar' 'au' 'bn' 'ca' 'cs' 'de' 'es' 'ev' 'fa' 'fi' 'fr' 'gu' 'haw' 'he' 'hi' 'hr' 'id' 'it' 'jpn' 'kan' 'kho' 'kln' 'ko' 'la' 'lt' 'lzh' 'mr' 'my' 'nl' 'no' 'pgd' 'pl' 'pra' 'pt' 'ro' 'ru' 'si' 'sk' 'sl' 'sld' 'sr' 'sv' 'ta' 'th' 'uig' 'vi' 'vu' 'xct' 'xto' 'zh'
do
    echo "=== $lang ==="
    name="suttas_lang_$lang"

    ./scripts/bootstrap_suttas_lang.py $lang | tee out.log

    ok=$(grep "0 suttas for $lang, exiting." out.log)
    if [ "$ok" != "" ]; then
        f="$DIST_DIR/$name.sqlite3"
        if [ -e "$f" ]; then rm "$f"; fi
        continue
    fi

    cd "$DIST_DIR" || exit
    tar cjf "$name.tar.bz2" "$name.sqlite3"
    mv "$name.tar.bz2" "$RELEASE_DIR"
    cd - || exit

    ./run.py import-suttas-to-userdata "$DIST_DIR/$name.sqlite3"

    ./run.py index suttas-lang $lang

    cd "$ASSETS_DIR" || exit
    tar cjf "$name.tar.bz2" "index/$name"_*
    mv "$ASSETS_DIR/$name.tar.bz2" "$RELEASE_DIR"
    cd - || exit
done

echo "=== Bootstrap Hungarian from Buddha Ujja ==="

lang="hu"
name="suttas_lang_$lang"

./scripts/buddha_ujja.py

cd "$DIST_DIR" || exit
tar cjf "$name.tar.bz2" "$name.sqlite3"
mv "$name.tar.bz2" "$RELEASE_DIR"
cd - || exit

./run.py import-suttas-to-userdata "$DIST_DIR/$name.sqlite3"

./run.py index suttas-lang $lang

cd "$ASSETS_DIR" || exit
tar cjf "$name.tar.bz2" "index/$name"_*
mv "$ASSETS_DIR/$name.tar.bz2" "$RELEASE_DIR"
cd - || exit

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
