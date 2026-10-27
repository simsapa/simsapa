#!/usr/bin/env python3
"""
Show or set the versions that live in more than one place, each in the form its
place needs (with or without a `v` prefix).

    scripts/set-version.py                         # show every place, flag disagreements
    scripts/set-version.py --app 1.1.0-alpha.4     # 'v1.1.0-alpha.4' is accepted too
    scripts/set-version.py --db 1.1.0-alpha.4
    scripts/set-version.py --android-code next     # or an integer
    scripts/set-version.py --dpd-fallback 1.0.9
    scripts/set-version.py --app 1.1.0-alpha.4 --db 1.1.0-alpha.4 --dry-run

The kinds and their places are in PLACES below. Every place must match its
pattern exactly once (lock files: at least once), or nothing is written. All
files are written only after every edit has been computed and validated.

The show mode exits 1 when the places of one kind disagree, so it doubles as a
check. See AGENTS.md, "Versions".
"""

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

SEMVER = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


@dataclass
class Place:
    path: str
    # The value is the named group `value`. Matched with re.MULTILINE.
    pattern: str
    # "v" if the place stores the version with a v prefix.
    prefix: str = ""
    # Exact match count, or None for "at least one".
    count: int | None = 1


CARGO_PACKAGE_VERSION = r'\A\[package\]\n(?:[^\[\n][^\n]*\n|\n)*?version = "(?P<value>[^"]*)"'
# The [[package]] entries of our own crates, which cargo would otherwise
# rewrite on the next build.
CARGO_LOCK_OWN_CRATES = (
    r'^name = "(?:simsapa_backend|simsapa_api|simsapa_bridges|simsapa_backend_cli)"\n'
    r'version = "(?P<value>[^"]*)"'
)
NPM_PACKAGES = ["cli", "cli-linux-x64", "cli-linux-arm64", "cli-darwin-arm64", "cli-win32-x64"]

PLACES: dict[str, list[Place]] = {
    # The app version is also the simsapa CLI's and the npm packages' version.
    # The binary reports backend/Cargo.toml's (get_app_version() is the backend
    # crate's CARGO_PKG_VERSION), and bridges/Cargo.toml's becomes the Android
    # versionName and the AppImage / macOS bundle version.
    "app": [
        Place("cpp/gui.cpp", r'app\.setApplicationVersion\("(?P<value>[^"]*)"\)', prefix="v"),
        Place("backend/Cargo.toml", CARGO_PACKAGE_VERSION),
        Place("bridges/Cargo.toml", CARGO_PACKAGE_VERSION),
        Place("api/Cargo.toml", CARGO_PACKAGE_VERSION),
        Place("backend_cli/Cargo.toml", CARGO_PACKAGE_VERSION),
        *[Place(f"{c}/Cargo.lock", CARGO_LOCK_OWN_CRATES, count=None)
          for c in ["backend", "bridges", "api", "backend_cli", "bootstrap_cli"]],
        *[Place(f"npm/{p}/package.json", r'^  "version": "(?P<value>[^"]*)"') for p in NPM_PACKAGES],
        Place("npm/cli/package.json", r'^    "@simsapa/cli-[a-z0-9-]+": "(?P<value>[^"]*)"', count=4),
    ],
    "db": [
        Place("bootstrap_cli/src/bootstrap/appdata.rs", r'^pub static DB_VERSION: &str = "(?P<value>[^"]*)";'),
    ],
    # The first line that is not blank or a #-comment is the value.
    "android-code": [
        Place("android/version.txt", r"^(?P<value>\d+)[ \t]*$"),
    ],
    "dpd-fallback": [
        Place("backend/src/dictionary_catalog.rs", r'^pub const FALLBACK_TAG: &str = "(?P<value>[^"]*)";', prefix="v"),
    ],
}

PINNED_SERIES = re.compile(r"^pub const PINNED_SERIES: \(u32, u32\) = \((\d+), (\d+)\);", re.MULTILINE)


class VersionError(Exception):
    pass


def bare_semver(value: str, what: str) -> str:
    """The version without a v prefix, validated as semver."""
    bare = value[1:] if value[:1] in ("v", "V") else value
    if not SEMVER.match(bare):
        raise VersionError(f"{what}: {value!r} is not a version like 1.1.0 or 1.1.0-alpha.3")
    return bare


def read(path: Path) -> str:
    # newline="" keeps each file's own line endings.
    with open(path, encoding="utf-8", newline="") as f:
        return f.read()


def write(path: Path, text: str) -> None:
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(text)


def find(root: Path, place: Place) -> tuple[str, list[re.Match]]:
    path = root / place.path
    try:
        text = read(path)
    except OSError as e:
        raise VersionError(f"{place.path}: cannot read: {e}") from e
    matches = list(re.finditer(place.pattern, text, re.MULTILINE))
    if place.count is None:
        if not matches:
            raise VersionError(f"{place.path}: no version found (pattern {place.pattern!r})")
    elif len(matches) != place.count:
        raise VersionError(
            f"{place.path}: expected {place.count} match(es), found {len(matches)} "
            f"(pattern {place.pattern!r}); update PLACES in scripts/set-version.py"
        )
    return text, matches


def line_of(text: str, pos: int) -> int:
    return text.count("\n", 0, pos) + 1


def current_values(root: Path, kind: str) -> list[tuple[str, int, str]]:
    """(path, line, stored value) for every match of every place of a kind."""
    out = []
    for place in PLACES[kind]:
        text, matches = find(root, place)
        out += [(place.path, line_of(text, m.start("value")), m.group("value")) for m in matches]
    return out


def stored_form(place: Place, bare: str) -> str:
    return place.prefix + bare


