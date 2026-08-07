# PRD: CXX-Qt catch-up, then Qt 6.10.3 on Android

- **Date:** 2026-08-07
- **Status:** Draft (not yet implemented)
- **Scope:** **Android only.** Linux, Windows and macOS stay on Qt 6.9.3.
- **Two stages, in order:** (1) catch our CXX-Qt fork up to upstream,
  (2) move the Android build to Qt 6.10.3.

> **Supersedes an earlier draft of this file** that targeted Qt 6.11.1 on all
> platforms. The target changed to **6.10.3** and the scope narrowed to Android
> after 6.11.1 was uninstalled and 6.10.3 installed with all Android ABIs. Every
> measurement below has been **re-taken against 6.10.3**; the 6.11.1 figures are
> retained only where the contrast is instructive, and are labelled as such.

> **Reviewed and corrected 2026-08-07 (second pass).** Three findings changed
> the shape of the work and are folded in below rather than appended:
>
> 1. **The Linux desktop and AppImage builds are not on Qt 6.9.3 today** — they
>    resolve to Arch's **system Qt 6.11.1**, while cxx-qt is separately handed
>    `~/Qt/6.9.3/gcc_64/bin/qmake6`. §2.1 and FR-24 now treat this as a live
>    shipping defect, not a hygiene item, and Part C becomes a **prerequisite**
>    rather than "can land at any point".
> 2. **cxx-qt 0.8.0 removed the `QmlModule` API `bridges/build.rs` uses.** Open
>    question 3 is answered: yes, there are breaking changes, and they are in the
>    build script. New **FR-3b**.
> 3. **`CXX_QT_AUTORCC_OPTIONS` is not declared with `rerun-if-env-changed`**, so
>    changing it does not rebuild. Folded into FR-2.

Companion documents (read; not repeated here):

- [docs/android-soft-keyboard.md](../docs/android-soft-keyboard.md) §4 — the
  Gboard/Thai mid-word Shift bug that motivates this work.
- [docs/android-qt-upgrade-considerations.md](../docs/android-qt-upgrade-considerations.md)
  — the deferred work and pitfalls.
- [docs/qt-6.10.1-appimage-issues.md](../docs/qt-6.10.1-appimage-issues.md) —
  desktop breakage measured on 6.10.1; the reason desktop is out of scope.
- [docs/android-multi-abi-and-chromeos.md](../docs/android-multi-abi-and-chromeos.md)
  — multi-ABI mechanism, manifest/permission rules, the AGP pin.
- [docs/pure-rust-audio-backend.md](../docs/pure-rust-audio-backend.md) — the
  NDK constraint.
- `tasks/archive/2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md`
  — the source PRD for the deferred items. **In `tasks/archive/`.**

---

## 1. Introduction / Overview

On Android with Gboard and a Thai layout, pressing Shift **mid-word** is forced
straight back to the base layer. Thai's shift layer holds *distinct characters*,
not capitals, so those characters become untypeable — in **every text field in
the app**. There is no app-side fix: Qt's Android input-connection layer
restarts the input connection on every keystroke, and `restartInput()` is what
resets an IME's shift state.

The upstream fix is qtbase
[`f5c0296fdaad`](https://code.qt.io/cgit/qt/qtbase.git/commit/?id=f5c0296fdaad1f4f824e9bd96c525000f658fa81)
("Android: Add support for GET_EXTRACTED_TEXT_MONITOR", 2025-10-08, `Fixes:`
QTBUG-140694). It landed **eight days after** Qt 6.9.3 was released.

**Verified in the installed 6.10.3 sources:** `QtInputConnection.java` has **2**
`restartImmInput()` call sites (6.9.3 has 12) and contains
`GET_EXTRACTED_TEXT_MONITOR` / `m_isComposing`. **Qt 6.10.3 ships the fix.**

Whether it *resolves the Thai symptom* remains unproven and must be measured on
device — that measurement (FR-20) is the single most important acceptance test
here.

Standing in front of the Qt bump is a dependency problem. The CXX-Qt bridge
layer is pinned to **a fork** two minor versions behind upstream, whose four
patches exist to fix the **Android and iOS builds**. Upstream has since
refactored the exact files those patches touch. Stage 1 resolves that; stage 2
does the Qt bump.

## 2. Scope: why Android only, and why 6.10.3

**Android only.** The upgrade is motivated entirely by an Android text-entry
bug. Desktop platforms work on 6.9.3 and have measured, unresolved problems on
6.10.x — the AppImage's `libtiff.so.5`-vs-`.so.6` failure and the WebEngine
SIGSEGV under a FUSE mount. Coupling them would let a desktop packaging problem
block a functional fix for Thai users.

This **reverses an earlier decision in this PRD's history** to set all five
`QT_*` variables at once. The per-platform variables in `CMakeLists.txt:8-12`
exist precisely so the platforms can diverge; this is the case they were built
for. Only `QT_ANDROID` moves.

**6.10.3, not 6.11.1.** 6.10.3 carries the Thai fix (measured above) and is a
far more conservative step. Measured contrast in Qt's own Android Gradle
template:

| | Qt 6.9.3 (ours) | **Qt 6.10.3 (target)** | Qt 6.11.1 (rejected) |
|---|---|---|---|
| AGP | 8.8.0 | **8.10.1** | 9.0.0 |
| Gradle wrapper | 8.12 | **8.14.3** | 9.3.1 |
| Kotlin plugin | — | **—** | 2.3.0 (new) |
| `compileOptions` | Java 1.8 | **Java 1.8** | Java 17 |
| `packagingOptions.jniLibs` | absent | **absent** | present |
| Template delta vs 6.9.3 | — | **2 lines** | ~8 hunks |

6.10.3's whole template change is the AGP bump plus `-Xlint:all`. It stays
inside AGP 8.x, needs no Kotlin plugin, no Java-level change, and leaves
`useLegacyPackaging` ours to control. 6.11.1 would have forced all four at once.

### 2.1 The Linux build is not on 6.9.3 today — measured, and it must be

**"Linux stays on 6.9.3" describes an intent, not the current behaviour.**
Measured on the working tree 2026-08-07:

| Step | What actually happens |
|---|---|
| `Makefile:11` (non-Darwin `BUILD_CMD`) | `cmake -S . -B ./build/…` with **no** `-DCMAKE_PREFIX_PATH` |
| `CMakeLists.txt:23-74` | No Linux branch, so nothing sets one |
| `find_package(Qt6 …)` (`:192`) | Resolves `/usr/lib/cmake/Qt6` — Arch's system `qt6-base`, **version 6.11.1** (`/usr/bin/qmake6 -query QT_VERSION`) |
| `CMakeLists.txt:187` | Separately hands cxx-qt `~/Qt/${QT_LINUX}/gcc_64/bin/qmake6` — **6.9.3** |

So the desktop binary is **already built against two Qt versions at once**: the
C++/QML/CMake half against 6.11.1, the Rust bridge half against 6.9.3. The macOS
branch escapes this only because `Makefile:5-6` passes `-DCMAKE_PREFIX_PATH`
explicitly; Linux is the one platform with neither a CMake branch nor a Makefile
argument.

**The AppImage inherits it, and the ordering makes it worse.**
`build-appimage.sh` sets `QT_BASE_DIR` / `PATH` / `QMAKE` from
`~/Qt/6.9.3/gcc_64` inside `create_appimage()` (lines 187-221) — but
`build_app()` (lines 130-138) runs `make build -B` **before** that, at line 407,
with none of it set. The published AppImage is therefore a binary **compiled and
linked against system Qt 6.11.1** that linuxdeploy then bundles with **Qt 6.9.3**
libraries and plugins.

> Inferred from the sources above rather than from a run. Verify with
> `ldd build/simsapadhammareader | grep libQt6Core` and
> `strings build/simsapadhammareader | grep -m1 'Qt 6\.'` before and after
> FR-24 — enough to confirm the diagnosis and the fix, and no further.
>
> **This is not evidence for 6.11.1.** That the app builds and runs in this
> mixed state says nothing usable: it is bundled with 6.9.3 libraries, so no
> user has ever run a coherent 6.11.1 build of it. 6.11.1 remains rejected
> (§2, §6 non-goal 4) and the desktop target stays **6.9.3**. The accidental
> exposure is a defect to remove, not a result to build on.

**Decision (2026-08-07): Linux, including the AppImage, uses 6.9.3 as
intended.** `QT_LINUX` stays `6.9.3` and the build is made to honour it. This
converts FR-24 from tidy-up into the requirement that makes the desktop half of
this PRD true at all, and is why Part C is now sequenced **before** FR-8 (see
§7.1). The consequence to expect, and not to misread as breakage: **the first
Linux configure after FR-24 + FR-27 land will `FATAL_ERROR`** if
`~/Qt/6.9.3/gcc_64` is missing, where before it silently used whatever the
system had.

## 3. Goals

1. The CXX-Qt dependency is on a **supported, documented** footing — ideally
   unpatched upstream — with any remaining fork patches recorded and justified.
2. `QT_ANDROID` is `6.10.3`; the other four `QT_*` variables stay `6.9.3` and
   the desktop builds are **unaffected**.
3. The multi-ABI AAB builds on 6.10.3 and the **Thai mid-word Shift bug is
   verified fixed on device**.
4. The Android-relevant deferred work
   ([android-qt-upgrade-considerations.md](../docs/android-qt-upgrade-considerations.md)
   §2.1, §2.2, §2.4, §2.5, §2.6, §2.7) is discharged — each item applied or
   re-deferred with a measured reason.
5. A build configured against the wrong Qt **fails at configure time with a
   clear message**.
6. Desktop platforms are provably untouched: Linux still builds and tests green
   against 6.9.3 after every change — and, per §2.1, **actually against 6.9.3**,
   which is a change from today.
7. The Linux AppImage is compiled against the same Qt it bundles (6.9.3), not
   against whatever the build host happens to have installed.

## 4. User Stories

- **As a Thai-reading user,** I want Shift to work mid-word in the search field
  and the Gloss text area, so that I can type Thai script without shift-lock.
- **As an Android user,** I want the back gesture to dismiss the dialog or
  window I am in, not close the whole app.
- **As a Linux/Windows/macOS user,** I want nothing to change — my platform is
  not part of this work and must not regress.
- **As the maintainer,** I want to know what our CXX-Qt fork changes and whether
  it can be dropped, so the dependency stops being an unknown.
- **As the maintainer,** I want `cmake` to tell me immediately if it picked up
  the wrong Qt, rather than producing a binary that mixes two versions.

---

## 5. Functional Requirements

### Part A — CXX-Qt catch-up (stage 1, do this first)

The bridge layer is pinned to a fork: `bridges/Cargo.toml:21-23,51` names
`github.com/simsapa/cxx-qt.git` at rev `8a597414` for `cxx-qt`, `cxx-qt-lib`,
`qt-build-utils` and `cxx-qt-build`; `CMakeLists.txt:200` pins the matching
`cxx-qt-cmake` at `GIT_TAG 0.7`.

| | Value |
|---|---|
| Our fork | rev `8a597414`, crate version **0.7.2** |
| Fork base | `c6710b71` (upstream, ~1 yr 1 mo old) |
| Upstream now | **0.9.1** (`src-lib/cxx-qt`, KDAB) |
| Net fork diff | **5 files, +69 / −7** |
| Upstream CI Qt versions | 6.2.4, 6.7.3, **6.10.1** |

Upstream testing against 6.10.1 is why this upgrade is viable at 6.10.3 and
would have been speculative at 6.11.1.

**FR-1. Port the fork against current upstream in the `simsapa` branch of
`src-lib/cxx-qt-simsapa/`.** The four fork commits partly supersede each other
(`8a597414` reverts callers added by `73b13685`), so **work from the cumulative
`c6710b71..8a597414` diff, not commit by commit.** Five distinct changes:

| # | Change | File (fork) | Purpose | Upstream 0.9.1 state | Action |
|---|---|---|---|---|---|
| **A** | rcc args `--no-zstd --format-version 1 --compress-algo zlib` | `qt-build-utils/src/tool/rcc.rs` | **Android** | **Obsoleted by an upstream feature** — see FR-2 | **Drop the patch** |
| **B** | `flag_if_supported` → `flag` for `-F<framework>` | `qt-build-utils/src/installation/qmake.rs` | iOS | **Moved** to `installation/shared.rs:153`, still `flag_if_supported` | Re-apply at the new location, iOS only |
| **C** | framework `.prl` path — iOS flat vs `Versions/A/Resources` | `qt-build-utils/src/installation/qmake.rs` | iOS + macOS | **Mechanism replaced** — `shared.rs::find_prl_for_qt_module()` now searches `path_lib` for `libQt6<Module><arch>.prl` with an arch-suffix loop; no framework `Resources/` path exists | **Do not port.** Re-derive only if an Apple build fails |
| **D** | apple fallback `Some(filename)` when prefix/suffix strip fails | `qt-build-utils/src/parse_cflags.rs` | iOS + macOS | **Unchanged** — identical lines still present | Applies cleanly if needed |
| **E** | `is_ios_target()` + `thin_generated_fat_library_with_lipo()` | `qt-build-utils/src/utils.rs`, `cxx-qt-build/src/lib.rs` | iOS | Absent upstream | **Drop `lipo`** — dead code since `8a597414` removed its callers. `is_ios_target` is needed only by **C** |

All five files still exist at their original paths, so orientation is easy; it
is **C** where the code around it was rewritten and a mechanical re-apply is
impossible.

**FR-2. The Android patch is no longer needed — use the upstream feature
instead.** This is the finding that makes stage 1 tractable. Upstream 0.9.1 has
added a supported path for exactly what patch **A** hardcoded:

- `QtToolRcc::custom_args()` (`qt-build-utils/src/tool/rcc.rs:42`)
- `QtBuild::autorcc_options()` (`qt-build-utils/src/lib.rs:240`, applied at `:461`)
- driven by the **`CXX_QT_AUTORCC_OPTIONS` environment variable**, split on
  `':'` (`cxx-qt-build/src/lib.rs:1253-1258`)

> ### ⚠ Corrected 2026-08-07 (third pass, measured): set this from CMake, **not**
> from the shell
>
> An earlier version of this requirement said to export the variable in
> `build-android.sh`. **That would not work.** cxx-qt-cmake **0.9.1 already sets
> `CXX_QT_AUTORCC_OPTIONS` itself**, derived from CMake's own
> `CMAKE_AUTORCC_OPTIONS`:
>
> ```cmake
> # cxx-qt-cmake 0.9.1, cmake/CxxQt.cmake:99,111
> list(JOIN CMAKE_AUTORCC_OPTIONS ":" CXX_QT_AUTORCC_OPTIONS)
> corrosion_set_env_vars(${CRATE}
>   …
>   $<$<BOOL:${CMAKE_AUTORCC_OPTIONS}>:CXX_QT_AUTORCC_OPTIONS=${CXX_QT_AUTORCC_OPTIONS}>)
> ```
>
> Corrosion applies those with `${CMAKE_COMMAND} -E env VAR=VALUE cargo …`, and
> an explicit assignment on the command **overrides the inherited
> environment**. Since `CMAKE_AUTORCC_OPTIONS` is non-empty in this project
> (`CMakeLists.txt:80-81`), the generator expression is always active, so a
> shell-exported `CXX_QT_AUTORCC_OPTIONS` is silently discarded on every
> CMake-driven build — which is all of them.
>
> This is **new in 0.9.x**: cxx-qt-cmake 0.7.3 has no `AUTORCC_OPTIONS` handling
> at all, and our fork's `cxx-qt-build` does not read the variable — which is
> precisely why patch **A** hardcoded the flags.

So the wiring is a **one-line CMake change**, not an environment export:

```cmake
set(CMAKE_AUTORCC_OPTIONS --format-version 1)
list(APPEND CMAKE_AUTORCC_OPTIONS --compress-algo zlib)
list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)   # was fork patch A
```

which cxx-qt-cmake turns into
`CXX_QT_AUTORCC_OPTIONS=--format-version:1:--compress-algo:zlib:--no-zstd`
for the bridge crate's `rcc`.

**This structurally answers open question 5.** Each per-ABI ExternalProject
sub-build re-runs CMake over this same source tree, so it sets the variable for
its own cargo invocation. Nothing has to propagate through the environment.

**Consequence to decide deliberately: the two `rcc` lists now converge.**
`--no-zstd` added here reaches **both** invocations — CMake's AUTORCC for
`assets/icons.qrc` as well as cxx-qt's for the bridge. That is safe (`--no-zstd`
is a valid `rcc` option in both 6.9.3 and 6.10.3, verified by `rcc --help`), and
it is what upstream's design intends: post-0.9 the cxx-qt list is *derived from*
the CMake list and cannot diverge unless someone reintroduces a shell override.
**This inverts the "do not tidy the two lists into agreement" advice below**,
which described the pre-migration world where the fork hardcoded its own list.
Keep the paired comments (they explain the derivation), but the asymmetry itself
goes away.

