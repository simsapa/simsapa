# Qt kit selection: which Qt this project builds against

**Short version:** `CMakeLists.txt`'s `QT_*` variables are the single source of
the Qt version. Everything else reads them. A build that resolves a different Qt
**fails at configure time**, and a build whose toolchain is wrong **fails before
it starts**.

> **Status:** the mechanisms below are in place and all five `QT_*` variables
> are `6.9.3`.
>
> They were built during the August 2026 attempt to move **Android** to 6.10.3.
> That upgrade was **tested on device and reverted** — it did not fix the bug it
> was for, and it broke the Android UI
> ([android-qt-upgrade-considerations.md §0](./android-qt-upgrade-considerations.md)).
> **This machinery is deliberately kept.** It is version-independent, it fixed a
> real shipping defect of its own (§1), and §7's split is exactly what a future
> upgrade will need again. Keep the gate green while the versions agree: it is
> dormant, not useless.

## 1. Why this exists — the measured defect

Until 2026-08-08, **"Linux builds against Qt 6.9.3" was an intention, not a
fact.** Measured on the working tree:

| Step | What actually happened |
|---|---|
| `Makefile` non-Darwin `BUILD_CMD` | `cmake -S . -B …` with **no** `-DCMAKE_PREFIX_PATH` |
| `CMakeLists.txt` platform block | had **no Linux branch**, so nothing set one |
| `find_package(Qt6 …)` | resolved `/usr/lib/cmake/Qt6` — Arch's system `qt6-base`, **6.11.1** |
| `qmake_path` (for cxx-qt) | separately pinned to `~/Qt/6.9.3/gcc_64/bin/qmake6` — **6.9.3** |

So the binary was built **against two Qt versions at once**: the C++/QML/CMake
half against 6.11.1, the Rust bridge half against 6.9.3. macOS escaped this only
because the `Makefile` passed `-DCMAKE_PREFIX_PATH` explicitly; Linux was the one
platform with neither a CMake branch nor a Makefile argument.

**It was worse than "always wrong" — it was non-deterministic.** `find_package`
searches the parent directories of entries on `PATH`, so a build directory
configured from a shell with `~/Qt/6.9.3/gcc_64/bin` on `PATH` resolved 6.9.3,
while a clean-environment configure resolved 6.11.1. Same tree, two answers,
depending on which terminal you happened to run `cmake` from. Neither was a
`-D` cache entry, and `~/.cmake/packages/` was empty.

**The AppImage inherited it and made it shippable.** `build-appimage.sh` set up
the Qt environment inside `create_appimage()`, but `main()` called `build_app()`
*first* — so `make build -B` ran with none of it set. The published AppImage was
a binary **compiled and linked against system Qt 6.11.1** that linuxdeploy then
bundled with **Qt 6.9.3** libraries and plugins.

None of this produced a visible symptom, which is the point: mixed-Qt builds
fail in ways that look like unrelated bugs, much later.

## 2. The single source

```cmake
# CMakeLists.txt
set(QT_LINUX   "6.9.3")
set(QT_MACOS   "6.9.3")
set(QT_WINDOWS "6.9.3")
set(QT_ANDROID "6.9.3")
set(QT_IOS     "6.9.3")
```

**There is deliberately no single "the Qt for this project".** The variables are
per-platform so the platforms *can* diverge — see §7.

## 3. The four readers

Four languages read those five lines. They must agree, and
`scripts/qt-env-verify.sh --all` checks that they do.

| Reader | Where | How |
|---|---|---|
| CMake | `CMakeLists.txt` | natively |
| bash | `scripts/qt-env.sh` | `qt_version_for LINUX` |
| Make | `Makefile` | `$(call qt_version_for,MACOS)` |
| PowerShell | `build-windows.ps1` | `Get-QtVersion` |

**Why one shared bash reader rather than a copied `sed` one-liner in each
script:** duplicating the *reader* re-creates the same drift one level down.
Three copies of an expression that must all keep matching `CMakeLists.txt`'s
syntax will eventually disagree — reformat that `set()` line and one script
breaks while another does not.

The cost of sharing is that `scripts/qt-env.sh` has **two roles**, and build
scripts want only one of them:

