# Anki CSV Background Export Implementation

## Summary

The Anki CSV export process has been successfully moved to a background thread in Rust to prevent UI freezing during export. This implementation follows the same pattern as the SuttaSearchWindow.results_page() background processing.

## Changes Made

### 1. New Rust Types (backend/src/types.rs)

Added the following types to support Anki CSV export:

```rust
pub enum AnkiCsvFormat { Basic, Cloze, Templated, TemplatedCloze, Data }
pub struct AnkiCsvExportInput { ... }
pub struct AnkiCsvTemplates { front, back }
pub struct AnkiCsvExportResult { success, files, error }
pub struct AnkiCsvFile { filename, content }
```

### 2. New Rust Module (backend/src/anki_export.rs)

Created a new module to handle CSV generation in Rust:
- `export_anki_csv()` - Main export function that runs in background thread
- `clean_stem()` - Removes disambiguating numbers from stems (e.g., "ña 2.1" → "ña")
- `escape_csv_field()` - Properly escapes CSV fields with quotes, commas, newlines
- `format_csv_row()` - Formats front,back CSV rows
- Template rendering with tinytemplate (using `{word_stem}` syntax instead of `${word_stem}`)

Supports all export formats:
- **Simple**: Basic and Cloze formats
- **Templated**: Custom templates with full DPD data access
- **DataCsv**: Full data export with all DPD fields

### 3. Rust Bridge Updates (bridges/src/sutta_bridge.rs)

Added new signal and function:
- Signal: `ankiCsvExportReady(results_json: QString)`
- Function: `export_anki_csv_background(input_json: &QString)`

The function spawns a background thread using `thread::spawn()` and emits the signal when complete.

### 4. QML Type Definition (bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml)

Added signal and stub function for qmllint:
```qml
signal ankiCsvExportReady(results_json: string);
function export_anki_csv_background(input_json: string) { ... }
```

### 5. GlossTab Updates (bridges/assets/qml/GlossTab.qml)

#### State Management
```qml
property bool is_exporting_anki: false
property int exporting_note_count: 0
```

#### Signal Handler
```qml
Connections {
    target: SuttaBridge
    function onAnkiCsvExportReady(results_json: string) {
        // Handle export completion
    }
}
```

#### Background Export Function
```qml
function start_anki_export_background(folder_url) {
    // Count notes
    // Set exporting state
    // Call SuttaBridge.export_anki_csv_background()
}
```

#### UI Feedback
Added "Exporting x notes..." message next to "Export As..." ComboBox:
```qml
Text {
    id: exporting_message
    text: `Exporting ${root.exporting_note_count} notes...`
    visible: root.is_exporting_anki
}
```

The message appears when export starts and disappears when complete.

### 6. Tests

#### QML Tests (bridges/assets/qml/tst_GlossTabAnkiCsvExport.qml)
Created comprehensive tests for:
- Basic CSV format
- Cloze format  
- Templated format
- Data CSV format
- CSV escaping (quotes, commas, newlines)
- Multiple paragraphs
- Stem number removal
- Context snippets with bold markers

#### Rust Tests (backend/tests/test_anki_export.rs)
Created unit tests for:
- `clean_stem()` - Stem number removal
- `escape_csv_field()` - CSV field escaping
- `format_csv_row()` - CSV row formatting
- Stem number removal validation

All tests pass successfully.

## Technical Details

### Template Syntax Change

**QML (old):** JavaScript template literals with `${variable}`
```javascript
`Stem: ${word_stem} and construction: ${dpd.construction}`
```

**Rust (new):** tinytemplate with `{variable}`
```rust
"Stem: {word_stem} and construction: {dpd.construction}"
```

### Context Structure

Templates receive a context with:
```rust
{
    word_stem: String,           // Cleaned stem (no numbers)
    context_snippet: String,     // Context sentence with <b> markers
    original_word: String,       // Original word form
    clean_word: String,          // Cleaned word
    vocab: {
        uid: String,
        word: String,
        summary: String,
    },
    dpd: {                       // All DPD headword fields
        lemma_1, lemma_2, pos, grammar,
        derived_from, meaning_1, construction,
        derivative, example_1, synonym, antonym, ...
    }
}
```

### Background Processing Flow

1. User clicks "Export As..." → "Anki CSV"
2. GlossTab checks for existing files
3. If user confirms, calls `start_anki_export_background()`
4. Function:
   - Counts total notes to export
   - Sets `is_exporting_anki = true` (shows feedback message)
   - Prepares input JSON with gloss data, templates, options
   - Calls `SuttaBridge.export_anki_csv_background(input_json)`
5. Rust:
   - Spawns background thread
   - Generates all CSV files
   - Emits `ankiCsvExportReady` signal with results
6. QML:
   - Receives signal via `onAnkiCsvExportReady()`
   - Saves files to disk
   - Sets `is_exporting_anki = false` (hides feedback message)
   - Shows confirmation dialog

### File Output

The export generates files based on format:
- **Simple**: `gloss_export_anki_basic.csv` (+ optional `_cloze.csv`)
- **Templated**: `gloss_export_anki_templated.csv` (+ optional `_templated_cloze.csv`)
- **DataCsv**: `gloss_export_anki_data.csv` (with header row)

## Benefits

1. **Non-blocking UI**: Export runs in background thread, UI remains responsive
2. **User feedback**: Clear "Exporting x notes..." message during export
3. **Faster execution**: Rust implementation is more efficient than QML
4. **Better error handling**: Proper Result types and error propagation
5. **Testable**: Both Rust and QML tests verify correctness
6. **Maintainable**: Clean separation between UI (QML) and logic (Rust)

## Files Modified

- `backend/src/types.rs` - Added Anki export types
- `backend/src/anki_export.rs` - New module (CSV generation logic)
- `backend/src/lib.rs` - Added anki_export module
- `bridges/src/sutta_bridge.rs` - Added signal and background function
- `bridges/assets/qml/com/profoundlabs/simsapa/SuttaBridge.qml` - Added signal/function stubs
- `bridges/assets/qml/GlossTab.qml` - Updated to use background export with feedback UI
- `bridges/assets/qml/tst_GlossTabAnkiCsvExport.qml` - New QML tests
- `backend/tests/test_anki_export.rs` - New Rust tests

## Build & Test

```bash
# Build project
make build -B

# Run Rust tests
cd backend && cargo test test_anki_export

# Run QML tests  
make qml-test

# Run all tests
make test
```

## Future Improvements

1. Context snippets could be extracted during glossing and stored in words_data
2. Progress updates could be sent during long exports (currently just start/complete)
3. Export could be cancellable with a cancel button
4. Template preview could be added to Anki Export Dialog
