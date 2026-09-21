# QML AOT staleness: why every build goes through `qmltypes-guard.sh`

Adding or removing a `#[qinvokable]` or `#[qsignal]` on a bridge and then
building **once** produces a binary that segfaults on startup. Not a slow one,
not a warning — a `SIGSEGV` in `QMetaObject::methodOffset()` before the first
window paints, with a backtrace that points at a QML binding and names no
source file you edited.

`scripts/qmltypes-guard.sh` wraps every build target in the `Makefile` so this
cannot ship. This document is why it exists, what it does, and the two fixes
that look right and are not.

## 1. The mechanism

`qmlcachegen` AOT-compiles QML bindings to C++. A binding that calls a bridge
singleton, such as `SuttaSearchWindow.qml`'s

``` qml
readonly property bool render_use_flat_results_background: SuttaBridge.get_render_use_flat_results_background()
```

becomes generated C++ carrying a **hard-coded `QMetaObject` method index**:

``` cpp
aotContext->initCallObjectPropertyLookup(54, s.v2_0, 323);
```

`323` is the position of `get_render_use_flat_results_background` in
`SuttaBridge`'s metaobject — signals first, then slots, then invokables.
`qmlcachegen` takes it from `plugin.qmltypes`.

If that index does not match the metaobject the binary actually links, Qt calls
`QMetaObject::method(i)` with an index that walks off the end of the method
table, recurses into a garbage `superdata` pointer, and dies in
`methodOffset()`. There is no bounds check and no diagnostic.

## 2. Why a single build gets it wrong

The build script's own outputs, timestamped within one contiguous run:

| Time | Step |
|---|---|
| 07:11:53.99 | cxx-qt codegen (`cxxqtgen/`) |
| 07:11:55.40 | **moc** → `moc_sutta_bridge.cxxqt.h.cpp.json`, the authoritative metaobject |
| 07:11:56.12 | **`qmldir`** written |
| 07:11:57.13 – 07:12:03 | **`qmlcachegen`** compiles the QML tree |
| 07:12:05.72 | **`plugin.qmltypes`** written by `qmltyperegistrar` |

`qmlcachegen` reads `plugin.qmltypes` **nine seconds before `qmltyperegistrar`
writes it**. So it necessarily reads the copy the *previous* build left behind,
and every AOT index it bakes describes the previous build's bridge API.

Two consequences worth stating outright, because both are easy to guess wrong:

- **`qmldir` does not lag.** It is written one second *before* `qmlcachegen`
  runs, with current content. Adding a QML component is safe; the guard does not
  need to watch it.
- **moc's JSON is already on disk and already correct** two seconds before
  `qmlcachegen` runs. The data exists at the right moment; only the
  `qmltypes` transform of it is late. This is a step-ordering bug in
  `cxx-qt-build`, not a missing dependency.

`qmlcachegen` regenerates the **whole** QML tree on every build-script run, not
only the files that changed — which is why one stale input poisons every file
at once, and equally why one extra run fixes every file at once.

## 3. What the guard does

`scripts/qmltypes-guard.sh <build-dir> <command...>`:

1. Hash every `plugin.qmltypes` under `<build-dir>` as one set (paths included,
   so a newly added Android ABI counts as a change), and read the **stamp** —
   `<build-dir>/.qmltypes-guard-stamp`, the hash the last guarded build in this
   directory converged on.
2. Run the build.
3. Re-hash, and retry if **any** of three things holds:
   - the hash changed during the build (the obvious case: `qmlcachegen` read
     the pre-build file);
   - there is no stamp — a clean tree, where `qmlcachegen` had no `qmltypes` at
     all and silently dropped the bridge bindings from the AOT path, or a
     directory last built by a bare `cmake --build` that bypassed the guard;
   - the stamp disagrees with the current file — a previous guarded build did
     not converge.
4. To retry: touch the QML tree, build again, verify the hash settled, and
   write the stamp.

**The third condition is the one a before/after comparison alone misses**, and
it is reachable in ordinary use. If a build fails *after* the cargo build
script ran — say the bridge API changed and a C++ file further down the build
has an error — the stale AOT code is already on disk. Fix the C++ error and
build again, and cargo legitimately skips the build script, because no Rust
source changed since. The hash is then identical before and after, over stale
artifacts. Measured: with the API moved from index 323 to 324, that second
build reports `before == after` and the stamp is what catches it.

Step 4's touch is not cosmetic. Cargo decides for itself whether to re-run the
build script, and **nothing in the source tree changes between the two passes**
— so without a trigger the second pass is a no-op and the stale code survives.
This is why "just build twice" does not work as a manual habit: the second
`make build -B` rebuilds the C++ but skips the cargo build script entirely.
Touching the QML tree is the cheap trigger; it re-runs the build script without
forcing a Rust recompile, and contents are untouched — only mtimes move.

