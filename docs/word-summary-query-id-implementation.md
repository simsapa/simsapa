# Word Summary Query ID Implementation

## Problem

When word lookup is triggered through `requestWordSummary()` signal, the query runs asynchronously and results are returned via a signal to WordSummary. When queries are slow (e.g., database operations), users can make another query while the previous one is still running. When the earlier results arrive via the signal, they override the current query results in WordSummary, displaying stale data.

## Investigation

### Query Flow

1. **GlossTab.qml** - User clicks a word
   - Emits `requestWordSummary(word)` signal (line 42, 1486, 1665)

2. **SuttaSearchWindow.qml** - Receives the signal
   - Connects to signal at line 1224
   - Calls `set_summary_query(word)` and `word_summary.search_btn.click()`

3. **WordSummary.qml** - Performs lookup
   - `run_lookup()` function (line 94)
   - Calls `SuttaBridge.dpd_lookup_json_async(query)` (line 109)

4. **bridges/src/sutta_bridge.rs** - Bridge layer
   - `dpd_lookup_json_async()` function (line 465)
   - Spawns a thread to prevent blocking Qt event loop (line 471)
   - Performs database lookup via `app_data.dbm.dpd.dpd_lookup_json(&query_text)` (line 473)
   - Emits `dpd_lookup_ready` signal with results (line 478)

5. **backend/src/db/dpd.rs** - Database layer
   - `dpd_lookup_json()` function (line 402)
   - Performs actual database query

6. **WordSummary.qml** - Receives results
   - `onDpdLookupReady()` handler (line 56)
   - Populates `summaries_model` with results

## Solution

Implemented query identification using timestamp-based unique IDs to track which query is current and discard stale results.

### Changes Made

#### 1. bridges/src/sutta_bridge.rs

**Signal signature update (line 68-69):**
```rust
#[qsignal]
#[cxx_name = "dpdLookupReady"]
fn dpd_lookup_ready(self: Pin<&mut SuttaBridge>, query_id: QString, results_json: QString);
```

**Function signature update (line 110):**
```rust
#[qinvokable]
fn dpd_lookup_json_async(self: Pin<&mut SuttaBridge>, query_id: &QString, query: &QString);
```

**Implementation update (line 465-483):**
```rust
pub fn dpd_lookup_json_async(self: Pin<&mut Self>, query_id: &QString, query: &QString) {
    info("SuttaBridge::dpd_lookup_json_async() start");
    let qt_thread = self.qt_thread();
    let query_id_string = query_id.to_string();
    let query_text = query.to_string();

    // Spawn a thread so Qt event loop is not blocked
    thread::spawn(move || {
        let app_data = get_app_data();
        let s = app_data.dbm.dpd.dpd_lookup_json(&query_text);
        let results_json = QString::from(s);
        let query_id_qstring = QString::from(query_id_string);

        // Emit signal with the query_id and results
        qt_thread.queue(move |mut qo| {
            qo.as_mut().dpd_lookup_ready(query_id_qstring, results_json);
        }).unwrap();

        info("SuttaBridge::dpd_lookup_json_async() end");
    });
}
```

#### 2. bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml

**Signal signature update (line 19):**
```qml
signal dpdLookupReady(query_id: string, results_json: string);
```

**Function signature update (line 139-150):**
```qml
function dpd_lookup_json_async(query_id: string, query: string) {
    console.log("dpd_lookup_json_async():", query_id, query);
    // Simulate async behavior
    Qt.callLater(function() {
        query = normalize_query_text(query);
        let result = "[{}]";
        if (root.dpd_lookup_test_data[query]) {
            result = JSON.stringify(root.dpd_lookup_test_data[query]);
        }
        dpdLookupReady(query_id, result);
    });
}
```

#### 3. bridges/assets/qml/WordSummary.qml

**Added query ID tracking property (line 32):**
```qml
property string current_query_id: ""
```

**Updated signal handler with stale check (line 53-70):**
```qml
Connections {
    target: SuttaBridge

    function onDpdLookupReady(query_id: string, results_json: string) {
        // Ignore results from stale queries
        if (query_id !== root.current_query_id) {
            console.log("Discarding stale query results:", query_id, "current:", root.current_query_id);
            return;
        }

        root.is_loading = false;
        summaries_model.clear();
        let sum_list = JSON.parse(results_json);
        for (let i=0; i < sum_list.length; i++) {
            summaries_model.append({
                uid: sum_list[i].uid,
                word: sum_list[i].word,
                summary: sum_list[i].summary,
            });
        }
        // clear the previous selection highlight
        summaries_list.currentIndex = -1;
    }
}
```

**Updated run_lookup to generate query ID (line 94-111):**
```qml
function run_lookup(query: string, min_length = 4) {
    if (query.length < min_length)
        return;

    // Generate a unique query ID using timestamp
    root.current_query_id = new Date().toISOString() + "_" + Math.random().toString(36).substring(2, 9);

    root.is_loading = true;

    // Get deconstructor list synchronously (it's fast)
    deconstructor_model.clear();
    let dec_list = SuttaBridge.dpd_deconstructor_list(query);
    for (let i=0; i < dec_list.length; i++) {
        deconstructor_model.append({ words_joined: dec_list[i] });
    }
    deconstructor.currentIndex = 0;

    // Start async lookup for summaries (this can be slow)
    SuttaBridge.dpd_lookup_json_async(root.current_query_id, query);
}
```

## Query ID Format

The query ID is generated as: `ISO_TIMESTAMP_RANDOMSTRING`

Example: `2025-10-17T14:23:45.123Z_abc123x`

This ensures:
- Unique identification for each query
- Temporal ordering is visible for debugging
- Collision resistance through random suffix

## Behavior

- When a new query is initiated, a unique query_id is generated and stored in `current_query_id`
- The query_id is passed through the entire async pipeline
- When results arrive, the query_id is checked against the current one
- If the query_id doesn't match (older query), results are discarded with a console log
- Only results from the most recent query are displayed to the user

## Testing

Build completed successfully with all changes integrated.

The implementation follows the CXX-Qt bridge pattern used throughout the codebase and maintains backward compatibility with the QML type definitions for qmllint.
