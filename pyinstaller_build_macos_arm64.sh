#!/usr/bin/env bash

set -e

#poetry export --without-hashes -o requirements.txt

if [ -d build ]; then rm -r build; fi
if [ -d dist ]; then rm -r dist; fi

pyinstaller run.py \
    --name "Simsapa Dhamma Reader" \
    --onedir \
    --windowed \
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

appdir="dist/Simsapa Dhamma Reader.app"
wrapper_path="$appdir/Contents/MacOS/wrapper"
plist_path="$appdir/Contents/Info.plist"

cat << EOF > "$wrapper_path"
#!/usr/bin/env bash
open -n "/Applications/Simsapa Dhamma Reader.app/Contents/MacOS/Simsapa Dhamma Reader"
EOF

chmod +x "$wrapper_path"

cat "$plist_path" | sed '/CFBundleExecutable/{ n; s/<string>Simsapa Dhamma Reader<\/string>/<string>wrapper<\/string>/; }' > "$plist_path.new"
mv "$plist_path.new" "$plist_path"

cd dist/
create-dmg "Simsapa Dhamma Reader.app"

# Creates:
# dist/Simsapa Dhamma Reader 0.0.0.dmg
