# The CXX-Qt fork, and the move back to unpatched upstream

**Status: the app builds against unpatched upstream KDAB/cxx-qt 0.9.1.** The
fork is retired for Linux, Windows and Android. Nothing in `bridges/Cargo.toml`
points at it any more.

This document exists because the fork's rationale was previously recorded
**nowhere** — not in `docs/`, `PROJECT_MAP.md`, `AGENTS.md` or any PRD — so
anyone asking "what does our fork change, and can we drop it?" had to
re-derive the answer from a year-old diff. Source PRD:
`tasks/2026-08-07-175059-prd---cxx-qt-and-qt-6-10-3-android-upgrade.md`
(FR-1 – FR-10; moved to `tasks/archive/` when that work completes).

## 1. What the fork was

| | Value |
|---|---|
| Fork | `github.com/simsapa/cxx-qt.git`, branch `simsapa`, rev `8a597414`, crate version **0.7.2** |
| Fork base | upstream `c6710b71` (~1 yr 1 mo old at the time of the migration) |
| Net diff | **5 files, +69 / −7** |
| Consumed by | `bridges/Cargo.toml` — `cxx-qt`, `cxx-qt-lib`, `qt-build-utils`, `cxx-qt-build`, plus `CMakeLists.txt`'s `cxx-qt-cmake` at `GIT_TAG 0.7` |
| Local checkout | `src-lib/cxx-qt-simsapa/` (upstream is checked out separately at `src-lib/cxx-qt/`) |

Four commits, which **partly supersede each other** — `8a597414` reverts callers
added by `73b13685`. Read the cumulative `c6710b71..8a597414` diff, never commit
by commit:

```
321b4317  add --no-zstd to rcc command to build for Android
43260f9e  ios Resources path and lib filename without suffix
73b13685  builder.flag() and lipo thinning for ios
8a597414  lipo not needed, framework path fix for iOS build with Qt 6.10.1 / iOS 26.2 SDK
```

Their content is five distinct changes, of which **exactly one was needed for
Android** and four were Apple-only:

| # | Change | File | For | Disposition |
|---|---|---|---|---|
| **A** | rcc args `--no-zstd --format-version 1 --compress-algo zlib` | `qt-build-utils/src/tool/rcc.rs` | **Android** | **Dropped** — upstream feature replaces it (§2) |
| **B** | `flag_if_supported` → `flag` for `-F<framework>` | `qt-build-utils/src/installation/qmake.rs` | iOS | Re-apply on the `simsapa` branch at its new home, `installation/shared.rs` |
| **C** | framework `.prl` path: iOS flat vs `Versions/A/Resources` | `qt-build-utils/src/installation/qmake.rs` | iOS + macOS | **Not ported** — upstream replaced the mechanism (§3) |
| **D** | apple fallback `Some(filename)` when prefix/suffix strip fails | `qt-build-utils/src/parse_cflags.rs` | iOS + macOS | Applies cleanly; kept on the branch |
| **E** | `is_ios_target()` + `thin_generated_fat_library_with_lipo()` | `qt-build-utils/src/utils.rs`, `cxx-qt-build/src/lib.rs` | iOS | **Deleted** — dead code since `8a597414` removed its callers |

**The answer to "can we go back to upstream?" is yes for every platform we
ship.** For an Android-and-desktop project the fork carried nothing needed:
patch A is obsolete, and B–E are Apple-only with iOS dormant and macOS on its
own unfinished PRD.

## 2. Patch A is obsolete: `CXX_QT_AUTORCC_OPTIONS`

Patch A hardcoded three `rcc` flags because our Android build must not ship
zstd-compressed Qt resources. Upstream 0.9.1 has a supported path for exactly
that:

```
CXX_QT_AUTORCC_OPTIONS env var  (cxx-qt-build/src/lib.rs:1253-1258, split on ':')
  → QtBuild::autorcc_options()  (qt-build-utils/src/lib.rs:240, applied at :461)
  → QtToolRcc::custom_args()    (qt-build-utils/src/tool/rcc.rs:42, appended after --name)
```

**Set it from CMake, never from the shell.** cxx-qt-cmake 0.9.1 sets the variable
*itself*, joining CMake's own `CMAKE_AUTORCC_OPTIONS` with `:`
(`cmake/CxxQt.cmake:99,111` → `corrosion_set_env_vars` → `cmake -E env
VAR=VALUE cargo …`). An explicit assignment on the command line **overrides the
inherited environment**, so a shell-exported `CXX_QT_AUTORCC_OPTIONS` — e.g. from
`build-android.sh` — is silently discarded on every CMake-driven build, which is
all of them. The whole of the replacement is therefore three lines in
`CMakeLists.txt`:

```cmake
set(CMAKE_AUTORCC_OPTIONS --format-version 1)
list(APPEND CMAKE_AUTORCC_OPTIONS --compress-algo zlib)
list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)   # was fork patch A
```

Three consequences worth knowing:

