#!/usr/bin/env bash

set -e

cd ../bootstrap-assets-resources/dpd-db

for i in config.ini dpd.db
do
    if [ -e "$i" ]; then rm "$i"; fi
done

source .venv/bin/activate

poetry run bash bash/build_db.sh

poetry run bash bash/makedict.sh

today=$(date +%Y-%m-%d)
dest_dir="../dpd-db-for-bootstrap/$today-dev"

mkdir -p "$dest_dir"

mv dpd.db "$dest_dir"

cp tbw/output/*.json "$dest_dir"
