# Relocated storage recovery (Android)

What happens when the storage location the user chose for the database is no
longer where it was — a microSD card moved to another socket or another phone, a
card removed, a volume that came back with a different path.

Before this feature the app answered that situation by silently downloading
everything again, into a location the user never chose. The reported symptom was
*"the app asks me to download everything again after I moved my SD card"*.

PRD and task list (the requirement authority; FR numbers below refer to it):

- `tasks/2026-08-01-085526-prd---relocated-storage-recovery-on-android.md`
- `tasks/2026-08-01-085526-tasks-relocated-storage-recovery-on-android.md`

The whole feature is **mobile-only**. On desktop the predicate short-circuits
before reading anything and `cpp/gui.cpp` guards the branch with its own
`is_mobile` test, so startup is byte-for-byte what it was.

## 1. The two path notions — never conflate them

| | |
|---|---|
| **recorded path** | `trim(read("<internal_app_root>/storage-path.txt"))` — what the user chose. |
| **resolved path** | `get_create_simsapa_dir()` — what the app will actually use. Equals the recorded path when it is reachable; equals the **internal app root** when it is not (the FR-20 fallback). |

The fallback is a way to keep running, **not** a user choice (FR-21). Nothing in
the app may treat "the resolved path holds a database" as "the user's location is
fine" — that conflation is the original bug: an internal copy plus an unreachable
recorded path made `appdata_db_exists()` true, so the app booted against a
database the user never asked for, or (worse) the startup sweeps deleted it.

## 2. The four-state predicate

`storage_path_state() -> (StorageState, Option<PathBuf>)` in `backend/src/lib.rs`
(with `storage_path_state_of_file()` as the testable inner form).

| State | Meaning | JSON / QML string |
|---|---|---|
| `Absent` | no `storage-path.txt`, or empty/whitespace-only after trim | `"absent"` |
| `Unreachable` | recorded path does not exist, or its metadata is unreadable | `"unreachable"` |
| `ReachableEmpty` | recorded path exists but holds no usable installation | `"reachable_empty"` |
| `Ok` | recorded path exists and holds a usable installation | `"ok"` |

"Usable installation" (FR-5) is deliberately cheap and deliberately narrow:
`<path>/app-assets/appdata.sqlite3` exists **and** `metadata().len() > 0`. A
zero-byte stub is not an installation — a fabricated empty database is not zero
bytes, but a truncated or interrupted one is, and that is the case this catches.

Three properties are load-bearing:

- **It is read-only.** `try_exists()` + `metadata()` only. No `create_dir_all()`,
  no writes (FR-35). This is what makes it legal to run it before
  `init_app_globals()`. It resolves the internal root through
  `get_simsapa_internal_app_root_path()` — the *non-creating* variant — precisely
  so the letter of that promise holds; `get_create_simsapa_internal_app_root()`
  would have created a directory.
- **It is stable across launches.** If the predicate (or anything before it)
  created the recorded directory, the next launch would classify the same
  situation as `ReachableEmpty` and the recovery message would vanish for good.
  This is FR-20a, and it is why `get_create_simsapa_dir()`'s mobile branch no
  longer creates a missing recorded path (see §3).
- **It is `is_mobile()`-gated internally** and returns `Absent` on desktop
  *without reading the file* (FR-35a), so a stray desktop `storage-path.txt`
  cannot reach any of this.

All errors classify; none propagate (FR-8). Everything unreadable is
`Unreachable`, everything ambiguous is "no installation".

FFI: `storage_path_state_c() -> i32` and `recorded_storage_path_c() -> *mut c_char`
(freed with `free_rust_string()`). QML: `StorageManager.storage_path_state()` /
`recorded_storage_path()`, returning the string forms.

## 3. `get_create_simsapa_dir()`: trim, and never create

Two changes to the mobile branch:

- The file contents are **trimmed**. A trailing newline resolves correctly, and a
  whitespace-only file means "no recorded path" → internal app root (FR-22a,
  FR-24). Before this, an `echo`-written file yielded a path containing a newline
  that resolved to nothing on any volume.
- A missing recorded path is **not created**. It falls back to the internal app
  root with a logged warning naming the unreachable path (FR-20/FR-20a). Creating
  the directory for a *newly chosen* location belongs to the download flow —
  `get_create_simsapa_app_assets_path()` at download time — not here.

The `SIMSAPA_DIR` env branch and the desktop branch are untouched.