- **One list now feeds both `rcc` invocations** — CMake's AUTORCC for
  `assets/icons.qrc`, and the bridge crate's. Under the fork they were
  independent and had drifted (`--no-zstd` existed only on the cxx-qt side).
  Post-0.9 they cannot diverge unless someone reintroduces a shell override.
- **Per-ABI propagation is structural.** Each per-ABI ExternalProject sub-build
  re-runs CMake over this same tree and sets the variable for its own cargo run.
  Verified on all three ABIs: the joined value
  `--format-version:1:--compress-algo:zlib:--no-zstd` appears in the top-level
  `build.ninja` and in `android_abi_builds/{x86_64,armeabi-v7a}/`, each producing
  an identical 2,046,446-byte rcc output. Nothing is exported anywhere.
- **⚠ Changing the options does not trigger a rebuild.** Upstream reads the
  variable with `env::var_os` and emits **no**
  `cargo::rerun-if-env-changed=CXX_QT_AUTORCC_OPTIONS`; the only declared env
  deps in the whole workspace are `QMAKE`, `QT_VERSION_MAJOR`,
  `QT_MINIMAL_DOWNLOAD_ROOT` and `TARGET`. Cargo happily reuses the previous rcc
  output while the build reports success. **Every experiment with this variable
  must `touch bridges/build.rs` or `cargo clean -p simsapa_bridges` first**, and
  "it worked first try" is suspicious until reproduced from clean. This is worth
  filing upstream — a one-line `println!` next to the read fixes it for everyone.

**Measured caveat: `--no-zstd` is currently a no-op.** With `--compress-algo
zlib` already in the list, Qt 6.9.3's rcc produces byte-identical output with and
without it (2,046,158 bytes both ways). So it was `--compress-algo zlib` doing
the work in patch A. `--no-zstd` is kept because patch A carried it and because
it preserves the guarantee if the compress-algo is ever changed — but do not
describe it as the load-bearing flag.

## 3. What stays on the `simsapa` branch, and why C is not ported

The branch is kept alive for **Apple only**: iOS is dormant, not abandoned, and
macOS has its own unfinished PRD. Nothing consumes it while `bridges/Cargo.toml`
points at upstream.

Patch **C** is not ported because upstream **replaced the mechanism**, not just
moved it: `shared.rs::find_prl_for_qt_module()` now searches `path_lib` for
`libQt6<Module><arch>.prl` with an arch-suffix loop, and no framework
`Resources/` path exists to patch. A mechanical re-apply is impossible.
**Re-derive C only if an Apple build actually fails.** Note when doing so that C
is not purely additive: it also rewrote the *non*-iOS path from upstream's
`…framework/Resources/….prl` to `…framework/Versions/A/Resources/….prl`, i.e. it
changed macOS behaviour too. Both halves are in play.

`is_ios_target()` is needed only by C, so it lives or dies with it. The `lipo`
helper is deleted outright rather than rebased — it has had no callers since
`8a597414`.

## 4. The 0.7 → 0.9 API migration

Upstream 0.8.0 removed the API `bridges/build.rs` used. This is a certainty, not
a risk, and it is why the crate bump and the build-script rewrite are **one
commit**.

| 0.7 | 0.9.1 |
|---|---|
| `CxxQtBuilder::new().qml_module(QmlModule { … })` | `CxxQtBuilder::new_qml_module(module)` |
| `QmlModule { uri, …, ..Default::default() }` | fields private, no `Default` → `QmlModule::new(uri)` builder style |
| `rust_files: &[…]` **inside** `QmlModule` | `CxxQtBuilder::files([…])` — **one directory per call** (QTBUG-93443; all nine bridges are under `src/`) |
| `.cc_builder(\|cc\| { cc.include(…); cc.file(…) })` | now `unsafe fn`; use the safe `include_dir()` / `cpp_files()` instead — no `unsafe` block is needed |

The bridge **macros are untouched**: `#[qinvokable]` ×339, `#[qsignal]` ×64,
`#[qproperty]` ×8, `#[qobject]` ×8, `#[qml_element]` ×8, `#[qml_singleton]` ×1,
`cxx_qt::Threading` ×9, `cxx_qt::CxxQtThread` ×2 — none appears in any 0.8/0.9
changelog "Removed" or "Changed" entry, and **no bridge source file needed
editing.** The feature flags survive too: keep `features = ["full"]` on
`cxx-qt-lib` and `features = ["link_qt_object_files"]` on `cxx-qt-build` (the
latter is required for statically linking Qt 6 — do not drop it).

One forced extra change: `cxx` had to move from `1.0.148` to **`1.0.176`**.
cxx-qt 0.9.1 requires `^1.0.176` and cargo would not resolve against the
lockfile's `1.0.169`.

**Pin revs, not branches, on both halves.** `bridges/Cargo.toml` pins upstream
rev `2180c12`; `CMakeLists.txt` pins `cxx-qt-cmake` at the **tag** `0.9.1`
(`06a121e`). The previous CMake pin, `GIT_TAG 0.7`, was the *branch* — kdab
publishes `refs/heads/0.7` alongside tags `0.7.0`–`0.7.3` — which is the same
non-reproducibility the Cargo pin was written to avoid, simply less visible. The
two pins are a **coupled pair**: a mismatch fails inside generated code, the
hardest place to read an error. Move them together.