If the `--no-zstd` flag ever needs to be Android-only, scope the `list(APPEND …)`
inside `if (ANDROID)` rather than reintroducing an environment export.

> ### ⚠ Changing this variable does not trigger a rebuild
>
> Upstream reads it with a plain `env::var_os` in `CxxQtBuilder::build()` and
> **never emits `cargo::rerun-if-env-changed=CXX_QT_AUTORCC_OPTIONS`** — the only
> declared env deps in the whole workspace are `QMAKE`, `QT_VERSION_MAJOR`,
> `QT_MINIMAL_DOWNLOAD_ROOT` and `TARGET`. So setting, changing or removing the
> variable leaves cargo happily reusing the previous `rcc` output, and the build
> succeeds while the resources are still compressed the old way.
>
> **Every experiment with this variable must force `build.rs` to re-run** —
> `touch bridges/build.rs` or `cargo clean -p simsapa_bridges`. Bake that into
> whatever mechanism sets it, and treat a "it worked first try" result with
> suspicion until a from-clean build reproduces it. This is the most likely way
> to lose a day on FR-2.
>
> Consider filing it upstream; a one-line `println!` in `cxx-qt-build` fixes it
> for everyone, and would be a cheap first contribution back.

**Superseded by the box above — retained because it describes the *pre*-migration
state accurately.** Under the fork, the two lists were related but **not
identical**: `CMAKE_AUTORCC_OPTIONS` (`CMakeLists.txt:80-81`) sets only
`--format-version 1` and `--compress-algo zlib`. **`--no-zstd` appears on the
cxx-qt side only.** Two different `rcc` invocations are involved — CMake's
AUTORCC for the app's own `.qrc` files, and cxx-qt's for the bridge's — so the
asymmetry is not obviously a bug, but it is also not obviously intended: the
fork patch simply hardcoded all three. **Do not "tidy" the two lists into
agreement without establishing which flags each invocation actually needs.**
Comment both sites to point at each other so a future reader sees the pair.

**Consequence: for an Android-only upgrade the fork carries nothing we need.**
Patches B–E are all Apple-only, and iOS is dormant.

**FR-3. Move `bridges/Cargo.toml` to unpatched upstream, pinned at the current
upstream revision.** **Decided 2026-08-07.** Point `cxx-qt`, `cxx-qt-lib`,
`qt-build-utils` and `cxx-qt-build` at `github.com/KDAB/cxx-qt.git` — the same
four-crate shape as today, just a different repo and rev — pinned to the
current upstream commit (**`2180c12`**, version **0.9.1**, in
`src-lib/cxx-qt`). Keep `features = ["full"]` on `cxx-qt-lib` and
`features = ["link_qt_object_files"]` on `cxx-qt-build` (the latter is required
for statically linking Qt 6 — **do not drop it**).

Pin a **rev, not a branch**, mirroring the existing comment's reasoning at
`bridges/Cargo.toml:20`: a branch re-resolves to the latest commit on every
build and makes the dependency non-reproducible.

**Accepted risk, deliberately taken.** Two distinct risks, previously conflated:

- **Dropping Apple fork patches C and D** affects macOS and iOS **only**. Both
  are out of scope; macOS already does not link for unrelated reasons (§10.1).
  Linux and Windows cannot regress from this, because neither patch is on their
  code path — an earlier draft argued the safety of dropping C/D by pointing at
  the next Linux build, which proves nothing about them.
- **The 0.7.2 → 0.9.1 jump itself** is what puts Linux and Windows at risk, and
  that risk is real: two minor versions of a code-generation crate, with known
  removals in `cxx-qt-build` (FR-3b). It surfaces **at build time**, loudly, on
  the very next desktop build — which FR-8 runs before the Qt bump. That is a
  cheap and immediate signal, so attempting upstream first is the right order.

If a desktop platform does break, the fallback is the rebased `simsapa` branch
from FR-6, not a return to the old 0.7.2 pin.

**FR-3b. Migrate `bridges/build.rs` to the 0.9 builder API.** **This is a
certainty, not a risk** — verified against `src-lib/cxx-qt` at `2180c12`. The
0.8.0 changelog's "CXX-Qt-build: QML modules no longer include Rust files" and
"Only allow one QML module per `CxxQtBuilder`" describe exactly the API the
build script uses:

| `bridges/build.rs` today | cxx-qt 0.9.1 |
|---|---|
| `CxxQtBuilder::new().qml_module(QmlModule { … })` | `qml_module()` is **gone** → `CxxQtBuilder::new_qml_module(module)` (`cxx-qt-build/src/lib.rs:436`) |
| `QmlModule { uri, rust_files, qml_files, ..Default::default() }` | all fields `pub(crate)`, no `Default` → builder style: `QmlModule::new("com.profoundlabs.simsapa").qml_files([…])` (`qml_modules.rs:16-25`) |
| `rust_files: &["src/api.rs", …]` (9 bridges) | **removed from `QmlModule`** → `CxxQtBuilder::files([…])` (`lib.rs:451`). One directory only per call (`lib.rs:877`); ours are all under `src/`, so this is satisfied |
| `.cc_builder(\|cc\| { … })` | now `pub unsafe fn` (`lib.rs:674`) — but **no `unsafe` is needed**: our closure only calls `cc.include()` and `cc.file()`, which have safe equivalents `include_dir()` (`lib.rs:517`) and `cpp_files()` (`lib.rs:641`) |

The **bridge macros are unaffected** — a survey of `bridges/src/` finds
`#[qinvokable]` ×339, `#[qsignal]` ×64, `#[qproperty]` ×8, `#[qobject]` ×8,
`#[qml_element]` ×8, `#[qml_singleton]` ×1, `cxx_qt::Threading` ×9,
`cxx_qt::CxxQtThread` ×2, and none of these appears in any 0.8/0.9 changelog
"Removed" or "Changed" entry. The feature flags survive too:
`cxx-qt-lib`'s `full` and `cxx-qt-build`'s `link_qt_object_files` both still
exist in 0.9.1.