## 4. Startup ordering in `cpp/gui.cpp::start()`

Ordering is nearly the whole correctness argument of this feature.

```
dotenv_c(); find_port_set_env_c();

state, recorded = storage_path_state_c()      // FR-36: BEFORE anything resolves
record_storage_path_state(state, recorded)    //         or creates a path

init_app_globals()                            // freezes APP_GLOBALS from the resolved path
remove_download_temp_folder()                 // non-destructive: always

ensure_no_empty_db_files(sweep = state != Unreachable)   // FR-36a

if state == Unreachable:
    log("Skipping destructive startup sweeps; recorded storage path unreachable: …")
else:
    check_delete_files_for_upgrade()          // DELETES DATABASE FILES
    check_remove_lang_index_dirs()

// FR-2 exemption — these adopt nothing and may read a fallback DB:
if appdata_db_exists(): maybe qputenv(QSG_RENDER_LOOP, "basic")
QApplication app
if appdata_db_exists(): apply_link_colors()

skip_for_upgrade = (state == ReachableEmpty && peek_auto_start_download_c())

if is_mobile && state in (Unreachable, ReachableEmpty) && !skip_for_upgrade:
    StorageRecoveryWindow                     // §6
elif !appdata_db_exists():
    DownloadAppdataWindow                     // the existing first-run flow
else:
    init_app_data(); open_main_window()
```

**Why the sweeps are gated (FR-36).** `check_delete_files_for_upgrade()` deletes
database files at whatever path is *resolved*. In the `Unreachable` state that is
the internal fallback — so an upgrade marker plus a moved card used to destroy the
copy in internal storage before the user was ever asked anything. Gating on the
pre-sweep state is the data-loss guard.

**`state` is a pre-sweep snapshot, deliberately.** Do not re-evaluate it after the
sweeps. On an ordinary upgrade launch the predicate runs while the databases still
exist, so `state == Ok`; the sweep then deletes them and the branch reaches the
upgrade download through `elif !appdata_db_exists()` — *not* through
`skip_for_upgrade`, whose `ReachableEmpty + marker` case only occurs when a
previous upgrade launch was itself interrupted. Re-evaluating would also make
FR-22's report a record of what the sweeps did rather than of what the app found.

**`ensure_no_empty_db_files(sweep: bool)`** keeps its per-database
sweep-then-record order in both modes. With `sweep = false` a zero-byte file is
**recorded as missing but not deleted** (FR-36a) — recording is never skipped,
because `record_db_presence()` is first-write-wins and a record-first split would
lie about what was there.

**The FR-2 invariant:** in the `Unreachable` state every terminating branch ends
in `throw NormalExit` after `app.exec()` returns, and the one non-terminating
branch (a failed write, §8) leaves the dialog on screen. `init_app_data()` sits
after all of it, so the app can never boot silently against the fallback database
in such a session.

**Startup cost on a healthy launch** is one `try_exists()` plus one `metadata()`.
The enumeration, the scan and the probes all run after `app.exec()`.

## 5. Two tiers of classification

Tier 1 is cheap and runs anywhere; tier 2 writes and runs only in dialogs.

### Tier 1 — enumeration + scan (no probes, no database opens)

`cpp/utils.cpp` enumerates: `get_app_data_storage_paths()` over
`getExternalFilesDirs()`, each row carrying `path`, `label`, `is_internal`,
`megabytes_available`, `megabytes_total`, `is_emulated`, `is_removable`,
`is_usable`, `unusable_reason`. `append_unmatched_storage_volumes()` adds a pass
over Android's `StorageManager.getStorageVolumes()` and appends any volume that
matches **no** enumerated candidate as an unusable row — "Not usable for app data
(this device may only allow file transfers here)" — so no volume the app can see
silently vanishes from the list (FR-27/FR-33).

The volume↔candidate match is exact: `StorageManager.getStorageVolume(File)` +
`StorageVolume.equals()` (API 24). The three string tests (uuid-in-path,
`isPrimary()`, `getDescription()`-vs-label) survive only as fallbacks, and each
per-volume log line carries `by=volume|primary|uuid-in-path|description` so a
device run shows which test carried the match. `StorageVolume.getDirectory()`
(API 30) is above the minSdk floor and is not used.

`scan_storage_candidates(enumeration_json, recorded)` in `backend/src/lib.rs`
applies all **policy** — the enumeration reports facts, the scan decides — and is
therefore unit-testable off-device (`backend/tests/test_storage_candidates_scan.rs`).
It produces the row shape consumed by every list in the feature:

