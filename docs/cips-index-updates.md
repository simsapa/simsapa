# CIPS index updates

How the Topic Index gets its data, and how a user can replace it from inside the
app without waiting for a Simsapa release.

The **CIPS** (Comprehensive Index of Pāli Suttas) general index is maintained as
a tab-separated `general-index.csv` in a GitHub repository. Until this feature
the app only ever saw that file through the **bootstrap**: a CLI command parsed
it into `assets/general-index.json`, which was `include_str!`'d into the binary.
Every correction the index author made therefore needed a new build and a new
release before any reader could see it.

Now the parser lives in the backend, the parsed index is resolved from **two**
sources at runtime, and the Topic Index window has **Update** and **Reset**
buttons.

Related docs:
[window-lifecycle-and-reuse.md](./window-lifecycle-and-reuse.md) (the Topic Index
window is single-instance and destroyed on close — §1.1 below explains why that
is a *prerequisite* for this feature, not a coincidence),
[database-migrations.md](./database-migrations.md),
[sutta-display-settings-and-multi-column-view.md](./sutta-display-settings-and-multi-column-view.md)
§8 (the paragraph jump a Topic Index reference performs).

---

## 1. The pieces

| Where | What |
|---|---|
| `backend/src/cips_parse.rs` | The parser, **moved** from `cli/src/bootstrap/parse_cips_index.rs`. Pure: no file access, no printing |
| `backend/src/topic_index.rs` | The four data structs (declared **once**), the swappable cache, source resolution, store and reset |
| `backend/src/cips_update.rs` | Fetch → gate → parse → validate → store, the two process-globals, the log block |
| `backend/migrations/appdata/2026-08-13-000000_topic_index_data/` | The single-row `topic_index_data` table |
| `assets/general-index.json` | The index shipped with the build (`CIPS_GENERAL_INDEX_JSON`) |
| `assets/general-index-date.txt` | The **source CSV's** date for that shipped index (`app_settings::cips_general_index_date()`) |
| `bridges/src/sutta_bridge.rs` | `update_topic_index()`, `reset_topic_index()`, `topic_index_source_info()`, `is_topic_index_update_running()`, `cancel_topic_index_update()`, and the three signals |
| `assets/qml/TopicIndexWindow.qml` | Header buttons, the two confirm dialogs, the refresh handler |
| `assets/qml/TopicIndexUpdateWindow.qml` | The progress / results window |
| `cli/src/bootstrap/parse_cips_index.rs` | Reduced to `parse_cips_to_json()` — the only part that writes files and prints |

### 1.1 Why the window lifecycle work came first

`SuttaBridge` is a **per-engine** QML singleton: every `*Window` C++ class
constructs its own `QQmlApplicationEngine`, so each window owns a *distinct*
`SuttaBridge` instance with its own `qt_thread()` and its own properties. A
signal emitted for a run started in window A is **never delivered to window B**.

Before phase 1, `create_topic_index_window()` appended a *new* window on every
open and never destroyed one, so two Topic Index windows could coexist — and a
`topicIndexDataChanged` from the one that ran the update could not reach the
other, which would sit on pre-update data with no in-QML mechanism able to fix
it. The only correct alternative would have been a C++ `WindowManager` broadcast
that does not exist. Making the window **single-instance and destroyed on close**
removed the need for it. See
[window-lifecycle-and-reuse.md](./window-lifecycle-and-reuse.md).

The same fact has two further consequences that show up throughout this feature:

- the "an update is already running" guard is a Rust `static AtomicBool`, never a
  field or `#[qproperty]` on the bridge, which would guard only its own window;
- the update run easily outlives the window that started it, so **every**
  `qt_thread.queue()` in the new code goes through `crate::queue_or_log()` —
  `ObjectDestroyed` is a live path here, and `.unwrap()` would panic the worker.

---

## 2. Storage

```sql
CREATE TABLE topic_index_data (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    index_json TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_etag TEXT,
    csv_line_count INTEGER,
    headword_count INTEGER,
    ref_count INTEGER,
    updated_at TEXT NOT NULL
);
```