Two things to check that fail **at runtime, not build time** (so FR-9 is the
only guard):

- **How 0.9 derives each QML file's resource path** from
  `"../assets/qml/Foo.qml"`. The `:/qt/qml/com/profoundlabs/simsapa/…` paths are
  load-bearing across the codebase; a changed basename-vs-relative-path rule
  breaks QML loading silently.
- **`qmldir` ownership.** 0.8.0 added "correct QML module export (qmldir, .qml
  files, qmltypes etc)" for qmllint/qmlls. We hand-maintain
  `assets/qml/com/profoundlabs/simsapa/qmldir` and its type stubs. Confirm
  upstream does not now generate a competing one.

**This also invalidates project documentation.** `CLAUDE.md` and `AGENTS.md`
both instruct the reader to add new bridges to "the `rust_files` list in
`bridges/build.rs`" and show a `QmlModule { … rust_files: &[…] }` snippet. Both
become wrong the day FR-3 lands — see FR-31.

**FR-4. Bump `cxx-qt-cmake` to `GIT_TAG 0.9.1`** (`CMakeLists.txt:200`). The
CMake handoff and the Rust crates are a **coupled pair**; a mismatch fails
inside generated code, which is the hardest place to read an error.

> **The current pin is a branch, not a tag.** `git ls-remote` on
> `kdab/cxx-qt-cmake` shows `refs/heads/0.7` alongside tags `0.7.0`-`0.7.3`;
> `GIT_TAG 0.7` matches the **branch** (`7ea06dd`, currently equal to tag
> `0.7.3`). That is precisely the non-reproducibility FR-3 rejects for
> `bridges/Cargo.toml` — already live in CMake and previously unnoticed. So:
> **pin the tag `0.9.1` (`06a121e`), not the branch `0.9`**, even though they
> point at the same commit today. Add the same "a branch re-resolves on every
> build" comment here that `bridges/Cargo.toml:20` carries.

**FR-5. `cxx-qt` 0.9.1 declares `rust-version = "1.85.0"`. ✅ SATISFIED
2026-08-07.** The active toolchain is **1.96.1** (stable), with
`aarch64-linux-android`, `armv7-linux-androideabi`, `thumbv7neon-linux-androideabi`
and `x86_64-linux-android` installed. Caveat worth knowing: there is **no
`rust-toolchain.toml`** in the repo, so nothing pins or enforces this — a
contributor on an older stable would hit it, and re-check if the toolchain is
ever downgraded.

**FR-6. Keep the `simsapa` branch alive for Apple.** Rebase B, D and (if
re-derived) C onto upstream and keep them on the branch, even though nothing
consumes them while `bridges/Cargo.toml` points at upstream. iOS is dormant, not
abandoned, and macOS has its own unfinished PRD. **Drop the `lipo` helper
entirely** rather than rebasing dead code.

**FR-7. Document the fork.** Its rationale is currently recorded **nowhere** —
not in `docs/`, `PROJECT_MAP.md`, `AGENTS.md`, `CLAUDE.md` or any PRD, archived
or current. Write it down (FR-31): what each patch was for, which upstream
release absorbed or obsoleted it, and what remains. Without this the next person
faces the same archaeology.

**FR-8. Verify stage 1 on desktop Linux against Qt 6.9.3, before any Qt
change.** This is what makes the two stages independently attributable: a cxx-qt
regression must be visible while Qt is still the known-good version. Run
`make build -B`, `make test`, and launch the app.

> **Ordering, changed by §2.1: FR-24 must land first.** "Against Qt 6.9.3" is
> not what a Linux build does today — it resolves system Qt **6.11.1**. Running
> FR-8 before FR-24 would verify stage 1 against a Qt this PRD does not target
> and, worse, against a *mixed* pair (6.11.1 C++ / 6.9.3 cxx-qt), so a failure
> would be unattributable in exactly the way FR-10 is trying to prevent. Land
> FR-24 (with FR-26/FR-27), confirm the configure log names
> `~/Qt/6.9.3/gcc_64`, and only then treat FR-8's result as meaningful.

> Known timing-assertion drift in `cargo test` is not a regression from this
> work.

**FR-9. Re-verify every `#[qinvokable]` bridge and the QML module registration
on device.** A bridge that fails to register produces a **runtime QML error, not
a build error** — a green build proves nothing here. cxx-qt is a
code-generation dependency and this is a two-minor-version jump.

Because FR-3b rewrites how QML files and Rust files are declared, extend this to
the **resource layer**, which is the part most likely to break invisibly:

- Every window and dialog opens (the `qml_files` list in `bridges/build.rs` is
  long, and a mis-registered file fails only when that screen is first shown).
- `:/qt/qml/com/profoundlabs/simsapa/…` paths still resolve.
- The `Logger`, the bridge singletons and `qmllint` still see the module.
- Run this on **desktop first** (FR-8), where the feedback loop is seconds, not
  an APK install.

**FR-10. Stage 1 lands as its own commit (or commits), separately from any Qt
change.** If stage 1 cannot be made to work, stop and reassess: the Qt bump on
top of a broken bridge layer produces unattributable failures.

### Part B — Qt 6.10.3 on Android (stage 2)

**FR-11. Set `QT_ANDROID` to `6.10.3`** in `CMakeLists.txt:11`. **Leave
`QT_LINUX`, `QT_MACOS`, `QT_WINDOWS` and `QT_IOS` at `6.9.3`.**

```cmake
set(QT_LINUX   "6.9.3")
set(QT_MACOS   "6.9.3")
set(QT_WINDOWS "6.9.3")
set(QT_ANDROID "6.10.3")   # Thai mid-word Shift fix; Android only
set(QT_IOS     "6.9.3")
```

Add a comment recording *why* Android diverges, so the split does not look like
an oversight to be "tidied up".

**FR-12.** `build-android.sh:25`'s `QT_ANDROID_VERSION` default must become
`6.10.3`, ideally derived from `QT_ANDROID` in `CMakeLists.txt` (FR-24) rather
than hardcoded a second time.

**FR-13. The multi-ABI AAB must build** for `arm64-v8a;x86_64;armeabi-v7a` via
`make android-aab`, against the 6.10.3 kits (`android_arm64_v8a`,
`android_x86_64`, `android_armv7` — all confirmed installed, alongside `Src`).