```json
{ "path", "label", "is_internal", "is_recorded",
  "group": "found" | "available" | "unusable",
  "unusable_reason",
  "megabytes_available", "megabytes_total", "low_space_warning",
  "appdata_bytes", "modified", "is_complete" }
```

Rules worth knowing:

- **`appdata_bytes` (database size) ≠ `megabytes_available` (volume free space).**
  Never collapse them (FR-9).
- **`is_recorded` is a field, never a suffix baked into `label`** (FR-11a) — the
  label comes from the enumeration and is compared elsewhere. The delegate renders
  "(current selection)" from the field.
- **Unusable rows carry no figures** (FR-34). `QStorageInfo` reports zeros for an
  unreachable path, and "0.0 GB free" reads as a space problem rather than an
  availability one. Same reasoning makes `megabytes_available` **null, not 0**,
  on the recorded-path extra candidate, whose free space was never measured — a
  fabricated 0 also tripped the low-space warning.
- **The recorded path is appended as an extra candidate** when the enumeration
  does not already contain it (FR-6). When it is unreachable it is *by definition*
  not enumerated, so without this step the rule "test the recorded path like any
  other candidate" is a no-op. It is classified unusable/"Not available" when it
  does not exist.
- **`same_path()`** (FR-6a): trim, drop trailing separators, compare by path
  components. Never `canonicalize()` — that fails on a path that does not exist,
  which is the case that matters.
- **`is_complete`** = all three of `appdata.sqlite3`, `dictionaries.sqlite3`,
  `dpd.sqlite3` present (existence only). False renders as "Partial"; the missing
  filenames go to the **log**, not the JSON (FR-10).
- **`low_space_warning`** from `LOW_SPACE_THRESHOLD_MB` (2048). It warns, never
  disqualifies (FR-30), and the warning always comes with its figure.
- **Ordering:** group order found → available → unusable, internal first within
  each group (FR-9/FR-11).
- One `metadata()` call yields both the length (the FR-5 test) and `modified`.
- An unreadable candidate is logged and skipped; it never aborts the scan (FR-8).

**Emulated-duplicate de-duplication.** On a phone with no card the enumeration
reports the same physical storage twice — `/data/user/0/<pkg>/files` and
`/storage/emulated/0/Android/data/<pkg>/files`, identical totals — because primary
"external" storage is a FUSE view of the same partition. `is_duplicate_emulated_candidate()`
drops the external one when it is `is_emulated && !is_removable` and an internal
candidate exists. A real card reports `is_emulated = false` and is never dropped —
that is the entire scenario this feature exists for — and the **recorded path is
never dropped whatever it is**. Every dropped path is logged.

**Volume labels are paths on Android.** `QStorageInfo::displayName()` returns the
mount point (`/data/data/<pkg>`), not a friendly name, so it is never empty and
`createStorageInfo()`'s "Internal Storage" / "SD Card" / "External Storage"
fallbacks never fired. Any label starting with `/` is now discarded as
path-shaped. A real FAT volume label is not path-shaped and is still used.

Bridge: `StorageManager.find_storage_candidates_json()`.

### Tier 2 — the write/SQLite probe

`backend/src/storage_probe.rs`: create `simsapa-write-probe.sqlite3` in the
candidate directory, open it with a Diesel `SqliteConnection` (no `rusqlite` —
do not add it), `CREATE TABLE` + `DROP TABLE`, close, and delete the file **and
its `-wal` / `-shm` / `-journal` siblings** on every exit path including failures.
Cleanup is a `Drop` guard (`ProbeCleanup`), not a happy-path statement.

File creation is tested *separately* from the SQLite open, because SQLite reports
"unable to open database file" for a permission problem too and the two failure
classes must stay distinguishable:

| Failure | Reason string |
|---|---|
| file creation fails | "The app cannot write here" |
| SQLite open / DDL fails | "This location cannot store the app database" |

**The probe is callable only from dialogs** — never from the predicate, never from
the scan (FR-31/FR-31a). It writes, so it must not run on the startup path or
inside a QML engine load.

Bridge: the async pair `probe_storage_candidate(path, request_id)` →
`probeCompleted(path, request_id, result_json)`, following `PromptManager`'s
`qt_thread.queue()` convention (CXX-Qt invokables run on the calling thread, so
the invokable returns nothing and the JSON arrives in the signal). Return shape:
`{ "path", "is_usable", "unusable_reason" }`.

