# Versions — set them with `scripts/set-version.py`, never by hand

Four versions are each stored in more than one place, some with a `v` prefix
and some without. `scripts/set-version.py` knows every place and its form:

| Option | Places | Form |
|---|---|---|
| `--app` | `cpp/gui.cpp` `setApplicationVersion` | `v1.1.0-alpha.3` |
| | `[package] version` of `backend/`, `bridges/`, `api/`, `backend_cli/` `Cargo.toml`; the `simsapa_*` entries of the five `Cargo.lock`s; the five `npm/*/package.json` `version`s and the main package's four `optionalDependencies` pins | `1.1.0-alpha.3` |
| `--db` | `bootstrap_cli/src/bootstrap/appdata.rs` `DB_VERSION` | `1.1.0-alpha.3` |
| `--android-code` | `android/version.txt` | an integer, or `next` |
| `--dpd-fallback` | `backend/src/dictionary_catalog.rs` `FALLBACK_TAG` | `v1.0.8` |

``` sh
scripts/set-version.py                          # show every place; exit 1 if a kind disagrees
scripts/set-version.py --app 1.1.0-alpha.4      # "v1.1.0-alpha.4" works too
scripts/set-version.py --android-code next --dry-run
```

The app version is also the `simsapa` CLI's and the npm packages'. The binary
reports **`backend/Cargo.toml`'s** (`get_app_version()` is the backend crate's
`CARGO_PKG_VERSION`), and `bridges/Cargo.toml`'s becomes the Android versionName
and the AppImage / macOS bundle version, so they must never drift apart.

The script refuses, and writes nothing, when a value is not semver, when
`--android-code` does not increase (Play requires it), when `--dpd-fallback`
is outside `PINNED_SERIES`, or when a place no longer matches its pattern.
**If you add a place that stores one of these versions, add it to `PLACES` in
the script.** Version changes are the maintainer's: an agent runs the script
only when asked to set a version, and may always run the show mode.