Hashing the *set* under a build directory is what makes one invocation cover a
multi-ABI Android build, where each ABI has its own `OUT_DIR` and its own
independent lag.

The retry's touch is verified rather than assumed: if the QML tree ever moves,
a touch matching nothing would make the retry a silent no-op and the guard
would then report success over stale code, so the script fails loudly instead.

The script uses `shasum -a 256` where GNU `sha256sum` is absent, and avoids
`sort -z` and `xargs -r`, because `make macos` runs it on BSD userland.

## 4. The two fixes that look right and are not

### Touching the QML tree before the build

Unconditionally, or conditionally on `bridges/src/` being newer. It does not
work, in either form: the touch happens *before* the build, and `qmltypes` is
still stale at the moment `qmlcachegen` reads it. All it guarantees is that the
build script runs. The lag then heals on the *next* build — silently, one build
late, which is exactly how the segfault reached a `make run`.

An mtime predicate is also wrong on its own terms: `git stash pop` and
`git checkout` stamp mtimes without changing content, and editing a *function
body* in `sutta_bridge.rs` fires it while leaving the metaobject identical.
The honest predicate — did `plugin.qmltypes` content change — is only knowable
after the build.

### Cleaning the build directory for production

A clean build is **safe but degraded**, which is worse than it sounds. With no
`plugin.qmltypes` present at all, `qmlcachegen` cannot type the bridge
singletons and silently drops those expressions from the AOT path. Measured
tree-wide over the 99 generated files:

| State | object-property lookups | singleton lookups | generated bytes |
|---|---|---|---|
| Clean tree, no `qmltypes` | 163 | 110 | 15,198,655 |
| Warm, API unchanged | 232 | 176 | 15,475,412 |
| Guard's second build | 232 | 176 | 15,475,412 |

A clean build therefore loses **66 of 176 singleton lookups (38%)** and **69 of
232 object-property lookups (30%)** — a third of the bridge-call sites in the
app fall back to interpreted bindings. A clean tree has no `qmltypes` on its
first build, and for a production target that is the only build, so a
"clean before release" policy would ship that degraded binary every time, with
no signal that it happened.

The guard's second build reproduces the warm-correct output **exactly** — all
three aggregates identical — so it is the only option that is both correct and
fast.

Cleaning for the other documented reasons (stale artifacts, the
`android-build/` wedging trap in
[android-multi-abi-and-chromeos.md](./android-multi-abi-and-chromeos.md))
remains fine on its own merits. Note that `appimage-clean` and `macos-clean`
remove only the AppDir/dmg/dist artifacts and never touch
`build/simsapadhammareader/`.

## 5. Coverage, and what is not covered

`appimage` and `macos` both depend on the `build` target and build into the
**same** `build/simsapadhammareader/`, so they inherit its guarded state.
Android targets are wrapped individually because they use their own build
directories.

| Path | Guarded |
|---|---|
| `make build`, `make build-offline`, `make run` | yes |
| `make appimage`, `make macos`, `make macos-app` | yes, via `build` |
| `make android-aab` / `-apk` / `-apk-debug` / beta targets | yes |
| A direct `./build-android.sh …` invocation | **no** — go through the `Makefile` |
| `build-windows.ps1` | **no** — no PowerShell equivalent yet |

Two limits of the guard itself:

- It detects "the input moved under me", not "the two tools disagree". If
  `qmlcachegen`'s view of a type ever differs from moc's *within* one build,
  re-running cannot help.
- `make build` alone is a no-op, because the `build:` target collides with the
  `build/` directory. Use `make build -B`, as the rest of the project's docs
  already say.

## 6. Recognising it in the wild

The backtrace to look for — note that it names a QML file you probably did not
touch, and a Qt function with no obvious connection to the bridge:

```
Thread 1 received signal SIGSEGV
#0  QMetaObject::methodOffset() const
#1  QMetaObject::method(int) const
#3  …SuttaSearchWindow_qml::aotBuiltFunctions::{lambda…}
```

To confirm it rather than guess, compare the baked index against moc's table:

``` sh
O=build/simsapadhammareader/cargo/build/x86_64-unknown-linux-gnu/debug/build/simsapa_bridges-*/out
grep -o 'initCallObjectPropertyLookup([0-9]*, s\.v2_0, [0-9]*)' \
  $O/qmlcachegen/com/profoundlabs/simsapa/SuttaSearchWindow.qml.cpp
```

then read `$O/moc/moc_sutta_bridge.cxxqt.h.cpp.json` and index its
`signals` + `slots` + `methods` lists concatenated in that order. If the name at
that index is not the one the binding calls, this is the bug, and one guarded
build fixes it.