Cancellation has two halves, and both are needed: an `Arc<AtomicUsize>` generation
on `StorageManagerRust`, bumped by `cancel_storage_probes()` and checked before the
probe writes anything and again before emitting; plus the dialog-scoped
`request_id` echoed back so QML can discard a verdict from a dialog that has since
been closed and reopened. **Every terminal path cancels** — the handoffs, the
adoption branch, every Quit button, `onClosing`, and every re-scan — because a
process exit inside the probe would leave `simsapa-write-probe.sqlite3` on the
user's card.

**Verdicts are demote-only.** A probe can move a row to `unusable`; it can never
promote one. When the demoted row was selected, the selection is cleared and the
confirm button disabled (FR-31a/FR-34).

## 6. The startup recovery flow

`assets/qml/StorageRecoveryWindow.qml`, hosted by
`cpp/storage_recovery_window.{h,cpp}` and created through
`WindowManager::create_storage_recovery_window()`.

`app.exec()` runs **once**. The flow is a signal-driven state machine, not a
sequence of blocking calls — there is no nested or re-entered `exec()`. The window
starts **invisible** and posts its first scan with `Qt.callLater`, which keeps the
enumeration out of the engine load (FR-32) *and* means the zero-hit short-circuit
below never flashes a screen.

`branch_on_state()`, in order:

1. `state == "ok"` (only reachable via Try Again) → "Storage is available again.
   Please restart Simsapa." → quit. Checked **first**, before the hit count:
   otherwise the recorded path — itself a `found` row in this state — would be
   offered to the user for "adoption" of the location they already have.
2. hits ≥ 1 → the grouped selection screen.
3. `state == "reachable_empty"` → the ordinary download flow, **no message**, and
   `StorageDialog` still opens (FR-23a). This is the interrupted-first-run case
   and it must show no new screens.
4. `state == "absent"` → first-run flow, no message (FR-24).
5. `state == "unreachable"` → the FR-23 message naming the recorded path, with the
   grouped list beneath it, **all rows non-selectable, tier-1 verdicts only**, and
   Try Again / Set Up Again / Quit.

**Try Again re-checks *and* re-scans and then re-branches on the new state**
(FR-25) — never a cached result. After re-seating a card the recorded path may be
reachable again, and re-showing "not currently available" for a path that now
exists is the exact falsehood FR-23a forbids.

Endings (FR-14/FR-15):

| Action | Ending |
|---|---|
| adopt a `found` row | verified write → "Storage location updated. Please restart Simsapa." → quit |
| pick an `available` row ("Download Here") | verified write → download flow with `skip_storage_dialog = true`; no restart notice, no quit |
| "Create New Location" / "Set Up Again" | first-time install; **the recovery dialog itself writes nothing** |

Adoption needs a restart because `APP_GLOBALS` was frozen in `init_app_globals()`.
A group-2 download does not, because `AssetManager::run_download()` builds
`AppGlobalPaths::new()` at download time and re-reads `get_create_simsapa_dir()`.

**A tier-2 demotion can invalidate the screen.** If the probe demotes the last
`found` row, the user is left on a screen headed *"Existing Simsapa data was
found"* with nothing to adopt — and in `unreachable` with no Try Again button to
escape. `rebranch_if_the_last_hit_was_demoted()` re-runs `branch_on_state()` on the
same (not re-scanned) state, which is where tier 1 would have sent the user had it
known. Verdicts are merged into **both** `StorageCandidatesList` instances — the
selectable one and the read-only one under the FR-23 message — because they are
the same picture of the device and must not say two different things about one
volume.

**If the QML fails to load**, `m_root` is null and `gui.cpp` falls through to the
ordinary first-run download window. Entering `app.exec()` with no window is a hang
with no way out: no window can ever close, so `quitOnLastWindowClosed` never fires.

## 7. The `auto_start_download` marker

`AssetManager::should_auto_start_download()` **deletes** the marker as a side
effect of reporting it. Three consequences the recovery flow has to respect:

- `gui.cpp` needs to know about the marker before any window exists, and must not
  consume it — hence the non-consuming `peek_auto_start_download_c()` /
  `AssetManager.peek_auto_start_download()`. The single consuming call stays in
  `DownloadAppdataWindow.qml`'s `Component.onCompleted`.
