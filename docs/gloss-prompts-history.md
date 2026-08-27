# Gloss / Prompts session history

The **Gloss** tab (`bridges/assets/qml/GlossTab.qml`) and the **Prompts** tab
(`bridges/assets/qml/PromptsTab.qml`) each persist the user's recent sessions so they can
re-open earlier work — a previous gloss session (paragraphs + resolved words + AI
translations) or a previous AI chat conversation. The two histories are almost
identical and share the DB table, the Rust bridge logic, and the QML list/item
UI, distinguished only by an `item_type` field (`"gloss"` | `"prompts"`).

This doc captures the **shared session-lifecycle state machine** and the
load-bearing **correctness gotchas**. The PRD/tasks files
(`tasks/2026-06-27-131935-*`) are transient; the gotchas below are not, because
both tabs must keep them in sync and future edits to either tab will re-hit them.

## Components

| Layer | Location |
|-------|----------|
| Table `gloss_prompts_history` | migration `backend/migrations/appdata/2026-07-23-000000_initial_schema` (originally `2026-06-27-131935_create_gloss_prompts_history`, since squashed), schema `backend/src/db/appdata_schema.rs` |
| Model + `HistoryItemType` | `backend/src/db/appdata_models.rs` (`GlossPromptsHistory`, `NewGlossPromptsHistory`) |
| CRUD helpers + tests | `backend/src/db/appdata.rs` (`get_history_for_type` / `save_new_history` / `update_history` / `delete_history_item` / `clear_history`; `history_tests`) |
| Bridge (shared, `item_type`-parameterised) | `bridges/src/sutta_bridge.rs` (`save_history_session_impl` + the `*_background` / `*_blocking` fns + `historyListReady`/`historySaved`/`historyChanged` signals) |
| Shared list item + helper | `bridges/assets/qml/HistoryListItem.qml`, `bridges/assets/qml/HistoryUtils.qml` |
| Tab lifecycle + history UI | `bridges/assets/qml/GlossTab.qml`, `bridges/assets/qml/PromptsTab.qml` |
| App-close flush | `bridges/assets/qml/SuttaSearchWindow.qml` (`onClosing` + tab `Component.onDestruction`) |

`data_json` is **opaque to the backend** (stored/returned as text); each tab owns
its own serialization shape:

- **Gloss:** `{ text, paragraphs:[{ text, words, translations, selected_ai_tab }],
  no_duplicates_globally, skip_common, global_shown_stems,
  global_unrecognized_words, paragraph_unrecognized_words }`.
- **Prompts:** `{ messages:[{ role, content, content_html, responses_json,
  selected_ai_tab }] }`.

## Persistence model

- Writes are **debounced**, never on the typing-latency path. Change events only
  set `session_needs_saving = true`; a repeating 60 s `Timer` performs the write
  when dirty (and no write is in flight).
- All normal saves/list/delete/clear are **background** (off the UI thread),
  reporting back via signals. The **one** synchronous exception is the app-close
  flush, which must complete before the process exits.
- The table is indexed on `(item_type, updated_at)` for `WHERE item_type = ?
  ORDER BY updated_at DESC`. There is **no per-save `ANALYZE`** — see
  [user-data-and-sqlite-analyze.md](./user-data-and-sqlite-analyze.md).
- **No retention cap.** Sessions persist until Delete / Clear.
- Sessions **survive an appdata re-download**: the upgrade export/import cycle in
  `backend/src/app_data.rs` writes the whole table to
  `import-me/gloss_prompts_history.json` (`export_gloss_prompts_history`) and
  restores it with the original timestamps afterwards (`import_history_row`,
  deduplicated on `(item_type, created_at, data_json)`; source ids are not
  carried over). Both tabs' sessions ride in that one file — it keys on nothing
  but `item_type`. See
  [gloss-ai-word-selection.md](./gloss-ai-word-selection.md) §6.

## The shared state machine

Both tabs share these root properties:

- `current_session_id` — **string**, `""` = no DB row yet (INSERT on next save).
- `session_needs_saving` — dirty flag; the single source of truth for autosave +
  the save-state indicator.
- `save_in_flight` — an async write is outstanding; gates the autosave tick.
- `save_again_pending` — a save was requested while one was in flight; coalesced
  into exactly one follow-up after the current write resolves.
- `refresh_list_on_save` — set by explicit Save / New Session so the History list
  refreshes when the write completes; the autosave path leaves it false.
- `selected_history_id` — highlighted history row (selection only; Open loads).

Key functions (same shape in both tabs): `save_session(blocking)`,
`save_session_now()`, `flush_if_needed()`, `new_session()`, `open_history_item()`,
`load_session()`, `load_history()`, `session_data_json()`, `is_session_empty()`.

## Gotchas / fixes (must hold in BOTH tabs)