## 5. The `..` trap: why the QML files are not `qml_files`

This is the one runtime-only defect the migration produced, and the reason
`bridges/build.rs` looks unusual. **A green build does not detect it** — the app
failed on its first launch with:

```
Type Logger unavailable
qrc:/qt/qml/com/profoundlabs/assets/qml/Logger.qml: No such file
```

— note the missing `simsapa/` segment.

**Cause: cxx-qt feeds a `qml_files` path string, verbatim, into three
derivations that disagree about a leading `../`.** Our list has always used
`"../assets/qml/Foo.qml"`, because the paths are relative to `bridges/`:

| Consumer | Result for `Logger.qml` |
|---|---|
| rcc alias (`qt-build-utils/src/lib.rs:364`) | `..` folded → `:/qt/qml/com/profoundlabs/simsapa/assets/qml/Logger.qml` ✅ |
| qmldir component line (**new in 0.8**) | `Logger 1.0 ../assets/qml/Logger.qml`, resolved as a URL against the module dir → one level too high ❌ |
| qmlcachegen (`tool/qmlcachegen.rs:74`) | `--resource-path /qt/qml/…/simsapa/../assets/qml/Logger.qml`, inserted **unnormalized** ❌ |

Under 0.7 the generated `qmldir` carried **no** component lines, so type lookup
fell through to implicit same-directory resolution and the mismatch was
invisible. 0.8's "correct QML module export" made the broken entry
authoritative.

**The fix: register the QML files with `CxxQtBuilder::qrc_resources` and derive
the alias in `build.rs`** (there is no alias API on `QmlFile`). The list keeps
its documented `"../assets/qml/Foo.qml"` form — so the rule for adding a QML
component is unchanged — and a malformed entry now `panic!`s at build time
naming the expected shape instead of failing when that screen is first shown.
`qt-build-utils` is in `[build-dependencies]` for `QResourceFile`; `cxx-qt-build`
re-exports only `QResource` and `QResources`.

Two alternatives were rejected: a `bridges/assets` **symlink** (breaks on Windows
without `core.symlinks`, and makes every QML file visible at two paths to
`rg`/`qmllint`/`cargo package`), and **`set_current_dir("..")`** (breaks
incremental builds — cxx-qt-build emits `rerun-if-changed` with the **raw** path
at `cxx-qt-build/src/lib.rs:459,586,645,964`, and cargo resolves relative rerun
paths against the package root, so they would point at `bridges/bridges/src/`).
Both also leave the trap armed: `"../assets/qml/Foo.qml"` still compiles and
still fails at runtime.

**Verified against the pre-change build**, not by inspection: the 87 registered
aliases are byte-identical to the old build's once its `../` is folded, the
prefix is still `/qt/qml/com/profoundlabs/simsapa`, **zero** `..` remain in the
generated `.qrc`, and the `qmldir` is back to its five-line 0.7 form. So every
`qrc:/qt/qml/com/profoundlabs/simsapa/assets/qml/*.qml` literal in `cpp/` (~16
sites) keeps resolving.

### Side finding: the AOT QML cache has never been used in this project

While choosing the fix it emerged that qmlcachegen's output was **never
consulted** — under 0.7 either. The generated loader inserts its keys raw
(`"/qt/qml/com/profoundlabs/simsapa/../assets/qml/SuttaSearchWindow.qml"`) but
looks them up through `QDir::cleanPath`, which strips `..`, so a key containing
`/../` can never match. **87 compiled units — 12.4 MB of generated C++ plus a
52 KB loader — were compiled into the binary and never used.** That is what made
the chosen fix free rather than a trade-off: not generating them loses nothing.

Enabling AOT for real is a separate, **measured** change, not a regression to
repair. It requires `..`-free paths, i.e. moving `assets/qml/` under `bridges/`
(alias-neutral, so no `qrc:` literal would change). Whether app UI belongs under
`bridges/` is a genuine design question, and if the measured win is small, not
moving is a legitimate outcome. A green build proves nothing there either —
unmatched AOT units fail silently by falling back to parsing source, which is
today's behaviour.

Two upstream defects worth reporting, with a one-file reproducer (`"../foo/Bar.qml"`):

1. a `qml_files` path containing `..` is folded by rcc but not by the qmldir
   writer or qmlcachegen, so the three disagree;
2. `qmlcache_loader.cpp` cleans the path on lookup but not on insert, so such
   keys are unreachable.

## 6. Related documents

- [qt-kit-selection.md](./qt-kit-selection.md) — which Qt each platform builds
  against, and the `CMakeLists.txt`-as-single-source arrangement that
  `CXX_QT_AUTORCC_OPTIONS` rides on.
- `AGENTS.md` / `CLAUDE.md` (a symlink to it) — "New Rust bridges" and "New QML
  components", which teach the 0.9 API and the `../` rule above.