- **The marker suppresses recovery only in `ReachableEmpty`.** In `Unreachable` it
  must not, or the upgrade download silently lands in the internal fallback — a
  location the user never chose (FR-2, Goal 2).
- On the "set up a new database" handoff the marker must not be consulted at all.
  Otherwise declining the recovery dialog immediately auto-started a download into
  the very fallback location the user had just declined. The flag for this
  (`skip_auto_start_download`) and `skip_storage_dialog` are passed as **initial**
  properties: `cpp/download_appdata_window.cpp` constructs its
  `QQmlApplicationEngine` empty, calls `setInitialProperties()`, and *then*
  `load()`s. A property set after construction arrives after
  `Component.onCompleted` has already consumed the marker.

The marker is deliberately **left in place** across a decline: a failed download
afterwards leaves the state `reachable_empty`, where `skip_for_upgrade` is true and
the download auto-starts at the freshly chosen location.

**`skip_storage_dialog` is for group 2 only.** It must not be set on FR-23a's
fall-through, even though that path also reaches the download flow with a location
already recorded — there the choice was made in a *previous* session, before a
download that then failed, and the location itself is the prime suspect.
Suppressing the dialog would pin the user to it on every relaunch.

## 8. Verified writes (FR-37)

`StorageManager::save_storage_path()` returns `bool`, from `save_to_file_checked()`.
Every caller checks it:

- Recovery dialog: an error dialog opens over the selection screen; **no quit, no
  handoff**. Quitting would loop the user through the same dialog on every launch
  with no message.
- `StorageDialog`'s Select button: error dialog, and it does **not** proceed to the
  download.
- Database Validation adoption: error dialog, and the lookup stays open.

The write goes to `storage-path.txt` in the **internal** root, whatever location
was selected.

## 9. Where the lists are shown

`assets/qml/StorageCandidatesList.qml` is the one grouped list and the one
delegate, used by four screens with different rules:

| Screen | `selectable_groups` | `exclude_recorded` | Probes |
|---|---|---|---|
| Recovery selection (startup) | `["found", "available"]` | false | `probeable_paths(false)` |
| Recovery "not available" (FR-23) | `[]` | — | none (tier 1 only) |
| Database Validation lookup (FR-18) | `["found"]` | **true** | `probeable_paths(true)` |
| First-run `StorageDialog` | `["found", "available"]` | false | `probeable_paths(false)`, posted from `onOpened` |

Database Validation probes **selectable rows only**: probing rows nobody can pick
writes to volumes to produce a demotion nobody can act on.

Its nothing-found condition is `found_count_excluding_recorded()`, **not**
`found_count()` — on a healthy install the recorded path is itself a `found` row,
so branching on `found_count()` shows a selection screen on which nothing can be
picked instead of FR-19's "No existing database was found on the other storage
locations" message (which still carries the grouped list, so the user sees what was
examined).

Adoption from Database Validation quits the **whole application** (`Qt.quit()`),
which tears down live `AppData` pools, the Rocket thread and Tantivy dirs; the
terminal screen is not dismissable (`Popup.NoAutoClose`), since the path has already
been recorded while the running app still holds the old one open.

Rendering rules that are easy to break:

- **Row selectability is computed from the delegate's required properties, not
  from a function call.** A JS function is not re-evaluated when a model role
  changes, so a demoted row would have stayed clickable.
- **A demoted row is moved to the end of the model**, not just re-grouped in
  place: the `ListView`'s section headings come from row *order*, so a demotion
  inside the `found` run splits the section and renders a stray heading.
- **A pending probe does not make a row unselectable** — it blocks *confirming*,
  through `selection_probe_pending` (explicit state refreshed at every mutation; a
  binding over a `ListModel` role would not re-evaluate). Making the row itself
  unselectable stripped the pre-selected hit's radio button while leaving the
  confirm button live.
- **Colours come from the palette**, never hardcoded. `ThemeHelper` is not usable
  in this flow — it reads the saved theme through `SuttaBridge`, and the database
  may be missing.
- **A `Dialog`'s content is anchored left/right only.** `anchors.fill: parent`
  inverts Dialog sizing (the dialog measures the content's implicit height, so
  anchoring the content to the dialog leaves nothing to measure — measured
  `implicitHeight = 41`). And a `ListView` cannot be sized from its own
  `contentHeight`: a view with no height creates no delegates.
