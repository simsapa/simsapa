# EPUB Image Loading Fix

## Problem

When opening an epub chapter that includes images (e.g., cover page), the HTML viewer would show images as missing/broken.

## Root Causes

There were two issues that needed to be fixed:

### Issue 1: Missing Base URL in QML

The HTML was loaded into the WebEngineView (Desktop) and WebView (Mobile) components without a base URL.

**In `SuttaHtmlView_Desktop.qml` and `SuttaHtmlView_Mobile.qml`:**

```javascript
// BEFORE (BROKEN)
function load_book_spine_uid(uid) {
    if (SuttaBridge.is_spine_item_pdf(uid)) {
        // PDF handling...
    } else {
        var html = SuttaBridge.get_book_spine_html(root.window_id, uid);
        web.loadHtml(html);  // ❌ Missing baseUrl parameter
    }
}
```

The `loadHtml()` function was called with only one argument (the HTML content). Without a base URL, the WebEngine/WebView doesn't know how to resolve paths like `/book_resources/...`.

According to Qt WebEngine documentation, `loadHtml()` has this signature:
```qml
void loadHtml(string html, url baseUrl)
```

When `baseUrl` is omitted or empty, relative and absolute paths cannot be resolved correctly, causing all resource requests (images, CSS, fonts) to fail.

### Issue 2: Missing OEBPS Directory Prefix in Resource Paths

In EPUB files, resources are stored with their full path including the OEBPS directory:
- **Database storage**: `OEBPS/assets/photos/92dpi-ebook-sRGB/jacket/cover.jpg`
- **HTML reference**: `assets/photos/92dpi-ebook-sRGB/jacket/cover.jpg` (relative to OEBPS/)

The path rewriting logic was not including the OEBPS prefix, so rewritten paths like `/book_resources/bmc/assets/photos/cover.jpg` couldn't find resources stored as `OEBPS/assets/photos/cover.jpg` in the database.

## Solution

### Fix 1: Add Base URL to loadHtml()

Pass the localhost API URL as the base URL when loading HTML:

```javascript
// AFTER (FIXED)
function load_book_spine_uid(uid) {
    if (SuttaBridge.is_spine_item_pdf(uid)) {
        // PDF handling...
    } else {
        // Get API URL for resolving resource paths
        const api_url = SuttaBridge.get_api_url();
        var html = SuttaBridge.get_book_spine_html(root.window_id, uid);
        web.loadHtml(html, api_url);  // ✅ Now includes baseUrl
    }
}
```

### Fix 2: Add OEBPS Prefix During Path Rewriting

Modified the `rewrite_resource_links()` function to:
1. Extract the base directory from the spine item path (e.g., `OEBPS` from `OEBPS/cover.xhtml`)
2. Prepend this base directory to normalized relative paths
3. Generate correct API URLs: `assets/photos/cover.jpg` → `/book_resources/bmc/OEBPS/assets/photos/cover.jpg`

**In `backend/src/epub_import.rs`:**

```rust
// Extract base directory from spine item path
let base_dir = std::path::Path::new(&resource_path)
    .parent()
    .and_then(|p| p.to_str())
    .unwrap_or("");

// Pass base_dir to rewriting function
let content_html = rewrite_resource_links(&content_html, book_uid, base_dir);

// Inside rewrite_resource_links:
let normalized_path = normalize_path(path);

// Prepend base directory if it exists
let full_path = if !base_dir.is_empty() && !normalized_path.starts_with(base_dir) {
    format!("{}/{}", base_dir, normalized_path)  // e.g., "OEBPS/assets/photos/cover.jpg"
} else {
    normalized_path
};

format!(r#"{}="/book_resources/{}/{}""#, attr, book_uid, full_path)
```

## How It Works

1. **EPUB Import** (`backend/src/epub_import.rs`):
   - Extracts images from EPUB as binary blobs
   - Stores in `book_resources` table with full paths (e.g., `OEBPS/assets/photos/cover.jpg`)
   - Rewrites HTML paths with OEBPS prefix: `assets/photos/cover.jpg` → `/book_resources/bmc/OEBPS/assets/photos/cover.jpg`

2. **HTML Display** (QML files):
   - Calls `get_book_spine_html()` to get HTML with rewritten paths (including OEBPS prefix)
   - Loads HTML with `api_url` as base URL (e.g., `http://localhost:8000`)
   - WebEngine resolves `/book_resources/...` relative to base URL

