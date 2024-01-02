#!/usr/bin/env bash

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

# When LICENSE.txt is present in the working directory, the
# sindresorhus/create-dmg tool would try to add it as a software agreement when
# opening the image, but this will fail to build.
#
# In this case the LICENSE.txt needs to be deleted.
#
# However we are building with dist/ as working directory, so it is not
# necessary to remove LICENSE.txt.

cd dist/ && create-dmg "Simsapa Dhamma Reader.app"

# Creates:
# dist/Simsapa Dhamma Reader 0.0.0.dmg

mv "Simsapa Dhamma Reader 0.0.0.dmg" "Simsapa Dhamma Reader.dmg"