- **The `StorageManager` qmllint stub is a `QtObject`**, not an `Item`. As an
  `Item` it counted as a second visual child of any `Dialog` that declared one,
  suppressing Popup implicit sizing — which is what hid the sizing bug above from
  desktop harnesses while the device showed it.

**One-location devices skip the dialog.** With the emulated duplicate dropped, a
phone with no card offers exactly one location, so `StorageDialog` would be a modal
asking the user to choose between a single option. `auto_select_single_location()`
records it and continues to the download. It branches on
`selectable_count()` — not `row_count` (which includes unusable rows) and not
`found_count()`. A **failed** write falls through to opening the dialog, where
Select surfaces the FR-37 error, so the "a failed write never reaches the download"
rule holds on both paths.

This amends the PRD, whose §5 lists "any change to how the storage location is
chosen on first run" as out of scope; the mechanism is untouched, only the needless
prompt is suppressed.

## 10. Diagnostics

**The startup report** (`StartupDbReport`, `backend/src/db/mod.rs`) gained a
**top-level** `storage_path` object — not a per-database field:

```json
"storage_path": { "recorded": "…", "state": "unreachable" }
```

Written once, first-write-wins, before `init_app_globals()`. Reaches QML through
`SuttaBridge.get_startup_db_report()` unchanged, and `DatabaseValidationDialog`
reports "the configured storage location is unavailable" naming the path instead
of the generic missing-database message (FR-22).

That branch is **currently unreachable in practice**: on mobile, `unreachable`
always routes into the recovery flow and every download-flow ending is "quit and
start again", so no main window opens in such a session. FR-22 is a SHOULD; the
wiring is correct and stays. Do not spend device time trying to observe it.

**Per-volume logging** in `append_unmatched_storage_volumes()` is permanent — one
line per volume with `uuid`, `description`, `primary`, `matched`, `by=…`, plus the
volume count against the enumerated-candidate count and explicit errors when
`STORAGE_SERVICE` or `getStorageVolumes()` come back invalid.

**The scan dump marker.** A file `log-storage-scan.txt` in the internal app root
makes every launch log the state, the raw enumeration JSON and the classified
tier-1 candidates (`storage_scan_log_requested_c()` / `log_storage_scan_c()`,
called from `gui.cpp` right after the `QApplication` is constructed). It is **not**
consumed, so it keeps dumping until deleted, and it costs one `try_exists()` when
absent. On a healthy install this is the only way to see the enumeration at all.

**Log via `log_info_c()` / `log_error_c()` from C++, never `qInfo()`.** On Android
Qt tags its own messages with the *application name*, so `qInfo()` output appears
under neither the `simsapa` tag nor `Qt` and is invisible in the documented logcat
filter — indistinguishable from code that never ran. This already cost one device
round trip on this feature.

## 11. Simulating states with `adb` (no card needed)

Use the **beta debug** build (`make android-beta-debug`); `run-as` needs a
debuggable package, and the beta is a separate package that cannot disturb a
Play-installed copy.

```sh
adb logcat -s simsapa Qt QtCore QtQml
```

**Write the recorded path with `printf '%s'`, never `echo`** — a trailing newline
tests the trim, not the relocation.

**`run-as … sh -c` is blocked by SELinux on some devices** (observed on an
SM-S911B / Android 16). Stage the file in `/data/local/tmp`, which the `shell`
user can write, and copy it in:

```sh
printf '%s' /storage/DEAD-BEEF/Android/data/io.github.simsapa.app.beta/files \
  > /tmp/storage-path.txt
adb push /tmp/storage-path.txt /data/local/tmp/
adb shell run-as io.github.simsapa.app.beta \
  cp /data/local/tmp/storage-path.txt files/storage-path.txt
```

| State to reach | Recipe |
|---|---|
| `unreachable` | point `storage-path.txt` at a non-existent path (above) |
| `reachable_empty` | point it at a directory you created with `mkdir -p` |
| `absent` | `printf ' '` — whitespace only |
| `ok` | point it at a real installation |
| a second `found` candidate | `mkdir -p files/fake/app-assets && printf 'x' > .../appdata.sqlite3` — FR-5 tests existence and non-zero length, so one byte is a "found" installation |
| a probe failure | `run-as … chmod 555` on the internal app root (restore with `chmod 771` — **not** 755, which is not what Android creates) |