**The table starts empty, and an empty table means "use the index embedded in
this build".** Nothing is written at bootstrap; the shipped data is not
duplicated into the shipped database.

**A new, empty table — deliberately not a rewrite.** `appdata.sqlite3` is kept
across app updates (it also holds bookmarks, gloss/prompts history, chanting
recordings and imported books) and carries a populated
`__diesel_schema_migrations` ledger, so on the first launch after the app update
`run_appdata_migrations()` applies exactly this one migration, once, in a
transaction, and stamps it. No `DB_VERSION` bump, no forced re-download, no
re-bootstrap. That was verified against a real pre-existing `appdata.sqlite3`:
exactly **1** migration applied, a `sqlite_master` diff showing **only**
`topic_index_data` added, and a second run applying **0**.

**A migration failure is non-fatal by design** (see
[database-migrations.md](./database-migrations.md)), so "the table does not
exist" is a *reachable* runtime state, not a broken invariant. Every read
tolerates it: `read_stored_row()` logs the error and returns `None`, and the app
falls through to the embedded index. The Update action then fails with a plain
message rather than crashing.

### 2.1 The JSON is minified — this is not a free choice

`store_topic_index()` uses `serde_json::to_string`, **never**
`to_string_pretty`. The embedded `assets/general-index.json` is minified at
2.3 MB (the CLI target passes `--minify`); pretty-printing the runtime copy
would put two to three times that into `appdata.sqlite3` — the database the user
keeps across every future app update.

### 2.2 The write is transactional, and the cache swaps only after it commits

The write goes through `AppData::dbm.appdata.do_write(…)` (which serialises
writers on its `write_lock`) with the `INSERT … ON CONFLICT(id) DO UPDATE`
wrapped in `conn.transaction(…)`. A failed update leaves the previous state —
stored row, or no row — exactly as it was, and **does not** swap the in-memory
cache. `set_cached_index()` is called only after the write returns `Ok`.

---

## 3. Source resolution — the newer of the two wins

`build_current_index()` resolves the index like this:

1. read row `id = 1` (via `try_get_app_data()` → `dbm.appdata.do_read`);
2. use it **only if `stored_row_wins()`** — its `updated_at` is *strictly*
   greater than `cips_general_index_date()` — **and** its `index_json` parses;
3. otherwise use `CIPS_GENERAL_INDEX_JSON`, the copy embedded in this build.

Every fallback reason is **logged**: no `AppData` yet, the table missing, the
query failing, the row not being newer, the JSON not parsing.

**Why a date comparison rather than plain row-first ordering.** Row-first is the
obvious reading of "the database is the source of truth", and it is wrong in one
direction that matters: a user who presses Update once keeps that row forever, so
a later release shipping a *newer* `assets/general-index.json` would be silently
shadowed by the older downloaded index, with no message and no automatic
recovery. Comparing dates is sound in both directions — a shipped index generated
after the user's download wins, and a download made after installing that release
wins again — and it needs no user action and no notification. The Info dialog
states the situation rather than telling the reader to press Reset, because there
is nothing for them to do.

**Both dates are fixed-width UTC ISO 8601 to the second, and the comparison is a
plain string compare.** A bare date, a local time or a non-`Z` offset breaks it
silently. `updated_at` is written by `cips_update.rs` as
`Utc::now().format("%Y-%m-%dT%H:%M:%SZ")`.

### 3.1 `assets/general-index-date.txt`

A separate file, not a field in the JSON, because `assets/general-index.json` is
a **bare array** that both the embedded path and the stored `index_json`
deserialize as `Vec<TopicIndexLetter>` — a metadata slot would be a format change
on both.

**It stamps the source CSV's date, not the generator run's.**
`parse_cips_to_json()` derives it from the last commit touching the CSV in the
CIPS checkout (`git -C <dir> log -1 --format=%ct`), falling back to the file's
mtime and only then to now, printing which source it used. The question the
comparison has to answer is *"whose CIPS data is newer"*, so re-running
`make parse-cips` over an unchanged CSV must not make the shipped index look
newer than a download carrying the same content.