1. **Stale `current_session_id` after its row is removed → silent data loss.**
   Deleting the active session (Delete) or clearing all (Clear) leaves
   `current_session_id` pointing at a gone row; a later UPDATE matches 0 rows and
   the work vanishes. **Two-layer fix:**
   - *Backend:* `update_history` returns the affected row count;
     `save_history_session_impl` **falls back to INSERT when UPDATE affects 0
     rows** (shared bridge code, so both tabs inherit it).
   - *QML:* reset `current_session_id = ""` on **Clear**, and on **Delete** only
     when the deleted id is the active session.
2. **Stuck `save_in_flight` on failure → autosave dies.** The async save bridge
   **always** emits `historySaved`; an **empty resolved id = failure**. The QML
   handler then resets `save_in_flight`, **keeps the session dirty** (next tick
   retries), and does **not** clobber `current_session_id`.
3. **Duplicate INSERT from concurrent writes.** Before the first save resolves an
   id, two overlapping writes would both INSERT. **Single-writer + coalesce:** the
   async `save_session` returns early setting `save_again_pending = true` when
   `save_in_flight`; `onHistorySaved` runs the one coalesced follow-up **after**
   the id is known (so it UPDATEs). The autosave `Timer` also gates on
   `!save_in_flight`.
4. **Blocking flush for Open / New Session is deliberate** (not a background-save
   violation): an async flush's `historySaved` would arrive **after** the
   subsequent `load_session`/reset and clobber `current_session_id` with the
   flushed session's new id. `data_json` is serialized synchronously *before* the
   load, so the blocking write is safe and the pause is brief. The app-close flush
   is blocking for a different reason — guaranteed completion before exit.
5. **Spurious "unsaved" right after a load.** Setting input fields
   programmatically fires their change handlers, so `load_session`/`new_session`
   set `session_needs_saving = false` **last**, after populating the model. Per-item
   delegate change handlers guard against firing during instantiation on load
   (Gloss: `if (text !== model_text)` / `if (currentIndex !== selected_index)`;
   Prompts: `if (text !== message_item.content)` on the message editor).
6. **Main-input edits are savable events.** The top-level input
   (`gloss_text_input.onTextChanged`; the Prompts message editors, response
   received, retry/regenerate, tab-select, send) all mark the session dirty.
7. **External entry points must not overwrite the active session.** The sutta
   HTML menu's "Gloss Selection" (`SuttaSearchWindow.gloss_text` →
   `GlossTab.gloss_selected_text`) and "Summarize/Translate/Analyse"
   (`SuttaSearchWindow.new_prompt` → `PromptsTab.new_prompt`): if a non-empty
   session is in progress, **confirm** then `new_session()` + load the selection;
   if the session is empty **but still carries a `current_session_id`** (a loaded
   session whose text was cleared), **detach** (`current_session_id = ""`) so a
   fresh row is INSERTed instead of overwriting the old one.

## Restore-fidelity gotcha: RichText height (AssistantResponses)

`bridges/assets/qml/AssistantResponses.qml` renders the selected response as **RichText**.
Its `TextArea.contentHeight` settles only after the document is laid out at the
final width, which on **restore** happens *after* a height binding first runs (the
delegates are rebuilt before layout). A one-shot `Layout.preferredHeight:
itemAt(currentIndex).Layout.preferredHeight` binding is **not reactive** to those
late `contentHeight` updates (`itemAt()` isn't a tracked property), so a restored
multi-line response was **truncated**. Fix: the `StackLayout` height is a plain
`content_height` property the inner response item **pushes up** via
`Layout.onPreferredHeightChanged` + `Component.onCompleted`, plus a
`refresh_height()` on tab/visibility change — so it always grows to fit. (Live
sends happened to evaluate after layout, which is why only restore showed the bug.)
`open_history_item()` also switches to the working-area tab **before**
`load_session()`, building the delegates while visible (matching the live path).

## In-flight response normalization (Prompts)

Prompts responses arrive asynchronously with a per-response `status`
(`waiting`/`pending`/`completed`/`error`). Transient/in-flight state is **not**
persisted, so `session_data_json()` normalizes any non-terminal response to
`error` (with an "Interrupted" message if empty) at serialize time, so a restored
conversation never shows a "zombie spinner" for a request that will never finish.

## Residual known limitations (accepted — rare, no data loss)

- **Edit during an in-flight autosave:** the dirty flag is cleared on completion
  even if a later edit re-set it; bounded by the 60 s cadence + close flush.
- **Blocking flush while an async autosave is in flight** (Open/New within ~ms of
  a tick): at worst one duplicate row + transiently wrong `current_session_id`;
  self-corrects on the next save/load.
- The AI-translations tab-select on restore does **not** mark dirty —
  `AssistantResponses` emits `tabSelectionChanged` only from a real tab-button
  click (never from programmatic `TabBar.currentIndex` sync or model-reset
  churn), so the restore emits nothing. See
  [ai-model-management-and-fallback.md](./ai-model-management-and-fallback.md).