**Two storage candidates on a device with no card slot.** Point
`storage-path.txt` at the **emulated** external path
(`/storage/emulated/0/Android/data/<pkg>/files`) and plant a one-byte
`app-assets/appdata.sqlite3` there with plain `adb shell` (the `shell` user can
write under `Android/data/<pkg>` where `run-as` cannot). The recorded path is never
de-duplicated, so the emulated row survives as "(current selection)"; the real
install at the internal path becomes the adoption candidate. Note that the runtime
creates zero-byte `dictionaries.sqlite3` / `dpd.sqlite3` next to the fabricated
appdata, so `is_complete` goes true and no "Partial" marker appears — that is
correct. `chmod` is a no-op on the emulated FUSE view, so a probe can only be made
to fail on the internal path.

**Traps when testing this way:**

- `get_create_simsapa_app_assets_path()` recreates `app-assets` on every launch,
  so restoring a renamed `app-assets` with `mv` nests it inside a freshly created
  empty one. **Stop the app before restoring.**
- Do not adopt a fabricated location; restore `storage-path.txt` afterwards.

## 12. Tests

| Where | What |
|---|---|
| `backend/tests/test_storage_path_state.rs` | the predicate: trim, whitespace-only, zero-byte stub, read-only + stable across launches, desktop gate, the report's JSON shape |
| `backend/tests/test_storage_candidates_scan.rs` | tier-1 classification, ordering, the recorded-path extra candidate + trailing-slash de-dup, "Partial", low space, unusable rows, emulated de-duplication, resilience, `same_path()` |
| `backend/tests/test_ensure_no_empty_db_files_no_sweep.rs` | `sweep = false` records a zero-byte stub as missing without deleting it (its own test binary — it sets `SIMSAPA_DIR` before the `OnceLock`) |
| `backend/tests/test_storage_probe.rs` | probe verdicts and litter-free cleanup, including the two reason strings |
| `assets/qml/tst_StorageCandidatesList.qml` | selectability rules, the demote-only merge, group rules, the Database-Validation exclusions, preselect, path matching, selection cleared on demotion |