The CLI derives the path from the `--json-path` it was given (`<stem>-date.txt`),
so a comparison run writing the JSON to a scratchpad stamps that copy and leaves
the shipped one alone.

### 3.2 The cache

```rust
static TOPIC_INDEX_CACHE: RwLock<Option<Arc<TopicIndex>>> = RwLock::new(None);
```

A plain const-initialized static, the same shape as `FULLTEXT_SEARCHER` — not the
`OnceLock<RwLock<Option<…>>>` shape used by `RELEASES_INFO`, which needs an extra
init function for no benefit here. The old `OnceLock<TopicIndex>` could not
survive: the value has to be **replaceable in a running process**.

Three rules come with it, and each one is load-bearing:

- **Every accessor clones the `Arc` out and drops the guard before doing any
  work.** No guard may be held across a search, a `latinize()` call, or anything
  that might re-enter the cache — this is the guard-scoping rule already
  documented for `AppData::app_settings_cache`; std `RwLock` read-read re-entry
  on one thread can deadlock against a queued writer.
- **Load-on-miss builds outside the write lock** (read → miss → **drop guard** →
  build → write), because the build performs database queries.
- **The write path double-checks under the write guard** and keeps whatever is
  already there. Dropping the read guard opens a race `OnceLock::get_or_init`
  closed for free, and it is reachable in practice: the window's warm-up runs on a
  spawned thread while the GUI thread also calls the accessors. Two concurrent
  builds are wasteful, not incorrect.

A second consequence of resolving from the database: if a GUI-thread accessor
call beats the warm-up thread, the GUI thread pays for those queries. That is
accepted deliberately — the queries are small and the warm-up normally wins.

**`load_topic_index() -> &'static TopicIndex` could not survive either** (a
swappable value cannot hand out `&'static`); it became
`ensure_topic_index_loaded()`. The **bridge** method keeps its name —
`SuttaBridge::load_topic_index()` is what QML calls.

`is_topic_index_loaded()` keeps its meaning: *"the in-memory cache is
populated"*, never *"a downloaded index exists"*. Which is why **reset replaces,
never clears**: `reset_topic_index()` builds the embedded index **first** and
stores it in a single write, so the property never flickers back to `false`.
`None` is only ever the pre-first-load state.

---

## 4. Fetching

```
https://raw.githubusercontent.com/thesunshade/CIPS/main/src/data/general-index.csv
```

Only the `raw.githubusercontent.com` form may be requested — the browsable
`blob/` URL returns an HTML page.

**Retry policy** (`fetch_csv_with_retry`): up to 5 attempts with 2/4/8/16/32 s
of backoff, a 60 s request timeout, and each attempt's status reported to the UI
as "attempt N of 5".

**The status policy is the part the model loop got wrong.** The asset-download
loop in `asset_manager.rs` retries only when `send()` returns `Err`; a 500, 502
or 429 comes back as `Ok(r)` and breaks out of the loop *as if it had
succeeded*, to be diagnosed as a hard failure later. So `status_is_retryable()`
is stated explicitly and unit-tested: **retry 429 and 5xx; do not retry 4xx** (a
404 on the raw URL means the file moved, and five more attempts will not find
it). Retried statuses count as attempts in the "attempt N of 5" text. That loop
was **adapted, not called** — it is welded to the asset download's temp folders.

**Cancellation lands within ~0.2 s.** The backoff sleeps in 200 ms steps checking
`UPDATE_CANCELLED`, rather than sleeping up to 32 s at a stretch.

**The `ETag` is recorded only.** `raw.githubusercontent.com` returns a *strong*
ETag that is the SHA-256 of the file content and sends **no** `Last-Modified`, so
the stored value is a content hash: two fetches of unchanged content compare
equal. This feature makes no conditional request and no automatic check; the
column exists for diagnostics and for a possible future "check for updates".

