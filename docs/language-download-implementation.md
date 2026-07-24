# Language Download Implementation

## Overview

This document describes the implementation of the language database download mechanism in the Simsapa app. The feature allows users to download and import sutta translations in various languages beyond the default Pāli and English.

## Architecture

The implementation follows the existing download/extraction pattern but adds language-specific logic:

1. **User Interface (QML)**: `DownloadAppdataWindow.qml` provides language selection UI
2. **Bridge Layer (Rust)**: `asset_manager.rs` handles downloads and database imports
3. **Backend (Rust)**: `lookup.rs` provides language code mappings via `LANG_CODE_TO_NAME`

## Components

### DownloadAppdataWindow.qml

**Location**: `assets/qml/DownloadAppdataWindow.qml`

**Key Features**:
- Language input field accepting comma-separated codes (e.g., "hu, pt, it") or "*" for all languages
- Scrollable list displaying available languages with codes and names
- Language validation before download
- Auto-initialization from `download_languages.txt` file

**Properties**:
```qml
property string init_add_languages: ""  // Initial languages from file
property var available_languages: []    // List of "code|Name" strings
```

**Functions**:
- `validate_and_run_download()`: Validates language codes against available languages
- `run_download()`: Generates download URLs for selected languages

### AssetManager (Rust Bridge)

**Location**: `bridges/src/asset_manager.rs`

**New Functions**:

1. `get_available_languages() -> QStringList`
   - Returns language codes and names in format "code|Name"
   - Filters out base languages (en, pli, san)
   - Source: `LANG_CODE_TO_NAME` from `backend/src/lookup.rs`

2. `get_init_languages() -> QString`
   - Reads `download_languages.txt` from app_assets_dir
   - Returns comma-separated language codes
   - Deletes the file after reading

3. `import_suttas_lang_to_appdata(extract_temp_dir, appdata_database_url)`
   - Finds `suttas_lang_*.sqlite3` files in extract directory
   - Imports suttas to appdata database
   - Called after extraction, before moving files to assets

4. `import_suttas_from_db(import_db_path, appdata_database_url)`
   - Connects to language database and appdata database
   - Reads all suttas from language database
   - Deletes existing suttas with same uid
   - Inserts new suttas into appdata

**Type Definition**: `assets/qml/com/profoundlabs/simsapa/AssetManager.qml`

## Download Flow

1. User opens DownloadAppdataWindow
2. Component reads `download_languages.txt` if present (auto-initialization)
3. Component fetches available languages from AssetManager
4. User enters language codes or "*" for all
5. User clicks Download button
6. Validation checks entered codes against available languages
7. URLs are generated: `https://github.com/simsapa/simsapa-assets/releases/download/{version}/suttas_lang_{lang}.tar.bz2`
8. AssetManager downloads each tar.bz2 file
9. AssetManager extracts to temp folder
10. AssetManager detects `suttas_lang_*.sqlite3` files and imports to appdata
11. AssetManager moves remaining files to app-assets
12. Download completes

## Database Import Details

The import process for language databases:

1. **Detection**: After extracting each archive, check if filename matches `suttas_lang_*.tar.bz2`
2. **Import**: Call `import_suttas_lang_to_appdata()` before moving files
3. **Connection**: Establish connections to both language db and appdata db
4. **Read**: Load all suttas from language database
5. **Replace**: Delete existing suttas with same uid in appdata
6. **Insert**: Insert new suttas (without id, let database auto-generate)
7. **Cleanup**: Remove language database file after successful import

## Language Code Format

Language codes follow ISO 639 standards where applicable. The `LANG_CODE_TO_NAME` map in `backend/src/lookup.rs` contains the complete mapping.

**Examples**:
- `hu` - Magyar (Hungarian)
- `pt` - Português (Portuguese)  
- `it` - Italiano (Italian)
- `fr` - Français (French)
- `de` - Deutsch (German)
- `th` - ไทย (Thai)

**Base languages** (always included, cannot be downloaded separately):
- `en` - English
- `pli` - Pāli
- `san` - Sanskrit

## File Naming Convention

- Archive files: `suttas_lang_{lang}.tar.bz2`
- Database files in archive: `suttas_lang_{lang}.sqlite3`
- Auto-init file: `download_languages.txt` (in app_assets_dir)

## Comparison with Legacy Python Implementation

### Similarities
- Same URL format for language databases
- Same import-to-database pattern (now targets the single `appdata.sqlite3`)
- Same `download_languages.txt` initialization mechanism
- Same language filtering (excluding en, pli, san)

### Differences
- **Simpler**: No separate AssetManagement class hierarchy
- **Integrated**: Import happens during download/extract, not as separate step
- **Type-safe**: Rust provides compile-time safety vs Python's runtime checking
- **Single-threaded import**: Python used workers; Rust uses single thread in download worker

## Testing

To test the language download feature:

1. Create `download_languages.txt` in app-assets folder with content: `"hu, pt"`
2. Build and run the app
3. Open Download Appdata Window
4. Verify the input field is pre-filled with "hu, pt"
5. Verify the languages list shows available languages
6. Click Download
7. Verify downloads and imports complete successfully
8. Check appdata.sqlite3 for imported suttas

## Single-Database Architecture

All user-generated and seeded data now lives in a single `appdata.sqlite3`. Rows carry an `is_user_added` boolean so export/import flows can filter user-generated content (default `TRUE` for runtime inserts; bootstrap seeds explicitly set `FALSE`). The legacy `userdata.sqlite3` is no longer created; alpha testers upgrading from it are handled by a one-shot bridge at startup (see PROJECT_MAP.md → "One-Shot Legacy Userdata Bridge").

## Future Enhancements

Potential improvements:
- Progress indication for database import step
- Language removal functionality (already exists in legacy version)
- Index management for imported languages
- Language-specific search index generation
- Display imported language statistics