3. **Resource Serving** (`bridges/src/api.rs`):
   - WebEngine requests: `http://localhost:8000/book_resources/bmc/OEBPS/assets/photos/cover.jpg`
   - API endpoint queries database for matching resource with path `OEBPS/assets/photos/cover.jpg`
   - Returns binary blob with proper Content-Type header
   - WebEngine displays the image

## Complete Resource Loading Flow

```
1. User opens EPUB chapter (e.g., cover.xhtml stored at path "OEBPS/cover.xhtml")
   ↓
2. QML calls load_book_spine_uid(spine_item_uid)
   ↓
3. Backend extracts base_dir from spine item path: "OEBPS"
   ↓
4. Rewrites HTML paths with OEBPS prefix:
   <image xlink:href="assets/photos/cover.jpg"> 
   → <image xlink:href="/book_resources/bmc/OEBPS/assets/photos/cover.jpg">
   ↓
5. QML gets API URL: http://localhost:8000
   ↓
6. Load HTML with base URL: web.loadHtml(html, "http://localhost:8000")
   ↓
7. WebEngine parses HTML and sees image tag
   ↓
8. Resolves path: base_url + /book_resources/... 
   = http://localhost:8000/book_resources/bmc/OEBPS/assets/photos/cover.jpg
   ↓
9. Makes HTTP request to localhost API
   ↓
10. API endpoint serve_book_resources() handles request
    ↓
11. Queries database: get_book_resource("bmc", "OEBPS/assets/photos/cover.jpg")
    ↓
12. Returns binary blob with Content-Type: image/jpeg
    ↓
13. WebEngine receives image data and displays it ✅
```

## Files Modified

### QML Changes (Base URL Fix)
- `assets/qml/SuttaHtmlView_Desktop.qml` (lines 84-87)
- `assets/qml/SuttaHtmlView_Mobile.qml` (lines 105-108)

### Backend Changes (OEBPS Prefix Fix)
- `backend/src/epub_import.rs`:
  - Modified `rewrite_resource_links()` function signature (line 210)
  - Added base_dir extraction logic (lines 127-131)
  - Updated path rewriting to include base_dir (lines 234-240)
  - Added new tests (lines 305-318)

## Related Files

- **EPUB Import:** `backend/src/epub_import.rs` (lines 155-231)
- **API Endpoint:** `bridges/src/api.rs` (lines 279-328)
- **Database Query:** `backend/src/db/appdata.rs` (line 223)
- **Database Schema:** `backend/migrations/appdata/2026-07-23-000000_initial_schema/up.sql` (the books tables arrived in `2025-12-04-130316_create_books_tables`, since squashed into the 1.0.0 baseline)

## Testing

### Unit Tests

Run the EPUB import tests:
```bash
cd backend && cargo test epub_import::tests
```

All tests should pass, including:
- `test_normalize_path` - Path normalization
- `test_rewrite_resource_links` - Basic rewriting with OEBPS prefix
- `test_rewrite_resource_links_absolute` - Absolute URLs not rewritten
- `test_rewrite_resource_links_with_oebps_prefix` - OEBPS prefix handling
- `test_rewrite_resource_links_empty_base_dir` - Empty base directory handling

### Integration Testing

To test with a real EPUB:

1. Build the application: `make build -B`
2. Run the application: `make run`
3. Import an EPUB file with images (e.g., `backend/tests/data/its-essential-meaning.epub`)
4. Open the cover page or any chapter containing images
5. Verify that images load correctly in the HTML viewer
6. Check that there are no 404 errors in the application logs

### Expected Results

- ✅ Images display correctly
- ✅ No 404 errors in logs for book_resources requests
- ✅ API logs show correct paths like: `Serving book resource: book_uid=bmc, path=OEBPS/assets/photos/cover.jpg`

## Notes

- The same base URL pattern was already used correctly for PDF files
- Other content types (suttas, dictionary words) don't need the OEBPS prefix because they don't use the book_resources system
- The fix applies to both Desktop (WebEngineView) and Mobile (WebView) platforms
- EPUB files can have different directory structures; this fix handles the common OEBPS structure
- If an EPUB has a flat structure without OEBPS, the base_dir will be empty and paths work as before