### 4.1 The plausibility floor

`check_plausible()` rejects a body that is empty, has fewer than **1,000**
non-blank lines, or is not tab-separated with ≥ 3 columns on the majority of
lines. This guards against a GitHub error page, an HTML redirect, or the `blob/`
URL being stored as index data. The floor sits well below the measured 21,792
lines so a genuine but heavily edited CSV is never rejected. Each failed check
has its own user-facing reason naming the URL.

### 4.2 The size ceiling, and why the floor was not enough

The floor guards against *implausible content*. It says nothing about
*unbounded size*, and every one of its checks runs on a body that has **already
been read into memory** — on a phone. So `read_body_capped()` enforces a 50 MB
ceiling (≈45× the measured 1,120,102 bytes) **twice**:

- against the declared `Content-Length`, *before* buffering;
- against the bytes actually read, via `Read::take(MAX_CSV_BYTES + 1)` — the
  `+ 1` is what makes "exactly at the cap" distinguishable from "over it", so a
  missing or lying header cannot defeat the guard.

This is the only guard against an OOM on Android.

---

## 5. The run

Five stages, reported as `(stage_index, total_stages, message)`:

1. Downloading `general-index.csv`
2. Parsing the index data
3. Looking up sutta titles
4. Validating references and paragraph locations
5. Saving to the database

**Stage 3 is truthful, via a lazily-loaded title map.** The natural coding order
is fetch → load titles → parse, because `parse_cips_index_str()` takes
`title_lookup` by value — which would make stage 3 report *after* the parse
finished. Instead the closure holds a `RefCell<Option<SuttaLookups>>` and loads
on the builder's **first** title lookup, announcing stage 3 at that moment.
`IndexBuilder::build()` is the only caller, so stage 2 really is the CSV scan,
and stage 4's anchor validation reuses the already-loaded `known_uids`.

**One query, both lookups.** `load_sutta_lookups()` returns the `title_map`
(`language = 'pli'`, `source_uid = 'ms'`, uid truncated at the first `/`,
lowercased) *and* the `known_uids` set in one pass. `known_uids` includes rows
with a **NULL title** — building it from the title map instead would misreport
those suttas as unresolved uids. Per-uid `content_json` fetches are lazy and
cached for the run.

**Both closures are built inside the worker thread.** The CLI's versions capture
`RefCell`s and are therefore **not `Send`**; building them on the GUI thread and
moving them into `thread::spawn` will not compile — and "fixing" that with a
`Mutex` held across the parse is worse. They go through `dbm.appdata.do_read(…)`
per uid rather than holding a long-lived `SqliteConnection`.

**Abort conditions.** The update aborts, leaving the previous index in place,
only when: the fetch finally fails; the plausibility gate rejects the response;
the parse returns `Err`; the parse yields **zero headwords**; or serialization or
the database write fails. Each has its own plainly-worded message naming the URL.

**Validation is advisory.** Both `validate_index()` (xref targets, sutta
reference format) and `validate_anchors()` (paragraph locations) run on every
update and their results are shown, but a validation warning **never** aborts the
update or discards the parsed data — which matches the CLI's long-standing
behaviour, where anchor validation contributes nothing to
`ValidationResult::errors`.

**`UpdateError { cancelled, message }`, not `anyhow`.** A cancellation and a
failure both leave the database untouched but must be *reported* differently, and
the caller should not have to string-match to tell them apart.

**`UPDATE_RUNNING` is cleared by a `Drop` guard**, so every exit path is covered
including a panic; entry is a `compare_exchange`, so a second concurrent call
fails fast with a plain message rather than racing.

### 5.1 The summary and its deltas

The summary reports headword, sub-entry and reference counts with **signed
deltas** against the index that was in use before the run, formatted in Rust
(`format_summary_text`) so every caller shows the same wording. `format_delta()`
guarantees `±0` rather than a blank for an unchanged total. The raw counts and
deltas are in the payload too, so the UI can lay them out differently without
re-deriving them.