```sh
# lookup helpers only -- does NOT put the desktop kit on PATH
QT_ENV_NO_ACTIVATE=1 . ./scripts/qt-env.sh
qt_version="$(qt_version_for ANDROID)"
```

Sourcing it bare *also* exports the desktop kit, which is actively wrong inside
`build-android.sh`.

## 4. How CMake resolves the kit

Two **separate** top-level blocks, and the asymmetry between them is deliberate:

```
if (WIN32 AND NOT CMAKE_PREFIX_PATH)      <-- block 1: sets CMAKE_PREFIX_PATH
elseif (APPLE AND NOT IOS)
elseif (UNIX AND NOT APPLE AND NOT ANDROID)
endif()
…
if (ANDROID)                              <-- block 2: a SEPARATE block
    if(NOT CMAKE_PREFIX_PATH)                 Android's own guard
```

> ### ⚠ Never write the Linux branch as a bare `else()`
>
> In CMake **`ANDROID` is also `UNIX`-true**. On Android, `WIN32 AND NOT
> CMAKE_PREFIX_PATH` is false and `APPLE AND NOT IOS` is false — so a bare
> `else()` in block 1 **catches Android** and points it at the desktop Linux
> kit. Block 2's `if(NOT CMAKE_PREFIX_PATH)` guard then finds the variable
> already set and skips, and the Android build links against **Qt for Linux**:
> wrong version *and* wrong platform, in a build that appears to succeed.
>
> A bare `else()` would also catch Windows whenever `CMAKE_PREFIX_PATH` is
> preset.

The inner `if(NOT CMAKE_PREFIX_PATH)` guards are load-bearing twice over: an
explicitly passed `-DCMAKE_PREFIX_PATH` must win, and the **multi-ABI Android
build depends on it** — Qt does not forward the parent's `CMAKE_PREFIX_PATH` to
its per-ABI ExternalProject sub-builds, which is what lets each sub-build
resolve its own ABI's kit.

**A second chain, further down, computes `qmake_exe_name` — and *that* one
correctly starts with `if (ANDROID)`**, so a bare `else()` meaning "Linux
desktop" is right there. The two chains are deliberately asymmetric. Do not
"harmonise" them.

## 5. The two backstops

### 5.1 Configure-time: the version assertion

```
-- Using CMAKE_PREFIX_PATH: /home/you/Qt/6.9.3/gcc_64
-- Qt 6.9.3 (expected 6.9.3) at /home/you/Qt/6.9.3/gcc_64/lib/cmake/Qt6
-- Using qmake: /home/you/Qt/6.9.3/gcc_64/bin/qmake6
```

After `find_package`, the resolved `Qt6_VERSION` is compared against the
declared `QT_*` and a mismatch is a `FATAL_ERROR` naming expected, found and
`Qt6_DIR`.

**On a host with system Qt installed, this assertion — not `REQUIRED` — is the
effective backstop.** `-DCMAKE_PREFIX_PATH=/nonexistent` does *not* stop
`find_package` finding system Qt through CMake's default search paths, so
`REQUIRED`'s failure path is largely unreachable there. Do not cite `REQUIRED`
as the thing protecting you.

**Consequence, and it is intended:** a Linux machine without
`~/Qt/6.9.3/gcc_64` (or `/opt/Qt/6.9.3/gcc_64`) now gets a `FATAL_ERROR` where
it previously used system Qt silently. Install the kit, or configure with
`-DCMAKE_PREFIX_PATH=<qt-dir>`.

### 5.2 `qmake` is derived, never rebuilt

`qmake_path` comes from `Qt6_DIR`, not from a second path built out of `QT_*`:

```cmake
get_filename_component(_qt_prefix "${Qt6_DIR}/../../.." ABSOLUTE)
set(qmake_path "${_qt_prefix}/bin/${qmake_exe_name}")
```

Each platform branch contributes only the **name**, which genuinely varies:
Android's is a POSIX shell wrapper called `qmake` (it applies the Android
`qt.conf`), Windows needs `.exe`, everything else is `qmake6`. Do **not** derive
the suffix from `CMAKE_EXECUTABLE_SUFFIX` — when cross-compiling that describes
the *target*.

