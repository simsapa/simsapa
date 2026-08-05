# PRD — Relocated Storage Recovery (Android MicroSD moved to another socket)

## 1. Introduction / Overview

On mobile, the user chooses where the app databases are downloaded to
(internal storage or an external volume such as a MicroSD card). The choice is
persisted as a **plain absolute path string** in `storage-path.txt` inside the
app's internal storage, and read back on every launch.

An Android external volume's path contains a volume identifier, e.g.:

```
/storage/1A2B-3C4D/Android/data/io.github.simsapa.app/files
```

That identifier is a property of **how the card is currently mounted**, not of
the card itself. A user reported that after moving the same MicroSD card into a
**card reader**, the app could no longer find its already downloaded databases:
the recorded path no longer resolves, so `appdata_db_exists()` returns false and
the app falls back into the first-run download screen — even though a complete,
working installation is sitting on the card the user just re-inserted.

The reported case involved a **card reader** — the card attached over USB OTG
rather than in the phone's own slot. Android does not reliably expose
`Android/data/<pkg>/` on USB-attached volumes, so such a volume may or may not
appear in `getExternalFilesDirs()`, and we have no reader on hand to find out.

**The design deliberately does not depend on the answer.** Because the storage
list reports every visible volume, grouped into *existing app data found* /
*available (no app data found)* / *not usable for the database, with a reason*
(FR-9, FR-27 – FR-34), each possible outcome is already self-explanatory to the
user: the reader volume appears in one of the first two groups and can be
selected; or in the third with a reason, so they know to use the phone's card
slot; or it is absent, in which case FR-23 still names the unreachable recorded
path. No branch leaves the user staring at a bare download screen wondering where
their card went.

This feature adds a **recovery step**: when the recorded storage path does not
contain the database, the app re-enumerates the storage locations it would have
offered at first run, checks each one for an installation it would itself have
created, and — if it finds one — offers to adopt that location instead of
forcing a fresh multi-hundred-megabyte download. The same recovery is also
reachable on demand from the Database Validation window on mobile.

## 2. Goals

1. A user who moves their MicroSD card between sockets/readers can recover their
   existing installation without re-downloading, and without losing the user data
   (bookmarks, gloss/prompts history, recordings, imported books) that lives in
   `appdata.sqlite3` on that card.
2. The recovery is explicit: the user is told what was found and where, and
   confirms it. The app never silently switches storage locations.
3. A genuine first run is untouched: with no recorded location — **and equally
   with a recorded location whose download never completed** — the user sees
   today's storage-selection + download flow and no new messages (FR-23a,
   FR-24). A new message appears only when the recorded location is genuinely
   unreachable.
4. A stale/unreachable recorded path can no longer cause the app to fall back to
   an arbitrary working directory (see FR-20, FR-21), nor to silently reopen an
   unrelated database at a location the user did not choose (FR-2).
5. Recovery is reachable both automatically (at startup, when the recorded
   location no longer holds the database) and manually (from Database
   Validation, on mobile).
6. When the recorded location is genuinely unreachable and nothing is found, the
   user is told *that*, naming the path — instead of being silently returned to
   the first-run download screen as though they had never installed anything
   (FR-23).
7. Every storage location **the app can enumerate** appears somewhere in the
   app's list, under one of three headings — found / available / not usable with
   a reason — so no enumerable volume ever silently vanishes (FR-9, FR-27).

   This is bounded by what `getStorageVolumes()` reports, which is exactly the
   unknown of §9.7: if a USB-OTG card reader is invisible to that API too, the
   volume cannot be listed at all. That case is not left dangling — it is the
   third branch of §1, where FR-23 names the unreachable recorded path instead.
   The guarantee is "no volume the app can see disappears", not "the app can see
   every volume".

## 3. User Stories

- **As a user who moved my SD card to a different slot**, I want the app to find
  my existing databases on the card, so that I don't have to download ~1 GB again
  over mobile data and don't lose my bookmarks.
- **As a user with two cards that both once held app data**, I want to see which
  locations were found and choose the right one, so that I don't get silently
  attached to an old, stale copy.
- **As a user who genuinely wants a fresh install**, I want to be able to decline
  the found location and continue to the normal setup screen.
- **As a user whose card is simply gone**, I want the app to be told my chosen
  location is unavailable and named, rather than being dropped into a setup
  screen as though I had never installed anything.
- **As a user who moved from internal storage to a card**, I want the app to tell
  me my card is missing rather than quietly reopening the old internal database
  and showing me stale bookmarks.

## 4. Functional Requirements

### Detection

Two paths must be distinguished throughout this PRD, because conflating them is
what makes the naive design unsafe:

- **Recorded path** — the string in `storage-path.txt`, i.e. the location the
  user actually chose. May be absent (never configured) or unreachable.
- **Resolved path** — what `get_create_simsapa_dir()` returns, which after FR-20
  may be an internal-storage *fallback* that the user never chose.

The recorded path has **three** states, and they must not be collapsed into two.
`StorageDialog` writes `storage-path.txt` at **Select** time
(`StorageDialog.qml:190` → `save_storage_path()`), *before* any download runs, so
"a recorded path exists" does **not** imply "an installation was ever completed":

| State | Meaning | Behaviour |
|---|---|---|
| **Absent** | genuine first run | today's flow, unchanged (FR-24) |
| **Unreachable** | volume gone / moved | scan → adopt (FR-9) or FR-23 message |
| **Reachable but empty** | first download never finished, or the databases were deleted | scan → adopt (FR-9) or **today's download flow, no message** (FR-23a) |

1. The recovery **scan** MUST be triggered by the **storage-path condition**, not
   by `appdata_db_exists()`. It MUST run when, on mobile, **all** of the
   following hold:

   a. `storage-path.txt` exists (the user has configured a location before), and
   b. the recorded path is **unreachable** (FR-1c), **or** is reachable but holds
      no usable installation (FR-5).

   Evaluating this costs one file read plus one `try_exists()` and one
   `metadata()` — all of which already happen on every launch inside
   `get_create_simsapa_dir()` and `appdata_db_exists()` — so normal launches pay
   nothing measurable. The predicate is **read-only**: see FR-35.

   The **outcome** when the scan finds nothing differs between the two halves of
   (b): the unreachable case gets FR-23's explanatory message, the
   reachable-but-empty case falls straight through to today's download flow
   (FR-23a). The predicate of FR-35 MUST therefore report *which* of the three
   states holds, not a bare boolean.

   c. **"Unreachable"** means: `try_exists()` on the recorded path returns
      `Ok(false)`, or returns `Err`, or its metadata cannot be read. A path that
      **exists** counts as **reachable** even when it is empty.

      **The predicate MUST NOT attempt `create_dir_all()` to decide this.**
      An earlier draft defined "unreachable" as "does not exist *and* cannot be
      created", which is wrong in three ways: it makes the predicate mutate the
      filesystem, it contradicts the cost stated above, and — decisively — on
      any recorded path whose parent happens to be writable it would *create*
      the directory and thereby reclassify `unreachable` as `reachable_empty`,
      suppressing the very FR-23 message this feature exists to show. This is
      the same principle as FR-5's note: creating a directory is not evidence of
      anything.

      **FR-20a is the other half of this rule and is not optional.** Keeping the
      predicate pure is pointless if `get_create_simsapa_dir()` creates the
      directory moments later in the same launch — which is exactly what it does
      today (`backend/src/lib.rs:785-787`), and FR-36 orders the predicate
      *before* that call. Without FR-20a the classification is
      self-erasing: launch 1 reports `unreachable` and shows FR-23, `init_app_globals()`
      then creates the directory, and launch 2 reports `reachable_empty` and
      shows nothing. The real-card case is masked (the create fails on an absent
      volume), so this would bite the internal / adopted-storage cases and
      **every `adb` test recipe in §8**.
2. **The app MUST NOT boot silently from a fallback location while the recorded
   path is unreachable.** When FR-1's condition holds, the recovery flow (scan →
   prompt, or FR-23's message) takes precedence, **even if a usable
   `appdata.sqlite3` happens to exist at the resolved fallback location**.
   Without this rule, a user who previously used internal storage and later moved
   to a card would silently be returned to their old internal database — a stale,
   unannounced switch of storage location, which Goal 2 and §5 forbid. This is a
   deliberate restriction on FR-20: the fallback keeps path *resolution* total,
   and never doubles as an implicit adoption.

   **Scope:** this rule governs **adoption** and `init_app_data()` — i.e. running
   the app against a database. It does **not** apply to the two settings reads at
   `cpp/gui.cpp:395` (`render_loop_basic_c()`, before `QApplication`, which is
   constructed at `:403`) and `cpp/gui.cpp:413` (`theme_link_colors_c()`, after
   `QApplication` but before the QML engine load), which already consult the
   resolved path today. Both are guarded by `appdata_db_exists()`, so under
   FR-20's fallback both may read a fallback database. Those MAY continue to read a fallback database: they
   set an environment variable and a palette colour, adopt nothing, and skipping
   them would degrade the very screens the recovery flow is about to show. This
   exemption MUST be noted in the code.
3. The check MUST be limited to **mobile platforms** (the `is_mobile()` gate that
   already guards the `storage-path.txt` mechanism in
   `get_create_simsapa_dir()`). Desktop behaviour is unchanged.
4. The candidate locations MUST be the same set the storage-selection dialog
   offers: the internal app data location plus every path returned by Android's
   `getExternalFilesDirs()` — i.e. the existing `get_app_data_storage_paths()`
   in `cpp/utils.cpp:143` — **plus the recorded path itself when it is not
   already in that set** (FR-6). No other filesystem scanning is performed: no
   `/storage/*` walk, no `MediaStore`, no SAF trees.
5. For each candidate path `P`, a **usable installation** is defined as:
   `P/app-assets/appdata.sqlite3` exists and is **non-zero length**. This mirrors
   the existing `check_file_exists_print_err()` semantics; a zero-byte stub does
   not count. (`appdata_db_exists()` itself tests existence only, with no length
   check — it relies on `ensure_no_empty_db_files()` having swept the *resolved*
   path first. Other candidates are never swept, so the scan MUST do its own
   length check rather than reusing that helper.)

   **Terminology:** a *usable installation* (this requirement) is about the
   **database file**; a *usable location* (FR-29) is about the **volume**. A
   volume can be perfectly usable and hold no installation — that is FR-9's
   second group, and the distinction is what the grouping exists to make
   visible.

   Note that **`getExternalFilesDirs()` creates the app directories on the
   volumes it returns** — enumeration is not a read-only operation. The existence
   of a candidate directory therefore proves nothing at all, which is exactly why
   the test is on the database file rather than on the folder.