> 6.10.3 also installs a fourth kit, **`android_x86`**, which we deliberately do
> not ship. That is the reason `build-android.sh` avoids
> `QT_ANDROID_BUILD_ALL_ABIS` — it would autodetect that kit and demand the
> `i686-linux-android` Rust target, which is not installed and which nothing
> needs. Keep the explicit `ANDROID_ABIS` list. (§7.3 says "all four ABI kits
> installed"; this is the same fact, and both statements are true.)

**FR-13b. Both `~/Qt/6.9.3` and `~/Qt/6.10.3` must remain fully installed —
including both `gcc_64` kits.** They serve different roles and neither is
redundant:

| Kit | Role |
|---|---|
| `6.9.3/gcc_64` | Builds the **desktop app** (`QT_LINUX` stays 6.9.3) |
| `6.10.3/gcc_64` | Supplies **host tools** (moc, rcc, androiddeployqt) for the Android cross-build. The Android kit hardcodes this path as `__qt_platform_initial_qt_host_path` (`Qt6Dependencies.cmake:15-16`), so it resolves automatically and nothing sets `QT_HOST_PATH` |
| `6.10.3/android_*` | The Android target kits |

Consequence worth stating because it is easy to misread as a bug: **the Android
build runs 6.10.3's `rcc` while the desktop build runs 6.9.3's.** That is
correct — host tools must match the target Qt — and it is the `rcc` that
FR-2's `CXX_QT_AUTORCC_OPTIONS` applies to.

**FR-13c. Delete the two empty Qt leftovers. ✅ DONE 2026-08-07.**
`~/Qt/6.10.1` and `~/Qt/6.8.3` each held **zero files** — 16 KB of empty
directories (`gcc_64/resources/locales`) left behind by the MaintenanceTool,
with no `bin/`, `lib/`, `plugins/` or `qmake`, and nothing in either that was
missing from 6.10.3. Removed.

`~/Qt` now holds exactly the two real installs, **6.9.3 and 6.10.3** (12 GB
each), which is the state FR-13b requires. Qt Creator's `qtversion.xml` had
registered ten already-dangling qmake paths across the two dead versions; it
dropped them by itself once the directories were gone, so its Qt Versions list
now shows only 6.9.3 and 6.10.3 — no stale single-ABI Android kits left to
produce a confusing IDE build.

**FR-14. NDK — unchanged; pin it explicitly; do NOT install r28.**

**The NDK is a non-variable in this upgrade.** Qt 6.9.3 *and* 6.10.3 were both
built against NDK **27.2.12479018** (`modules/Core.json` in each kit; 6.11.1 was
too). The machine has **27.3.13750724** (`r27d`, clang 18.0.4), which is what
shipped 1.0.0 to Play.

**Keep 27.3.13750724.** Do not downgrade to match Qt exactly: r27c and r27d
share an NDK major and clang major, libc++ ABI is stable within a major, and
27.3 is the only known-good data point. A clean build would **not** prove a
downgrade safe — most NDK problems are loud (r28's `pthread_cond_clockwait`
failure is a *link* error) but codegen differences and runtime-resolved paths
(cpal→AAudio, JNI, unwinding) are not. Hold 27.2.12479018 in reserve as a
single-variable diagnostic lever.

**Do not install r28:** Qt does not ask for it; **both** Qt's auto-detect
(`QtAutoDetectHelpers.cmake`, `SORT … ORDER DESCENDING`, `GET 0`) and
`build-android.sh:45` (`ls … | sort -V | tail -1`) take the **highest installed**
NDK, so installing it silently swaps the compiler; the libc++
`pthread_cond_clockwait` exclusion still holds at minSdk 28 (bionic needs 30+);
and its only draw — default 16 KB alignment — is redundant (FR-17).

**Therefore:** change `build-android.sh:45` from "newest installed" to an
explicit pin with a clear error if absent, keeping the `ANDROID_NDK_ROOT`
override. `android/build.gradle:56` already carries `ndkVersion androidNdkVersion`,
so the pin propagates into Gradle with no new plumbing.

**FR-15. Raise `minSdkVersion` 27 → 28** in `android/build.gradle:153`
(`defaultConfig`) — the only source; there is no `<uses-sdk>` in the manifest.
**Decided 2026-08-07: do it as part of this upgrade.**

> **Correction to an earlier draft, which does not change the decision.** The
> claim that "Qt 6.11.1 declares `QT_ANDROID_MIN_SDK_VERSION "28"` explicitly,
> which is why this is no longer ignorable" **does not apply to 6.10.3.**
> 6.10.3's `Qt6AndroidMacros.cmake:309` only *reads* the target property,
> exactly as 6.9.3 does, and androiddeployqt's `main.cpp:1183` is byte-identical
> in both. **On 6.10.3 the raise is a deliberate choice, not a forced one.**

The reason to do it anyway is unchanged and independent of the target version:
Qt has declared a floor of 28 since before 6.9.3 and we override it back down,
so the app has been shipping one API level below what Qt says it supports. A Qt
upgrade — which re-tests the whole Android surface on device — is the cheapest
moment to align the two, because the verification is happening regardless.

Confirm the generated `qtMinSdkVersion` at build time, and verify the result
with `aapt2 dump badging | grep minSdkVersion` — **never** by reading the
generated `gradle.properties`. Note in the release notes that Play stops
offering updates to API 27 devices; existing installs keep the last compatible
version.

**FR-16. Predictive back — keep the opt-out. Settled by research.**

Qt 6.10.3 **still registers no `OnBackInvokedCallback`** (zero matches across
its Android Java sources) and still relies on the legacy `KEYCODE_BACK` path.
Meanwhile **Qt's own manifest template now ships the same opt-out we do** —
`android:enableOnBackInvokedCallback="false"` is present in 6.10.3's
`src/android/templates/AndroidManifest.xml` and absent from 6.9.3's. Our
workaround is upstream-sanctioned.

Keep `android:enableOnBackInvokedCallback="false"`
(`android/AndroidManifest.xml:115`). Two follow-ups:

- **Placement differs** — Qt puts it on `<application>`, ours is on `<activity>`.
  Both work (activity overrides application), but after the template re-merge
  (FR-18) ensure it appears **exactly once**, deliberately placed.
- Still run the four back cases in FR-22; the attribute is unchanged but the Qt
  beneath it is not.

**FR-17. The 16 KB link flag.** `CMakeLists.txt:340` carries
`target_link_options(simsapadhammareader PRIVATE "-Wl,-z,max-page-size=16384")`.
Qt 6.10+ ships 16 KB page support, so it should be redundant. **Measure before
removing:** build with and without, comparing
`readelf -lW <lib>.so | awk '/LOAD/{print $NF}' | sort -u` across **every**
64-bit library (sweep, do not sample) plus `zipalign -c -P 16 4 <apk>`. If in
any doubt, keep it — it costs nothing and its absence is silent until a device
rejects the library. `armeabi-v7a` at `0x1000` is correct, not a failure.

**FR-18. AGP, Gradle wrapper, JDK — a modest, in-8.x step.**

Qt 6.10.3's template delta from 6.9.3 is **two lines**: AGP `8.8.0` → `8.10.1`,
and `options.compilerArgs += ['-Xlint:all']`. Nothing else changes.

- Bump `android/build.gradle:14` from AGP `8.6.0` toward **8.10.1**.
- **AGP 8.10 requires Gradle ≥ 8.11.1**, so our wrapper
  (`android/gradle/wrapper/gradle-wrapper.properties:3`, currently **8.10**)
  **must** move — Qt ships 8.14.3 and matching it is the safe choice. The
  wrapper is **ours**, checked into the repo; Qt's copy is never used.
- **Keep the JDK pin** (`MAX_JDK_MAJOR=21` in `build-android.sh`) and re-verify.
  The failure mode is a `lintVitalAnalyzeRelease` crash whose *entire* message
  is the JDK version number, **after** all three ABIs have compiled and signed.
- Re-apply our local additions: the `androidComponents { beforeVariants }`
  debug-variant switch, the `packagingOptions.jniLibs.excludes` cross-ABI
  workaround, `useLegacyPackaging` (FR-19), and the hardcoded `minSdkVersion` /
  `targetSdkVersion`.
- **Separate commit from the Qt bump**, with a build in between.

> **Trap — the template reads `qtTargetSdkVersion` / `qtMinSdkVersion`.** Qt's
> template uses `minSdkVersion qtMinSdkVersion` and `targetSdkVersion
> qtTargetSdkVersion`; **our** copy hardcodes 27 and 36 instead. That local
> divergence is exactly what makes the existing note "androiddeployqt writes a
> stale `qtTargetSdkVersion=35` that `build.gradle` never reads" true.
> Re-merging without re-applying the hardcoded values would silently drop the
> app to **targetSdk 35** — a Play compliance regression. Coordinate with FR-15,
> which touches the same `defaultConfig` block, and confirm with `aapt2 dump
> badging`.

**FR-19. `useLegacyPackaging` stays ours to decide.**

> **Correction to an earlier draft.** Qt **6.11.1** moved this into its template
> as `useLegacyPackaging = legacyPackaging`, defaulting to `false`, which would
> have flipped our behaviour as a merge side effect. **6.10.3's template has no
> `packagingOptions.jniLibs` block at all** — so `android/build.gradle:59`'s
> `packagingOptions.jniLibs.useLegacyPackaging true` remains a purely local
> setting and nothing flips underneath us.

Re-evaluate it on its merits. If flipped, measure **both ways**: AAB/APK size,
on-device install footprint, `zipalign -c -P 16 4`, and `readelf -lW` `p_align`
for the app `.so` and a Qt library. If the measurements do not clearly favour
flipping, leave it `true` and record the measurement. Note this becomes an
upstream-controlled property whenever Qt 6.11+ is eventually adopted.

**FR-20. Verify the Thai mid-word Shift fix on device.** The acceptance test the
whole upgrade exists for. Re-run the exact table from
[android-soft-keyboard.md](../docs/android-soft-keyboard.md) §4:

| Where | Layout | Expected after upgrade |
|---|---|---|
| Search field (`SearchBarInput.qml`) | Thai | Shift works mid-word |
| Gloss text area (`GlossTab.qml`) | Thai | Shift works mid-word |
| Search field | US | Still works (`dHaMmA`) — no regression |

If it is **not** fixed, that is a finding of equal value: record it in §4 of that
doc and re-open the question of what else restarts the input connection. Do not
silently drop the item.

**FR-21. Re-verify the multi-ABI mechanism** end to end: the per-ABI
ExternalProject setup, the `if(NOT CMAKE_PREFIX_PATH)` per-ABI guard (which
relies on Qt **not** forwarding the parent's `CMAKE_PREFIX_PATH` to sub-builds —
re-confirm on 6.10.3), the cross-ABI plugin-staging bug and its
`packagingOptions.jniLibs.excludes` workaround, and the `armeabi-v7a` →
`armv7-linux-androideabi` corrosion mapping (**not** thumbv7neon).

**FR-22. On-device checks** beyond FR-20: back navigation from the sutta reader,
the tab list dialog, the search help dialog and the Chanting Practice window;
the safe-area top inset (one inset, not zero and not doubled); the `DrawerMenu`
"Menu" label in portrait *and* landscape; the soft keyboard raising on the
**first** tap.

**FR-23. Permissions and features must not change.** `android/AndroidManifest.xml`
has androiddeployqt's `<!-- %%INSERT_PERMISSIONS -->` / `<!-- %%INSERT_FEATURES -->`
markers **deleted** on purpose. Confirm with `aapt2 dump badging` that no new
permission or *required* `<uses-feature>` has appeared and that the app is still
offered to ChromeOS devices in the Play device catalogue. A regression here is
the July 2026 Chromebook incident repeating.

**FR-23b. Non-arm64 runtime validation.** `x86_64` and `armeabi-v7a` have been
built, packaged and installed but **never functionally run**. This upgrade
changes many surfaces at once, so run at least the x86_64 slice: audio
record/playback (cpal AAudio), fulltext search (tantivy index files), dictionary
lookup, SAF file save (`android_saf.rs`, `ndk_context`). If no non-arm64 device
is available, **say so explicitly** rather than implying it was tested.

**FR-23c. The private-Qt include — re-check, do not assume.**
`cpp/android_raw_pick.cpp:55` includes `<QtCore/private/qandroidextras_p.h>` and
calls `QtAndroidPrivate::startActivity` (`:186`). Private headers carry **no API
or ABI guarantee across minor versions**, making this the most version-fragile
C++ in the project. (Against 6.11.1 the `startActivity` declaration region was
byte-identical to 6.9.3's; **re-run that comparison against 6.10.3**, since that
is the kit being adopted.) Record the result in
[file-selection-test.md](../docs/file-selection-test.md) so the next upgrade
re-runs the check rather than rediscovering the exposure.

**FR-23d. Deprecated bar-colour APIs will remain in Play's report.** On 6.11.1
`getStatusBarColor` / `setStatusBarColor` / `setNavigationBarColor` had moved
into a new `QtWindowInsetsController.java` with *more* call sites, not fewer.
Re-check the location on 6.10.3 and update
[android-qt-upgrade-considerations.md](../docs/android-qt-upgrade-considerations.md)
§2.3 — but **do not treat clearing that report as a goal**; it is not achievable
by this upgrade and should be removed from the list of reasons to upgrade.

### Part C — Qt selection hygiene (applies to all platforms)

This part exists because the build does **not** reliably use the Qt it names,
and an Android/desktop version split makes that dangerous rather than merely
untidy. See §7.2.

> **Part C is a prerequisite, not a cleanup.** §2.1 measured the Linux build
> using system Qt **6.11.1** while claiming 6.9.3. Until FR-24/FR-26/FR-27 land,
> "Linux stays on 6.9.3" is not a true statement about this project, FR-8 cannot
> mean what it says, and Success Metric 3 cannot be evaluated. Land Part C
> **first**.

**FR-24. Add the missing Linux `CMAKE_PREFIX_PATH` branch — but NOT with a bare
`else()`.**

**This is the requirement that makes `QT_LINUX` real.** Today it is consulted
only by `qmake_path` at line 187 (the cxx-qt half), never by `find_package`, so
the two halves of the build resolve different Qt installs — see §2.1 for the
measurement and the AppImage consequence.

The actual structure is **two separate top-level blocks**, not one chain:

```
 23: if (WIN32 AND NOT CMAKE_PREFIX_PATH)
 37: elseif (APPLE AND NOT IOS)
 74: endif()
 …
 96: if (ANDROID)          <-- a SEPARATE block
123:     if(NOT CMAKE_PREFIX_PATH)   <-- Android's own guard
```

Linux has no branch, so `find_package(Qt6)` resolves from ambient `PATH` /
CMake defaults while `qmake_path` (line 187) is pinned from `QT_LINUX`.

> ### ⚠ A bare `else()` before line 74 would break the Android build silently
>
> An earlier draft of this requirement proposed exactly that. **Do not do it.**
> On Android, `WIN32 AND NOT CMAKE_PREFIX_PATH` is false and
> `APPLE AND NOT IOS` is false, so an unguarded `else()` **catches Android**
> and sets `CMAKE_PREFIX_PATH` to the **desktop Linux `QT_LINUX` kit**. The
> `if (ANDROID)` block at line 96 then finds `CMAKE_PREFIX_PATH` already set,
> its own guard at line 123 skips, and the Android build links against
> **Qt 6.9.3 for Linux** — wrong version *and* wrong platform.
>
> Under this PRD that failure is especially nasty: the build would appear to
> succeed and ship **without the Thai fix**, which is the only reason the
> upgrade exists. It is the exact defect Part C is meant to eliminate, so
> introducing it here would be self-defeating. `else()` would also catch
> Windows whenever `CMAKE_PREFIX_PATH` is already set.

Guard the branch explicitly instead. `ANDROID` is `UNIX`-true in CMake, so it
**must** be excluded by name:

```cmake
elseif (UNIX AND NOT APPLE AND NOT ANDROID)   # Linux desktop only
    if(NOT CMAKE_PREFIX_PATH)
        if(EXISTS "$ENV{HOME}/Qt/${QT_LINUX}/gcc_64")
            set(CMAKE_PREFIX_PATH "$ENV{HOME}/Qt/${QT_LINUX}/gcc_64")
        elseif(EXISTS "/opt/Qt/${QT_LINUX}/gcc_64")
            set(CMAKE_PREFIX_PATH "/opt/Qt/${QT_LINUX}/gcc_64")
        else()
            message(FATAL_ERROR
                "Qt ${QT_LINUX} not found under $ENV{HOME}/Qt or /opt/Qt. "
                "Install it, or configure with -DCMAKE_PREFIX_PATH=<qt-dir>.")
        endif()
    endif()
    message(STATUS "Using CMAKE_PREFIX_PATH: ${CMAKE_PREFIX_PATH}")
endif()
```

The inner `if(NOT CMAKE_PREFIX_PATH)` guard is **also required** — an
explicitly-passed prefix path must win, and multi-ABI depends on it.

**Verification for this requirement specifically:**

1. Configure for **Linux** and confirm the status line names
   `~/Qt/6.9.3/gcc_64` — not `/usr/lib/cmake/Qt6`. Cross-check the built binary
   with `ldd build/simsapadhammareader/simsapadhammareader | grep libQt6Core`;
   it must resolve under `~/Qt/6.9.3`, where today it resolves to `/usr/lib`.
2. Configure for **Android** and confirm `CMAKE_PREFIX_PATH` resolves under
   `~/Qt/6.10.3/android_<abi>`, not `~/Qt/6.9.3/gcc_64`.

FR-27's assertion is the backstop that would catch either mistake, which is a
further reason to land FR-26/FR-27 **before or with** this change rather than
after.

> **Expect the Linux configure to start failing where it used to succeed.**
> A machine without `~/Qt/6.9.3/gcc_64` gets a `FATAL_ERROR` instead of silently
> falling back to system Qt. That is the intent, and it is the only way the
> AppImage can be compiled against the Qt it bundles — but it is a change in
> developer-visible behaviour and belongs in the commit message and in the new
> Qt-kit-selection doc (FR-31), not just here.

**FR-25. `FATAL_ERROR`, not `WARNING`, for a missing Qt kit — but only for the
two that mean "Qt not found".**

`CMakeLists.txt` has **four** `message(WARNING …)` calls. Only two are in scope:

| Line | Message | Action |
|---|---|---|
| **32** | "Qt6 installation not found in standard Windows locations…" | **→ `FATAL_ERROR`** |
| 51 | "macOS SDK not found via xcrun, using system default" | **Leave as `WARNING`** |
| **70** | "Qt6 installation not found in standard locations…" (APPLE) | **→ `FATAL_ERROR`** |
| 120 | "ANDROID_ABI not detected, defaulting to android_arm64_v8a" | **Leave as `WARNING`** |

Lines 51 and 120 are **legitimate fallbacks with sensible defaults** — the
macOS SDK falls back to the system default, and an undetected `ANDROID_ABI`
falls back to `arm64-v8a`. Converting them would break working builds for no
benefit. A blanket "convert all `message(WARNING)` to `FATAL_ERROR`" is the
obvious wrong reading of this requirement; convert **32 and 70 only**.

> **While here: comment the *other* bare `else()`.** `CMakeLists.txt:186` — in
> the **second** block, the one selecting `qmake_path` — is already a bare
> `else()` meaning "Linux". It is safe **only** because `if (ANDROID)` is the
> first branch of that same chain, unlike the first block where Android is not
> handled at all. Once FR-24 gives the first block an explicitly-guarded
> `elseif (UNIX AND NOT APPLE AND NOT ANDROID)`, the two blocks will look
> gratuitously inconsistent and someone will "harmonise" them — in whichever
> direction is at hand. One comment at line 186 stating why the bare `else()` is
> correct *there* prevents that.

**FR-26. Make `find_package(Qt6 …)` `REQUIRED`.** `CMakeLists.txt:192` has no
`REQUIRED`, so a failed or partial find does not stop the configure — leaving
`Qt6_VERSION` empty and `Qt6_DIR` as `Qt6_DIR-NOTFOUND`. This is a
**prerequisite** for FR-27 and FR-28, not an independent nicety.

**FR-27. Assert the found Qt matches the intent.**

```cmake
if(NOT Qt6_VERSION VERSION_EQUAL "${qt_expected_version}")
    message(FATAL_ERROR
        "Qt version mismatch: expected ${qt_expected_version}, "
        "found ${Qt6_VERSION} at ${Qt6_DIR}")
endif()
```

Set `qt_expected_version` from the appropriate `QT_*` in each platform branch.
**This is what makes the Android/desktop version split safe** — with two Qt
versions in play on one machine, picking up the wrong one is now a live
possibility rather than a theoretical one. Note Arch's system `qt6-base` is
itself a Qt 6.x at `/usr/bin/qmake6`, distinct from any `~/Qt/<version>` kit.

**FR-28. Derive `qmake_path` from the Qt that `find_package` found.** FR-27
*detects* divergence; this makes it **impossible**. Four of five branches
currently rebuild the path independently from `$ENV{HOME}/Qt/${QT_*}/…` (lines
154, 165, 174-181, 187). **The Android branch already does it correctly** —
line 129 derives from `${ANDROID_QT_DIR}`, the same variable feeding
`CMAKE_PREFIX_PATH` at line 123. Extend that pattern. There is a clean window:
`find_package(Qt6 …)` is at line **192** and `cxx_qt_import_crate(…)` begins at
line **207** (its `QMAKE ${qmake_path}` argument is line 211), so `Qt6_DIR` is
known ~15 lines before `qmake_path` is consumed.

```cmake
get_filename_component(_qt_prefix "${Qt6_DIR}/../../.." ABSOLUTE)
set(qmake_path "${_qt_prefix}/bin/${qmake_exe_name}")
if(NOT EXISTS "${qmake_path}")
    message(FATAL_ERROR "qmake not found at ${qmake_path} (from Qt6_DIR=${Qt6_DIR})")
endif()
```

Keep each branch's executable **name** in `qmake_exe_name` and derive only the
directory. Do **not** compute the suffix from `CMAKE_EXECUTABLE_SUFFIX` — when
cross-compiling it describes the target, not the host. (The Android kit ships
both `qmake` and `qmake6`, so there is slack there; the real divergence is
Windows' `.exe`.)

> ### ⚠ Do not "simplify" this by dropping the `QMAKE` argument
>
> cxx-qt-cmake 0.9.1 will fall back to the Qt that `find_package` found if
> `QMAKE` is omitted — `get_target_property(QMAKE Qt::qmake IMPORTED_LOCATION)`
> at `cmake/CxxQt.cmake:33-38`, with a `FATAL_ERROR` if neither is available.
> That looks like an even purer single-source than the derivation above, and on
> desktop it is. **It is wrong for Android.** `Qt::qmake` resolves to the
> **host** qmake (`~/Qt/6.10.3/gcc_64/bin/qmake6`, an x86-64 ELF), whereas
> `~/Qt/6.10.3/android_arm64_v8a/bin/qmake` is a **POSIX shell wrapper** that
> applies the Android `qt.conf`. Handing cxx-qt the host binary would make
> `qt-build-utils` query desktop paths for a cross-build.
>
> Keep passing `QMAKE` explicitly on every platform, derived as above. Record
> the fallback's existence in the new Qt-kit-selection doc so the next reader
> does not rediscover it as an "obvious cleanup".

**FR-29. Delete the bare-`qmake6` Windows fallback** (`CMakeLists.txt:181`).
`set(qmake_path "qmake6")` resolves from ambient `PATH` — the defect in its
purest form, already shipped. Replace with a `FATAL_ERROR`.

**FR-30. Build scripts derive their version from `CMakeLists.txt`**, environment
override still winning:

| File | Line(s) | Source |
|---|---|---|
| `build-android.sh` | 25 | `QT_ANDROID` |
| `build-appimage.sh` | 194-199 | `QT_LINUX` |
| `build-macos.sh` | 103-110 | `QT_MACOS` |
| `build-windows.ps1` | 8, 27, 107, 116, 178 | `QT_WINDOWS` |
| `Makefile` | 5 | `QT_MACOS` |

Also remove or loudify `build-appimage.sh`'s final `elif command -v qmake6`
fallback (lines **198-199**) — ambient `PATH` again.

**FR-30b. The AppImage must be compiled against the Qt it bundles.** This is the
concrete payoff of Part C on the desktop side, and it needs one more change
beyond FR-24.

`build-appimage.sh` resolves `qt6_path` and exports `QT_BASE_DIR` / `PATH` /
`QMAKE` / `LD_LIBRARY_PATH` inside **`create_appimage()`** (lines 187-221), but
`build_app()` (lines 130-138) runs `make build -B` **before** that — `main()`
calls `build_app` at line 407. So the compile step sees none of it, and the
shipped AppImage is a binary built against system Qt bundled with
`~/Qt/6.9.3/gcc_64`'s libraries (§2.1).

Required:

- **Hoist the Qt resolution** out of `create_appimage()` into a helper called
  before `build_app()`, so one `qt6_path` serves both the compile and the
  bundling.
- **Assert the two agree.** After configuring, check the Qt that CMake reports
  matches `qt6_path`; fail loudly if not. FR-27 covers the CMake half, but the
  AppImage's bundling half is a separate decision made by a different script and
  deserves its own check.
- **`build_app()`'s "already built" short-circuit is a hazard here** — it skips
  `make build -B` whenever `$BUILD_DIR/simsapadhammareader` exists, so a binary
  left over from a *differently-configured* build gets packaged. At minimum warn
  that the existing binary's Qt was not verified; better, drop the
  short-circuit for release builds.
- Verify the result: `ldd` the binary inside the AppDir and confirm every
  `libQt6*.so.6` resolves under the bundled Qt, and that
  `strings … | grep -m1 'Qt 6\.'` reports **6.9.3**.

> Line numbers in Part C are **pre-change**; FR-24 shifts everything below it.
> Re-locate by content.

### Part D — Documentation

**FR-31.** Update:

- **A new doc on the CXX-Qt fork** (FR-7) — what each patch was for, what
  upstream absorbed, what remains on the `simsapa` branch, and the
  `CXX_QT_AUTORCC_OPTIONS` mechanism that replaced the Android patch.
- **A new doc on Qt-kit selection** — `CMakeLists.txt` as single source, scripts
  deriving from it, the FR-27 assertion, system `qt6-base` vs `~/Qt/<version>`,
  **and the Android-on-6.10.3 / desktop-on-6.9.3 split with its reason.** Must
  also record: the **pre-fix state** measured in §2.1 (Linux silently on system
  6.11.1 while `QT_LINUX` said 6.9.3, and the AppImage compiled against one Qt
  and bundled with another) so the defect is recognisable if it recurs; that
  `~/Qt/6.9.3/gcc_64` is now **required** on a Linux dev machine; and the
  `Qt::qmake` fallback trap from FR-28.
- **`CLAUDE.md` and `AGENTS.md`, "New Rust bridges" and "New QML components"** —
  both currently document the **removed** cxx-qt 0.7 API (`rust_files` inside
  `QmlModule`, the `.qml_module(QmlModule { … })` snippet). Rewrite for the 0.9
  builder API per FR-3b. These are the instructions every future bridge is added
  by, so a stale version here costs more than a stale doc.
- [android-soft-keyboard.md](../docs/android-soft-keyboard.md) §4 — the measured
  on-device result, either way.
- [android-qt-upgrade-considerations.md](../docs/android-qt-upgrade-considerations.md)
  — §1 version table, §2.1-2.7, §3 reasons (drop the deprecated-API one), §4
  pitfalls. Fix its "Source PRD" citation to point at `tasks/archive/`.
- [qt-6.10.1-appimage-issues.md](../docs/qt-6.10.1-appimage-issues.md) — record
  that desktop deliberately stayed on 6.9.3 and why.
- [android-multi-abi-and-chromeos.md](../docs/android-multi-abi-and-chromeos.md)
  — AGP/wrapper versions, NDK pin.
- [pure-rust-audio-backend.md](../docs/pure-rust-audio-backend.md) — NDK outcome.
- [file-selection-test.md](../docs/file-selection-test.md) — FR-23c result.
- `AGENTS.md` / `CLAUDE.md` — the AGP pin section, the NDK r28 rule, the
  Qt-version-per-platform note.
- `PROJECT_MAP.md` — if the Qt-selection changes alter what it describes.

---

## 6. Non-Goals (Out of Scope)

1. **Desktop Qt upgrades.** Linux, Windows and macOS stay on 6.9.3. The
   AppImage's libtiff and WebEngine/FUSE problems are explicitly **not** being
   solved here.
2. **iOS.** Stays dormant. `QT_IOS` stays at 6.9.3. Apple fork patches are
   preserved on the branch (FR-6) but not exercised.
3. **The macOS build fix.** `make macos -B` does not link, for reasons unrelated
   to this work; that is `tasks/2026-08-05-175516-prd---macos-build-audio-framework-linking.md`.
4. **Qt 6.11.x.** Evaluated and rejected for now (§2).
5. **AGP 9.x.** Qt 6.10.3 ships AGP 8.10.1; staying in 8.x is now aligned with
   upstream rather than a divergence from it.
6. **Any shell-configuration change** (`~/.profile`, fish, `environment.d`). The
   build must be correct with an empty Qt-related environment.
7. **New features.** No QML, no signals, no schema, no migrations.
8. **CI.** None exists for these builds and none is added.

---

## 7. Technical Considerations

### 7.1 Order of work

**Part C (Qt selection) → Stage 1 (CXX-Qt) → verify on Linux/6.9.3 → Stage 2
(Qt 6.10.3 Android).**

Part C moved to the front in the 2026-08-07 review. It was previously "can land
at any point, most valuable before stage 2"; §2.1 showed that without it the
Linux verification step in the middle of that sequence does not verify what it
claims to, which removes the whole point of the ordering.

Within each stage, change **one variable at a time**. Stage 2's own sub-order:
Qt bump → build → AGP/Gradle cluster → build → minSdk/packaging decisions. Each
of these fails late and unhelpfully; combined, failures are unattributable.

Stage 1's own sub-order is now also non-trivial: **FR-3b (`build.rs` migration)
lands with FR-3/FR-4 in one commit**, because the crate bump does not compile
without it. That commit is therefore larger than "change three git revs" —
budget for it.

### 7.2 Why Part C is not optional here

**Correction to an earlier draft.** It claimed: *"Until now, one Qt version was
installed and used everywhere, so the missing Linux `CMAKE_PREFIX_PATH` branch
was invisible."* **Both halves are wrong.** Two Qt versions have coexisted all
along (`~/Qt/6.9.3` and Arch's system `qt6-base`, now **6.11.1**), and the
missing branch has not been invisible — it has been actively selecting the wrong
one, in the same build that hands cxx-qt the right one (§2.1). This PRD does not
*create* a two-version machine; it makes the existing two-version machine
**legible**, and adds a third target version for Android.

The same class of defect appears in five places — the toolchain chosen by what
happens to be present rather than what the project declares:

| Site | Defect |
|---|---|
| `CMakeLists.txt` lines 23-74 | No Linux branch — `find_package` silently takes system Qt **6.11.1** while `QT_LINUX` says 6.9.3 |
| `CMakeLists.txt:181` | `set(qmake_path "qmake6")` — bare, from `PATH` |
| `CMakeLists.txt:200` | `GIT_TAG 0.7` is a **branch**, not a tag — re-resolves on every fetch (FR-4) |
| `build-appimage.sh:198-199` | `elif command -v qmake6` fallback — **and** `build_app()` compiles before any Qt env is set (FR-30b) |
| `build-android.sh:45` + Qt's own auto-detect | "newest installed NDK wins" |

Remedies, strongest first: **structural single-source** (FR-28 — cannot drift);
**assertion** (FR-27 — detects drift); **fail-fast instead of fallback**
(FR-25, FR-29, FR-30, FR-14); **documentation** (FR-31 — weakest, and what the
project relied on until now).

### 7.3 Facts established by measurement (do not re-derive)

| Fact | Evidence |
|---|---|
| **Qt 6.10.3 contains the Thai fix** — 2 `restartImmInput()` sites vs 12 in 6.9.3; `GET_EXTRACTED_TEXT_MONITOR` + `m_isComposing` present | `~/Qt/6.10.3/Src/qtbase/.../QtInputConnection.java` |
| 6.10.3 Android kits built against **NDK 27.2.12479018**, `api_version android-36` — **identical to 6.9.3** | `modules/Core.json` in each kit |
| 6.10.3 template delta from 6.9.3 is **2 lines**: AGP 8.8.0→8.10.1, `-Xlint:all` | `diff` of the two `src/android/templates/build.gradle` |
| Qt 6.10.3 ships Gradle wrapper **8.14.3** (6.9.3: 8.12); **ours is 8.10** and is the one actually used | `src/3rdparty/gradle/.../gradle-wrapper.properties` |
| 6.10.3 has **no** `packagingOptions.jniLibs` block — `useLegacyPackaging` stays ours (6.11.1 *would* have taken it) | same template |
| 6.10.3 does **not** force minSdk 28 — `Qt6AndroidMacros.cmake:309` reads the property exactly as 6.9.3 does; `androiddeployqt/main.cpp:1183` identical | both kits |
| Qt 6.10.3 registers **no** `OnBackInvokedCallback`, but **its manifest template ships the opt-out** (6.9.3's does not) | Android Java sources; `templates/AndroidManifest.xml` |
| All four Android ABI kits + `gcc_64` + `Src` installed for 6.10.3 | `ls ~/Qt/6.10.3/` |
| **Only 6.9.3 and 6.10.3 are real installs under `~/Qt`** (12 GB each). `~/Qt/6.10.1` and `~/Qt/6.8.3` were uninstall leftovers containing **zero files** and are now removed | `du -sh ~/Qt/*`; `find … -type f \| wc -l` = 0 |
| **6.11.1 is NOT gone — it is Arch's system `qt6-base`**, at `/usr/lib/cmake/Qt6` with `/usr/bin/qmake6`, and it is what a Linux `find_package(Qt6)` resolves to today. An earlier draft's "6.11.1 is gone entirely" referred only to the `~/Qt/6.11.1` **kit** and was misleading | `/usr/bin/qmake6 -query QT_VERSION` → `6.11.1` |
| **The Linux build mixes two Qt versions**: `find_package` → system 6.11.1, `qmake_path` (`:187`) → `~/Qt/6.9.3`. `Makefile:11` passes no `-DCMAKE_PREFIX_PATH`; only the Darwin branch (`:5-6`) does | §2.1 |
| **The AppImage compiles before it chooses a Qt** — `build_app()` (`:130-138`, called at `:407`) runs `make build -B`; `create_appimage()` sets `QT_BASE_DIR`/`PATH`/`QMAKE` only afterwards (`:187-221`) | `build-appimage.sh` |
| **cxx-qt 0.8.0 removed `QmlModule::rust_files` and `CxxQtBuilder::qml_module`**; `cc_builder` is now `unsafe`. `bridges/build.rs` uses all three | `cxx-qt-build/src/lib.rs:436,451,674`; `qml_modules.rs:16-25`; CHANGELOG 0.8.0 |
| The bridge **macros** are unchanged across 0.7.2→0.9.1 — `#[qinvokable]`, `#[qsignal]`, `#[qproperty]`, `#[qobject]`, `#[qml_element]`, `#[qml_singleton]`, `cxx_qt::Threading`, `cxx_qt::CxxQtThread` appear in no "Removed"/"Changed" entry | CHANGELOG 0.8.0-0.9.1 vs. survey of `bridges/src/` |
| `cxx-qt-lib`'s `full` and `cxx-qt-build`'s `link_qt_object_files` features **still exist** in 0.9.1 | each crate's `Cargo.toml` `[features]` |
| **`CMakeLists.txt:200`'s `GIT_TAG 0.7` is a branch**, not a tag (`refs/heads/0.7` = `7ea06dd`). The 0.9 equivalent tag is **`0.9.1`** = `06a121e`, identical to the head of branch `0.9` | `git ls-remote --heads --tags kdab/cxx-qt-cmake` |
| **Nothing declares `rerun-if-env-changed` for `CXX_QT_AUTORCC_OPTIONS`** — only `QMAKE`, `QT_VERSION_MAJOR`, `QT_MINIMAL_DOWNLOAD_ROOT`, `TARGET` are declared workspace-wide | grep of `src-lib/cxx-qt/crates/` |
| **cxx-qt-cmake 0.9.1 falls back to `Qt::qmake` `IMPORTED_LOCATION`** when `QMAKE` is omitted — the **host** qmake, wrong for Android, whose kit `bin/qmake` is a POSIX shell wrapper | `cmake/CxxQt.cmake:33-38`; `file ~/Qt/6.10.3/android_arm64_v8a/bin/qmake` |
| Toolchain is **Rust 1.96.1**, ≥ the 1.85.0 cxx-qt 0.9.1 requires; all four Android targets installed; **no `rust-toolchain.toml`** pins it | `rustup show` |
| **The Android 6.10.3 kit hardcodes `~/Qt/6.10.3/gcc_64` as its host Qt** — `__qt_platform_initial_qt_host_path`. Host tools (moc, rcc, androiddeployqt) come from there automatically; nothing needs to set `QT_HOST_PATH`, which is why our build never has | `android_arm64_v8a/lib/cmake/Qt6/Qt6Dependencies.cmake:15-16` |
| Fork net diff is **5 files, +69/−7**; the last commit reverts callers added by the third | `git diff c6710b71 8a597414` |
| **Upstream 0.9.1 supports `CXX_QT_AUTORCC_OPTIONS`** (colon-separated) → `autorcc_options()` → `QtToolRcc::custom_args()`, obsoleting the Android fork patch | `cxx-qt-build/src/lib.rs:1253`; `qt-build-utils/src/lib.rs:240,461`; `tool/rcc.rs:42` |
| **cxx-qt-cmake 0.9.1 sets `CXX_QT_AUTORCC_OPTIONS` itself**, joining `CMAKE_AUTORCC_OPTIONS` with `:` and passing it via `corrosion_set_env_vars` — which corrosion applies as `cmake -E env VAR=VALUE cargo …`, **overriding any shell-exported value**. 0.7.3 has no such handling | `cxxqt-cmake@0.9.1 cmake/CxxQt.cmake:99,111`; `Corrosion.cmake:570,1139` |
| `--no-zstd` is a valid `rcc` option in **both** 6.9.3 and 6.10.3, so adding it to `CMAKE_AUTORCC_OPTIONS` is safe for the app's own `.qrc` files too | `~/Qt/<ver>/gcc_64/libexec/rcc --help` |
| **`cxx_qt_import_qml_module` survives unchanged in cxx-qt-cmake 0.9.1** — same `URI` / `SOURCE_CRATE` arguments, plus a new optional `OUTPUT_DIR`. `CMakeLists.txt:216` needs no edit. `cxx_qt_import_crate`'s argument surface is likewise unchanged (`MANIFEST_PATH` / `CRATES` / `LOCKED` pass through to corrosion as `UNPARSED_ARGUMENTS`) | `diff` of 0.7.3 vs 0.9.1 `cmake/CxxQt.cmake` |
| **The generated `qmldir` / `plugin.qmltypes` go to `${CMAKE_CURRENT_BINARY_DIR}/cxxqt/qml_modules/<uri>/`** — the *build* directory, never the source tree. They cannot overwrite the hand-maintained `assets/qml/com/profoundlabs/simsapa/qmldir`, which is a qmllint stub with a different consumer | `cxx-qt-build/src/lib.rs:967-984`, `dir.rs:118-127`; `CxxQt.cmake:29` (`CXX_QT_EXPORT_DIR` default) |
| `QmlModule::files()` **panics** (does not silently misbehave) if the Rust sources span more than one directory — Qt bug QTBUG-93443. Our nine bridges are all under `src/` | `cxx-qt-build/src/lib.rs:868-882` |
| 0.9's `QmlFile` carries a `singleton(bool)` flag. **None of our compiled `qml_files` is a `pragma Singleton`** — the only one in the tree is `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml`, a qmllint stub that is not passed to cxx-qt | `qt-build-utils/src/qml/qmlfile.rs:30-39`; `grep -rl "pragma Singleton" assets/qml/` |
| The `Qt::qmake` `IMPORTED_LOCATION` fallback is **not new in 0.9.1** — it is already present in the pinned 0.7.3, at the same lines. FR-28's box applies to the status quo, not to a new hazard | `_deps/cxxqt-src/cmake/CxxQt.cmake:33-38` |
| Upstream **replaced** the framework `.prl` mechanism — `shared.rs::find_prl_for_qt_module()` searches `path_lib` with an arch-suffix loop; no framework `Resources/` path | `qt-build-utils/src/installation/shared.rs:14-31` |
| `parse_cflags.rs`'s apple branch is **unchanged** upstream; `flag_if_supported` for `-F` moved to `shared.rs:153` | upstream sources |
| Upstream CI tests Qt **6.2.4, 6.7.3, 6.10.1** — 6.10.3 is in range, 6.11.1 was not | `.github/workflows/` |
| cxx-qt 0.9.1 needs **Rust 1.85.0** | upstream `Cargo.toml:40` |
| The project has **no Qt-version-conditional code of its own** | zero matches for `qt_version_at_least` / `QT_VERSION_CHECK` / `#if QT_VERSION` in `bridges/src/`, `backend/src/`, `cpp/` |

### 7.4 Traps

- **`aapt2 dump badging` is the only trustworthy check** for package identity,
  SDK levels, permissions and features. The generated `gradle.properties`
  carries a stale `qtTargetSdkVersion=35`.
- **Never hand-delete `android-build/`** — it wedges the tree. Use
  `make android-clean`.
- **Never build release packages from Qt Creator** — single-ABI kits; an
  arm64-only bundle is filtered off Intel/AMD Chromebooks.
- **Installing any NDK changes the build** (highest-wins on both paths).
- **A stale `build/` caches `CMAKE_PREFIX_PATH`** — reconfigure clean when
  changing Qt version, or FR-27 may pass against a cached value.
- **`ANDROID_VERSION_CODE` must strictly increase** for any Play upload
  (`android/version.txt`).
- `.simsapa-package-identity` handles beta↔non-beta switching; ninja would
  otherwise report the previous artifact.
- **`bridges/Cargo.toml`'s cxx-qt pin is not platform-scoped.** Stage 1 changes
  the bridge layer for **every** platform even though the Qt bump is
  Android-only — hence FR-8.
- **A bare `else()` in the platform block catches Android**, not just Linux —
  the `if (ANDROID)` block is *separate*, at line 96. See FR-24's boxed warning;
  this would silently build Android against desktop Qt 6.9.3, i.e. ship with no
  Thai fix while appearing to succeed.
- **Only 2 of the 4 `message(WARNING)` calls are "Qt not found."** Lines 51 and
  120 are working fallbacks. See FR-25.
- **Two `rcc` invocations exist** (CMake AUTORCC and cxx-qt's). Under the fork
  they had different flag lists; from 0.9 the cxx-qt list is **derived from**
  `CMAKE_AUTORCC_OPTIONS` by cxx-qt-cmake, so they converge. See FR-2.
- **Exporting `CXX_QT_AUTORCC_OPTIONS` from a shell script is a no-op** under
  any CMake-driven build — corrosion sets it explicitly with `cmake -E env` and
  wins. Set `CMAKE_AUTORCC_OPTIONS` instead. See FR-2's box.
- **`CXX_QT_AUTORCC_OPTIONS` does not trigger a rebuild when it changes** — no
  `rerun-if-env-changed` upstream, and this applies to the CMake-set value too.
  Force `build.rs` to re-run for every experiment, or you will be reading stale
  `rcc` output. See FR-2's box.
- **Do not drop `cxx_qt_import_crate`'s `QMAKE` argument.** The 0.9.1 fallback
  picks the *host* qmake, which is wrong for Android. See FR-28's box.
- **`CMakeLists.txt:200` pins a git *branch*** (`0.7`), not a tag — the same
  non-reproducibility the Cargo pin was written to avoid. See FR-4.
- **The Linux build silently uses system Qt 6.11.1 today**, and the AppImage
  compiles against it before bundling 6.9.3. Anyone reasoning about "our Qt
  6.9.3 desktop behaviour" from before this PRD is reasoning about a mixed
  build. See §2.1, FR-24, FR-30b.
- **`build_app()` skips the build when the binary already exists**, so a stale
  binary from a different Qt configuration can be packaged into an AppImage
  without any warning. See FR-30b.
- **`bridges/build.rs` will not compile after the crate bump** — the 0.7
  `QmlModule` API is gone. Plan FR-3/FR-4/FR-3b as one commit. See FR-3b.
- **`CLAUDE.md` / `AGENTS.md` teach the removed cxx-qt 0.7 build API.** Anyone
  adding a bridge from the docs after stage 1 writes code that does not compile.
- **Two `gcc_64` kits are both live and neither is redundant** — 6.9.3 builds
  the desktop app, 6.10.3 supplies Android host tools. See FR-13b.
- **Line numbers in this PRD are pre-change.** FR-24 alone shifts everything
  below line 74. Re-locate by content, not by number. (All line references were
  re-verified against the working tree on 2026-08-07.)

### 7.5 Dependencies

- A device with a **Thai Gboard layout** — blocks FR-20, the primary acceptance
  test.
- A non-arm64 Android device — blocks FR-23b (record honestly if unavailable).
- ~~Rust ≥ 1.85.0 — blocks FR-3/FR-5.~~ **Satisfied** (1.96.1, all Android
  targets installed). Unpinned, so re-check if the toolchain is downgraded.
- **`~/Qt/6.9.3/gcc_64` present on the build machine** — after FR-24 this stops
  being optional on Linux. It is installed today, but the build no longer
  silently substitutes system Qt for it, so a fresh checkout on another machine
  now needs it (or an explicit `-DCMAKE_PREFIX_PATH`).
- Nothing in this PRD depends on a Mac or a Windows machine.

---

## 8. Success Metrics

1. **Thai Shift works mid-word** in both the search field and the Gloss text
   area on an Android device, with the US layout unregressed. *(The single
   metric this upgrade exists for.)*
2. `bridges/Cargo.toml` points at **unpatched upstream cxx-qt**, with the
   Android rcc options supplied via `CXX_QT_AUTORCC_OPTIONS` — or, if that
   proves impossible, a fork whose remaining patches are documented and
   justified.
3. `make build -B` and `make test` pass on **Linux against Qt 6.9.3** after
   stage 1 and again after stage 2 — and "against 6.9.3" is now **verifiable**,
   not assumed: the configure log names `~/Qt/6.9.3/gcc_64`, FR-27 asserts the
   version, and `ldd` on the binary resolves `libQt6Core` under that prefix.
   Note this is a *change* from the pre-PRD state, not a preservation of it
   (§2.1), so "desktop is untouched" means untouched **relative to the intended
   6.9.3 baseline**, not byte-identical to the last release build.
3b. The Linux **AppImage** is compiled against the same 6.9.3 it bundles
   (FR-30b), verified with `ldd` inside the AppDir.
4. `make android-aab` produces a signed multi-ABI bundle on 6.10.3, passing
   `aapt2 dump badging` (minSdkVersion 28, targetSdkVersion 36, three
   native-code ABIs, unchanged permissions/features), `zipalign -c -P 16 4`, and
   the full-sweep `readelf` `p_align` check.
5. Back navigation behaves correctly in all four previously-broken cases.
6. Configuring against a wrong or missing Qt produces a `FATAL_ERROR` naming
   expected and found versions — verifiable by pointing `CMAKE_PREFIX_PATH` at
   the wrong kit. In the same run `qmake_path` resolves **under that same
   prefix** (FR-28).
7. No remaining "fall back to `PATH`" or "take the newest" clause in the Qt or
   NDK selection path.
8. The CXX-Qt fork is documented (FR-7) — the question "can we go back to
   upstream?" has a written answer.
9. Every doc in FR-31 updated; none still implies a uniform Qt version across
   platforms.

---

## 9. Open Questions

1. **Does the fix actually resolve the Thai symptom?** The mechanism is
   plausible (fewer `restartInput()` calls → the IME keeps its shift state) but
   unproven, and it does not explain why a US layout tolerated the same
   restarts. FR-20 settles it.
2. ~~Is upstream 0.9.1 on crates.io?~~ ~~Residual: the matching `cxx-qt-cmake`
   tag.~~ **Fully settled 2026-08-07.** Pin the upstream **rev** (`2180c12`) for
   the four Rust crates and **tag `0.9.1`** (`06a121e`) for `cxx-qt-cmake` — not
   the branch `0.9`, and note the existing `0.7` pin is itself a branch (FR-4).
3. ~~**Does 0.9.1 introduce breaking API changes our bridges use?**~~
   **Answered 2026-08-07: yes, and they are contained.** The bridge *macros* are
   untouched; `bridges/build.rs` must migrate to the 0.9 builder API — see
   **FR-3b** for the exact mapping. Two narrow residuals, both runtime-only:
   how 0.9 derives QML resource paths from our `../assets/qml/*.qml` entries,
   and whether it now generates a `qmldir` competing with the hand-maintained
   one. FR-9 guards both.
4. **Was fork patch C (the framework `.prl` path) fixing a real Apple bug that
   upstream's rewrite also fixes, or one it still has?** Unanswerable without an
   Apple build, and deliberately deferred — but it is the one patch that cannot
   be mechanically re-applied.
5. ~~**Does `CXX_QT_AUTORCC_OPTIONS` reach the per-ABI ExternalProject
   sub-builds?**~~ **Answered 2026-08-07 (third pass), and the question was
   posed backwards.** Environment inheritance is not the mechanism and would not
   have worked anyway: cxx-qt-cmake sets the variable itself, from
   `CMAKE_AUTORCC_OPTIONS`, via `corrosion_set_env_vars` → `cmake -E env`, which
   **overrides** anything exported by a parent shell (§7.3). Each per-ABI
   sub-build re-runs CMake over this same source tree, so each one sets it for
   its own cargo invocation — propagation is structural. Set the flag in
   `CMAKE_AUTORCC_OPTIONS` (FR-2); do not export it.

   > The verification advice still stands, for a different reason: **the
   > no-`rerun-if-env-changed` trap applies to the CMake-set value too.** Editing
   > `CMAKE_AUTORCC_OPTIONS` changes what corrosion passes, but cargo will
   > happily reuse the previous `rcc` output. Force `build.rs` to re-run for
   > every experiment.
6. **Is `useLegacyPackaging` worth flipping?** FR-19 asks for measurements; the
   prior analysis found no constraint tight enough to justify it.
7. **Is a non-arm64 Android device available for FR-23b?** If not, the
   validation carries over and must be stated as outstanding.
8. **When do the desktop platforms move?** This PRD deliberately leaves them on
   6.9.3. A follow-up needs the AppImage libtiff/FUSE work, and 6.10.3 may
   behave differently from the 6.10.1 that was measured.

   > **Correction.** An earlier draft claimed 6.10.1's `gcc_64` was "still
   > installed alongside 6.10.3, so the comparison is available cheaply".
   > **That is wrong** — `~/Qt/6.10.1` is an uninstall leftover containing
   > **zero files** (16 KB of empty directories). Re-measuring the AppImage
   > defects against 6.10.3 requires no comparison install anyway: the plugin is
   > inspected directly with `objdump -p` on the 6.10.3 kit.

   > **Not 6.11.1, and not now. Decided 2026-08-07.** §2.1 found that the
   > desktop's C++/QML half has accidentally been compiling against system Qt
   > **6.11.1**. That is *not* a reason to reconsider 6.11.1 — it was never a
   > controlled experiment (the binary was bundled with 6.9.3 libraries, so what
   > a user ran was never what a developer ran), and §2's reasons for rejecting
   > 6.11.1 are unchanged: AGP 9.0.0, Gradle 9.3.1, a new Kotlin plugin, Java 17
   > and an upstream-controlled `useLegacyPackaging`, none of which upstream
   > cxx-qt CI covers. **Treat the accidental 6.11.1 exposure as a defect to
   > remove (FR-24), not as evidence to act on.** Do not spend time
   > characterising the mixed build before changing it.
   >
   > When the desktop platforms do move, the target is chosen fresh at that
   > time against the AppImage libtiff/FUSE work — not inherited from this
   > accident.

---

## 10. Related PRDs and documentation

### 10.1 PRDs

**Current (`tasks/`):**

| PRD | Status | Relationship |
|---|---|---|
| `2026-08-05-175516-prd---macos-build-audio-framework-linking.md` | Draft, not implemented (+ tasks list) | **Out of scope here.** macOS stays on 6.9.3 and still does not link. Independent. |
| `2026-08-05-193727-prd---fulltext-index-on-filesystems-without-flock.md` | Draft, blocked on its phase 1 | Touches Tantivy index handling — the same surface FR-23b exercises on non-arm64. Sequence so both are not in flight on Android together. |
| `2026-08-05-201545-prd---run-storage-diagnostics.md` | Implemented (phase 1 of the flock pair) | Its diagnostics window is part of the Android UI surface FR-22 re-tests. |
| `2026-07-31-180502-prd---picker-url-handling-and-chromebook-import-failure.md` | Implemented | Produced `cpp/android_raw_pick.cpp` — the private-Qt include of FR-23c. Respect its terms for confining that include. |

**Archived (`tasks/archive/`, 122 files):**

| PRD | Relationship |
|---|---|
| `2026-07-27-131601-prd---android-api-36-compliance-and-packaging-follow-ups.md` (+ tasks) | **The source PRD** for every deferred item this one discharges — minSdk, predictive back, AGP pin, 16 KB flag, `useLegacyPackaging`, multi-ABI. Cited by `docs/android-qt-upgrade-considerations.md`. Read it before starting Part B. |

### 10.2 Documentation

Must be updated: see FR-31. Read-only context, not expected to change:
`android-edge-to-edge-and-safe-areas.md` (FR-22 re-tests its cases),
`android-beta-distribution-and-play-policy.md` (the beta path rides the same
script), `app-packaging-and-identifiers.md`,
`webengine-stale-black-frame-workaround.md`,
`mobile-rendering-troubleshooting.md`, `android-file-saving-saf.md`,
`relocated-storage-recovery.md`, `storage-diagnostics.md`,
`windows-portable-install.md`.

---

## 11. Implementation surface: reuse, adapt, build new

No new runtime feature, no new signal, no QML surface. Almost all verification
is "does the existing thing still work", not "does the new thing work".

### 11.1 Reuse as-is

| Mechanism | Where | Reuse for |
|---|---|---|
| **Per-platform `QT_*` variables** | `CMakeLists.txt:8-12` | Exactly the mechanism the Android/desktop split needs. No structural change — one value moves. |
| **`ANDROID_QT_DIR` → both `CMAKE_PREFIX_PATH` and `qmake_path`** | `CMakeLists.txt:111-129` | **Reference implementation for FR-28.** The one branch that cannot drift. |
| **`if(NOT CMAKE_PREFIX_PATH)` guard** | `CMakeLists.txt:123` | FR-24. Load-bearing for multi-ABI. |
| **`CMAKE_AUTORCC_OPTIONS`** | `CMakeLists.txt:80-81` | Already sets `--format-version 1` / `--compress-algo zlib`; FR-2 mirrors it on the cxx-qt side. Keep the pair commented as a pair. |
| **Env-overridable script defaults** (`${VAR:-default}`) | `build-android.sh:25,42-45` | FR-30's pattern. |
| **`ndkVersion androidNdkVersion`** | `android/build.gradle:56` | Already wires androiddeployqt's NDK into Gradle, so FR-14's pin propagates with no new plumbing. |
| **`.simsapa-package-identity` re-package guard** | `build-android.sh` | Precedent for "ninja will not notice this changed"; same reasoning applies to a Qt-version switch in one build dir. |
| **Cross-ABI plugin-staging check** | `build-android.sh` | FR-21 re-runs it rather than re-deriving. |

### 11.2 Adapt

| File | Change | Risk |
|---|---|---|
| `bridges/Cargo.toml:21-23,51` | Fork → upstream 0.9.1 (FR-3) | **High** — two minor versions of a codegen crate; affects **all** platforms |
| `bridges/build.rs:102-138` | **`QmlModule` → 0.9 builder API; `rust_files` → `CxxQtBuilder::files`; `cc_builder` becomes `unsafe` (FR-3b)** | **High, and certain** — will not compile otherwise; must be in the same commit as FR-3/FR-4. Resource-path and `qmldir` fallout is runtime-only |
| `CMakeLists.txt:200` | `cxx-qt-cmake` branch `0.7` → **tag `0.9.1`** (FR-4) | High — coupled to the above; failures land in generated code |
| `build-android.sh` | version from `QT_ANDROID` (FR-12/30); **pin** the NDK (FR-14). **Not** the rcc options — those move to `CMAKE_AUTORCC_OPTIONS` (FR-2) | Medium |
| `CMakeLists.txt:80-81` | `list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)` — the whole of FR-2 (FR-2) | Low, but the linchpin of stage 1 |
| `CMakeLists.txt:11` | `QT_ANDROID` → 6.10.3, with a comment (FR-11) | Trivial |
| `CMakeLists.txt:23-74` | Linux branch as `elseif (UNIX AND NOT APPLE AND NOT ANDROID)` — **never a bare `else()`** (FR-24); `WARNING`→`FATAL_ERROR` at **lines 32 and 70 only** (FR-25) | **High, despite looking trivial** — a bare `else()` silently redirects the *Android* build to desktop Qt 6.9.3; converting all four warnings breaks two working fallbacks. **Also changes what Linux builds against today** (system 6.11.1 → 6.9.3), so treat the first post-change desktop build as a real behaviour change, not a no-op |
| `CMakeLists.txt:192` | Add `REQUIRED` (FR-26) | Low, but **prerequisite** for FR-27/28 |
| `CMakeLists.txt:129-187` | `qmake_path` derived post-`find_package`; delete bare fallback (FR-28/29) | Medium — five platforms, only Linux/Android testable here |
| `CMakeLists.txt:340` | 16 KB flag — measure, probably keep (FR-17) | Low |
| `android/build.gradle` | AGP 8.6.0→8.10.1; re-apply local additions; minSdk 28 (FR-15/18/19) | **Highest** — a silent targetSdk-35 regression lives here |
| `android/gradle/wrapper/gradle-wrapper.properties:3` | 8.10 → 8.14.3 (FR-18) | Medium — AGP 8.10 *requires* ≥ 8.11.1 |
| `android/AndroidManifest.xml:115` | Keep opt-out; reconcile placement with Qt's `<application>`-level one (FR-16) | Low, but easy to end up with it twice |
| `build-appimage.sh:130-138,187-221,407` | **Hoist Qt resolution above `build_app()`; assert compile-Qt == bundle-Qt; reconsider the "already built" short-circuit (FR-30b)** | **Medium-high** — currently ships a binary built against one Qt and bundled with another; the fix is small but the failure is invisible until a user's machine |
| `build-macos.sh` / `build-windows.ps1` / `Makefile` | Version from `CMakeLists.txt`; drop the `PATH` fallback (FR-30) | Low — no version change for these platforms |

### 11.3 Genuinely new

1. **The rcc-options wiring** (FR-2) — replaces a fork patch with one
   `list(APPEND CMAKE_AUTORCC_OPTIONS --no-zstd)` line, which cxx-qt-cmake
   forwards to the bridge crate's `rcc` as `CXX_QT_AUTORCC_OPTIONS`. Tiny, but
   the linchpin of stage 1.
2. **A rebased `simsapa` branch** carrying only the Apple patches (FR-6):
   **B** re-applied at `shared.rs:153`, **D** applied unchanged, **C**
   re-derived or dropped, **E**'s `lipo` deleted.
3. **The `Qt6_VERSION` assertion** (FR-27) — ~6 lines plus a
   `qt_expected_version` per branch.
4. **The `qmake_path` derivation block** (FR-28) — new CMake between
   `find_package` and `cxx_qt_import_crate`, plus `qmake_exe_name` per branch.
5. **A version-reading helper** for the build scripts (FR-30) — needs **three**
   implementations: bash, PowerShell, Make. Keep each to a one-line
   `grep`/`sed`.
6. **Two new docs** (FR-31): the CXX-Qt fork record, and Qt-kit selection.
7. **The `bridges/build.rs` migration to the 0.9 builder API** (FR-3b) — the
   largest single piece of stage-1 work, and the one an earlier draft asserted
   would not be needed at all. Mechanical, but it touches the declaration of
   every QML file and every Rust bridge in the project.
8. **The Linux `CMAKE_PREFIX_PATH` branch** (FR-24) — genuinely new CMake, not
   an adaptation: no such branch has ever existed, which is why the desktop
   build has been resolving system Qt (§2.1).
9. **The AppImage compile-vs-bundle Qt agreement check** (FR-30b).

### 11.4 Explicitly not touched

No Rust bridge signatures, no QML files, no signals, no database schema, no
migrations. The project has no Qt-version-conditional code of its own.

> **Correction to an earlier draft**, which listed "`bridges/build.rs`'s
> `qml_files` / `rust_files` lists are unaffected" here. **They are affected** —
> the `rust_files` field no longer exists in cxx-qt 0.9 and the whole
> `QmlModule` literal must be rewritten (FR-3b). The *contents* of both lists
> are unchanged; their **declaration** is not. This was the single largest
> unacknowledged piece of stage-1 work.

Two exceptions to "nothing else is touched":

- The private-header dependency in `cpp/android_raw_pick.cpp` (FR-23c), which
  must be **re-checked against 6.10.3** but is expected to need no change.
- `bridges/build.rs` (FR-3b), per the correction above.