This is what makes the §1 defect structurally impossible: the two halves of the
build now read the same `Qt6_DIR`.

> **Do not drop the `QMAKE` argument to `cxx_qt_import_crate()`.**
> cxx-qt-cmake's fallback resolves `Qt::qmake`, which is the **host** qmake —
> wrong for Android, whose kit qmake applies the Android `qt.conf`.

## 6. Build-time: the environment gate

`scripts/qt-env-verify.sh` runs **from the build scripts**, not as a habit
someone has to remember:

| Script | Call site |
|---|---|
| `build-android.sh` | after `JAVA_HOME` / `ANDROID_NDK_ROOT` / `ANDROID_ABIS` are resolved |
| `build-appimage.sh` | after `resolve_qt()`, before `build_app()` |
| `build-macos.sh` | after `check_dependencies` |
| `build-windows.ps1` | `Invoke-EnvVerify` — a hand-maintained PowerShell twin |

A forgotten check looks exactly like a passing one, so the build simply does not
start when the environment is wrong.

**Two tiers.** `CRITICAL` means wrong output or no output → **exit 1**.
`ADVISORY` prints and continues. `--report-only` inspects without stopping.

It prints the toolchain the build actually used — compiler, cmake, ninja,
rustc/cargo, installed Rust targets, git rev — into the build's own log, so
"what was this built with?" is answerable afterwards. Android additionally
reports SDK/NDK roots, NDK revision and clang version, JDK, Gradle wrapper, AGP,
min/target SDK, and per-ABI kit + Rust target.

**The check that the CMake assertion cannot make:** the gate asks the kit its
*real* version (`qmake -query QT_VERSION`) and compares it to the declared one.
Everything else trusts the **directory name**. A directory called `6.9.3`
containing 6.11.1 — a partial install, a hand-moved folder, a MaintenanceTool
leftover — is caught only here.

It also front-loads two Android traps that otherwise fail very late:

- **NDK r28+**, whose libc++ references `pthread_cond_clockwait` (bionic API
  30+) and breaks the `cxx` C++ build. Both Qt's auto-detect and
  `build-android.sh` take the **highest installed** NDK, so merely *installing*
  r28 silently swaps the compiler.
- **A JDK outside 17–21**, where AGP's bundled lint dies in
  `lintVitalAnalyzeRelease` with the JDK version string as its *entire* error
  message — after all three ABIs have compiled and signed.

Run it by hand with `make qt-checks`, or
`make qt-verify-{linux,android,macos}`.

> The bash gate and `Invoke-EnvVerify` are **deliberate duplicates** — PowerShell
> cannot source bash. Nothing but cross-reference comments keeps them in step.
> A check added to one must be added to the other.

## 7. Platforms may deliberately diverge

The per-platform variables exist so a single platform can move alone. **No split
is active today** — every `QT_*` is `6.9.3` — but the machinery below is what
makes one safe, and it has been exercised for real.

The worked case: **Android on 6.10.3 while desktop stayed on 6.9.3**, run in
August 2026 and since reverted (§0 of
[android-qt-upgrade-considerations.md](./android-qt-upgrade-considerations.md)).
It is described here in the present tense because it is the template for the next
one.

Two consequences of a split that read like bugs but are not:

- **Two `gcc_64` kits are live and neither is redundant.** The desktop version's
  builds the desktop app; the *Android* version's supplies **host tools** (moc,
  rcc, androiddeployqt) for the cross-build, resolved automatically via
  `__qt_platform_initial_qt_host_path`.
- Therefore **the Android build runs the Android Qt's `rcc` while the desktop
  build runs the desktop Qt's.** Host tools must match the target Qt. (Measured
  during the attempt: the two produced byte-different `rcc` output from identical
  input — 2,040,098 vs 2,046,446 bytes — which is correct, not a fault.)

**What the split actually caught**, and why the gate earns its keep: while the
versions differed, the pre-flight gate found (a) a stale `QT_ANDROID_VERSION`
exported by the convenience shell layer, which would have silently built Android
against the **desktop** kit, and (b) a desktop Qt on `LD_LIBRARY_PATH` being
loaded by the Android host tools. Both are invisible while all five versions
agree, and both return the instant one moves.