**The "before" counts are taken at the very top of the run**, before the fetch.
`store_topic_index()` swaps the cache as soon as the write commits, so reading
them afterwards would report the new counts as the old ones. They come from
`topic_index_counts()`, which exists for this: no other accessor exposes
sub-entry or reference totals.

### 5.2 The log block

Every line of the run's log carries the `CIPS-UPDATE:` prefix, so a user's
`log.txt` can be grepped for the whole run: the start and URL, the HTTP status
per attempt, the downloaded size and ETag, the accepted line count, the parsed
letter/headword/sub-entry/ref counts, the validation summary **and every
individual warning and anchor line**, and the closing timing plus deltas. The
detail is in the log even when the user dismissed the window without reading it.

Nothing writes an FR-45 block until Update is confirmed — merely opening the
Topic Index window must never fetch anything.

---

## 6. Reporting source-data defects, never repairing them

The parser **reports** what is wrong with the CSV and never fixes it: duplicated
xrefs are preserved, misspellings are quoted verbatim. No behaviour added by this
feature may normalize, deduplicate or "fix" CSV content — the index author is the
one who has to see the defect, and an app that silently corrects it hides the
signal. (This is the general rule for external source data in this project.)

Three invariants inside the parser must survive any future edit, and are
commented in place:

- `IndexBuilder`'s nested **`BTreeMap`** — never substitute a `HashMap`. Key
  order *is* the output order;
- `sorted_xref_targets()` returns a **`Vec`**, never a `BTreeSet` — a set would
  silently drop the duplicates the previous paragraph says must be preserved;
- `display_label()` and the QML `format_sutta_ref()` must keep agreeing.

---

## 7. The move, and what "unchanged output" meant

The parser was **`git mv`'d**, not retyped, so the diff shows exactly what
changed and the untouched logic is provably untouched.

The regression check was **byte-identical CLI output**: the same pinned CSV run
through the CLI before and after the move produced a byte-identical
`general-index.json` (2,328,764 bytes, matching md5), with stderr identical line
for line including the anchor-validation summary. The only stdout difference was
the echoed output *path*, because the two runs were told to write different
filenames.

**All diagnostics are returned, not printed.** `parse_csv_str()` returns the
malformed-line warnings, and `parse_custom_locator()`'s two warnings are threaded
back through `parse_sutta_ref()` → `build()` into `CipsParseOutcome::warnings`.
The PRD allowed an escape hatch here — land the CSV-scan warnings only and leave
the locator ones printing — and **it was not needed**: `add_row()` never calls the
locator parser, so the sink touches one call chain rather than the whole builder.
The CLI is now the only thing in the CIPS path that prints.

Warning strings carry **no `Warning: ` prefix**; the CLI adds it when printing,
matching how `validate_index` warnings have always been handled, so the printed
lines are unchanged character-for-character while the app's report gets clean
message text with no console-shaped prefix baked in.

Note what the CLI diff could *not* prove: this CSV produces **zero**
malformed-line and zero locator warnings, so nothing moved position and the
warnings sink was never exercised by it. Three unit tests cover that instead
(`test_parse_csv_str_returns_malformed_line_warnings`,
`test_parse_custom_locator_returns_its_warnings`, and
`test_parse_cips_index_str_merges_both_warning_sources`, which pins the
CSV-scan-then-locator ordering).

---

## 8. The UI

**Header** — "Update" and "Reset" sit immediately after "Info" in
`TopicIndexWindow.qml`. "Reset" is enabled from `has_stored_row` (a row that is
*not in use* because it is older still exists and is still resettable);
"Update" is disabled while a run is in flight. Both confirm dialogs have a
`title` **and** wrapping text, so both use
`header: DialogHeader { text: <id>.title }`.

**`TopicIndexUpdateWindow.qml` is a split model**, and each half comes from a
different place for a stated reason:

| Aspect | Modelled on | Why |
|---|---|---|
| Layout | `DictionaryIndexProgressWindow.qml` | `ApplicationWindow`, `flags: Qt.Dialog`, `modality: Qt.ApplicationModal`, full-screen on mobile / fixed on desktop, `extra_top_margin` |
| Lifecycle | `TopicIndexInfoDialog.qml` | an inline `visible: false` sibling — keeps it in-tree for `MobileOverlayTracker` and inside the same engine, so it shares the window's `SuttaBridge` instance |
| Start | `StorageDiagnosticsDialog.qml` | `open_and_run()`: `show(); raise(); requestActivate(); start_run();` |

`DictionaryIndexProgressWindow` is emphatically **not** a lifecycle model: C++
loads it into a *stack-local* `QQmlApplicationEngine` pumped by a nested
`QEventLoop` with `visible: true`, which is why it can start its work from
`Component.onCompleted`.

**It must not start the run in `Component.onCompleted`.** An inline
`visible: false` child's `onCompleted` runs during the **engine load**, so that
would fire a network fetch every time the Topic Index window is opened, before
the user has confirmed anything.

**`extra_top_margin` is a `required property int`**, copied literally from the
sibling's declaration form. A *required* property the parent forgets to set is a
**load** error (surfacing as `Type … unavailable`), not a silently unmargined
window on Android — which is the failure you want.

Other things the window owns:

- **keep-screen-on** is acquired in `start_run()` and released in the
  **completion handler** on both success and failure — never in `onClosed`, since
  the backend thread continues after the window closes;
- it **refuses a mid-run close** (`close.accepted = false`), so Cancel is the only
  way out during a run. Cancel does **not** close the window: it asks the backend
  to stop and lets the completion handler report the cancellation, which is what
  releases the keep-screen-on lock exactly once;
- `payload.summary_text` is rendered verbatim, and the full validation warning
  list is shown in a scrollable **selectable** area.

### 8.1 Three flows share one completion signal

`topicIndexUpdateCompleted` carries the update's summary, the update's
failure/cancellation message **and** the reset's confirmation. So every listener
needs its own guard: the update window filters on `run_initiated_here`,
`TopicIndexWindow` filters on a separate `reset_initiated_here`. Without the
second flag the reset's confirmation would pop for an update, and vice versa.

On success `summary_json` is `UpdateSummary::to_json()`; otherwise it is
`{"cancelled": bool, "message": str}`. The same helper builds the reset's
payload, so QML has one parse path for every terminal message and reads `success`
to branch.

`update_is_running` is cleared on *any* completion, outside both guards: it drives
the buttons' `enabled`, and a run this window did not start (discovered by the
`is_topic_index_update_running()` poll) still has to re-enable them when it ends.

`topicIndexDataChanged` is emitted **before** `topicIndexUpdateCompleted`, inside
the same queued closure, so a completion handler that re-reads the index already
sees the new one whichever order QML connects them in. It is a **new** signal,
never a reuse of `topicIndexLoaded` — that one drives the window's first-load
state machine, and conflating the two makes an update indistinguishable from a
first load. Neither an update nor a reset resets `topic_index_loaded` to `false`.

### 8.2 State is re-read on show, not only on completion

After the lifecycle change the window is **reused across opens**, so
`Component.onCompleted` runs once per *instance*. The source info, the Reset
button's state and the in-flight poll therefore live in `onVisibleChanged`. The
Info dialog's "which index is in use" line is refreshed the same way **and**
bound to `topicIndexDataChanged`, so it is correct whether the dialog is opened
after an update or is already open when one lands.

---

## 9. Regenerating the shipped index

```sh
make parse-cips
```

runs `cli … parse-cips-index --csv-path <CIPS checkout>/src/data/general-index.csv
--json-path ../assets/general-index.json --db-path <appdata.sqlite3> --minify`,
writing both `assets/general-index.json` and `assets/general-index-date.txt`. A
rebuild re-embeds them.

Keep `--minify` (§2.1), and remember that the date stamp comes from the **CSV's**
git history, so regenerating from an unchanged CSV correctly leaves the stamp
where it was.
