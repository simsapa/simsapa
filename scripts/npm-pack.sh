#!/usr/bin/env bash
#
# Packs the @simsapa/cli npm packages into dist/npm/*.tgz, for a local dry run
# or for the maintainer to publish by hand. Publishes nothing.
#
# The npm version is the CLI's version, from backend_cli/Cargo.toml. It must
# equal backend/Cargo.toml's: `simsapa --version` and the database release
# lookup both use get_app_version(), which is the backend crate's version, so a
# mismatch would label the binary with a version it does not report. The
# script refuses to pack on a mismatch.
#
# The tracked npm/*/package.json files carry that version, in `version` and in
# the main package's optionalDependencies pins. The script writes it into them
# when they are out of date (after the maintainer's version bump) and says so;
# commit those changes with the bump. Each package folder is then copied to
# dist/npm/stage/<package>/ and packed from there.
#
# A platform package gets its binary from dist/cli/<platform>/, written by the
# `make cli-<platform>` targets. The darwin and win32 binaries are built on
# their own machines; copy them into dist/cli/ here before packing. A platform
# without a binary is skipped with a warning.
#
# See docs/backend-cli-and-npm-package.md.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PLATFORMS=(linux-x64 linux-arm64 darwin-arm64 win32-x64)
OUT="dist/npm"
STAGE="$OUT/stage"

# cargo_version <Cargo.toml> — the version in its [package] section.
cargo_version() {
    sed -n '/^\[package\]/,/^\[/ s/^version *= *"\(.*\)"/\1/p' "$1" | head -n 1
}

VERSION="$(cargo_version backend_cli/Cargo.toml)"
BACKEND_VERSION="$(cargo_version backend/Cargo.toml)"
if [[ -z "$VERSION" || -z "$BACKEND_VERSION" ]]; then
    echo "npm-pack: could not read the version from backend_cli/Cargo.toml or backend/Cargo.toml" >&2
    exit 1
fi
if [[ "$VERSION" != "$BACKEND_VERSION" ]]; then
    echo "npm-pack: backend_cli/Cargo.toml is $VERSION but backend/Cargo.toml is $BACKEND_VERSION." >&2
    echo "npm-pack: the binary reports the backend's version, so set both to the same version." >&2
    exit 1
fi
echo "npm-pack: version $VERSION"

# sync_version <package.json> — writes $VERSION into the version field and into
# every @simsapa/cli-* optionalDependencies pin. Prints the file name if it
# changed anything.
sync_version() {
    node - "$1" "$VERSION" <<'JS'
const fs = require("fs");
const [file, version] = process.argv.slice(2);
const before = fs.readFileSync(file, "utf8");
const pkg = JSON.parse(before);
pkg.version = version;
for (const name of Object.keys(pkg.optionalDependencies || {})) {
  if (name.startsWith("@simsapa/cli-")) pkg.optionalDependencies[name] = version;
}
const after = JSON.stringify(pkg, null, 2) + "\n";
if (after !== before) {
  fs.writeFileSync(file, after);
  console.log(file);
}
JS
}

for dir in npm/cli npm/cli-*; do
    changed="$(sync_version "$dir/package.json")"
    if [[ -n "$changed" ]]; then
        echo "npm-pack: wrote $VERSION into $changed (commit it with the version bump)"
    fi
done

rm -rf "$STAGE"
mkdir -p "$STAGE"
rm -f "$OUT"/*.tgz

# pack <stage dir> — npm pack into dist/npm/, printing the tarball name.
pack() {
    local tgz
    tgz="$(cd "$1" && npm pack --silent --pack-destination "$ROOT/$OUT")"
    echo "npm-pack: $OUT/$tgz ($(du -h "$OUT/$tgz" | cut -f1))"
}

packed_platforms=0
for platform in "${PLATFORMS[@]}"; do
    exe="simsapa"
    [[ "$platform" == win32-* ]] && exe="simsapa.exe"
    binary="dist/cli/$platform/$exe"

    if [[ ! -f "$binary" ]]; then
        echo "npm-pack: WARNING: $binary not found, skipping @simsapa/cli-$platform" >&2
        continue
    fi

    dir="$STAGE/cli-$platform"
    cp -r "npm/cli-$platform" "$dir"
    mkdir -p "$dir/bin"
    cp "$binary" "$dir/bin/$exe"
    chmod 0755 "$dir/bin/$exe"
    cp LICENSE.txt "$dir/"
    pack "$dir"
    packed_platforms=$((packed_platforms + 1))
done

dir="$STAGE/cli"
cp -r npm/cli "$dir"
rm -f "$dir"/bin/*.test.js
cp LICENSE.txt "$dir/"
pack "$dir"

if [[ "$packed_platforms" -lt "${#PLATFORMS[@]}" ]]; then
    echo "npm-pack: packed $packed_platforms of ${#PLATFORMS[@]} platform packages; do not publish an incomplete set" >&2
fi