6. The recorded path MUST be tested by the scan like any other candidate: it
   may have become reachable again between the check and the scan (most likely
   under FR-25's **Try Again**), and it is the most likely correct answer when
   it is.

   **This requires an explicit extra candidate, not a hope.** When the recorded
   path is unreachable it is by definition *not* returned by
   `getExternalFilesDirs()`, so relying on the enumeration alone makes this
   requirement a no-op. The scan MUST therefore append the recorded path as an
   extra candidate whenever it does not already appear in the enumerated set —
   which is the bounded exception FR-4 names. Comparison is on **normalized**
   paths (FR-6a).
6a. **Path comparison MUST be normalized** wherever paths are matched against
   each other — the FR-6 de-duplication, FR-11a's "(current selection)" marking
   and FR-31a's merge of tier-2 verdicts into tier-1 rows. Trim per FR-22a,
   strip trailing separators, and compare with `Path`/`PathBuf` component
   equality rather than raw strings. A raw string compare silently fails on a
   trailing slash and produces a duplicated candidate row or an unmarked current
   selection, with no error anywhere. Do **not** use `canonicalize()`: it fails
   on a non-existent path, which is the case that matters here.
7. File existence checks MUST use `try_exists()`, never `.exists()`, per the
   project's Android rule.
8. The scan MUST be resilient: an unreadable or unmountable candidate is skipped
   with a logged warning, never propagated as an error that aborts startup.

### User confirmation

9. If **one or more** usable installations are found, the app MUST present a
   confirmation dialog before any download UI. The dialog lists **every visible
   storage location**, not only the hits, grouped into three labelled sections in
   this order:

   | Group | Contents | Selectable |
   |---|---|---|
   | **Existing app data found** | candidates holding a usable installation (FR-5) | yes |
   | **Available (no app data found)** | candidates that are usable for app data but hold no installation | **startup entry point only** — yes, starts a fresh download there; shown greyed and non-selectable from Database Validation (FR-18) |
   | **Not usable for the database** | volumes classified unusable by FR-29, each with its reason | no |

   Grouping is what makes the list self-explanatory: a user who can see their
   card in the phone must be able to find it *somewhere* in this list and read
   why it is or is not offered. An empty group MUST be omitted entirely, so the
   common single-hit case is still a short list.

   Each row in the first group MUST show: the volume label, the full path, the
   **database size** (the byte size of the found `appdata.sqlite3`) and its
   **last-modified time** — the single most useful field for telling two copies
   apart. Both are required, not best-effort: FR-5's non-zero-length test
   already calls `metadata()`, which carries `len()` and `modified()` together,
   so neither costs an extra syscall. Rows in the second group show label, full
   path and **free space** (the `megabytes_available` figure already produced by
   `createStorageInfo()`), plus FR-30's low-space warning where it applies.
   "Database size" and "free space" are different quantities and MUST be labelled
   distinctly in both the UI and the JSON — never a bare "size".
10. A **group 1** row whose location does not hold **all three** databases
    (`appdata.sqlite3`, `dictionaries.sqlite3`, `dpd.sqlite3`) MUST be marked
    **"Partial"** — a single short marker, not a per-database breakdown, so rows
    stay readable. The check is existence only, no opening and no version check
    (§5). FR-5 makes `appdata.sqlite3` alone sufficient to offer a location, so a
    partially downloaded install *can* be adopted; the marker is what stops that
    happening unknowingly. Which files were missing MUST be written to the log
    for support purposes.
11. The dialog MUST list **all** found locations and let the user select one.
    With a single hit that group has one entry, and it MUST be pre-selected.
    **Within each group** the **internal** location MUST be listed first,
    followed by external locations, matching the ordering convention already used
    by `StorageDialog.qml` (`Component.onCompleted` inserts the internal entry at
    index 0). Group order is fixed by FR-9's table and MUST NOT depend on where
    the hits happen to be.
11a. **The row corresponding to the recorded path MUST be marked as the current
    selection**, in whichever group it lands. In the `reachable_empty` state
    (FR-23a) the recorded path appears in group 2 with a "Download Here" button
    that would rewrite the value it already holds; without a marker the user has
    no way to tell which location they previously chose, which is the one thing
    they are most likely to want to know. A short "(current selection)" suffix
    rendered next to the label is enough.

    **The marker MUST be a separate `is_recorded: bool` field, not a string
    appended to `label`.** FR-12 pins `label` to `createStorageInfo()`'s value,
    and the label is reused and compared elsewhere; mangling it makes every such
    use subtly wrong with no error anywhere. The delegate renders the suffix.
    Matching a row to the recorded path uses FR-6a's normalized comparison.

    In the `unreachable` state the recorded path appears only as FR-6's extra
    candidate — normally in group 3, or in group 1 / 2 if it has come back. The
    common case is that the volume is gone, which is FR-23's case, not this one.
12. Volume labels MUST reuse the existing `createStorageInfo()` labelling in
    `cpp/utils.cpp`: `QStorageInfo::displayName()` when non-empty, otherwise the
    existing fallback guess ("Internal Storage" / "SD Card" / "External
    Storage"). The **full path is shown alongside the label in every row**, since
    on Android the label is frequently a guess and the path is the real
    discriminator between two external candidates.
13. Accepting the dialog MUST write the selected path to `storage-path.txt`
    through the existing `StorageManager::save_storage_path()` mechanism, so the
    selected location is used on subsequent launches. This is the same call
    `StorageDialog` already makes, with the same `(path, is_internal)` arguments.
14. **The two selectable groups end differently, and the confirm button MUST say
    which is about to happen.**

    - **Group 1 (existing app data found) — adoption.** After writing
      `storage-path.txt`, the app MUST show a **"Storage location updated. Please
      restart Simsapa."** message and quit. This applies to **both** entry
      points. Re-deriving the runtime paths in-process was considered and
      rejected as disproportionate for a rare event — see §7. From the startup
      path "quit" is the existing `Qt.quit()` → `NormalExit` route; from Database
      Validation it terminates the **whole running app** (live `AppData` pools,
      the Rocket server thread and the open Tantivy directories all go down with
      it). That is intended, not a window close — implement it as an application
      quit and word the message so the user expects the app to disappear.
    - **Group 2 (available, no app data found) — fresh download.** Writing
      `storage-path.txt` is all that is required; the app MUST then continue to
      the existing download flow at the newly selected location, exactly as
      accepting `StorageDialog` does today. **No restart notice and no quit** —
      nothing has been adopted, and the runtime paths have not yet been consumed
      for anything but this choice. This is the same outcome the user would get
      by declining (FR-15) and picking that location in `StorageDialog`; offering
      it inline just saves a dead end.

    Button labels MUST therefore be per-group, e.g. **"Use the Selected
    Database"** for group 1 and **"Download Here"** for group 2.
15. **Declining the dialog means "set up a new database", and MUST be treated as
    a first-time install.** The user had the option to adopt an existing
    installation (group 1) or to download to another location (group 2) and
    chose neither, so the only remaining intent is a fresh setup. Declining
    therefore runs the existing first-run flow — `StorageDialog` (where the user
    picks the location, writing `storage-path.txt` at Select time as today) →
    `DownloadAppdataWindow` → `Qt.quit()` → `gui.cpp`'s `NormalExit`, i.e. the
    user restarts into the new installation. The **declined** path MUST NOT be
    written to `storage-path.txt` by the recovery dialog itself, and the decision
    is **not** remembered — the prompt may appear again on the next launch if the
    same installation is still found.

    **This must be implemented explicitly, not inherited.** "Exactly as today"
    is not a sufficient specification here, because today the first-run flow is
    only reachable through `if (!appdata_db_exists())` at `cpp/gui.cpp:486`. In
    the FR-2 scenario — recorded path unreachable, a usable database sitting at
    the resolved internal fallback — `appdata_db_exists()` is **true**, so that
    branch was never entered under these conditions and its behaviour is
    undefined rather than inherited. The requirement is that the recovery flow
    enters `create_download_appdata_window()` and terminates in `NormalExit` on
    its **own** decision, independently of `appdata_db_exists()`.

    Consequently a decline in the FR-2 scenario **does** end with the user
    downloading a fresh database over the setup they just declined to adopt,
    which is the intended reading of "set up a new database". The pre-existing
    database at the fallback location is left in place on disk and is not
    deleted by the recovery flow; whether the download overwrites it depends
    only on whether the user picks that same location in `StorageDialog`, which
    is today's ordinary behaviour and is out of scope here.

### Manual recovery from Database Validation

16. The Database Validation window MUST offer a **"Look for Database on Other
    Storage"** action, **visible on mobile only** (`is_mobile`), for users whose
    database went missing for reasons other than a first-run-style startup —
    e.g. the card was moved while the app had already started, or the app fell
    back to the internal location.
17. That action MUST run the same scan (FR-4, FR-5) and present the same
    grouped selection dialog (FR-9 – FR-12) as the startup path, so there is one
    code path and one UI for both entry points.
18. Adoption from Database Validation MUST behave exactly as on the startup path:
    write `storage-path.txt` (FR-13), then show the restart notice and quit
    (FR-14). Group 2 ("available, no app data found") rows MUST NOT be
    selectable from this entry point — the app is already running against a
    database, so "download here instead" is not a meaningful answer and the
    download flow is not reachable from Database Validation. The group MAY still
    be shown, greyed, so the user can see the location was seen and is usable.

    **The recorded path's own group 1 row MUST NOT be selectable either.** When
    the recorded storage is healthy (state `ok`), the scan lists it as a found
    installation with `is_recorded` set — and "adopting" the location the app
    is already using would rewrite the identical path and quit the whole app
    for nothing. Show it with its "(current selection)" marker (FR-11a),
    non-selectable. This is the Database Validation expression of the same rule
    as 12.4's `state == OK` branch: never offer the user the option of adopting
    the location they already have.
19. If the scan finds no **existing installation**, the action MUST report that
    plainly ("No existing database was found on the other storage locations")
    rather than closing silently, and MUST show the grouped list alongside the
    message (all rows non-selectable, as in FR-23) so the user can see which
    volumes were examined and why each was rejected. "Nothing found" plus a list
    of what was looked at is a diagnosis; "nothing found" alone is a dead end.

### Robustness fix (in scope)

20. `get_create_simsapa_dir()` (`backend/src/lib.rs:670`, mobile branch at
    `:735-790`) currently calls `create_dir_all()` on the recorded path and
    returns `Err` if that fails, while several callers do
    `.unwrap_or(PathBuf::from("."))` — meaning an unreachable recorded path can
    silently redirect the app to an arbitrary working directory. This MUST be
    changed so that a recorded storage path which cannot be resolved falls back
    to the **internal app root**, with a clearly logged warning naming the
    unreachable path. (If `get_create_simsapa_internal_app_root()` itself fails
    — today's `"."` fallback at the top of the function — that pre-existing
    last resort is unchanged and out of scope; only the recorded-path failure
    mode is redirected.)
20a. **`get_create_simsapa_dir()` MUST NOT create a recorded path that does not
    exist.** Today it does (`if !p.try_exists()? { create_dir_all(&p)?; }`,
    `backend/src/lib.rs:785-787`). A missing recorded path MUST instead take
    FR-20's fallback, so that the `unreachable` classification is **stable
    across launches** — see FR-1c for why a self-erasing classification breaks
    both FR-23 and test 8f.

    Creating the directory is only ever correct immediately after the user
    chooses a location, so directory creation for a *newly chosen* path belongs
    with `save_storage_path()` / the download flow, which already runs
    `create_dir_all()` on the asset directories it writes into. A path the user
    chose in a previous session and that has since vanished is a diagnosis, not
    something to fabricate. Note that `get_create_simsapa_app_assets_path()`
    (`lib.rs:793`) still creates the *asset* subdirectory under whatever
    `get_create_simsapa_dir()` returned; that is unchanged and is fine, because
    it operates on the resolved (reachable) root.
21. That fallback is **path resolution only**. It MUST NOT write
    `storage-path.txt`, MUST NOT be treated as the user's choice, and MUST NOT
    suppress the recovery flow — see FR-2, which is the rule that keeps FR-20
    from silently re-homing the app.
22. The unreachable-recorded-path condition SHOULD be recorded in the existing
    `StartupDbReport` process-global (`backend/src/db/mod.rs:81-123`) so Database
    Validation can report "configured storage location is unavailable" rather
    than a generic missing-database message.

    This is a **new field and a JSON shape change**, not an existing one. The
    report currently holds three `DbReportEntry` values keyed by `DbKind`, and
    `get_startup_db_report_json()` emits one object per database. The new field
    is **not** per-database: it belongs at the top level of both the struct and
    the JSON (e.g. `"storage_path": { "recorded": "…", "state":
    "absent" | "unreachable" | "reachable_empty" | "ok" }`), and the QML consumer
    in `DatabaseValidationDialog.qml` MUST be updated in the same change.
    Recording follows the existing convention: written once, early, before
    anything can change the answer — which FR-36 pins to a specific call site.

    The shape change touches **three** places, not two: the struct and
    `get_startup_db_report_json()` (`backend/src/db/mod.rs:80-153`), the bridge
    method — which is QML-visible as `SuttaBridge.get_startup_db_report()`
    (`bridges/src/sutta_bridge.rs:3875`, wrapping
    `get_startup_db_report_json()`) — and its `qmllint` stub at
    `assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml:268`.
22a. **`storage-path.txt` MUST be trimmed when read.** `get_create_simsapa_dir()`
    currently does `PathBuf::from(contents)` with no trimming
    (`backend/src/lib.rs:784`), so a single trailing newline produces a path that
    can never resolve — permanently, and with no diagnosis beyond "not found".
    Any file written by hand, by a script, or by `adb` (see §8) is therefore
    unusable today, and this is itself a candidate explanation for
    relocated-storage reports. Trim leading/trailing whitespace, and treat an
    empty result as "no recorded path" (FR-1 state *absent*), not as an
    unreachable path. `save_storage_path()` writes no newline, so this changes
    nothing for files the app wrote itself.

### Unavailable storage location (independent of the scan)

23. When the recorded path is **unreachable** (FR-1c) and the scan finds **no**
    usable installation, the app MUST NOT silently present the first-run download
    screen as though nothing had ever been installed. On mobile it MUST first
    show an explanatory message naming the recorded path, e.g.:

    > *"Simsapa's app data is stored at `<recorded path>`, which is not
    > currently available. If it is on a memory card, make sure the card is
    > inserted in the phone's own card slot — a card in a USB card reader may
    > not be usable for app data. You can also choose a new location and
    > download the databases again."*

    with **Try Again** (re-check and re-scan) and **Set Up Again** actions.
    **Set Up Again** means the same thing as declining the recovery dialog and
    MUST take the same code path with the same end state — a first-time install:
    `StorageDialog` → `DownloadAppdataWindow` → quit → restart, per FR-15.

    **This message MUST also show the grouped storage list** (FR-9's three
    groups, all rows non-selectable here) beneath the text. Without it, the one
    screen the user sees in the reported scenario is the one screen that does
    *not* show them where their card is — and §1's and Goal 7's argument that
    "the app is its own diagnostic" would hold only after they press Set Up
    Again and reach `StorageDialog` (FR-28). The list costs nothing extra: the
    scan has already produced it, and it is what turns "your data is at
    `<path>`, which is unavailable" into "…and here is every volume I *can*
    see."
23a. **FR-23 MUST NOT fire when the recorded path is reachable but empty.** In
    that state the app MUST fall through to today's download flow with **no
    message** — the location is available, so telling the user it is not would be
    false, and telling them anything at all would be baffling.

    This state is not exotic: `StorageDialog` writes `storage-path.txt` on
    **Select**, before the download begins, so *every* first run that is
    cancelled, interrupted, or fails part-way lands here on the next launch. A
    naive reading of FR-1(b) would greet all of those users with an
    unavailable-storage message on the second launch of a brand-new install. The
    scan still runs (another volume may hold an installation worth adopting, and
    offering it is strictly better than re-downloading); only the *nothing found*
    outcome differs.

    **"Today's download flow" includes `StorageDialog`, and it MUST still
    open.** This is the one place where the fall-through must *not* set
    `skip_storage_dialog` (§12.4): the user's recorded choice here is one they
    made before a download that then failed, and the most likely reason it
    failed is the location itself — the card was too small, was pulled, or was
    never really writable. Suppressing the dialog would pin them to that
    location on every subsequent launch with no way to change it. Contrast
    FR-14 group 2, where the user has *just* chosen in the recovery dialog and
    re-asking would be asking twice.
24. Neither FR-23 nor any other new message MUST fire when `storage-path.txt`
    does not exist, or exists but is empty/whitespace-only after FR-22a's trim. A
    genuine first-run user has no recorded path and MUST see today's unchanged
    setup flow. This precondition is FR-1(a), restated here because it is the
    difference between a helpful message and a baffling one. Together with
    FR-23a, the rule is: **a new message appears only in the unreachable
    state.**
25. **Try Again** MUST re-evaluate the predicate (FR-35) *and* re-enumerate the
    storage volumes and re-run the scan from scratch, not re-display a cached
    result — the user's likely action between presses is physically re-seating
    the card.

    **It MUST then re-branch on the new state, not merely on whether the scan
    found anything.** A re-seated card can move the recorded path to
    `reachable_empty` (fall through to the download flow, no message — FR-23a) or
    to `ok`, in which case the recorded path is already correct: there is nothing
    to adopt, and the app MUST say so and ask for a restart ("Storage is
    available again. Please restart Simsapa.") rather than offering the user the
    option of adopting the location they already have. Re-displaying FR-23's
    "not currently available" for a path that has become available is the exact
    falsehood FR-23a forbids. A re-check that reports `absent` (the file
    deleted or emptied between presses) is FR-24's genuine first run: fall
    through to today's setup flow with no message.
26. FR-23 MUST hold **whether or not** `getExternalFilesDirs()` turns out to
    enumerate USB-attached volumes (§9.7). Together with the per-row verdicts of
    FR-27 – FR-33, it is what makes the feature's behaviour well-defined without
    knowing that answer in advance.

### Showing unsuitable locations (mobile)

27. The storage lists MUST show storage devices the app can **see but not use**,
    as non-selectable rows with a short reason, rather than omitting them. A
    volume the app can enumerate must not silently vanish from the app's list —
    that is precisely the confusion the reported case produced.

    "See" here means *reported by `getStorageVolumes()`*, which is not
    necessarily everything the user can see in their phone — see Goal 7 and
    §9.7. A volume neither `getExternalFilesDirs()` nor `getStorageVolumes()`
    reports cannot be listed; FR-23 is what covers that case.
28. This applies to **both** the recovery dialog (FR-9) and the existing
    first-run `StorageDialog.qml`, so an unusable location is never silently
    offered or silently hidden at either point. In the recovery dialog these rows
    are FR-9's third group; in `StorageDialog` they are appended after the
    selectable locations under the same "Not usable for the database" heading.
    One delegate serves both (§6).
29. A candidate MUST be classified as **unusable** — non-selectable — when any of
    the following holds, and the row MUST carry the corresponding reason text:

    | Condition | Detection | Reason shown |
    |---|---|---|
    | Visible volume with no app-writable directory (likely USB/SAF-only) | present in `StorageManager.getStorageVolumes()` but matching no `getExternalFilesDirs()` entry | "Not usable for app data (this device may only allow file transfers here)" |
    | Not mounted / removed | `Environment.getExternalStorageState(File)` | "Not available" |
    | Read-only | `getExternalStorageState()` returns `mounted_ro` | "Read-only — the app cannot write here" |
    | Cannot create files | write probe (FR-31) fails | "The app cannot write here" |
    | Cannot host a database | SQLite probe (FR-31) fails | "This location cannot store the app database" |

    The mounted / read-only checks apply to **external candidates only**.
    `Environment.getExternalStorageState(File)` reports the state of the volume
    containing the given file and is not meaningful for the internal app-data
    directory, which is always present and writable by definition; the internal
    row MUST NOT be classified from it. Rows 1–3 are tier 1 and rows 4–5 are
    tier 2, per FR-31a.

30. **Insufficient free space MUST be a warning, not a disqualification.** The row
    stays selectable and carries "May not have enough free space" plus the figure.
    This needs a field of its own — `low_space_warning: bool` in the tier-1 JSON
    (§7) — because `group` cannot express it: the row is `available` (or `found`)
    *and* warned. Deriving it in QML from `megabytes_available` would put the
    threshold in two places.
    The required size is not knowable when the dialog opens — it depends on which
    languages and bundles the user has not chosen yet, and on releases info — so
    treating a conservative estimate as a hard block risks locking a user out of
    the only card that would in fact have worked. Let them choose; the download
    already fails loudly and recoverably if space really runs out.
31. The write / SQLite probe MUST be: create a temporary SQLite file in the
    candidate directory, create and drop a trivial table, close it, delete the
    file. This tests file creation **and** the file locking SQLite requires,
    instead of inferring suitability from flags. It MUST be run only in the
    dialogs (user-initiated), **never** on the FR-1 startup *predicate* or the
    FR-4 *scan*, which stay existence-only per §7's "keep it cheap" constraint.

    "Only in the dialogs" means **only where a selection is possible**: the
    non-selectable grouped lists shown under FR-23's and FR-19's messages
    render tier-1 verdicts only — probing there gains nothing (nothing can be
    selected) and touches volumes needlessly.

    Two cleanup obligations, both of which produce user-visible litter on a
    memory card if skipped:

    - **Delete the `-wal` and `-shm` siblings too**, not just the main file. A
      Diesel/SQLite connection in WAL mode leaves them behind, and the probe's
      whole point is to touch a volume the user may be about to keep using.
      Use a distinctive name (e.g. `simsapa-write-probe.sqlite3`) so any
      leftover is identifiable, and delete the whole set in a
      run-on-all-exit-paths cleanup, including the failure paths.
    - **The probe MUST be cancellable and MUST NOT outlive its dialog.** It runs
      off the UI thread (FR-32) against a volume whose worst case is a
      half-mounted card, so a dialog closed mid-probe must not have a worker
      still writing to a candidate directory, and a late verdict MUST NOT be
      merged into a destroyed model.

    Note the distinction: the recovery dialog is reached *from* the startup path,
    so "not on the startup path" is not the same as "not before `app.exec()`" —
    see FR-32.

31a. **This forces a two-tier classification API, and FR-29's five conditions
    MUST be split across the tiers.** Enumeration cannot be a single call that
    "gives every consumer the same picture", because the scan and the dialogs
    have opposite constraints — the scan must not probe, the dialogs must.

    | Tier | Contains | Callable from |
    |---|---|---|
    | **Tier 1 — cheap** | enumeration, labels, `megabytes_available`, FR-5's database checks, and the *flag-based* rows of FR-29: volume-with-no-app-directory (FR-33 matching), not-mounted, read-only | the FR-1 predicate path, the FR-4 scan, and the dialogs' first render |
    | **Tier 2 — probing** | the FR-29 rows that require the FR-31 probe: "cannot create files" and "cannot host a database" | the dialogs only, posted out of the QML engine load and off the UI thread (FR-32) |

    A row's `group` is therefore **provisional after tier 1 and final after tier
    2**: tier 2 can only ever demote a row from `available`/`found` to
    `unusable`, never promote one. The dialogs MUST render the tier-1 list
    immediately and update rows in place as tier-2 results arrive (FR-32's
    pending state). If a row the user has already selected is demoted by a
    tier-2 result, the selection MUST be cleared and the confirm button disabled
    (FR-34).

    The FR-4 scan consumes tier 1 **only**. A candidate that would have failed
    the probe can therefore still appear as a group 1 hit in the scan's own
    result — which is harmless, because the only consumer of the scan result is
    the dialog, which then applies tier 2 to the same rows before the user can
    act on them.
32. **Probing MUST NOT run during the QML engine load, and MUST NOT block the UI
    thread.** These are two separate hazards and both bite here:

    - `StorageDialog` is instantiated **inline** in `DownloadAppdataWindow.qml`
      (`:180`), so its `Component.onCompleted` — which already calls
      `get_app_data_storage_paths_json()`, i.e. JNI plus a directory-creating
      `getExternalFilesDirs()` — runs **during the engine load, before
      `app.exec()`**, when the window paints nothing. Adding per-volume SQLite
      probes there is precisely the pre-exec stall documented in
      `docs/startup-sequence-and-caches.md` §6, on a code path whose slowest case
      is a flaky or half-mounted card. The probes MUST be posted out of the load
      (`Qt.callLater` / `singleShot(0)`), or the dialog deferred, per that doc's
      `Loader` vs `Component + createObject` rule.
    - Once running, probes MUST be off the UI thread. They run once per
      enumeration, their results are cached for the lifetime of the dialog, and
      the list MUST render immediately — showing a pending state per row if
      necessary — rather than stalling behind a slow or failing volume.
33. Volume-to-candidate matching for FR-29 row 1 MUST be conservative. Without
    `StorageVolume.getDirectory()` (API 30, above our floor), a volume is matched
    to a `getExternalFilesDirs()` entry by its UUID appearing in that path.
    **Only volumes matching nothing** are shown as extra unusable rows, so a
    matching failure can produce a missing warning but never a duplicated or
    wrongly-disabled usable location.

    Two volumes are known not to match by UUID and MUST be special-cased rather
    than reported as unusable:

    - **Primary emulated storage** — `getUuid()` returns null, and the path is
      `/storage/emulated/0/…`. Match it via `isPrimary()`.
    - **Adopted (internal-formatted) storage** — the volume reports a UUID that
      does not appear in the path. Where it cannot be matched, FR-33's safe
      direction still holds (an extra unusable row, never a disabled usable
      one), but the row would be wrong; prefer matching leftovers by
      `getDescription(Context)` before declaring them unusable.
34. An unusable row MUST NOT be selectable, and the confirm button MUST stay
    disabled while one is highlighted. Storage sizes MUST be **omitted** on
    unusable rows: `QStorageInfo` on an unreachable path reports zeros, and
    "0.0 GB free" reads as a space problem rather than an availability one.

### Predicate, startup ordering, and write verification

35. **One read-only, four-state predicate is the single definition of the
    recorded-path condition.** It MUST return `absent` / `unreachable` /
    `reachable_empty` / `ok` (FR-1's three states plus the healthy case), never a
    boolean, and MUST be callable from `gui.cpp`, from Rust and from QML so that
    the startup branch, the FR-23 vs FR-23a decision, the FR-22 report field and
    both dialogs all agree.

    It MUST be **free of side effects**: no `create_dir_all()`, no file writes,
    no database opens — `try_exists()` and `metadata()` only (FR-1c, FR-31).
    This is what makes it safe to call at the earliest startup point, which
    FR-36 requires.

    `absent` MUST cover both "no `storage-path.txt`" and "file present but
    empty or whitespace-only after FR-22a's trim" (FR-24).

35a. **The predicate MUST be gated on `is_mobile()` internally and return
    `absent` on desktop**, without reading the file at all. `get_create_simsapa_dir()`
    returns the internal app root before ever looking at `storage-path.txt` when
    `!is_mobile()` (`backend/src/lib.rs:730-733`), so on desktop the file has no
    effect on anything — and a stray one, left by a dev machine or a synced home
    directory, must not be able to reach the FR-36 gate and skip the three
    destructive startup sweeps. Putting the gate inside the predicate rather
    than at each call site is what keeps FR-3 ("desktop behaviour is unchanged")
    true for every consumer at once, including the FR-22 report field.

36. **The predicate MUST be evaluated, and FR-22's report field written, before
    `init_app_globals()` at `cpp/gui.cpp:348`.**

    FR-22's "written once, early, before anything can change the answer" has no
    valid site later than this. `init_app_globals()` calls `AppGlobals::new()` →
    `AppGlobalPaths::new()` → `get_create_simsapa_dir()`
    (`backend/src/lib.rs:549-554`), which under FR-20 both resolves the fallback
    and creates directories. Recording the state after that point risks
    reporting a condition the app itself produced.

    **The four startup sweeps MUST be gated on the predicate.**
    `remove_download_temp_folder()`, `ensure_no_empty_db_files()`,
    `check_delete_files_for_upgrade()` and `check_remove_lang_index_dirs()` run
    at `cpp/gui.cpp:349-364` and all resolve their paths through
    `get_create_simsapa_dir()`. Before FR-20 an unreachable recorded path sent
    them at `PathBuf::from(".")`; after FR-20 it sends them at a **real internal
    installation** — and `check_delete_files_for_upgrade()` **deletes database
    files**. A stale internal `delete_files_for_upgrade.txt` plus an unreachable
    recorded path would then destroy an installation the user still wants, and
    it is a candidate for adoption at the moment it is destroyed.

    Therefore: when the predicate reports `unreachable`, the three
    **destructive** sweeps (`ensure_no_empty_db_files()`,
    `check_delete_files_for_upgrade()`, `check_remove_lang_index_dirs()`) MUST
    be skipped, with the skip logged and naming the unreachable path. They run
    normally in the `absent`, `reachable_empty` and `ok` states, where the
    resolved path is the user's actual choice.
    `remove_download_temp_folder()` is non-destructive to installed data and MAY
    run unconditionally. The chosen ordering and the reason MUST be stated in a
    comment at the `gui.cpp` call site.

36a. **Skipping `ensure_no_empty_db_files()` MUST NOT also skip the presence
    record.** That function is not only a sweep: it is documented as *"the
    authoritative first writer of the presence record… first write wins"* and
    calls `crate::db::record_db_presence()` for all three databases
    (`backend/src/lib.rs:864-897`). In the `unreachable` state the app never
    reaches `init_app_data()` / `DbManager::new()` (FR-2's invariant), so nothing
    else ever records it, and Database Validation — the screen this feature
    routes the user toward — would report `present_at_start: null` for every
    database in precisely the session that needs diagnosing.

    Gate the deletion with an internal `sweep: bool` parameter —
    `ensure_no_empty_db_files(sweep)` — keeping the **per-database order
    sweep-then-record exactly as it is today**. Do **NOT** implement this as a
    separate `record_db_presence_at_start()` called before the sweep: the
    function's own doc comment states *"a stub deleted here is recorded as
    **missing**, which is what makes the diagnosis honest"*, and
    `record_db_presence()` is first-write-wins — so recording before the sweep
    would record a zero-byte stub as *present*, and neither the deletion a
    moment later nor `DbManager::new()`'s re-record could ever correct it.
    With `sweep = false` a zero-byte file MUST likewise be recorded as
    **missing** (len == 0 is not a usable database) even though it is not
    deleted, so the record means the same thing in both modes. The presence
    then recorded is the honest one for the *resolved* path, which under FR-20
    is the internal fallback — correct, and exactly what FR-22's new top-level
    `storage_path` field is there to put in context.

37. **Adoption MUST verify that `storage-path.txt` was actually written before
    telling the user to restart.** `save_storage_path()` currently returns `()`
    and only *logs* the outcome of `save_to_file()`
    (`bridges/src/storage_manager.rs:42-68`), so a failed write is invisible.
    Combined with FR-14's write-then-quit, a failure produces a silent infinite
    loop: restart → same unreachable path → same dialog → adopt → quit, forever,
    with no message.

    The bridge method MUST therefore report success — either by returning a
    bool or by reading the file back and comparing — and the recovery dialog
    MUST show an error and stay open on failure rather than quitting. **This is
    a signature change to `save_storage_path()`**, so it also updates the
    `qmllint` stub in `assets/qml/com/profoundlabs/simsapa/StorageManager.qml`
    and the existing `StorageDialog.qml:190` call site. FR-13's "the same call
    `StorageDialog` already makes, with the same `(path, is_internal)`
    arguments" is superseded on this point.

    **The same failure handling MUST be applied at the `StorageDialog.qml:190`
    call site**, not just its signature. The recovery dialog's failure path
    ends in Set Up Again → `StorageDialog` → Select → the *same*
    `save_storage_path()` call, so closing the loop only in the recovery
    dialog leaves its own escape route open — and worse, a first-run user
    whose write fails would proceed to download into whatever
    `get_create_simsapa_dir()` resolves, a location they did not choose. On a
    failed write `StorageDialog` MUST show an error and MUST NOT proceed to
    the download.

    While in that function: its doc comment says it writes `storage_path.txt`
    (underscore), but the code writes `storage-path.txt` (hyphen,
    `internal_app_root.join("storage-path.txt")`). Correct the comment in the
    same change — §8's `adb` test recipes depend on the exact filename, and a
    tester copying it from the comment would silently test nothing.

    Note also that `is_internal` is currently **ignored** by the implementation —
    it is logged and nothing more. The recovery dialog MUST still pass a correct
    value (it is part of the existing contract and the JSON already carries it),
    but no behaviour may be made to depend on it without changing the function.

## 5. Non-Goals (Out of Scope)

- Scanning arbitrary mount points (`/storage/*`), `MediaStore`, or SAF tree URIs
  for installations. Only `getExternalFilesDirs()` candidates are scanned, plus
  the recorded path itself when it is not among them (FR-6) — which is a single
  known path, not a search.
  (`StorageManager.getStorageVolumes()` is consulted **solely** to *report*
  volumes the app cannot use, per FR-29 — it never contributes scan candidates
  or selectable locations.)
- Desktop / removable-drive equivalents of this recovery.
- Opening, version-checking, or integrity-checking the found `appdata.sqlite3`
  beyond the existence + non-zero-length test. (Database Validation already
  exists for deeper checks and remains reachable afterwards.)
- Merging two installations, or copying data between storage locations.
- Storing a stable volume UUID instead of a path, or otherwise redesigning how
  the storage choice is persisted. This PRD is a recovery mechanism for the
  path-based scheme as it exists; the UUID alternative was investigated and
  rejected (§9.6 — it needs API 30 against a `minSdk` of 27, and would not have
  helped the reported case anyway).
- Remembering declined locations / suppressing repeat prompts.
- Any change to how the storage location is chosen on first run. (The recovery
  dialog's group 2 lets the user pick a fresh-download location inline, but it
  does so by calling the same `save_storage_path()` and entering the same
  existing flow — the mechanism is untouched.)
- **Restart-free (in-process) adoption of a new storage location**, from either
  entry point. Adoption always writes `storage-path.txt` and asks for a restart;
  see §7 for the analysis behind that decision. (Group 2 needs no restart because
  it adopts nothing — see FR-14.)
- **Automatic re-adoption without asking.** The app must never switch storage
  location on its own; every switch is a conscious, confirmed user decision.

## 6. Design Considerations

- The new dialog should visually match `StorageDialog.qml` (same list-of-volumes
  presentation, label + path + figures, `font_point_size: 12`, mobile-friendly
  touch targets), since it is showing the same kind of information.
- Suggested copy:
  - Title: *"Existing App Data Found"*
  - Body: *"Simsapa could not find its database at the previously selected
    location, but found an existing installation here. This can happen if a
    memory card was moved to a different slot."*
  - Group headings, in FR-9's order: *"Existing app data found"* / *"Available
    (no app data found)"* / *"Not usable for the database"*.
  - Buttons: **Use the Selected Database** (group 1) / **Download Here**
    (group 2) / **Create New Location** (decline, FR-15).

### Where the startup dialog lives, and when it is shown

At `cpp/gui.cpp:486` there is no host window yet — the only QML that exists is
whatever `create_download_appdata_window()` loads. Two options, and the choice
has a user-visible consequence:

- **Inside `DownloadAppdataWindow`** (like `StorageDialog`, `:180`). Simple, but
  `proceed_after_releases_check()` only runs on `onReleasesCheckCompleted`, so a
  user whose card moved waits on a **network round-trip to the releases
  endpoint** before being told their data is sitting on the card. If this option
  is taken, the recovery prompt MUST be shown **before** the releases check, not
  from `proceed_after_releases_check()`.
- **Its own `ApplicationWindow`** via `WindowManager`, shown at `gui.cpp:486`
  ahead of `create_download_appdata_window()`. No network dependency, no
  interaction with the download window's state machine, and it matches how the
  Database Validation entry point already works (that file is an
  `ApplicationWindow`, not a `Dialog` — note the name).

The second is recommended. Either way the prompt must appear **before** the
download-selection screen and before `storage_dialog.open()`
(`assets/qml/DownloadAppdataWindow.qml:93-94`), so the user is not asked to pick
a storage location for a fresh download they may not need. Whichever host is
chosen, apply the `Loader` vs `Component + createObject` rule from
`docs/startup-sequence-and-caches.md` (root element type decides) and keep the
enumeration out of the engine load (FR-32).

### The `auto_start_download` marker — three traps

The marker (database-upgrade scenario) currently takes priority over the storage
dialog, and the recovery prompt should **not** interfere with an intentional
upgrade re-download: if it is set, skip the recovery prompt — **but only in the
`reachable_empty` state.** Implementing that is not as simple as calling the
existing check.

- **The skip MUST NOT apply in the `unreachable` state.** An upgrade marker plus
  an unreachable recorded path would otherwise auto-start a
  multi-hundred-megabyte download into FR-20's *internal fallback* — a location
  the user never chose — without asking, which is the silent storage switch
  Goal 2 and FR-2 exist to forbid. Worse, it writes no `storage-path.txt`, so
  the next launch is `unreachable` all over again and the freshly downloaded
  copy turns up as a group 1 hit in the recovery dialog. In `unreachable`,
  recovery wins: show the recovery flow and **leave the marker in place**
  (the peek does not consume it, §6 below), so the intended upgrade download
  still auto-starts on the launch after the user has resolved where their data
  lives — provided the marker's root matches the adopted one; see the third
  trap below.

- **`should_auto_start_download()` consumes the marker.**
  `bridges/src/asset_manager.rs:249-263` **deletes** the file as a side effect of
  reporting it. The consuming call is `DownloadAppdataWindow.qml:41`, inside
  **`Component.onCompleted`** — which stores the answer in the
  `auto_start_download` property that `:81` later branches on. Calling
  `should_auto_start_download()` early from `gui.cpp` to decide whether to skip
  recovery would consume the marker, `:41` would read `false`, and `:81` would
  then never auto-start — converting an unattended upgrade into a stalled setup
  screen. A **non-consuming peek** is required; keep the consuming call where it
  is, as the single point of deletion.

  Note where that puts the consuming call: `Component.onCompleted` runs **during
  the QML engine load, before `app.exec()`**, which is the same hazard class as
  FR-32. The peek added for the recovery decision must not make that window
  longer — it is one `try_exists()` and must stay so.
- **The marker's location question collapses once the skip is
  `reachable_empty`-only.** Its path is `app_assets_dir/auto_start_download.txt`,
  derived from `get_create_simsapa_dir()`. The peek's result is consulted only
  in the `reachable_empty` state (12.3's short-circuit), where the resolved
  path *is* the recorded path — so the peek always reads the root the user
  chose, and there is no "which root is authoritative" decision to make. Log
  the path that was consulted. One honest caveat: after recovery from
  `unreachable`, the marker auto-starts on the next launch only when its root
  matches the post-adoption resolved root — a marker stranded on the
  unreachable card (or a stale internal one after adopting a card) is simply
  never seen again. That is an acceptable leftover, not something to engineer
  around.
- While in that function: it uses `.exists()`, contrary to the project's Android
  `try_exists()` rule. Fix it in the same change.

### Shared component and remaining UI notes

- The selection dialog should be a **reusable component** used by both entry
  points (startup and Database Validation). The accept behaviour differs only in
  which groups are selectable (FR-18) and in what follows acceptance (FR-14), so
  the component can own the list, the grouping and the write, and take the
  post-accept action as a callback or signal.
- In `DatabaseValidationDialog.qml` the action is a button labelled **"Look for
  Database on Other Storage"**, gated on `is_mobile` so it never appears on
  desktop.
- Keep rows compact: label, path, one figure (database size or free space per
  FR-9), and at most one short status marker ("Partial" per FR-10, or the
  unusable reason per FR-29). These are mobile dialogs — a row that has to be
  parsed is worse than one that reads at a glance. The group heading carries the
  meaning that would otherwise have to be repeated per row.
- Unusable rows (FR-27 – FR-34) should be visually de-emphasised (greyed,
  `enabled: false`) with the reason on its own line under the label and path, in
  the same delegate used for usable rows — so the list reads as one list of "what
  the app can see", sectioned, not three separate concepts.
- New QML files must be added to the `qml_files` list in `bridges/build.rs`.
- Any new `StorageManager` bridge method needs a matching stub in
  `assets/qml/com/profoundlabs/simsapa/StorageManager.qml` for `qmllint`.
- QML logging must use the `Logger` module (single concatenated string), not
  `console`.

## 7. Technical Considerations

- **Where the enumeration lives:** `get_app_data_storage_paths()` is C++/JNI in
  `cpp/utils.cpp` and is already exposed to Rust/QML as
  `get_app_data_storage_paths_json()`. Reuse it rather than adding a second
  enumeration path. The per-candidate database probe is plain filesystem work
  and belongs in Rust (`backend/src/lib.rs` next to `get_create_simsapa_dir()` /
  `appdata_db_exists()`), exposed via `StorageManager`.
- **Load-bearing invariant: `AppGlobalPaths` is re-derived on every
  construction, and `AssetManager` deliberately uses that.** This is what makes
  FR-14's group 2 correct, and it is *not* implied by the restart analysis
  below — read both together before touching either.

  `AppGlobalPaths::new()` calls `get_create_simsapa_dir()` afresh each time
  (`backend/src/lib.rs:549-554`), and
  **`AssetManager::download_urls_and_extract()`** constructs
  `AppGlobalPaths::new()` at the moment of download
  (`bridges/src/asset_manager.rs:335`) rather than reading the frozen
  `get_app_globals()`. So a `save_storage_path()` write is picked up by the very
  next download **without a restart** — which is exactly how today's
  `StorageDialog` already works, and the sole reason group 2 needs no restart
  notice.

  (The QML side is `DownloadAppdataWindow.run_download()`, which calls into it;
  the Rust function that re-derives the paths — the one that must not be
  "cleaned up" — is `download_urls_and_extract`.)

  The two mechanisms therefore coexist on purpose: `APP_GLOBALS` is frozen (so
  adoption needs a restart, §7's analysis below), while `AssetManager` re-reads
  (so a fresh-download location takes effect immediately). **Do not "clean up"
  `AssetManager` to use `get_app_globals().paths`** — that would silently break
  both FR-14 group 2 and today's first-run flow, with no compile error and no
  obvious symptom until a download lands in the wrong directory.
- **Suggested bridge surface** (names indicative):
  - `StorageManager::find_storage_candidates_json() -> QString` — returns a JSON
    array covering **every** candidate, not only the hits, since FR-9 lists all
    three groups from one call. Per FR-31a this is the **tier 1** (cheap,
    non-probing) call, and it is the only one the FR-4 scan uses:

    ```json
    { "path": "…", "label": "…", "is_internal": false,
      "is_recorded": false,
      "group": "found" | "available" | "unusable",
      "unusable_reason": "",
      "megabytes_available": 12345, "low_space_warning": false,
      "appdata_bytes": 512000000, "modified": "…", "is_complete": true }
    ```

    `group` is derived, not free-form: `unusable` from FR-29, else `found` when
    FR-5 holds, else `available`. `appdata_bytes` / `modified` / `is_complete`
    are meaningful only for `found` rows; `megabytes_available` and
    `low_space_warning` only for `found` and `available` rows (FR-34 omits
    figures on unusable ones). Ordering is group order, internal-first within
    each group (FR-11). `is_complete` is false when any of the three databases is
    missing (existence checks only) and drives the "Partial" marker; the specific
    missing filenames go to the log rather than into the JSON (FR-10).

    `is_recorded` marks the recorded path (FR-11a) — a **field**, never a
    suffix baked into `label` (FR-12), matched with FR-6a's normalized
    comparison. `low_space_warning` carries FR-30, which `group` cannot express
    because such a row is `available` *and* warned. `modified` is required on
    `found` rows, not best-effort: FR-5's length check already calls
    `metadata()`, which carries both (FR-9).

    **`appdata_bytes` (database size) and `megabytes_available` (volume free
    space) are different quantities** and must not be collapsed into one `size`
    field — FR-9 shows one in group 1 and the other in group 2.

    `group` is **provisional** in this tier-1 result (FR-31a): only tier 2 can
    demote a row to `unusable` on probe grounds.
  - `StorageManager::probe_storage_candidate_json(path) -> QString` — the
    **tier 2** per-candidate probe (FR-31), returning at most
    `{ "path": "…", "is_usable": bool, "unusable_reason": "" }`. Called only
    from the dialogs, once per candidate, off the UI thread and outside the QML
    engine load (FR-32); results are merged into the tier-1 rows in place.
    **It MUST NOT be reachable from the FR-1 predicate or the FR-4 scan.**
  - A predicate for FR-1's condition (FR-35), returning the **four-state**
    answer (`absent` / `unreachable` / `reachable_empty` / `ok`), not a boolean —
    callable from both `gui.cpp` and QML, so the startup branch, the FR-23 vs
    FR-23a decision and the dialogs all agree on one definition. It is
    side-effect-free (no `create_dir_all()`), which is what lets FR-36 call it
    before `init_app_globals()`, and it is `is_mobile()`-gated internally so
    desktop always sees `absent` (FR-35a). This is also what FR-22 records into
    `StartupDbReport`.
  - A **non-consuming** peek at the `auto_start_download` marker (§6), distinct
    from the existing `should_auto_start_download()`, which deletes it.
  - Reuse the existing `save_storage_path(path, is_internal)` for both adoption
    (group 1) and fresh-download selection (group 2).
- **Unsuitable-location detection extends the existing enumeration — but only
  its tier-1 half.** Add the `StorageManager.getStorageVolumes()` pass and the
  mounted / read-only state check to `get_app_data_storage_paths()` in
  `cpp/utils.cpp`, so every consumer of `get_app_data_storage_paths_json()` gets
  the same picture. Extend the JSON objects with `is_usable: bool` and
  `unusable_reason: string`; existing consumers ignoring the new fields keep
  working.

  **The FR-31 write/SQLite probe MUST NOT be added to this function.** It is
  called from the FR-4 scan and from `StorageDialog`'s `Component.onCompleted`,
  i.e. from inside the QML engine load — see FR-31a for the split and FR-32 for
  why. The JNI classes/methods needed
  are all API 24 or earlier — `getStorageVolumes()`, `StorageVolume.getUuid()`,
  `getDescription(Context)`, `isPrimary()`, `Environment.getExternalStorageState(File)`
  — so no API-level guard is required at `minSdk 27`. **`StorageVolume.getDirectory()`
  is API 30 and MUST NOT be used** (see §9.6); listing a volume as unsuitable
  needs only its description, not its path.
- **The SQLite probe belongs in Rust**, next to the other database helpers — not
  a hand-rolled file write. Use a Diesel `SqliteConnection`: `backend/Cargo.toml`
  has `diesel` (sqlite/r2d2) and `libsqlite3-sys` with `bundled`. **There is no
  `rusqlite` dependency** — do not add one for this.
- **Startup ordering matters.** The check runs before `init_app_data()`, around
  the `appdata_db_exists()` branch at `cpp/gui.cpp:486` that constructs
  `DownloadAppdataWindow`. Note that FR-1 changes the *trigger* — the branch can
  no longer be keyed on `appdata_db_exists()` alone, because FR-20's fallback can
  make that true at a location the user never chose (FR-2). Keep the check cheap
  (a handful of `try_exists()` + `metadata()` calls) — see
  `docs/startup-sequence-and-caches.md` §6 on pre-`exec()` stalls; do not add
  database opens here.
- **FR-20 changes what the four existing startup sweeps operate on — resolved in
  FR-36.** `remove_download_temp_folder()`, `ensure_no_empty_db_files()`,
  `check_delete_files_for_upgrade()` and `check_remove_lang_index_dirs()` all run
  at `cpp/gui.cpp:349-364`, well before any recovery check, and all resolve their
  paths through `get_create_simsapa_dir()`. With an unreachable recorded path
  they will now operate on the **internal fallback** instead of on
  `PathBuf::from(".")` — which is an improvement for the first, and a **data-loss
  hazard** for `check_delete_files_for_upgrade()`, which deletes database files
  and would be pointed at a real internal installation that is itself a
  candidate for adoption.

  This is not left to the implementer: **FR-36 requires the three destructive
  sweeps to be skipped in the `unreachable` state**, and requires the predicate
  to be evaluated before `init_app_globals()` at `:348`. FR-35's side-effect-free
  predicate is what makes evaluating it that early safe, and **FR-20a** is what
  keeps the answer stable across launches.

  **FR-36a is the exception inside that skip:** `ensure_no_empty_db_files()` also
  writes the authoritative `record_db_presence()` entries, so the recording half
  runs unconditionally — a `sweep: bool` gates only the deletion, with the
  per-database **sweep-then-record** order kept (see FR-36a for why a
  record-first split would falsify the record). Skipping both would
  blank the Database Validation report in exactly the session that needs it.
- **The log file follows the fallback too.** `Logger::new()`
  (`backend/src/logger.rs:311-316`) derives `log.txt` from
  `get_create_simsapa_dir()`, so under FR-20 the log for exactly the session the
  user needs diagnosed is written to the **internal** root, not to the card. That
  is the right outcome (it is the reachable one), but §9.8's "their `log.txt`
  records the resolved storage path" means *that* copy — say so when asking a
  user for logs.
- **Adoption covers the databases, not the search indexes.** FR-10's "Partial"
  marker looks at the three `.sqlite3` files only. An adopted location may also
  hold `app-assets/index/{suttas,dict_words,library}` that are absent, partial or
  stale relative to the adopted `appdata.sqlite3`. Nothing breaks — the indexes
  are rebuildable — but the first launch after adoption may show empty Fulltext
  Match results until they are rebuilt. Decide whether that is left to the user
  (Database Validation already offers a rebuild) or detected; either way do not
  let it read as "adoption lost my data", since ContainsMatch and all user data
  are unaffected.
- **What a "Partial" adoption lands in, after the restart.** FR-10 lets a
  location holding only `appdata.sqlite3` be adopted. On the next launch
  `appdata_db_exists()` is then **true**, so `gui.cpp:486` takes the normal
  branch and `init_app_data()` / `DbManager::new()` opens against a missing
  `dictionaries.sqlite3` and/or `dpd.sqlite3`. That is a defined state, not a
  crash: per `docs/database-migrations.md` the missing-database path records it
  in `StartupDbReport` and Database Validation reports it honestly ("Database
  file was missing"), which is why the "Partial" marker at adoption time is
  load-bearing — it is the user's only warning *before* the restart. Do not add
  a second confirmation; do make sure the marker text and the Database
  Validation message are recognisably about the same thing.
- **Adoption is effectively irreversible.** The next launch runs
  `run_pending_migrations()` against the adopted database and stamps the ledger,
  so an installation adopted from an older release cannot afterwards be handed
  back to that older app version. This is the same one-way step any upgrade
  performs, but it is worth stating: adopting is not a preview.
- **`getExternalFilesDirs()` has a side effect** — it creates the app directories
  on the volumes it returns (FR-5). Nothing breaks, and it already happens at
  first run today, but do not treat a candidate directory's existence as evidence
  of anything.
- **Why user data makes this worth doing:** `appdata.sqlite3` is not just shipped
  content — it also holds bookmarks, gloss/prompts history, chanting recordings
  and imported books (see `docs/database-migrations.md`). Re-downloading is not
  merely slow, it silently orphans the user's own data on the card.
- **Do not conflate this with the SAF `content://` path.** `save_file` /
  `android_saf.rs` deal with user-chosen document trees; the app data storage
  location is an ordinary filesystem path from `getExternalFilesDirs()` and stays
  that way.

### Why adoption ends in a restart (FR-14) — considered and rejected

Restart-free adoption was investigated and deliberately rejected: the cost falls
on the startup sequence, which is the riskiest code in the app, in exchange for
saving one tap in a rare situation.

The runtime paths are frozen once, early, in a write-once cell.
`cpp/gui.cpp:348` calls `init_app_globals()`, which does
`APP_GLOBALS.get_or_init(AppGlobals::new)` (`backend/src/lib.rs:171-176`);
`AppGlobals::new()` derives **every** path — `simsapa_dir`, the three database
paths and URLs, the four index dirs, the marker files — from
`get_create_simsapa_dir()`, i.e. from the stale `storage-path.txt`.
`appdata_db_exists()` is only consulted much later, at `cpp/gui.cpp:486`, and
`get_app_globals()` hands out `&'static AppGlobals` to ~61 call sites across
`backend/`, `bridges/` and `cli/`.

Adopting a new path in-process would therefore mean **(a)** replacing the
`OnceLock` with a swappable holder (a leaked `Box` behind an `AtomicPtr` /
`arc_swap`) so the `&'static` signature survives, **(b)** re-running the four
marker sweeps that were already evaluated against the old paths, **(c)**
re-initializing the logger, whose file path is likewise captured at construction
(`backend/src/logger.rs:312-316`), and **(d)** restructuring the
download-window branch at `gui.cpp:486-502` so it falls through to
`init_app_data()` instead of throwing `NormalExit`. That is four changes to
process-global startup state to save a restart.

From Database Validation it is worse still: `AppData` is live with open Diesel
pools, the fulltext searchers hold Tantivy directories open, and the Rocket web
server thread is serving from the old paths, so a swap would strand open handles
at the previous location.

A restart is also **consistent with what the app already does**: the first-run
setup flow ends in `Qt.quit()` (`DownloadAppdataWindow.qml`) with `gui.cpp`
throwing `NormalExit` after the window closes, so users already restart after
configuring storage. Recovery ending the same way needs no new startup
machinery — only the message.

**Adoption must not trigger migrations or writes before the restart.** Writing
`storage-path.txt` leaves the adopted database untouched; the usual
`DbManager::new()` / `run_pending_migrations()` sequence handles it on the next
launch.

## 8. Success Metrics

**Simulating a relocation without hardware.** Most of the behaviour below can be
exercised on any device — including one with no card slot and without a card
reader — by editing the recorded path directly:

```sh
adb shell run-as io.github.simsapa.app.beta \
  sh -c "printf '%s' /storage/DEAD-BEEF/Android/data/io.github.simsapa.app.beta/files \
         > /data/user/0/io.github.simsapa.app.beta/files/storage-path.txt"
```

**Use `printf '%s'`, not `echo`.** `save_storage_path()` writes the path with no
trailing newline, and until FR-22a lands `get_create_simsapa_dir()` does
`PathBuf::from(contents)` with no trimming (`backend/src/lib.rs:784`) — so an
`echo`-written file yields a path containing a newline, which resolves to nothing
on any volume. That tests "malformed file", not "relocated card". Once FR-22a is
implemented both forms work, and a deliberately `echo`-written file becomes a
regression test for the trim itself.

That produces an existing-but-unreachable recorded path, which is exactly FR-1's
*unreachable* state, and exercises FR-2, FR-20 – FR-25 and the scan. Pointing it
instead at a *reachable* location that holds a real installation exercises the
adoption path end to end; pointing it at a reachable **empty** directory
exercises FR-23a. Use the beta package (`make android-beta-debug`), since
`run-as` requires a debuggable build.

1. Manual device test: install to an external card, move the card to a different
   socket / reader so the volume path changes, launch the app → the recovery
   dialog appears, and after adopting it and restarting, the app opens against
   the card with bookmarks intact and no download.
2. Manual device test: card removed entirely → no recovery dialog; the
   unavailable-location message (FR-23) names the recorded path and offers Try
   Again / Set Up Again.
3. Manual device test with a **card reader** — the reported scenario: whichever
   outcome §9.7 has, the user gets an actionable result. Either the card is found
   and adopted, or it is listed as *seen but not usable for app data* and the
   FR-23 message tells them to use the phone's card slot. In no case are they
   dropped into a bare download screen with the card missing from the list.
4. Manual device test: decline the dialog → the normal first-run flow proceeds.
   The recovery dialog itself MUST NOT have written `storage-path.txt`; the file
   changes only when the user then picks a location in `StorageDialog`, exactly
   as on a genuine first run (FR-15).
5. Manual device test: "Look for Database on Other Storage" in Database
   Validation finds the relocated installation, and after the prompted restart
   the app opens against it. The action is not visible on desktop builds.
6. Manual device test: the first-run `StorageDialog` shows the same
   usable/unusable classification, so an unusable location cannot be chosen for
   the initial download; a low-space-but-writable location stays selectable with
   a warning (FR-30).
7. Regression test — **the case FR-2 exists for**: with an installation present
   in internal storage *and* `storage-path.txt` pointing at an unreachable
   external path, the app MUST show the recovery/unavailable UI, **not** boot
   silently from the internal copy.
8. Regression test: a genuine first run (no `storage-path.txt`) shows today's
   setup flow with no unavailable-location message (FR-24).
8a. Regression test — **the case FR-23a exists for, and the most likely way to
    ship a bad first impression**: start a genuine first run, pick a storage
    location in `StorageDialog`, then kill the app (or let the download fail)
    before it completes. Relaunch. `storage-path.txt` now exists with no
    database. The app MUST resume today's download flow with **no**
    unavailable-storage message.
8b. Regression test: a `storage-path.txt` containing a trailing newline resolves
    correctly (FR-22a); one containing only whitespace is treated as a genuine
    first run (FR-24), not as an unreachable path.
8c. Manual device test — the three groups (FR-9): on a device with a card
    inserted and the app installed internally, the recovery dialog shows the
    internal installation under *"Existing app data found"* and the card under
    *"Available (no app data found)"*, and selecting the card starts a download
    there **without** a restart notice (FR-14, group 2).
8d. Regression test: with the `auto_start_download.txt` marker present, the
    recovery prompt is skipped **and** the upgrade download still auto-starts —
    i.e. the early peek did not consume the marker (§6).
8e. Regression test — **the data-loss case FR-36 exists for**: place a complete
    installation in internal storage, write a `delete_files_for_upgrade.txt`
    marker into its `app-assets/`, and point `storage-path.txt` at an
    unreachable external path. Launch. The internal databases MUST still exist
    afterwards, and the recovery dialog MUST offer that internal installation
    under *"Existing app data found"*. (Before FR-36 the sweep at
    `cpp/gui.cpp:358` would have deleted them under FR-20's fallback.)
8f. Regression test — **FR-35's read-only predicate**: point `storage-path.txt`
    at a non-existent path whose *parent is writable* (e.g. a fresh subdirectory
    under the internal root). Launch. The state MUST be reported as
    `unreachable` and FR-23's message MUST appear — the directory MUST NOT have
    been created by the predicate, which would have downgraded it to
    `reachable_empty` and suppressed the message.
8g. Failure test — **FR-37**: make `storage-path.txt` unwritable (or simulate a
    `save_to_file()` failure), then adopt a found installation. The dialog MUST
    show an error and stay open. It MUST NOT show the restart notice and quit,
    which would loop the user through the same dialog on every launch with no
    message. Then take Set Up Again → `StorageDialog` → Select: the storage
    dialog MUST likewise show an error and MUST NOT proceed to the download
    (FR-37's `StorageDialog` half).
8h. Test — **FR-31a tiering**: with `STARTUP-TRACE` logging, confirm the FR-4
    scan performs **no** SQLite probes, and that the dialog's probes run after
    `app.exec()` and off the UI thread. A candidate that fails the probe MUST be
    demoted to *"Not usable for the database"* after the list has already
    rendered, and any selection on that row MUST be cleared (FR-31a, FR-34).
8i. Test — **FR-15 decline end state**: in the FR-2 configuration (internal
    installation present, recorded path unreachable), decline the recovery
    dialog. The app MUST proceed to `StorageDialog` → download → quit, i.e. a
    first-time install, and MUST NOT fall through into the running app against
    the internal database.
8j. Regression test — **FR-20a, the self-erasing classification**: repeat test 8f
    and then **launch a second time without touching anything**. The state MUST
    still be `unreachable` and FR-23's message MUST appear again. Before FR-20a,
    `init_app_globals()` → `get_create_simsapa_dir()` created the directory on
    the first launch, so the second launch reported `reachable_empty` and the
    message vanished for good. Verify on disk that the recorded directory was
    **not** created.
8k. Regression test — **the marker × `unreachable` hole (§6)**: write an
    `auto_start_download.txt` marker and point `storage-path.txt` at an
    unreachable path. Launch. The recovery flow MUST run (not the silent
    upgrade download into internal storage), and the marker MUST still exist
    afterwards, so that the upgrade download auto-starts on the launch following
    adoption.
8l. Regression test — **FR-23a still opens `StorageDialog`**: reproduce test 8a's
    state (recorded path reachable, no database), relaunch, and confirm the
    storage-selection dialog appears so the user can pick a *different* location.
    Then do the FR-14 group 2 flow and confirm it does **not** re-ask.
8m. Regression test — **the Try Again re-branch (12.4)**: with an unreachable
    recorded path, reach FR-23's message, then make that path reachable but
    empty (`adb shell mkdir -p …`) and press **Try Again**. The app MUST fall
    through to the download flow with no message — it MUST NOT redisplay
    "not currently available" for a path that now exists.
8n. Regression test — **FR-36a**: in the `unreachable` state, open Database
    Validation and confirm the per-database `present_at_start` values are
    populated (not `null`) even though the zero-byte sweep was skipped. A
    zero-byte stub present at the resolved path MUST be reported as *missing*,
    not present (FR-36a's sweep-then-record order and its `sweep = false`
    semantics).
8o. Regression test — **FR-35a**: place a `storage-path.txt` pointing at a
    non-existent path in the desktop internal app root. The desktop app MUST be
    entirely unaffected: no message, no recovery UI, and the three destructive
    startup sweeps MUST still run.
9. No measurable increase in startup time on the normal (database-present) path,
   and no new work inside the QML engine load: `STARTUP-TRACE` around the
   enumeration and probes shows them running after `app.exec()` (FR-32).
10. The reported class of support issue ("app asks me to download everything again
    after I moved my SD card") stops recurring.

## 9. Open Questions

Resolved during review:

1. **Volume-label quality** — reuse the existing labelling and fall back to the
   "SD Card" guess when `displayName()` is empty; the full path is shown in every
   row as the real discriminator (FR-12).
2. **Ordering** — three fixed groups (found / available / not usable), internal
   location(s) first within each (FR-9, FR-11).
3. **Restart** — required after **adoption**, on both entry points (FR-14).
   In-process adoption was investigated and rejected as disproportionate: it
   would mean replacing the `APP_GLOBALS` `OnceLock`, re-running the marker
   sweeps, re-initializing the logger and restructuring the `gui.cpp` startup
   branch, all to save a restart in a rare situation. A restart also matches the
   existing first-run setup flow. See §7. Choosing a group 2 location is *not*
   adoption and needs no restart.
4. **Manual entry point** — yes, a "Look for Database on Other Storage" action in
   Database Validation, mobile only (FR-16 – FR-19).
5. **Automatic re-adoption** — no. A storage-location change stays a conscious
   user decision so the user always knows what happened (§5).

6. **Persisting a volume UUID instead of a path** — investigated and **not
   pursued**. The idea was to store "volume `<fsUuid>` + relative path" and
   resolve the current mount point at launch, so a relocated card would be found
   automatically. Four reasons it does not pay for itself:

   - **It needs API 30.** Resolving a volume back to its current path requires
     `StorageVolume.getDirectory()` (API 30). `getStorageVolumes()` and
     `getUuid()` exist at API 24, but `getDirectory()` does not, and the old
     `getPath()` is hidden API blocked by the greylist since API 28. The app's
     `minSdkVersion` is **27** (28 after the eventual Qt upgrade), so a call
     would fail on supported devices and would need an API-level guard plus a
     pre-30 fallback — and that fallback is correlating volumes against
     `getExternalFilesDirs()`, i.e. the scan this PRD already implements.
   - **On modern Android it is often a no-op.** A public FAT/exFAT volume mounts
     at `/storage/<fsUuid>`, derived from the filesystem serial — a property of
     the card, not the socket. Where that holds, reconstructing "UUID + relative
     path" yields the identical string, so it would not have helped the reporting
     user at all. It only wins where the mount name is *not* the UUID (OEM or
     older index-style names such as `/storage/sdcard1`).
   - **It cannot replace this recovery.** A reformatted card, data copied to a
     different card, or any existing install whose `storage-path.txt` holds a
     bare path with no recorded UUID all still need the scan.
   - **It needs a migration** of existing `storage-path.txt` files.

   Worth revisiting only if the minimum API level rises to 30 *and* relocation
   proves common on devices that do not use UUID mount names.

7. **Whether `getExternalFilesDirs()` enumerates a card in a USB card reader** —
   unknown, no reader available to test, and **deliberately not a blocker**. The
   grouped storage list (FR-9, FR-27 – FR-34) makes the app itself the
   diagnostic, so the user gets an actionable answer in every outcome: the volume
   appears under *existing app data found* or *available* and is selectable; or
   under *not usable for the database* with a reason, so they know to use the
   phone's card slot; or it is absent, in which case FR-23 names the unreachable
   recorded path. Nothing in the implementation branches on this, so it is
   answered by the first device test rather than before it.

   One constraint to keep in mind while implementing: **SAF is not an escape
   hatch.** SQLite requires a real filesystem path, so a `content://` tree URI
   cannot host `appdata.sqlite3` — unlike `save_file`, which writes plain files
   and *can* use SAF (see `docs/android-file-saving-saf.md`). A SAF-only volume
   is genuinely unusable for app data, which is exactly what FR-29's first row
   reports.

8. **The reporting user's old and new paths** — no longer needed to proceed. The
   trigger is known (card moved into a reader), and the feature handles the
   possible mechanisms identically. Their `log.txt` records the resolved storage
   path if a specific diagnosis is ever wanted — note that under FR-20 that log
   is the one in **internal** storage, not one on the card (§7).

   Worth asking for anyway if the report resurfaces: the **contents of
   `storage-path.txt`, byte for byte**. FR-22a exists because a stray trailing
   newline produces exactly the reported symptom — a recorded path that resolves
   to nothing, on a card that is physically present and fine.

Nothing above blocks implementation.

## 10. Changes from the first draft (review, 2026-08-01)

Recorded so the reasoning is not re-litigated:

- **Three recorded-path states, not two** (FR-1, FR-1c, FR-23a). The first draft
  treated "recorded path exists but has no database" as one condition. Because
  `StorageDialog` writes the path at *Select* time, before downloading, that
  condition also covers every interrupted first install — which would have shown
  a brand-new user an "app data unavailable" message on their second launch.
- **The recovery dialog lists all volumes in three groups** (FR-9), rather than
  listing hits plus unusable rows — which had left *usable volumes with no
  installation* as the only category silently missing.
- **Group 2 does not restart** (FR-14). Only adoption freezes anything.
- **`storage-path.txt` is trimmed on read** (FR-22a) — and the §8 test recipe no
  longer writes a newline that would have made it untestable.
- **FR-2 is scoped to adoption**, exempting the two pre-`QApplication` settings
  reads that already consult the resolved path (`gui.cpp:395`, `:413`).
- **The `auto_start_download` skip needs a non-consuming peek**, because the
  existing check deletes the marker (§6).
- **FR-32 now covers the QML engine load**, not just the UI thread: the
  enumeration already runs pre-`app.exec()` today, and the probes must not join
  it there.
- Corrected: `get_create_simsapa_dir()` is at `backend/src/lib.rs:670`; there is
  no `rusqlite` dependency (use Diesel's `SqliteConnection`).

## 11. Changes from the second draft (review, 2026-08-01)

Every code citation in the PRD was checked against the source; the table in §4
and the line references in §6 – §8 verified correct except where noted below.

- **The predicate is read-only** (FR-1c, FR-35). The previous definition of
  *unreachable* — "does not exist **and** `create_dir_all()` fails" — made the
  predicate mutate the filesystem, contradicted FR-1's stated cost, and on any
  path with a writable parent would have *created* the directory and thereby
  reclassified `unreachable` as `reachable_empty`, suppressing FR-23's message.
  Test 8f exists for exactly this.
- **FR-35 now exists.** FR-1 and §7 referred to "the predicate of FR-35" when
  requirements stopped at FR-34. It is now a real requirement, carrying the
  three-state contract and the side-effect-free rule.
- **The probe is split into two tiers** (FR-31a). FR-31 forbade probing on the
  scan while §7 folded classification into `get_app_data_storage_paths()` — which
  the scan calls — so the two could not both hold. Tier 1 (enumeration, labels,
  FR-5 checks, flag-based unusable rows) is safe everywhere; tier 2 (the FR-31
  write/SQLite probe) is dialog-only and can only *demote* a row.
- **Startup ordering and the destructive sweeps are now specified** (FR-36),
  instead of §7 leaving "either move these sweeps or gate them" to the
  implementer. `check_delete_files_for_upgrade()` deletes database files and
  FR-20 repoints it from `PathBuf::from(".")` at a real internal installation —
  one that may be the adoption candidate. Regression test 8e.
- **Adoption must verify the write** (FR-37). `save_storage_path()` returns `()`
  and only logs failure (`bridges/src/storage_manager.rs:42-68`), so FR-14's
  write-then-quit could loop the user forever with no message. This is a
  signature change, superseding FR-13's "same call, same arguments".
- **The decline path is specified rather than inherited** (FR-15, FR-23).
  "Exactly as today" was undefined in the FR-2 scenario, because today's
  first-run flow is only reachable via `if (!appdata_db_exists())` at
  `cpp/gui.cpp:486` and that is *true* there. Declining and **Set Up Again** both
  mean "set up a new database" and run the full first-time install, ending in
  `NormalExit`. Test 8i.
- **`AppGlobalPaths::new()`'s live re-read is documented as an invariant** (§7).
  It re-derives from `get_create_simsapa_dir()` on every construction
  (`lib.rs:549-554`) and `AssetManager` uses it rather than the frozen
  `APP_GLOBALS` (`asset_manager.rs:335`) — which is the *only* reason FR-14's
  group 2 needs no restart, and is not implied by §7's restart analysis. A
  well-meant "cleanup" to `get_app_globals().paths` would break it silently.
- **Goal 7 and FR-27 no longer overclaim.** "Every location the user can see in
  their phone" is bounded by `getStorageVolumes()` — the very unknown of §9.7 —
  so the guarantee is now "no volume the app can enumerate disappears", with
  FR-23 covering the rest, as §1 already said correctly.
- **FR-29's mounted / read-only checks are external-only.**
  `Environment.getExternalStorageState(File)` is not meaningful for the internal
  app-data directory.
- **New:** the recorded path is marked "(current selection)" in the list
  (FR-11a); adoption does not carry the Tantivy indexes and may need a rebuild
  (§7).
- Corrected line references: `PathBuf::from(contents)` is at
  `backend/src/lib.rs:784` (not 782, two places); the `auto_start_download`
  marker is **consumed at `DownloadAppdataWindow.qml:41`** inside
  `Component.onCompleted` — i.e. pre-`app.exec()` — with `:81` only branching on
  the stored property; `asset_manager.rs:249-263`. FR-22's JSON shape change
  touches three places, including the QML-visible
  `SuttaBridge.get_startup_db_report()` and its `qmllint` stub.
- Corrected: `cpp/gui.cpp:413` (`theme_link_colors_c()`) is **after**
  `QApplication`, which is constructed at `:403` — it is pre-*engine-load*, not
  pre-`QApplication`. Only `:395` (`render_loop_basic_c()`) is pre-`QApplication`.
  FR-2's exemption is unchanged; only its description was wrong.
- **Added §12, a pseudo-code flow reference** covering startup ordering, the
  recovery flow, both probe tiers, the Database Validation entry point, and an
  outcome matrix over every `(state × hits × user action)` combination.
- **Surfaced while writing §12:** two paths reach the download flow with the
  location *already* written — FR-14 group 2 and FR-23a's fall-through — and in
  both, `DownloadAppdataWindow` must **not** re-open `StorageDialog`
  (`DownloadAppdataWindow.qml:93-94`), which it currently always does on mobile.
  Recorded as the `skip_storage_dialog` note in §12.4.

## 12. Flow reference (pseudo-code)

Normative only where it restates a requirement; the FR references in comments are
the authority. Written to make the *ordering* checkable at a glance, because the
correctness of this feature is almost entirely an ordering property.

### 12.1 States

```
StorageState =                      # FR-1's table + the healthy case; FR-35
    ABSENT                          # no storage-path.txt, or empty after trim (FR-22a, FR-24)
  | UNREACHABLE                     # recorded path does not exist / metadata unreadable (FR-1c)
  | REACHABLE_EMPTY                 # recorded path exists, no usable installation (FR-5)
  | OK                              # recorded path exists and holds a usable installation

CandidateGroup =                    # FR-9
    FOUND                           # holds a usable installation      -> selectable
  | AVAILABLE                       # usable volume, no installation   -> selectable (startup only)
  | UNUSABLE                        # FR-29, carries a reason          -> never selectable
```

### 12.2 The two path notions (never conflate — §4 preamble)

```
recorded_path()  = trim(read("<internal_app_root>/storage-path.txt"))   # what the user chose
resolved_path()  = get_create_simsapa_dir()                             # what the app will use
                   # == recorded_path() when reachable
                   # == internal_app_root when not (FR-20 fallback, NOT a user choice, FR-21)
```

### 12.3 Startup — `cpp/gui.cpp::start()`

```
dotenv_c(); find_port_set_env_c()

# ── FR-36: evaluate and record BEFORE anything resolves or creates paths ──
# FR-35's predicate is side-effect-free, which is what makes this point legal.
# FR-35a: on desktop it returns ABSENT without reading the file at all, so
# nothing below can be reached by a stray desktop storage-path.txt.
state, recorded = storage_path_state()
record_startup_storage_report(state, recorded)            # FR-22 (top-level JSON field)

init_app_globals()             # freezes APP_GLOBALS from resolved_path() (FR-20 may fall back;
                               # FR-20a: a missing recorded path is NOT created)

remove_download_temp_folder()                             # non-destructive: always

# FR-36a: recording is NEVER skipped, and per database the order stays
# sweep-then-record — a zero-byte stub records as MISSING in both modes.
# record_db_presence() is first-write-wins, so a record-first split would lie.
ensure_no_empty_db_files(sweep = state != UNREACHABLE)    # sweep=false: record only

if state == UNREACHABLE:                                  # FR-36
    log("Skipping destructive startup sweeps; recorded path unreachable: " + recorded)
else:
    check_delete_files_for_upgrade()                      # DELETES DATABASE FILES
    check_remove_lang_index_dirs()

# FR-2 exemption: these adopt nothing, and may read a fallback DB.
if appdata_db_exists(): maybe qputenv(QSG_RENDER_LOOP, "basic")     # :395, pre-QApplication
QApplication app                                                     # :403
if appdata_db_exists(): apply_link_colors()                          # :413, pre-engine-load

# ── The startup branch (replaces the bare `if (!appdata_db_exists())` at :486) ──
# FR-1: keyed on `state`, NOT on appdata_db_exists(), because FR-20's fallback
# can make the latter true at a location the user never chose (FR-2).

# §6: the upgrade marker suppresses recovery ONLY in REACHABLE_EMPTY. In
# UNREACHABLE it must not, or the upgrade download silently lands in FR-20's
# internal fallback — a location the user never chose (Goal 2, FR-2).
skip_for_upgrade = (state == REACHABLE_EMPTY
                    and peek_auto_start_download_marker())    # PEEK, never consume

if is_mobile() and state in (UNREACHABLE, REACHABLE_EMPTY) and not skip_for_upgrade:
    run_recovery_flow(state, recorded)                        # 12.4 — see the note below
elif not appdata_db_exists():
    run_first_time_install()                                  # 12.6 — always exits
else:
    init_app_data()                                           # normal launch
    open_main_window()
```

**Invariant (FR-2):** in the `UNREACHABLE` state every terminating branch ends in
`NormalExit`, and the one non-terminating branch (FR-37's failed write) leaves the
recovery dialog on screen. Neither reaches `init_app_data()`, so the app can never
boot silently against the fallback database in that session.

**`state` is a pre-sweep snapshot, deliberately.** FR-36 records it before
anything can change the answer — and the sweeps *can* change the on-disk truth.
On a normal upgrade launch the predicate runs while the databases still exist,
so `state == OK`; `check_delete_files_for_upgrade()` then deletes them, and the
startup branch reaches the upgrade download through
`elif not appdata_db_exists()` — **not** through `skip_for_upgrade`, whose
`REACHABLE_EMPTY + marker` case only occurs when a previous upgrade launch was
itself interrupted before the download completed. Do not "fix" either side:
re-evaluating `state` after the sweeps would break FR-36's/FR-22's
before-anything-changes recording, and the `OK` in the report for the session
whose own sweep deleted the databases is the intended, honest record of what
the predicate saw.

**§12.4 is a continuation, not a blocking call.** It is written below as
straight-line code so the *ordering* is checkable, but every `show_…` step is a
QML dialog driven by signals, and `app.exec()` runs **once** — entered by whichever
window is created first (§6 recommends a dedicated recovery `ApplicationWindow`
at `gui.cpp:486`, ahead of `create_download_appdata_window()`). Implement it as a
state machine whose transitions are dialog signals; do **not** implement the
literal control flow, which would need a nested or re-entered `exec()`. The one
hard requirement the shape encodes is that each end state is reached exactly once
and none is left undefined (12.8).

### 12.4 Recovery flow (startup entry point)

```
run_recovery_flow(state, recorded):
    rows = scan(recorded)                             # 12.5 — tier 1 only, no probes
    hits = [r for r in rows if r.group == FOUND]

    loop:                                             # re-entered only by TRY_AGAIN
        if state == OK:
            # Try Again brought the installation back AT THE RECORDED PATH. There
            # is nothing to adopt and nothing to choose — the recorded path is
            # already correct — so a restart is the entire remedy (FR-25). Checked
            # before `hits`, because in this state the recorded path is itself a
            # FOUND row and the dialog would otherwise offer the user the option
            # of "adopting" the location they already have.
            show_message("Storage is available again. Please restart Simsapa.")
            return quit()

        if hits: break                                # -> the grouped dialog, below

        if state == REACHABLE_EMPTY:
            # FR-23a: the location IS available. No message of any kind, and the
            # storage dialog still opens — the user's earlier choice may be the
            # very reason the first download failed.
            return run_first_time_install(skip_storage_dialog = false)

        if state == ABSENT:
            # Only reachable via TRY_AGAIN: storage-path.txt was deleted or
            # emptied between presses. FR-24 — a genuine first run, no message.
            return run_first_time_install()

        # state == UNREACHABLE -> FR-23: name the path, never a bare download screen.
        # The message carries the grouped list (all rows non-selectable) so the
        # user can see every volume the app CAN see (FR-23, Goal 7).
        choice = show_unavailable_message(recorded, rows)     # Try Again / Set Up Again
        if choice == SET_UP_AGAIN:
            return run_first_time_install()                  # FR-15/FR-23: same path
        # choice == TRY_AGAIN — FR-25: re-check AND re-scan, never a cached result.
        # Re-branch on the NEW state: after re-seating the card the recorded path
        # may be reachable, and re-showing "not currently available" for a path
        # that is now available is the exact falsehood FR-23a forbids.
        state, recorded = storage_path_state()
        rows = scan(recorded)
        hits = [r for r in rows if r.group == FOUND]

    # ── FR-9: one or more hits -> the grouped dialog ──
    selection = show_recovery_dialog(rows,
                    selectable = {FOUND, AVAILABLE},          # startup: both (FR-14)
                    preselect  = hits[0] if len(hits) == 1)   # FR-11

    if selection == DECLINED:                                 # FR-15
        return run_first_time_install()

    if not save_storage_path(selection.path, selection.is_internal):   # FR-37
        show_error_and_stay_open()                            # MUST NOT quit -> would loop forever
        return                                                # dialog stays up; the only ways out
                                                              # are a retry or Set Up Again (FR-15)

    if selection.group == FOUND:                              # FR-14 group 1 — ADOPTION
        show_message("Storage location updated. Please restart Simsapa.")
        quit()                                                # Qt.quit() -> NormalExit
    else:                                                     # FR-14 group 2 — FRESH DOWNLOAD
        return run_first_time_install(skip_storage_dialog = true)   # no restart notice, no quit
```

**Note — `skip_storage_dialog` is for group 2 only.** It suppresses
`DownloadAppdataWindow`'s `storage_dialog.open()`
(`DownloadAppdataWindow.qml:93-94`, which today always fires on mobile), because
the user has *just* chosen a location in the recovery dialog and asking again is
asking twice.

It MUST NOT be set on FR-23a's fall-through, even though that path also reaches
the download flow with a location already recorded. There the choice was made in
a **previous** session, before a download that then failed — and the location
itself is the prime suspect. Suppressing the dialog would pin the user to it on
every relaunch with no way to change it (FR-23a).

**Note — the failed-write branch.** FR-37's `return` is the one path out of
`run_recovery_flow()` that does not terminate the process. It is not a
fall-through into the rest of `start()`: the recovery window stays on screen with
an error, and the user's remaining options are to retry the write or take Set Up
Again. FR-2's invariant holds because `init_app_data()` is still never reached.

### 12.5 The scan and the two probe tiers (FR-31a)

```
scan(recorded):                                    # TIER 1 ONLY — cheap, no probes, no writes
    rows = get_app_data_storage_paths()            # cpp/utils.cpp:143 (creates app dirs — FR-5 note)

    # FR-6: the recorded path is a candidate in its own right. When it is
    # unreachable it is BY DEFINITION not in the enumeration, so without this
    # step FR-6 is a no-op. FR-4's one bounded exception.
    if recorded and not any(same_path(r.path, recorded) for r in rows):           # FR-6a
        rows.append(candidate_from(recorded))

    for r in rows:
        r.label, r.megabytes_available = createStorageInfo(r)                     # FR-12
        r.low_space_warning = r.megabytes_available < LOW_SPACE_THRESHOLD_MB      # FR-30 (warns,
                                                                                  # never blocks)
        r.is_recorded = same_path(r.path, recorded)    # FR-11a — a FIELD, not a label suffix
        r.unusable_reason = flag_checks(r)         # FR-29 rows 2-3; external candidates only
        if r.unusable_reason:      r.group = UNUSABLE
        elif usable_installation(r):                                              # FR-5
            r.group = FOUND
            r.appdata_bytes, r.modified = stat(r.path/app-assets/appdata.sqlite3) # one metadata()
            r.is_complete = all three .sqlite3 present     # FR-10 "Partial"; missing names -> log
        else: r.group = AVAILABLE

    # FR-29 row 1 produces EXTRA rows, not verdicts on the rows above: these are
    # volumes getStorageVolumes() reports that match no getExternalFilesDirs()
    # entry, so the loop above can never see them.
    for v in getStorageVolumes():                                                 # FR-27, FR-33
        if matches_any(v, rows):     continue      # UUID in path; isPrimary(); getDescription()
        rows.append(unusable_row(v, "Not usable for app data "
                                    "(this device may only allow file transfers here)"))

    for r in rows where r.group == UNUSABLE:
        omit(r.megabytes_available, r.appdata_bytes, r.modified)    # FR-34: zeros read as a
                                                                   # space problem, not an
                                                                   # availability one
    return sort(rows, by = group_order, then = internal_first)                    # FR-9, FR-11

same_path(a, b):                                   # FR-6a — never a raw string compare
    return normalize(a) == normalize(b)            # trim (FR-22a), drop trailing separators,
                                                   # compare by path components.
                                                   # NOT canonicalize(): it fails on a path that
                                                   # does not exist, which is the case that matters.

usable_installation(r):                            # FR-5: existence AND non-zero length
    p = r.path / "app-assets" / "appdata.sqlite3"
    return try_exists(p) == Ok(true) and metadata(p).len > 0      # FR-7: never .exists()
    # errors are caught, logged, and treated as "no" — never abort startup (FR-8)

# TIER 2 — runs only in the dialogs, after app.exec(), off the UI thread (FR-31, FR-32)
on_dialog_opened(rows):
    render(rows)                                   # immediately, with per-row pending state
    for r in rows where r.group != UNUSABLE:
        post_to_worker:
            verdict = sqlite_probe(r.path)         # create temp DB, CREATE+DROP table, close,
                                                   # delete it AND its -wal/-shm siblings (FR-31)
            if dialog_closed: return               # FR-31: never outlive the dialog, never merge
                                                   # a late verdict into a destroyed model
            if verdict.failed:
                r.group = UNUSABLE; r.reason = verdict.reason   # demote only, never promote
                omit(r.megabytes_available, r.appdata_bytes, r.modified)          # FR-34
                if selected == r: clear_selection(); disable_confirm()   # FR-31a, FR-34
```

### 12.6 First-time install (the shared "set up a new database" path)

```
run_first_time_install(skip_storage_dialog = false):     # FR-15, FR-23 Set Up Again
    create_download_appdata_window()
    #   Component.onCompleted -> should_auto_start_download()   (:41, CONSUMES the marker)
    #   proceed_after_releases_check():
    #       if auto_start_download:      start download            (:81)
    #       elif is_mobile and not skip_storage_dialog: storage_dialog.open()   (:93-94)
    #           -> Select writes storage-path.txt BEFORE downloading (StorageDialog.qml:190)
    #              == the origin of the REACHABLE_EMPTY state
    status = app.exec()
    throw NormalExit(status)                                       # gui.cpp:501
```

The download itself picks up a freshly written `storage-path.txt` **without a
restart**, because `AssetManager::run_download()` builds `AppGlobalPaths::new()`
at download time (`asset_manager.rs:335`) and that re-reads
`get_create_simsapa_dir()` (`lib.rs:549-554`). This is the invariant in §7 — it is
why group 2 needs no restart while adoption does.

### 12.7 Database Validation entry point (mobile only)

```
on_look_for_database_on_other_storage():                 # FR-16, gated on is_mobile
    state, recorded = storage_path_state()               # FR-35 — same predicate, one definition
    rows = scan(recorded)                                # same 12.5 code path (FR-17)
    if no FOUND rows other than is_recorded:             # the recorded path's own row is not
                                                         # an adoption candidate (FR-18)
        show_message("No existing database was found on the other storage locations",
                     rows)                               # FR-19: show WHAT was examined,
        return                                           #        never close silently

    selection = show_recovery_dialog(rows,
                    selectable = {FOUND} minus is_recorded rows)
                                                         # FR-18: AVAILABLE shown greyed — no
                                                         # download flow here — and the recorded
                                                         # path's own FOUND row (state OK) shows
                                                         # "(current selection)", non-selectable:
                                                         # there is nothing to adopt (cf. 12.4's
                                                         # state == OK branch)
    if selection == DECLINED: return                     # just closes; the app keeps running

    if not save_storage_path(selection.path, selection.is_internal): # FR-37
        show_error_and_stay_open(); return

    show_message("Storage location updated. Please restart Simsapa.")
    quit_whole_application()                             # FR-14/FR-18: tears down live AppData
                                                         # pools, Rocket thread, Tantivy dirs
```

### 12.8 Outcome matrix

Every reachable end state, so no combination is left undefined:

| `state` | hits | user action | End state |
|---|---|---|---|
| `ABSENT`, db present | — | — | normal launch, `init_app_data()` (no recorded path ≠ no install) |
| `ABSENT`, no db | — | — | today's first-run flow, no new message (FR-24) |
| `OK` | — | — | normal launch, `init_app_data()` |
| `REACHABLE_EMPTY` | none | — | download flow **with `StorageDialog`**, no message (FR-23a) |
| `REACHABLE_EMPTY` | ≥1 | adopt (group 1) | write → restart notice → quit (FR-14) |
| `REACHABLE_EMPTY` | ≥1 | group 2 | write → download there, `skip_storage_dialog`, no quit notice (FR-14) |
| `REACHABLE_EMPTY` | ≥1 | decline | first-time install (FR-15) |
| `UNREACHABLE` | none | Try Again | re-check + re-scan from scratch, then **re-branch on the new state** (FR-25) |
| `UNREACHABLE` | none | Set Up Again | first-time install (FR-15, FR-23) |
| `UNREACHABLE` | ≥1 | adopt / group 2 / decline | as the three `REACHABLE_EMPTY` rows above |
| `UNREACHABLE` → `OK` | via Try Again | — | "Storage is available again. Please restart." → quit (12.4) |
| any | — | write fails | error shown, dialog stays open, **no quit**, `init_app_data()` still never reached (FR-37, FR-2) |
| `OK` + `delete_files_for_upgrade` marker | — | — | sweeps delete the DBs; branch takes `elif not appdata_db_exists()` → upgrade download auto-starts (normal upgrade launch — see the pre-sweep-snapshot note, 12.3) |
| `REACHABLE_EMPTY` + `auto_start_download` | — | — | recovery skipped, upgrade download auto-starts (§6; the interrupted-upgrade *relaunch* — a normal upgrade launch is the `OK` row above) |
| `UNREACHABLE` + `auto_start_download` | — | — | recovery **still runs**; marker left unconsumed and auto-starts on the launch after adoption when its root matches the adopted one (§6) |
| `UNREACHABLE` → `ABSENT` | via Try Again | — | first-run flow, no message (FR-24; file deleted or emptied between presses) |
| desktop | — | — | entirely unchanged; predicate returns `ABSENT` without reading the file (FR-3, FR-35a) |

## 13. Changes from the third draft (review, 2026-08-01)

This pass reviewed §1 – §12 for internal consistency, re-checked every code
citation against the source, and reconciled the pseudo-code with the
requirements. Six changes are behavioural; the rest close gaps that would have
been resolved differently by different implementers.

**Behavioural:**

- **FR-20a — `get_create_simsapa_dir()` must stop creating a missing recorded
  path** (`backend/src/lib.rs:785-787`). FR-1c kept the *predicate* read-only,
  but FR-36 orders the predicate immediately before `init_app_globals()`, which
  creates the directory anyway. The classification was therefore self-erasing:
  launch 1 `unreachable` + FR-23's message, launch 2 `reachable_empty` and
  silence, permanently. Masked in the real-card case (the create fails on an
  absent volume), so it would have bitten the internal / adopted-storage cases
  and **every §8 test recipe**. Test 8j.
- **The `auto_start_download` skip is now `reachable_empty`-only** (§6, 12.3,
  12.8). Skipping recovery in the `unreachable` state would auto-start a
  multi-hundred-megabyte download into FR-20's internal fallback — a location
  the user never chose (Goal 2, FR-2) — write no `storage-path.txt`, and leave
  the next launch `unreachable` again with the fresh download showing up as a
  group 1 hit. In `unreachable`, recovery wins and the marker is left
  unconsumed. Test 8k.
- **FR-23a's fall-through must NOT set `skip_storage_dialog`** (FR-23a, 12.4).
  "Today's download flow" includes `StorageDialog`, and the recorded choice
  there was made *before* a download that failed — the location itself being the
  prime suspect. Suppressing the dialog pinned the user to it forever.
  `skip_storage_dialog` is now FR-14 group 2 only, where the user has just
  chosen. Test 8l.
- **The Try Again loop re-branches on the new state** (12.4). It previously only
  broke on hits, so a re-seated card that made the path reachable-but-empty got
  "not currently available" re-displayed about an available path — the exact
  falsehood FR-23a forbids. A Try Again that restores the recorded path to `OK`
  now ends in "Storage is available again. Please restart." Test 8m.
- **FR-36a — the presence record must not be gated with the sweep.**
  `ensure_no_empty_db_files()` is the documented *"authoritative first writer of
  the presence record"* (`backend/src/lib.rs:864-897`), and in `unreachable` the
  app never reaches `DbManager::new()`, so FR-36's skip left Database Validation
  reporting `present_at_start: null` for all three databases in exactly the
  session needing diagnosis. Recording is now unconditional; only the deletion is
  gated. Test 8n.
- **FR-35a — the predicate is `is_mobile()`-gated internally.** §12.3 gated the
  destructive sweeps on `state` with no mobile check, while
  `get_create_simsapa_dir()` ignores `storage-path.txt` entirely on desktop
  (`lib.rs:730-733`) — so a stray desktop file could skip them. Test 8o.

**Consistency and completeness:**

- **FR-4 and FR-6 no longer contradict.** FR-4 said candidates are *exactly*
  `get_app_data_storage_paths()`; FR-6 required the recorded path to be a
  candidate — which, when unreachable, it is by definition not. The recorded
  path is now an explicit extra candidate (FR-4's one bounded exception), added
  in `scan()`.
- **FR-6a — normalized path comparison** for the FR-6 de-duplication, FR-11a's
  marking and the tier-2 merge. A raw string compare fails on a trailing slash
  with no error; `canonicalize()` is wrong here because it fails on the
  non-existent path that matters.
- **FR-11a is a field, not a label suffix.** §12.5 did
  `r.label += " (current selection)"`, contradicting FR-12's "labels MUST reuse
  `createStorageInfo()`" and corrupting a value used for comparison. Now
  `is_recorded: bool` in the JSON, rendered by the delegate.
- **FR-30 gets `low_space_warning: bool`.** It had no representation in the §7
  schema or in §12.5, and `group` cannot express it — such a row is `available`
  *and* warned.
- **§12.5 now produces FR-29 row 1 at all.** Those are *extra* rows from
  `getStorageVolumes()` that match no `getExternalFilesDirs()` entry, so the
  loop over enumerated rows could never generate them. The merge step and
  FR-33's `isPrimary()` / `getDescription()` matching are now in the flow.
- **FR-31 gains two cleanup obligations**: delete the probe's `-wal`/`-shm`
  siblings (this runs on a card the user keeps), and make the probe cancellable
  so it never outlives its dialog or merges a late verdict into a destroyed
  model.
- **FR-9's group 2 row is qualified "startup entry point only"** — FR-18 makes
  it non-selectable from Database Validation.
- **`modified` is required, not "if cheaply available"** — FR-5's length check
  already calls `metadata()`, which carries `len()` and `modified()` together.
- **FR-2's invariant is stated correctly.** §12.3 annotated the recovery flow
  "always exits" while FR-37's failed-write branch returns without exiting. The
  invariant is now "never reaches `init_app_data()`", which is what it was
  actually protecting.
- **§12.4 is labelled a continuation, not a blocking call.** Every `show_…` step
  is a signal-driven QML dialog and `app.exec()` runs once; the literal control
  flow would need a nested or re-entered `exec()`.
- **FR-23 and FR-19 now show the grouped list** alongside their messages. The
  zero-hit screens were the only ones the user sees in the reported scenario,
  and were the only ones *not* showing where their card is — so §1's and Goal
  7's "the app is its own diagnostic" held only after pressing Set Up Again.
- **FR-35 is four-state**, not "three-state" (its own heading and FR-1 said
  three while enumerating four).
- **§12.8's `ABSENT` row is split.** It claimed "today's first-run flow"
  unconditionally, but §12.3 correctly routes `ABSENT` through
  `elif not appdata_db_exists()` — an install predating `storage-path.txt`
  launches normally.
- **§7 documents where a "Partial" adoption lands** after the restart:
  `appdata_db_exists()` is true, so `DbManager::new()` opens against the missing
  `dictionaries.sqlite3` / `dpd.sqlite3` and Database Validation reports it —
  which is why FR-10's marker is the user's only warning beforehand.
- Corrected: the Rust function re-deriving `AppGlobalPaths::new()` at
  `asset_manager.rs:335` is **`download_urls_and_extract()`**, not
  `run_download()` (that is the QML function in `DownloadAppdataWindow.qml`).
  The invariant and line number were right; only the name was wrong — and §7
  explicitly warns against "cleaning up" that call site, so it must name it
  correctly.

All other citations were re-verified against the source and are correct:
`lib.rs:549-554`, `:670`, `:784`, `:785-787`, `:793`, `:864-897`;
`gui.cpp:348`, `:349-364`, `:395`, `:403`, `:413`, `:486`, `:501`;
`storage_manager.rs:42-68`; `asset_manager.rs:249-263`, `:335`;
`db/mod.rs:81-123`, `:130-153`; `utils.cpp:143`;
`StorageDialog.qml:190`; `DownloadAppdataWindow.qml:41`, `:81`, `:93-94`, `:180`.

## 14. Changes from the fourth draft (review, 2026-08-01)

This pass re-verified the code citations underpinning FR-36a, FR-37 and §6
against the source and reconciled §12 with them. One change corrects a real
contradiction with shipped semantics; the rest close gaps that different
implementers would have resolved differently.

- **FR-36a / §12.3 — the presence record keeps its sweep-then-record order.**
  The pseudo-code called `record_db_presence_at_start()` *before*
  `ensure_no_empty_db_files(sweep = true)`, and FR-36a offered "a separate
  record function called first" as an equivalent implementation. It is not:
  the function's own doc comment (`backend/src/lib.rs:864-873`) says *"a stub
  deleted here is recorded as **missing**, which is what makes the diagnosis
  honest"*, and `record_db_presence()` is first-write-wins — so record-first
  would have recorded a zero-byte stub as *present*, permanently. Now a single
  `ensure_no_empty_db_files(sweep = state != UNREACHABLE)` call, sweep-then-
  record kept per database, and `sweep = false` records a zero-byte file as
  missing without deleting it. Test 8n extended.
- **FR-37 now covers `StorageDialog` too.** The recovery dialog's failure path
  ends in Set Up Again → `StorageDialog` → Select → the *same*
  `save_storage_path()` call, so handling the failed write only in the
  recovery dialog left its own escape route open — and a first-run user whose
  write failed would have downloaded into a location they did not choose.
  `StorageDialog` must show an error and not proceed on failure. Test 8g
  extended.
- **§12.3 — `state` is documented as a pre-sweep snapshot.** On a normal
  upgrade launch the predicate reports `OK` (the databases still exist),
  `check_delete_files_for_upgrade()` then deletes them, and the auto-start is
  reached through `elif not appdata_db_exists()` — the matrix's
  `REACHABLE_EMPTY + marker` row covers only the interrupted-upgrade
  *relaunch*. Stated so nobody "fixes" the matrix by re-evaluating `state`
  after the sweeps (breaking FR-36/FR-22) or assumes `skip_for_upgrade`
  carries the common upgrade case. §12.8 gained the
  `OK + delete_files_for_upgrade` row.
- **FR-18 / §12.7 — no self-adoption from Database Validation.** With healthy
  storage (state `ok`) the recorded path's own FOUND row was selectable, and
  "adopting" it would rewrite the identical path and quit the app for nothing.
  It now shows "(current selection)" (FR-11a) and is non-selectable — the
  Database Validation expression of 12.4's `state == OK` rule. The FR-19
  nothing-found condition is correspondingly "no FOUND rows other than
  `is_recorded`".
- **§12.4 / FR-25 — Try Again can land in `ABSENT`.** If `storage-path.txt` is
  deleted or emptied between presses, the loop previously fell into the
  `unreachable` arm and showed FR-23's message naming an empty path. `ABSENT`
  after Try Again is now FR-24's genuine first run: fall through to the setup
  flow with no message. §12.8 gained the row.
- **§6 — the marker-root "decision" is gone.** With the skip gated to
  `reachable_empty` only, the peek is consulted solely where the resolved path
  *is* the recorded path, so the "which root is authoritative" paragraph asked
  the implementer to decide something that no longer exists. Also qualified
  the `unreachable` promise: the marker auto-starts after adoption only when
  its root matches the adopted one; a stranded marker is an acceptable
  leftover.
- **FR-31 — tier-2 probes run only where a selection is possible.** The
  non-selectable lists under FR-23's and FR-19's messages render tier-1
  verdicts only; probing them gains nothing and touches volumes needlessly.
- **FR-20 — the pre-existing `"."` last resort is scoped out explicitly.**
  `get_create_simsapa_internal_app_root()` can itself fail; only the
  recorded-path failure mode is redirected to the internal root.
- Corrected a stray cross-reference: 12.4's `state == OK` comment cited FR-6
  for "a restart is the entire remedy"; the authority is FR-25.
- **FR-37 — fix the stale filename in `save_storage_path()`'s comment** while
  changing its signature: the comment says `storage_path.txt` (underscore),
  the code writes `storage-path.txt` (hyphen), and §8's `adb` recipes depend
  on the exact name.