def plan_edits(root: Path, kind: str, bare: str) -> tuple[dict[str, str], list[str]]:
    """New contents per changed file, and the report lines."""
    new_texts: dict[str, str] = {}
    report = []
    for place in PLACES[kind]:
        text = new_texts.get(place.path)
        if text is None:
            text, _ = find(root, place)
        matches = list(re.finditer(place.pattern, text, re.MULTILINE))
        new_value = stored_form(place, bare)
        # Replace from the end so earlier offsets stay valid.
        for m in reversed(matches):
            line = line_of(text, m.start("value"))
            old = m.group("value")
            if old == new_value:
                report.append(f"  {place.path}:{line}  {old}  (unchanged)")
            else:
                report.append(f"  {place.path}:{line}  {old} -> {new_value}")
                text = text[: m.start("value")] + new_value + text[m.end("value"):]
        new_texts[place.path] = text
    return new_texts, sorted(report)


def resolve_value(root: Path, kind: str, value: str) -> str:
    """The bare value to store for a kind, validated."""
    if kind == "android-code":
        (_, _, current), = current_values(root, kind)
        new = int(current) + 1 if value == "next" else None
        if new is None:
            if not value.isdigit():
                raise VersionError(f"--android-code: {value!r} is not a positive integer or 'next'")
            new = int(value)
        if new <= int(current):
            raise VersionError(
                f"--android-code: {new} is not greater than the current {current}; "
                "Google Play requires the versionCode to increase strictly"
            )
        return str(new)

    bare = bare_semver(value, f"--{kind}")

    if kind == "dpd-fallback":
        text = read(root / "backend/src/dictionary_catalog.rs")
        m = PINNED_SERIES.search(text)
        if not m:
            raise VersionError("backend/src/dictionary_catalog.rs: PINNED_SERIES not found")
        series = (int(m.group(1)), int(m.group(2)))
        major, minor = (int(x) for x in bare.split(".")[:2])
        if (major, minor) != series:
            raise VersionError(
                f"--dpd-fallback: v{bare} is outside PINNED_SERIES {series}. Raising the "
                "series is a deliberate code change (see docs/dictionary-import-pipeline.md §12)"
            )
    return bare


AFTER_NOTES = {
    "app": "Rebuild the GUI (make build -B) and the CLI; make npm-packages packs the new version.",
    "db": "DB_VERSION is written into appdata.sqlite3 by the bootstrap; it takes effect with the next bootstrap run.",
    "android-code": "make android-aab / android-apk read it on the next build.",
    "dpd-fallback": (
        "FALLBACK_TAG must be a published tag of digitalpalidictionary/other-dictionaries. "
        "Check each CatalogueEntry's fallback_size_bytes against that release's assets."
    ),
}


def show(root: Path) -> int:
    status = 0
    for kind in PLACES:
        rows = current_values(root, kind)
        place_of = {p.path: p for p in PLACES[kind]}
        bares = {v[len(place_of[path].prefix):] if v.startswith(place_of[path].prefix) else v
                 for path, _, v in rows}
        bad_prefix = [(path, line, v) for path, line, v in rows if not v.startswith(place_of[path].prefix)]
        agree = len(bares) == 1 and not bad_prefix
        print(f"{kind}: {', '.join(sorted(bares))}" + ("" if agree else "  ** places disagree **"))
        for path, line, v in rows:
            prefix = place_of[path].prefix
            note = f"  ** should start with {prefix!r} **" if not v.startswith(prefix) else ""
            print(f"  {path}:{line}  {v}{note}")
        if not agree:
            status = 1
    return status


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Show or set the app, DB, Android and DPD-fallback versions in every place they are stored.",
        epilog="With no option, shows every place and exits 1 if the places of one kind disagree.",
    )
    parser.add_argument("--app", metavar="VERSION", help="The app version, also the CLI's and the npm packages' (e.g. 1.1.0-alpha.4)")
    parser.add_argument("--db", metavar="VERSION", help="The database version, DB_VERSION in the bootstrap")
    parser.add_argument("--android-code", metavar="N|next", help="The Android versionCode; must increase")
    parser.add_argument("--dpd-fallback", metavar="VERSION", help="The DPD other-dictionaries FALLBACK_TAG (e.g. 1.0.9)")
    parser.add_argument("--dry-run", action="store_true", help="Print the changes without writing them")
    parser.add_argument("--root", type=Path, default=ROOT, help=argparse.SUPPRESS)
    args = parser.parse_args()

    requested = {
        "app": args.app,
        "db": args.db,
        "android-code": args.android_code,
        "dpd-fallback": args.dpd_fallback,
    }
    requested = {k: v for k, v in requested.items() if v is not None}

    try:
        if not requested:
            return show(args.root)

        new_texts: dict[str, str] = {}
        reports = []
        for kind, value in requested.items():
            bare = resolve_value(args.root, kind, value)
            texts, report = plan_edits(args.root, kind, bare)
            for path in texts:
                if path in new_texts:
                    raise VersionError(f"{path} belongs to more than one kind; fix PLACES")
            new_texts.update(texts)
            reports.append((kind, bare, report))
    except VersionError as e:
        print(f"set-version: {e}", file=sys.stderr)
        return 1

    for kind, bare, report in reports:
        print(f"{kind}: {bare}")
        print("\n".join(report))

    changed = {p: t for p, t in new_texts.items()
               if t != read(args.root / p)}
    if args.dry_run:
        print(f"dry run: {len(changed)} file(s) would change, none written")
        return 0
    for path, text in changed.items():
        write(args.root / path, text)
    print(f"wrote {len(changed)} file(s)")
    changed_kinds = [kind for kind, _, report in reports if any("->" in line for line in report)]
    for kind in changed_kinds:
        print(f"note ({kind}): {AFTER_NOTES[kind]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