If a split ever looks like an oversight to tidy up, read the comment at the
`QT_*` block before changing it.

## 8. The convenience layer is NOT load-bearing

Three things put the desktop kit on an interactive or agent shell's `PATH`:

| | |
|---|---|
| `.envrc` | direnv, interactive shells (needs a one-time `direnv allow`) |
| `.claude/settings.json` `env` | agent shells; carries **literal paths** |
| `scripts/qt-env.sh` | the implementation both use |

**The build must be correct with an empty Qt-related environment.** CMake
resolves its own `CMAKE_PREFIX_PATH` and asserts the result. Verified with
`env -i PATH=/usr/bin:/bin HOME=$HOME cmake -S . -B <dir>`, which resolves
6.9.3 — the same invocation that previously resolved system 6.11.1. **If
deleting all three ever breaks `make build`, that is a CMake bug, not a reason
to make them required.**

`.claude/settings.json` is the one place a version is necessarily duplicated (it
cannot run a script), so `make qt-env-check` fails if its literal paths drift
from `QT_LINUX`.

> ### ⚠ A convenience variable that changes build *output* is not a convenience
>
> `qt_env_activate()` used to export `QT_ANDROID_VERSION`. Because
> `build-android.sh` treats that variable as a **deliberate override**, every
> shell that had sourced `qt-env.sh` silently bypassed the `CMakeLists.txt`
> derivation — and the assignment was `:-` guarded, so a direnv *reload* did not
> refresh it. A shell opened before a `QT_ANDROID` bump would keep the old
> version indefinitely and build Android against the **desktop** Qt: a package
> that looks fine and ships without the Android-only fix the bump exists for.
>
> The export was removed. Set `QT_ANDROID_VERSION` by hand when you genuinely
> want to override; the build header reports which source it used.

### 8.1 The leak the convenience layer creates, and how each script closes it

The box above is one instance of a general trap, which has now produced **three
separate defects**. State it as a rule:

> **Anything `qt_env_activate()` exports is present in every direnv, agent and
> hand-sourced shell. A build script that reads such a variable — or lets the
> dynamic loader read it — has made the convenience layer load-bearing, which
> §8 says it must not be.**

The dangerous export is **`LD_LIBRARY_PATH`**. The Android cross-build runs the
*Android* Qt's **host tools** (moc, rcc, androiddeployqt from
`$QT_ANDROID_ROOT/gcc_64`, resolved via `__qt_platform_initial_qt_host_path`),
and those are dynamically linked against `libQt6Core.so.6`. With the desktop
kit's `lib/` first on the loader's search path, a 6.10.3 `rcc` loads 6.9.3's
`libQt6Core`. Measured, in the agent shell, before the fix:

```
$ ~/Qt/6.10.3/gcc_64/libexec/rcc --version
rcc: /home/…/Qt/6.9.3/gcc_64/lib/libQt6Core.so.6: version `Qt_6.10' not found
```

**This is invisible while `QT_ANDROID == QT_LINUX` and becomes live the moment
they diverge** — i.e. exactly when the Android-only Qt bump lands. That timing
is the whole hazard: the failure appears at the point the versions split, in a
build that otherwise looks fine.

**Per-platform exposure.** Checked, not assumed:

| Script | Exposed? | Why |
|---|---|---|
| `build-android.sh` | **Yes — the real case** | Runs host tools of a *different* Qt than the shell's. Closes it by **scrubbing**: `LD_LIBRARY_PATH` cleared unconditionally, `PATH` entry and `QT_PREFIX`/`QMAKE` removed |
| `build-appimage.sh` | No, by a different mechanism | `resolve_qt()` **prepends** its own kit to both `PATH` and `LD_LIBRARY_PATH`, so its kit wins for the loader regardless of what was inherited |
| `build-macos.sh` | No | `qt_env_activate()` never runs on macOS — `qt_prefix_for LINUX` looks for `$HOME/Qt/<v>/gcc_64`, which does not exist there, so it fails cleanly and exports nothing. macOS's equivalents are `DYLD_LIBRARY_PATH` / `DYLD_FRAMEWORK_PATH`, which nothing sets |
| `build-windows.ps1` | No today — **but the reasoning differs** | Same reason as macOS (the bash convenience layer does not run). See the warning below |

> **⚠ On Windows, `PATH` *is* the library search path.** The
> "`PATH` is only advisory, the build addresses its tools by absolute path"
> argument used in `build-android.sh` is a **Linux-specific** claim — there the
> loader reads `LD_LIBRARY_PATH`, not `PATH`. Windows resolves DLLs through
> `PATH`, so a foreign Qt `bin/` on it is as dangerous as a foreign `lib/` is on
> Linux. Nothing exercises this today (no Windows/Android split, no second Qt on
> Windows hosts), but do not port the "advisory" wording to a Windows script.

**Two ways a scrub goes wrong, both found and fixed here.** Both are easy to
re-introduce, which is why they are written down rather than only fixed:

1. **Do not gate the dangerous half on the harmless half.** The
   `build-android.sh` scrub was entirely inside `if [ -n "${QT_PREFIX:-}" ]`.
   `QT_PREFIX` and `LD_LIBRARY_PATH` are set *together* by `qt_env_activate()`,
   but nothing guarantees they *arrive* together — a hand-written export, a
   wrapper script or an inherited CI environment sets one without the other, and
   the whole block is then skipped. The `LD_LIBRARY_PATH` clear is now
   unconditional and in its own block. `PATH` stays gated, because `QT_PREFIX`
   is what names the entry to remove.
2. **Do not edit `PATH`-like lists with `sed`.** `qt_env_activate()`'s
   "idempotent" strip matched only the `"<entry>:"` form, so it missed the entry
   when it was **last** and — because a sole entry has neither a leading nor a
   trailing colon — could never converge: repeated activation settled at two
   copies. Worse, substring matching **corrupts a lookalike entry**:

   ```
   in : /opt/home/…/Qt/6.9.3/gcc_64/bin:/usr/bin
   out: /opt/usr/bin              # an entry that never existed
   ```

   Replaced by `_qt_env_list_remove()`, which splits on `:` and compares whole
   entries, so first / middle / last / only / duplicated are handled uniformly.

**Clearing rather than filtering is deliberate** where it is done: the Android
build needs no `LD_LIBRARY_PATH` at all (Qt's own scripts set what they need),
so an empty value cannot be wrong, whereas a filtered one can still carry a
third Qt from somewhere else on the list.

**Two independent layers, and the second must not depend on the first.**
`scripts/qt-env-verify.sh`'s *Android host tools* section re-checks the host kit
and flags a foreign Qt on `LD_LIBRARY_PATH` (CRITICAL) or `PATH` (advisory). It
deliberately does not assume the scrub ran — the scrub is the thing that can be
missing.

**Working by hand?** Any 6.10.3 command run outside `build-android.sh` in a
direnv or agent shell needs the desktop kit out of the way, or it dies with the
`Qt_6.10 not found` error above and reads as a broken install:

```sh
env -u LD_LIBRARY_PATH ~/Qt/6.10.3/gcc_64/bin/qmake -query QT_VERSION
```

## 9. Never invoke a bare `qmake6` / `rcc` / `moc` / `qmllint`

They resolve to the **system** Qt (`/usr/bin/qmake6`), which this project does
not target on any platform.

```sh
source scripts/qt-env.sh   # puts the desktop kit's bin/ first on PATH
qmake6 -query QT_VERSION   # now the project's Qt

"$QMAKE" -query QT_VERSION # or address it directly
```

For Android tooling use `build-android.sh`, which derives its own Qt version
from `QT_ANDROID`. Do not reuse the desktop kit for Android work.

## 10. Changing a Qt version

1. Edit the one `QT_*` line in `CMakeLists.txt`.
2. **Delete the build directory** — it caches `CMAKE_PREFIX_PATH`, so the
   assertion can otherwise pass against a stale value. (For Android use
   `make android-clean`; **never** hand-delete `android-build/`.)
3. `make qt-checks`, then build.
4. If `QT_LINUX` moved, `make qt-env-check` will fail until
   `.claude/settings.json` is updated to match.
5. Re-source `scripts/qt-env.sh` (or open a new shell) — long-lived shells carry
   stale exports.