The Android-only C++ (`cpp/utils.cpp`'s JNI) is **not compiled by `make build`**.
Syntax-check it against the Android Qt headers with the NDK clang
(`-fsyntax-only --target=aarch64-linux-android27`) until a real Android build runs;
a wrong JNI signature fails *silently* (the exception is cleared, an invalid object
comes back), which on a phone with no removable storage is indistinguishable from a
correct "no extra rows" result.

## 13. Manual device test checklist

The desk-testable halves are covered by §12; everything here needs a phone. Use
the beta debug build and the §11 recipes. The PRD's §8 numbering is kept so the
two documents can be read side by side.

**Already passed** (2026-08-05, SM-S911B / Android 16, beta debug) — re-run only
if the relevant code is touched again:

| # | Test | Expected |
|---|---|---|
| 7 | Internal install present **and** recorded path unreachable | The recovery UI appears. `init_app_data` / `start_webserver` never log — the app must **not** boot silently from the internal copy |
| 8 | Genuine first run, no `storage-path.txt` | Today's setup flow, no unavailable-location message |
| 8a | Kill a first-run download part-way, relaunch | Download flow resumes with **no** message, and `StorageDialog` still opens |
| 8b | Trailing newline / whitespace-only `storage-path.txt` | Newline trims and resolves; whitespace-only is a genuine first run |
| 8d | Marker present in `reachable_empty` | Recovery skipped **and** the upgrade download still auto-starts — the peek did not consume |
| 8f | Recorded path missing but its **parent writable** | `unreachable` + the FR-23 message; the directory must **not** have been created |
| 8h | Tiering | The scan performs no SQLite probes; probes log after `app.exec()` on a worker thread; a demoted candidate moves to "Not usable for the database" after the list has rendered and clears any selection on it |
| 8i | Decline in the FR-2 configuration | First-time install (`StorageDialog` → download → quit); must **not** fall through into the running app |
| 8j | Repeat 8f, then relaunch untouched | Still `unreachable`, message shown again, directory still absent |
| 8k | Marker × `unreachable` | Recovery runs anyway; the marker still exists afterwards |
| 8l | `reachable_empty` → group 2 | `StorageDialog` opens; after picking a location the download does **not** re-ask |
| 8m | Try Again after making the path reachable-but-empty | Falls through to the download flow with no message — never redisplays "not currently available" |
| 5 | Database Validation → "Look for Database on Other Storage" | Finds the relocated install, adopts it, prompts a restart; not visible on desktop |

**Still to run.** Each needs a device or a configuration not yet available:

- [ ] **1** — install to a card, move the card to another socket/reader, relaunch.
      The recovery dialog appears; after adopting and restarting the app opens
      against the card with bookmarks intact and **no download**. *(Needs a card
      slot.)*
- [ ] **2** — card removed entirely. No recovery dialog; the FR-23 message names
      the recorded path and offers Try Again / Set Up Again. *(Needs a card.)*
- [ ] **3** — the reported scenario, with a **card reader**. Either the card is
      found and adoptable, or it is listed as *seen but not usable for app data*
      with advice to use the phone's slot. In no case a bare download screen with
      the card missing from the list. *(Needs a reader.)*
- [ ] **4** — decline the recovery dialog. The normal first-run flow proceeds, and
      the recovery dialog itself must **not** have written `storage-path.txt` —
      the file changes only when the user then picks a location in
      `StorageDialog`. *(Partly covered by 8i; the "wrote nothing" half wants an
      explicit before/after check of the file's mtime.)*
- [ ] **6** — first run with **two real volumes**: `StorageDialog` opens rather
      than auto-selecting, an unusable location cannot be chosen, and a
      low-space-but-writable location stays selectable with its warning **and its
      figure**. *(Needs a second volume; the low-space half needs a nearly full
      one.)*
- [ ] **8c** — the three groups with a card inserted: the internal installation
      under *"Existing app data found"*, the card under *"Available (no app data
      found)"*, and selecting the card starts a download there **without** a
      restart notice. *(Needs a card.)*
- [ ] **8e** — the data-loss guard. A complete internal installation, a
      `delete_files_for_upgrade.txt` marker in its `app-assets/`, and an
      unreachable recorded path. After launch the internal databases must **still
      exist** and be offered under "Existing app data found". **Only run this on a
      device with a disposable installation** — a regression destroys a multi-GB
      install. Verified once in 3.10; re-run only if the sweep gating is touched.
- [ ] **8g** — FR-37. Make `storage-path.txt` unwritable, then adopt a found
      installation: the dialog shows an error and **stays open**, with no restart
      notice and no quit. Then Set Up Again → `StorageDialog` → Select: likewise
      an error, and it must **not** proceed to the download.
- [ ] **8n** — FR-36a. In the `unreachable` state, open Database Validation and
      confirm the per-database `present_at_start` values are populated (not
      `null`) even though the zero-byte sweep was skipped, and that a zero-byte
      stub at the resolved path is reported as **missing**.
- [ ] **9** — no measurable increase in startup time on the normal
      (database-present) path, and no new work inside the QML engine load.
      `STARTUP-TRACE` around the predicate, the scan and the probes shows only the
      predicate running before `app.exec()`. *(The "after `app.exec()`" half is
      already covered by 8h.)*
- [ ] **10** — the support signal: *"the app asks me to download everything again
      after I moved my SD card"* stops recurring.

**8o** is the desktop regression and needs no phone: put a `storage-path.txt`
pointing at a non-existent path in the desktop internal app root. The desktop app
must be entirely unaffected — no message, no recovery UI, and all three
destructive startup sweeps still run.

## 14. What has never executed

Worth stating separately from §13's checklist, because it is a property of the
*code*, not of the test plan: **the removable-media paths have never run at all.**
The only device available was an SM-S911B with no card slot, so every run had
exactly one usable location.

Concretely, the following have never been observed doing anything:

- **`getExternalStorageState()` returning anything but `"mounted"`** — so FR-29's
  read-only (`mounted_ro` → "Read-only — the app cannot write here") and removed
  ("Not available") classifications are a single string compare that has never
  fired.
- **`is_duplicate_emulated_candidate()` declining to drop a row** — every device
  run so far had an emulated duplicate to drop and no real card to preserve. The
  `is_removable` guard is unit-tested but has never seen a real removable volume.
- **A `found` row on a volume other than the internal one, from real hardware.**
  Adoption has been exercised end to end, but only against a fabricated second
  candidate (§11).
- **`getStorageVolume(File)` on adopted (internal-formatted) storage** — the log
  line should read `matched=true by=volume` with no extra "Not usable for app
  data" row.

None of this is known-broken; it is unexercised, which is a different and more
honest claim. The standing backlog with the reasons each item was deferred is §9
of the task file.
