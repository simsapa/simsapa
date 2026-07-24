#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use core::pin::Pin;
use cxx_qt_lib::{QString, QStringList, QUrl};
use cxx_qt::Threading;

use simsapa_backend::app_settings::AiRequestMode;
use simsapa_backend::query_task::SearchQueryTask;
use simsapa_backend::types::{SearchArea, SearchMode, SearchParams, SearchResultPage};
use simsapa_backend::theme_colors::ThemeColors;
use simsapa_backend::{get_app_data, try_get_app_data, get_app_globals, get_create_simsapa_dir, save_to_file_checked, check_file_exists_print_err, with_fulltext_searcher};
use simsapa_backend::dir_list::{generate_html_directory_listing, generate_plain_directory_listing};
use simsapa_backend::helpers::{extract_words, normalize_fulltext_query, normalize_query_text, query_text_to_uid_field_query};
use simsapa_backend::prompt_utils::markdown_to_html;
use simsapa_backend::provider_models_update::update_all_provider_models;
use simsapa_backend::logger::{info, warn, error, debug, get_log_level_str, set_log_level_str};
use simsapa_backend::topic_index;
use simsapa_backend::update_checker;
use simsapa_backend::types::SearchResult;
use simsapa_backend::db::appdata_models::HistoryItemType;
use simsapa_backend::db::{DbKind, MigrationOutcome, get_startup_db_report};

/// Cache for search result pages to avoid re-querying for previously fetched pages.
struct ResultsPageCache {
    /// Serialized query + params identifying this search.
    cache_key: String,
    /// Cached pages: page_num → highlighted results.
    pages: HashMap<usize, Vec<SearchResult>>,
    /// Total hits for this search.
    total_hits: i64,
    /// Configured page size (not the number of results on a given page).
    page_len: usize,
}

static RESULTS_PAGE_CACHE: Mutex<Option<ResultsPageCache>> = Mutex::new(None);

/// Cache for the bridge-orchestrated `Combined` mode. Deliberately isolated
/// from `RESULTS_PAGE_CACHE` to prevent cross-warming between Combined
/// sub-fetches (DPD Lookup + Fulltext) and standalone-mode searches that
/// happen to share the same query / params — bold-definition gating,
/// post-filter ordering, and result formatting can diverge between
/// Combined's sub-fetches and a standalone single-mode call. The two
/// sub-buffers cache the parallel DPD and Fulltext background queries that
/// Combined fans out on page 0 and tops up side-aware on later pages; the
/// merged combined page is computed on demand by slicing both buffers, so
/// no second memo layer is needed. Holding both sub-states under a single
/// mutex gives a coherent snapshot of `(dpd_buffer, dpd_total, ft_buffer,
/// ft_total)` for the merge — the lock is never held across an SQLite or
/// Tantivy call (sub-queries run unlocked and write back briefly).
struct CombinedCache {
    cache_key: String,
    page_len: usize,
    dpd_buffer: Vec<SearchResult>,
    dpd_total: Option<i64>,
    dpd_pages_fetched: usize,
    ft_buffer: Vec<SearchResult>,
    ft_total: Option<i64>,
    ft_pages_fetched: usize,
}

static COMBINED_CACHE: Mutex<Option<CombinedCache>> = Mutex::new(None);

/// Holds the most recent export-failure reason string from
/// `prepare_for_database_upgrade()` so that `force_database_upgrade()` can
/// log which errors the user chose to bypass. See PRD §11.6.
static LAST_EXPORT_FAILURE: Mutex<Option<String>> = Mutex::new(None);

/// Fetch, highlight, and cache a single page of search results.
/// Returns (results, total_hits, page_len) on success.
/// If the cache key has changed (new search started), returns None to signal abort.
/// Write the two marker files that trigger the upgrade flow on next app start.
///
/// Used by both `prepare_for_database_upgrade` (happy path) and
/// `force_database_upgrade` (after user confirms export failure).
///
/// Returns `Err(Vec<(category, message)>)` if either marker write fails.
/// A silently-missing marker file would produce a silently-failed upgrade
/// on the next start (the old DB is not deleted and the new download is
/// not auto-started), so marker-write failures must surface to the UI.
/// See PRD §11.3.
fn write_upgrade_marker_files() -> std::result::Result<(), Vec<(String, String)>> {
    let globals = get_app_globals();
    let mut errors: Vec<(String, String)> = Vec::new();

    let delete_marker_path = &globals.paths.delete_files_for_upgrade_marker;
    match std::fs::write(delete_marker_path, "") {
        Ok(()) => {
            info(&format!("Created marker file: {}", delete_marker_path.display()));
        }
        Err(e) => {
            error(&format!("Failed to create delete_files_for_upgrade.txt: {}", e));
            errors.push((
                "marker_delete_files".to_string(),
                format!("{}: {}", delete_marker_path.display(), e),
            ));
        }
    }

    let auto_download_path = &globals.paths.auto_start_download_marker;
    match std::fs::write(auto_download_path, "") {
        Ok(()) => {
            info(&format!("Created marker file: {}", auto_download_path.display()));
        }
        Err(e) => {
            error(&format!("Failed to create auto_start_download.txt: {}", e));
            errors.push((
                "marker_auto_start_download".to_string(),
                format!("{}: {}", auto_download_path.display(), e),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn format_category_errors(errs: &[(String, String)]) -> String {
    errs.iter()
        .map(|(cat, msg)| format!("{}: {}", cat, msg))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Resolve the highest assets release that is compatible with the running app
/// version, used to build the GitHub asset download URLs (appdata / dictionaries
/// / DPD / index / per-language archives).
///
/// Prefers the live releases info stored in the process-global (populated by a
/// successful `check_for_updates()`), and falls back to the embedded snapshot
/// (`assets/releases-fallback.json`) when the global is empty — e.g. the server
/// was unreachable, or no update check ran this session because update
/// notifications are disabled. This is why language downloads from
/// `SuttaLanguagesWindow` keep working offline without an explicit update check.
fn compatible_assets_release() -> Option<update_checker::ReleaseEntry> {
    let releases_info = simsapa_backend::try_get_releases_info()
        .or_else(update_checker::get_fallback_releases_info)?;
    let app_version = update_checker::to_version(&update_checker::get_app_version()).ok()?;
    update_checker::get_latest_app_compatible_assets_release(&releases_info, &app_version).cloned()
}

fn fetch_and_cache_page(
    cache_key: &str,
    query_text: &str,
    search_area_text: &str,
    params_json_text: &str,
    page_num: usize,
) -> Result<Option<(Vec<SearchResult>, i64, usize)>, String> {
    // Check if this page is already cached (another prefetch thread may have filled it)
    {
        let cache_guard = RESULTS_PAGE_CACHE.lock().unwrap();
        if let Some(ref cache) = *cache_guard {
            if cache.cache_key == cache_key && cache.pages.contains_key(&page_num) {
                return Ok(Some((cache.pages[&page_num].clone(), cache.total_hits, cache.page_len)));
            }
            // If the cache key changed, a new search was started — abort
            if !cache.cache_key.is_empty() && cache.cache_key != cache_key {
                return Ok(None);
            }
        }
    }

    let app_data = get_app_data();
    let params: SearchParams = serde_json::from_str(params_json_text).unwrap_or_default();

    let search_area_enum = match search_area_text {
        "Dictionary" => SearchArea::Dictionary,
        "Library" => SearchArea::Library,
        _ => SearchArea::Suttas,
    };

    let mut query_task = SearchQueryTask::new(
        &app_data.dbm,
        query_text.to_string(),
        params,
        search_area_enum,
    );

    let results = query_task.results_page(page_num).map_err(|e| format!("{}", e))?;
    let total_hits = query_task.total_hits();
    let page_len = query_task.page_len as usize;

    // Store in cache (only if cache_key still matches)
    {
        let mut cache_guard = RESULTS_PAGE_CACHE.lock().unwrap();
        let cache = cache_guard.get_or_insert_with(|| ResultsPageCache {
            cache_key: String::new(),
            pages: HashMap::new(),
            total_hits: 0,
            page_len,
        });

        if cache.cache_key != cache_key {
            // New search started while we were fetching — discard
            return Ok(None);
        }

        cache.pages.insert(page_num, results.clone());
        // Update total_hits and page_len in case they weren't set yet
        if cache.total_hits == 0 {
            cache.total_hits = total_hits;
        }
        if cache.page_len == 0 {
            cache.page_len = page_len;
        }
    }

    Ok(Some((results, total_hits, page_len)))
}

/// Run a single sub-query (one mode, one sub-page) for Combined. Free of
/// cache logic so it can run inside a spawned thread without coordinating
/// with `COMBINED_CACHE`. Each call constructs its own `SearchQueryTask`
/// against `&app_data.dbm`; sub-queries on different threads therefore use
/// independent SQLite connections.
fn run_sub_query(
    query_text: &str,
    area: SearchArea,
    params: SearchParams,
    sub_page_num: usize,
) -> Result<(Vec<SearchResult>, i64, usize), String> {
    let app_data = get_app_data();
    let mut task = SearchQueryTask::new(
        &app_data.dbm,
        query_text.to_string(),
        params,
        area,
    );
    let results = task.results_page(sub_page_num).map_err(|e| format!("{}", e))?;
    let total = task.total_hits();
    let page_len = task.page_len;
    Ok((results, total, page_len))
}

/// Serve a Combined-mode page by orchestrating two parallel sub-queries (DPD
/// Lookup + Fulltext Match) on page 0 (cold start), then topping up side-aware
/// on later pages. Returns the merged page, the combined total (`dpd_total +
/// ft_total`), and the page length. Returns `Ok(None)` if the cache key changed
/// mid-flight (a new search started).
///
/// Lock discipline: the `COMBINED_CACHE` mutex is acquired only for brief state
/// reads/writes; it is **never** held across an SQLite or Tantivy call. Each
/// sub-query thread runs unlocked and writes back its buffer + total atomically
/// after re-checking `cache_key`.
fn fetch_combined_page(
    cache_key: &str,
    query_text: &str,
    params_json_text: &str,
    page_num: usize,
) -> Result<Option<(Vec<SearchResult>, i64, usize)>, String> {
    let base_params: SearchParams = serde_json::from_str(params_json_text).unwrap_or_default();
    // Skip the DPD sub-fetch entirely when DPD is excluded from the
    // inclusion set. `DpdLookup` returns rows with `table_name ==
    // "dpd_headwords"` / `"dpd_roots"`, which the per-task post-filter
    // (`apply_dict_source_uids_filter`) only drops for `table_name ==
    // "dict_words"` — so without this gate, disabling DPD in the panel
    // still leaks DPD-native rows into Combined results.
    let dpd_enabled = match base_params.dict_source_uids.as_ref() {
        None => true,
        Some(set) => set.iter().any(|s| s == "dpd"),
    };
    let mut dpd_params = base_params.clone();
    dpd_params.mode = SearchMode::DpdLookup;
    // The Fulltext half searches the **complete compound exactly as typed** —
    // only `mode` is swapped, `query_text` is passed through untouched — and it
    // is never lock-filtered. The break-down lock chooses a *deconstruction of
    // the compound into sub-words*, so it scopes only the DPD Lookup stream
    // that displays those sub-words; a Fulltext row is a place where the whole
    // compound occurs in the texts, which no break-down choice makes more or
    // less applicable. Do not "helpfully" filter this stream or rewrite it into
    // component sub-queries: a short component (`vā`, `iti`, `ca`) matches vast
    // numbers of irrelevant rows, and the stream is not bounded (`vāti` → 6118
    // hits, 3.3 s / 3.7 MB to materialise), so it also stays lazily paged.
    // See docs/search-snippet-highlight-pipeline.md.
    let mut ft_params = base_params.clone();
    ft_params.mode = SearchMode::FulltextMatch;

    // Read a snapshot of current cache state. The cache cell must already
    // have been initialized for this `cache_key` by the `results_page`
    // entry path; if a stale call (e.g. a prefetch thread from a previous
    // search) finds a different key here, it returns `Ok(None)` rather
    // than overwriting — otherwise it could clobber a freshly-reset cache
    // belonging to the current search and cause an in-flight cold-start
    // join to abort, leaving the QML loader hung. Mirrors the standard
    // `fetch_and_cache_page` discipline (reset is the caller's job).
    let (mut dpd_total_opt, mut ft_total_opt, mut page_len) = {
        let guard = COMBINED_CACHE.lock().unwrap();
        let Some(c) = guard.as_ref() else {
            return Ok(None);
        };
        if c.cache_key != cache_key {
            return Ok(None);
        }
        (c.dpd_total, c.ft_total, c.page_len)
    };

    // Cold start: fan out DPD page 0 and Fulltext page 0 in parallel.
    // When DPD is excluded from the inclusion set, skip the DPD sub-fetch
    // entirely (its rows would otherwise leak through — `apply_dict_source_uids_filter`
    // only drops `table_name == "dict_words"` rows, but DpdLookup returns
    // `dpd_headwords` / `dpd_roots`).
    if dpd_total_opt.is_none() && ft_total_opt.is_none() {
        let ((dpd_buf, dpd_total, dpd_pl), (ft_buf, ft_total, ft_pl)) = if dpd_enabled {
            let qt_dpd = query_text.to_string();
            let dpd_p = dpd_params.clone();
            let dpd_handle = thread::spawn(move || {
                run_sub_query(&qt_dpd, SearchArea::Dictionary, dpd_p, 0)
            });
            let qt_ft = query_text.to_string();
            let ft_p = ft_params.clone();
            let ft_handle = thread::spawn(move || {
                run_sub_query(&qt_ft, SearchArea::Dictionary, ft_p, 0)
            });
            let dpd_res = dpd_handle
                .join()
                .map_err(|_| "DPD sub-query thread panicked".to_string())??;
            let ft_res = ft_handle
                .join()
                .map_err(|_| "Fulltext sub-query thread panicked".to_string())??;
            (dpd_res, ft_res)
        } else {
            let ft_res = run_sub_query(
                query_text,
                SearchArea::Dictionary,
                ft_params.clone(),
                0,
            )?;
            // Empty DPD side; page_len falls back to Fulltext's.
            ((Vec::new(), 0i64, ft_res.2), ft_res)
        };
        let pl = if dpd_pl > 0 { dpd_pl } else { ft_pl };

        let mut guard = COMBINED_CACHE.lock().unwrap();
        let Some(c) = guard.as_mut() else {
            return Ok(None);
        };
        if c.cache_key != cache_key {
            return Ok(None);
        }
        c.dpd_buffer = dpd_buf;
        c.dpd_total = Some(dpd_total);
        // Mark DPD as "page 0 fetched" even when skipped, so the top-up loop
        // never re-attempts it (dpd_total = 0 already short-circuits the
        // loop, but this keeps the bookkeeping consistent).
        c.dpd_pages_fetched = 1;
        c.ft_buffer = ft_buf;
        c.ft_total = Some(ft_total);
        c.ft_pages_fetched = 1;
        c.page_len = pl;

        dpd_total_opt = Some(dpd_total);
        ft_total_opt = Some(ft_total);
        page_len = pl;
    }

    let l = page_len;
    if l == 0 {
        return Ok(Some((Vec::new(), 0, 0)));
    }
    let dpd_total_usize = dpd_total_opt.unwrap_or(0).max(0) as usize;
    let ft_total_usize = ft_total_opt.unwrap_or(0).max(0) as usize;
    let lo = page_num.saturating_mul(l);
    let hi = lo.saturating_add(l);

    // Top-up loop. When both sides need more, run them concurrently.
    // The common case is a single side falling short by one sub-page;
    // the rare case (a Combined page that straddles the DPD/FT
    // boundary, or a dropped row in either sub-query) is the one that wins
    // wall-clock from the fan-out. Each iteration:
    //   1. Snapshot `(have, next_page)` for both sides under the lock.
    //   2. If both sides are short, fan out two threads and `join()`.
    //   3. If only one side is short, fetch it inline.
    //   4. Re-acquire the lock, re-check `cache_key`, install results.
    //   5. Loop until both sides are satisfied (or empty-rows short-circuit).
    let dpd_needed = hi.min(dpd_total_usize);
    let ft_required_end = hi.saturating_sub(dpd_total_usize);
    let ft_needed = ft_required_end.min(ft_total_usize);
    loop {
        let (dpd_have, dpd_next, ft_have, ft_next) = {
            let g = COMBINED_CACHE.lock().unwrap();
            let Some(c) = g.as_ref() else { return Ok(None); };
            if c.cache_key != cache_key {
                return Ok(None);
            }
            (
                c.dpd_buffer.len(),
                c.dpd_pages_fetched,
                c.ft_buffer.len(),
                c.ft_pages_fetched,
            )
        };
        let dpd_short = dpd_have < dpd_needed;
        let ft_short = ft_have < ft_needed;
        if !dpd_short && !ft_short {
            break;
        }

        // Run the needed sub-queries unlocked. When both are short, fan out
        // into two threads — same pattern as the cold-start fan-out.
        let (dpd_result, ft_result): (
            Option<Result<(Vec<SearchResult>, i64, usize), String>>,
            Option<Result<(Vec<SearchResult>, i64, usize), String>>,
        ) = if dpd_short && ft_short {
            let qt_dpd = query_text.to_string();
            let dpd_p = dpd_params.clone();
            let dpd_handle = thread::spawn(move || {
                run_sub_query(&qt_dpd, SearchArea::Dictionary, dpd_p, dpd_next)
            });
            let qt_ft = query_text.to_string();
            let ft_p = ft_params.clone();
            let ft_handle = thread::spawn(move || {
                run_sub_query(&qt_ft, SearchArea::Dictionary, ft_p, ft_next)
            });
            let dpd_r = dpd_handle
                .join()
                .map_err(|_| "DPD top-up thread panicked".to_string())?;
            let ft_r = ft_handle
                .join()
                .map_err(|_| "Fulltext top-up thread panicked".to_string())?;
            (Some(dpd_r), Some(ft_r))
        } else if dpd_short {
            let r = run_sub_query(query_text, SearchArea::Dictionary, dpd_params.clone(), dpd_next);
            (Some(r), None)
        } else {
            let r = run_sub_query(query_text, SearchArea::Dictionary, ft_params.clone(), ft_next);
            (None, Some(r))
        };

        let dpd_part = dpd_result.transpose()?;
        let ft_part = ft_result.transpose()?;

        let mut dpd_empty = false;
        let mut ft_empty = false;
        {
            let mut g = COMBINED_CACHE.lock().unwrap();
            let Some(c) = g.as_mut() else { return Ok(None); };
            if c.cache_key != cache_key {
                return Ok(None);
            }
            if let Some((rows, total, _)) = dpd_part {
                dpd_empty = rows.is_empty();
                c.dpd_buffer.extend(rows);
                c.dpd_pages_fetched += 1;
                c.dpd_total = Some(total);
            }
            if let Some((rows, total, _)) = ft_part {
                ft_empty = rows.is_empty();
                c.ft_buffer.extend(rows);
                c.ft_pages_fetched += 1;
                c.ft_total = Some(total);
            }
        }
        // Defensive: if a side reports total > buffer but yields no further
        // rows, stop fetching from it to avoid spinning. The next iteration
        // re-evaluates `dpd_short` / `ft_short` from the buffer length, so
        // an empty fetch on one side does not block the other from continuing.
        if (dpd_short && dpd_empty && ft_short && ft_empty)
            || (dpd_short && dpd_empty && !ft_short)
            || (ft_short && ft_empty && !dpd_short)
        {
            break;
        }
    }

    // Final coherent snapshot: slice [lo, hi) over the merged virtual stream
    // [DPD ..., Fulltext ...] and return.
    let g = COMBINED_CACHE.lock().unwrap();
    let Some(c) = g.as_ref() else { return Ok(None); };
    if c.cache_key != cache_key {
        return Ok(None);
    }
    let dpd_total_final = c.dpd_total.unwrap_or(0).max(0) as usize;
    let ft_total_final = c.ft_total.unwrap_or(0).max(0) as usize;

    let dpd_lo = lo.min(c.dpd_buffer.len()).min(dpd_total_final);
    let dpd_hi = hi.min(c.dpd_buffer.len()).min(dpd_total_final);

    let ft_lo_global = lo.saturating_sub(dpd_total_final);
    let ft_hi_global = hi.saturating_sub(dpd_total_final);
    let ft_lo = ft_lo_global.min(c.ft_buffer.len()).min(ft_total_final);
    let ft_hi = ft_hi_global.min(c.ft_buffer.len()).min(ft_total_final);

    // Header is emitted only for sections that contribute rows on this page,
    // and the range label reflects the slice shown on this page (1-based,
    // inclusive). If a section starts mid-page (transition), its header is
    // inserted at the transition point rather than at the top.
    let mut merged: Vec<SearchResult> = Vec::new();
    if dpd_hi > dpd_lo {
        merged.push(SearchResult::from_section_header(
            format!("DPD Lookup Results ({}-{})", dpd_lo + 1, dpd_hi),
        ));
        merged.extend(c.dpd_buffer[dpd_lo..dpd_hi].iter().cloned());
    }
    if ft_hi > ft_lo {
        merged.push(SearchResult::from_section_header(
            format!("Fulltext Results ({}-{})", ft_lo + 1, ft_hi),
        ));
        merged.extend(c.ft_buffer[ft_lo..ft_hi].iter().cloned());
    }

    let combined_total = (dpd_total_final + ft_total_final) as i64;
    Ok(Some((merged, combined_total, l)))
}

/// Spawn a background thread to prefetch pages into RESULTS_PAGE_CACHE.
fn prefetch_pages(
    cache_key: String,
    query_text: String,
    search_area_text: String,
    params_json_text: String,
    start_page: usize,
    count: usize,
    total_pages: usize,
) {
    thread::spawn(move || {
        // Combined+Dictionary uses its own cache and orchestrator. Detect
        // the mode once (parsing JSON per page in the loop is wasteful) and
        // dispatch accordingly.
        let is_combined_dict = search_area_text == "Dictionary"
            && serde_json::from_str::<SearchParams>(&params_json_text)
                .map(|p| matches!(p.mode, SearchMode::Combined))
                .unwrap_or(false);

        let end_page = (start_page + count).min(total_pages);
        for p in start_page..end_page {
            debug(&format!("Prefetching page {}", p));
            let res = if is_combined_dict {
                fetch_combined_page(&cache_key, &query_text, &params_json_text, p)
            } else {
                fetch_and_cache_page(&cache_key, &query_text, &search_area_text, &params_json_text, p)
            };
            match res {
                Ok(None) => {
                    // Cache key changed (new search), abort prefetch
                    debug("Prefetch aborted: cache key changed");
                    return;
                }
                Ok(Some(_)) => {
                    debug(&format!("Prefetched page {} successfully", p));
                }
                Err(e) => {
                    warn(&format!("Prefetch page {} failed: {}", p, e));
                }
            }
        }
    });
}

/// Convert a QUrl to a local file path string.
/// Handles Windows paths correctly - QUrl::path() returns "/C:/path" on Windows,
/// but we need "C:/path" for Rust's Path/PathBuf to work correctly.
fn qurl_to_local_path(url: &QUrl) -> String {
    let path_str = url.path().to_string();

    // On Windows, QUrl::path() returns "/C:/path" for local files
    // We need to remove the leading slash for Windows drive paths
    #[cfg(target_os = "windows")]
    {
        if path_str.len() >= 3 && path_str.starts_with('/') {
            let chars: Vec<char> = path_str.chars().collect();
            // Check for pattern like "/C:" where second char is a letter and third is ':'
            if chars.len() >= 3 && chars[1].is_ascii_alphabetic() && chars[2] == ':' {
                return path_str[1..].to_string();
            }
        }
    }

    path_str
}

/// Write bytes to a file in a user-chosen folder, shared by the text
/// (`save_file`) and binary (`export_gloss_docx`) export paths.
///
/// Android: FolderDialog returns a SAF content:// tree URI (not a path);
/// scoped storage forbids std::fs writes there. Route through the
/// ContentResolver. Pass the *fully-encoded* URI (to_encoded) — .path()
/// drops scheme/authority and toString() pretty-decodes %3A/%2F.
fn save_bytes_to_folder(folder_url: &QUrl, filename: &str, bytes: &[u8]) -> bool {
    #[cfg(target_os = "android")]
    {
        if folder_url.scheme().map(|s| s.to_string()).as_deref() == Some("content") {
            let tree_uri = String::from_utf8_lossy(folder_url.to_encoded().as_slice()).to_string();
            let mime = simsapa_backend::android_saf::mime_from_filename(filename);
            return match simsapa_backend::android_saf::write_to_tree_uri(
                &tree_uri, filename, mime, bytes) {
                Ok(_) => true,
                Err(e) => {
                    error(&format!("save_bytes_to_folder SAF write failed for {}: {}", filename, e));
                    false
                }
            };
        }
    }

    let folder_path = PathBuf::from(qurl_to_local_path(folder_url));
    let output_path = folder_path.join(filename);
    match output_path.to_str() {
        Some(p) => match save_to_file_checked(bytes, p) {
            Ok(_) => true,
            Err(e) => {
                error(&format!("save_bytes_to_folder failed to write {}: {}", p, e));
                false
            }
        },
        None => {
            error(&format!("save_bytes_to_folder: output path is not valid UTF-8: {:?}", output_path));
            false
        }
    }
}

/// Shared INSERT/UPDATE logic for a history session, used by both the async and
/// the blocking (app-close) save paths. `session_id` is the QML-side string:
/// empty means INSERT a new row, otherwise UPDATE the row with that parsed id.
/// Returns the resolved id as a string (the new id on INSERT), or `None` on a
/// parse/DB error.
fn save_history_session_impl(item_type: HistoryItemType, session_id: &str, data_json: &str) -> Option<String> {
    let app_data = get_app_data();
    let session_id = session_id.trim();

    if session_id.is_empty() {
        match app_data.dbm.appdata.save_new_history(item_type, data_json) {
            Ok(new_id) => Some(new_id.to_string()),
            Err(e) => {
                error(&format!("save_history_session_impl insert: {}", e));
                None
            }
        }
    } else {
        match session_id.parse::<i32>() {
            Ok(id) => match app_data.dbm.appdata.update_history(id, data_json) {
                // Row still exists: updated in place.
                Ok(rows) if rows > 0 => Some(id.to_string()),
                // Row was deleted/cleared while still the current session: INSERT
                // a fresh row instead of silently losing the data.
                Ok(_) => match app_data.dbm.appdata.save_new_history(item_type, data_json) {
                    Ok(new_id) => Some(new_id.to_string()),
                    Err(e) => {
                        error(&format!("save_history_session_impl reinsert: {}", e));
                        None
                    }
                },
                Err(e) => {
                    error(&format!("save_history_session_impl update: {}", e));
                    None
                }
            },
            Err(e) => {
                error(&format!("save_history_session_impl: invalid session_id {:?}: {}", session_id, e));
                None
            }
        }
    }
}

#[cxx_qt::bridge]
pub mod qobject {

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;

        include!("system_palette.h");
        fn get_system_palette_json() -> QString;

        include!("utils.h");
        fn copy_content_uri_to_temp_file(content_uri: &QString) -> QString;
        fn get_qt_version() -> QString;
    }

    impl cxx_qt::Threading for SuttaBridge{}

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, db_loaded)]
        #[qproperty(bool, searcher_ready)]
        #[qproperty(bool, sutta_references_loaded)]
        #[qproperty(bool, topic_index_loaded)]
        #[namespace = "sutta_bridge"]
        type SuttaBridge = super::SuttaBridgeRust;

        #[qsignal]
        #[cxx_name = "updateWindowTitle"]
        fn update_window_title(self: Pin<&mut SuttaBridge>, sutta_uid: QString, sutta_ref: QString, sutta_title: QString);

        #[qsignal]
        #[cxx_name = "resultsPageReady"]
        fn results_page_ready(self: Pin<&mut SuttaBridge>, results_json: QString);

        #[qsignal]
        #[cxx_name = "allParagraphsGlossReady"]
        fn all_paragraphs_gloss_ready(self: Pin<&mut SuttaBridge>, results_json: QString);

        #[qsignal]
        #[cxx_name = "paragraphGlossReady"]
        fn paragraph_gloss_ready(self: Pin<&mut SuttaBridge>, paragraph_index: i32, results_json: QString);

        #[qsignal]
        #[cxx_name = "dpdLookupReady"]
        fn dpd_lookup_ready(self: Pin<&mut SuttaBridge>, query_id: QString, results_json: QString);

        #[qsignal]
        #[cxx_name = "dpdLookupGroupedReady"]
        fn dpd_lookup_grouped_ready(self: Pin<&mut SuttaBridge>, query_id: QString, grouped_json: QString);

        #[qsignal]
        #[cxx_name = "ankiCsvExportReady"]
        fn anki_csv_export_ready(self: Pin<&mut SuttaBridge>, results_json: QString);

        #[qsignal]
        #[cxx_name = "ankiPreviewReady"]
        fn anki_preview_ready(self: Pin<&mut SuttaBridge>, preview_html: QString);

        #[qsignal]
        #[cxx_name = "databaseValidationResult"]
        fn database_validation_result(self: Pin<&mut SuttaBridge>, database_name: QString, is_valid: bool, message: QString);

        #[qsignal]
        #[cxx_name = "documentImportProgress"]
        fn document_import_progress(self: Pin<&mut SuttaBridge>, message: QString);

        #[qsignal]
        #[cxx_name = "documentImportCompleted"]
        fn document_import_completed(self: Pin<&mut SuttaBridge>, success: bool, message: QString);

        #[qsignal]
        #[cxx_name = "showChapterFromLibrary"]
        fn show_chapter_from_library(self: Pin<&mut SuttaBridge>, window_id: QString, result_data_json: QString);

        #[qsignal]
        #[cxx_name = "showSuttaFromReferenceSearch"]
        fn show_sutta_from_reference_search(self: Pin<&mut SuttaBridge>, window_id: QString, result_data_json: QString);

        #[qsignal]
        #[cxx_name = "bookMetadataUpdated"]
        fn book_metadata_updated(self: Pin<&mut SuttaBridge>, success: bool, message: QString);

        #[qsignal]
        #[cxx_name = "showBottomFootnotesChanged"]
        fn show_bottom_footnotes_changed(self: Pin<&mut SuttaBridge>);

        // Update checker signals
        #[qsignal]
        #[cxx_name = "appUpdateAvailable"]
        fn app_update_available(self: Pin<&mut SuttaBridge>, update_info_json: QString);

        #[qsignal]
        #[cxx_name = "dbUpdateAvailable"]
        fn db_update_available(self: Pin<&mut SuttaBridge>, update_info_json: QString);

        #[qsignal]
        #[cxx_name = "localDbObsolete"]
        fn local_db_obsolete(self: Pin<&mut SuttaBridge>, update_info_json: QString);

        #[qsignal]
        #[cxx_name = "noUpdatesAvailable"]
        fn no_updates_available(self: Pin<&mut SuttaBridge>);

        #[qsignal]
        #[cxx_name = "updateCheckError"]
        fn update_check_error(self: Pin<&mut SuttaBridge>, error_message: QString);

        #[qsignal]
        #[cxx_name = "releasesCheckCompleted"]
        fn releases_check_completed(self: Pin<&mut SuttaBridge>);

        #[qsignal]
        #[cxx_name = "topicIndexLoaded"]
        fn topic_index_loaded_signal(self: Pin<&mut SuttaBridge>);

        #[qsignal]
        #[cxx_name = "rebuildSearchIndexProgress"]
        fn rebuild_search_index_progress(self: Pin<&mut SuttaBridge>, message: QString);

        #[qsignal]
        #[cxx_name = "rebuildSearchIndexCompleted"]
        fn rebuild_search_index_completed(self: Pin<&mut SuttaBridge>, success: bool, message: QString);

        #[qsignal]
        #[cxx_name = "debugQueryReady"]
        fn debug_query_ready(self: Pin<&mut SuttaBridge>, debug_json: QString);

        #[qsignal]
        #[cxx_name = "waveformDataReady"]
        fn waveform_data_ready(self: Pin<&mut SuttaBridge>, recording_uid: QString, waveform_json: QString);

        // Gloss / Prompts history signals (item_type is "gloss" | "prompts").
        // The QML tabs filter on item_type so a single SuttaBridge serves both.
        #[qsignal]
        #[cxx_name = "historyListReady"]
        fn history_list_ready(self: Pin<&mut SuttaBridge>, item_type: QString, json: QString);

        #[qsignal]
        #[cxx_name = "historySaved"]
        fn history_saved(self: Pin<&mut SuttaBridge>, item_type: QString, session_id: QString);

        #[qsignal]
        #[cxx_name = "historyChanged"]
        fn history_changed(self: Pin<&mut SuttaBridge>, item_type: QString);

        #[qinvokable]
        fn emit_update_window_title(self: Pin<&mut SuttaBridge>, sutta_uid: QString, sutta_ref: QString, sutta_title: QString);

        #[qinvokable]
        fn emit_show_chapter_from_library(self: Pin<&mut SuttaBridge>, window_id: QString, result_data_json: QString);

        #[qinvokable]
        fn emit_show_sutta_from_reference_search(self: Pin<&mut SuttaBridge>, window_id: QString, result_data_json: QString);

        #[qinvokable]
        fn load_db(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn load_searcher(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn load_sutta_references(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn appdata_first_query(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn dpd_first_query(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn dictionary_first_query(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn reset_app_settings_to_defaults(self: Pin<&mut SuttaBridge>) -> bool;

        #[qsignal]
        #[cxx_name = "appSettingsReset"]
        fn app_settings_reset(self: Pin<&mut SuttaBridge>);

        #[qsignal]
        #[cxx_name = "exportFailed"]
        fn export_failed(self: Pin<&mut SuttaBridge>, reason: QString);

        #[qsignal]
        #[cxx_name = "exportSucceeded"]
        fn export_succeeded(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn query_text_to_uid_field_query(self: &SuttaBridge, query_text: &QString) -> QString;

        #[qinvokable]
        fn convert_verse_ref_to_uid(self: &SuttaBridge, sutta_ref: &QString) -> QString;

        #[qinvokable]
        fn results_page(self: Pin<&mut SuttaBridge>, query: &QString, page_num: usize, search_area: &QString, params_json: &QString);

        #[qinvokable]
        fn debug_query(self: Pin<&mut SuttaBridge>, query: &QString, search_area: &QString, params_json: &QString);

        #[qinvokable]
        fn extract_words(self: &SuttaBridge, text: &QString) -> QStringList;

        #[qinvokable]
        fn normalize_query_text(self: &SuttaBridge, text: &QString) -> QString;

        #[qinvokable]
        fn dpd_deconstructor_list(self: &SuttaBridge, query: &QString) -> QStringList;

        #[qinvokable]
        fn dpd_lookup_json(self: &SuttaBridge, query: &QString) -> QString;

        #[qinvokable]
        fn dpd_lookup_json_async(self: Pin<&mut SuttaBridge>, query_id: &QString, query: &QString);

        #[qinvokable]
        fn dpd_lookup_grouped_json_async(self: Pin<&mut SuttaBridge>, query_id: &QString, query: &QString);

        #[qinvokable]
        fn get_sutta_html(self: &SuttaBridge, window_id: &QString, uid: &QString) -> QString;

        #[qinvokable]
        fn get_word_html(self: &SuttaBridge, window_id: &QString, uid: &QString) -> QString;

        #[qinvokable]
        fn get_translations_data_json_for_sutta_uid(self: &SuttaBridge, sutta_uid: &QString) -> QString;

        #[qinvokable]
        fn find_related_sutta_json(self: &SuttaBridge, sutta_uid: &QString, relation: &QString) -> QString;

        #[qinvokable]
        fn qt_version(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn app_data_folder_path(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn is_app_data_folder_writable(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn app_data_contents_html_table(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn app_data_contents_plain_table(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_log_files_list(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_log_file_contents(self: &SuttaBridge, file_name: &QString) -> QString;

        #[qinvokable]
        fn get_log_file_path(self: &SuttaBridge, file_name: &QString) -> QString;

        #[qinvokable]
        fn get_theme_name(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_theme_name(self: Pin<&mut SuttaBridge>, theme_name: &QString);

        #[qinvokable]
        fn get_ai_models_auto_retry(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_ai_models_auto_retry(self: Pin<&mut SuttaBridge>, auto_retry: bool);

        #[qinvokable]
        fn get_ai_auto_fallback(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_ai_auto_fallback(self: Pin<&mut SuttaBridge>, auto_fallback: bool);

        #[qinvokable]
        fn get_gloss_ai_translate_mode(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_gloss_ai_translate_mode(self: Pin<&mut SuttaBridge>, mode: &QString);

        #[qinvokable]
        fn get_prompts_request_mode(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_prompts_request_mode(self: Pin<&mut SuttaBridge>, mode: &QString);

        #[qinvokable]
        fn get_api_key(self: &SuttaBridge, key_name: &QString) -> QString;

        #[qinvokable]
        fn set_api_keys(self: Pin<&mut SuttaBridge>, api_keys_json: &QString);

        #[qinvokable]
        fn get_system_prompt(self: &SuttaBridge, prompt_name: &QString) -> QString;

        #[qinvokable]
        fn get_default_system_prompt(self: &SuttaBridge, prompt_name: &QString) -> QString;

        #[qinvokable]
        fn set_system_prompts_json(self: Pin<&mut SuttaBridge>, prompts_json: &QString);

        #[qinvokable]
        fn get_system_prompts_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_gloss_word_selection_settings_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_gloss_word_selection_settings_json(self: Pin<&mut SuttaBridge>, settings_json: &QString);

        #[qinvokable]
        fn save_gloss_word_cache(self: &SuttaBridge, word: &QString, context_snippet: &QString, selected_uid: &QString, origin: &QString) -> bool;

        #[qinvokable]
        fn save_gloss_word_deconstruction_cache(self: &SuttaBridge, word: &QString, context_snippet: &QString, deconstruction: &QString, origin: &QString) -> bool;

        #[qinvokable]
        fn delete_gloss_word_cache(self: &SuttaBridge, word: &QString, context_hash: &QString) -> bool;

        #[qinvokable]
        fn gloss_word_cache_count(self: &SuttaBridge) -> i32;

        #[qinvokable]
        fn clear_gloss_word_cache(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn parse_word_selection_response(self: &SuttaBridge, response: &QString, expected_items_json: &QString) -> QString;

        #[qinvokable]
        fn build_word_selection_items_json(self: &SuttaBridge, paragraphs_json: &QString, forced: bool) -> QString;

        #[qinvokable]
        fn annotate_gloss_words_json(self: &SuttaBridge, words_data_json: &QString) -> QString;

        #[qinvokable]
        fn export_gloss_session_json(self: &SuttaBridge, session_json: &QString) -> QString;

        #[qinvokable]
        fn import_gloss_word_cache(self: &SuttaBridge, entries_json: &QString) -> QString;

        #[qinvokable]
        fn open_gloss_session_export(self: &SuttaBridge, file_path: &QString) -> QString;

        #[qinvokable]
        fn get_providers_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_providers_json(self: Pin<&mut SuttaBridge>, providers_json: &QString);

        #[qinvokable]
        fn update_model_lists(self: Pin<&mut SuttaBridge>);

        #[qsignal]
        #[cxx_name = "modelListsUpdated"]
        fn model_lists_updated(self: Pin<&mut SuttaBridge>, success: bool, report_json: QString);

        #[qinvokable]
        fn get_provider_api_key(self: &SuttaBridge, provider_name: &QString) -> QString;

        #[qinvokable]
        fn set_provider_api_key(self: Pin<&mut SuttaBridge>, provider_name: &QString, api_key: &QString);

        #[qinvokable]
        fn get_api_url(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_status_bar_height(self: &SuttaBridge) -> i32;

        #[qinvokable]
        fn run_gloss_in_sutta_window(self: &SuttaBridge, window_id: &QString, query_text: &QString);

        #[qinvokable]
        fn open_sutta_search_window(self: &SuttaBridge);

        #[qinvokable]
        fn open_sutta_search_window_with_result(self: &SuttaBridge, result_data_json: &QString);

        #[qinvokable]
        fn open_sutta_languages_window(self: &SuttaBridge);

        #[qinvokable]
        fn open_dictionaries_window(self: &SuttaBridge);

        #[qinvokable]
        fn open_library_window(self: &SuttaBridge);

        #[qinvokable]
        fn open_reference_search_window(self: &SuttaBridge);

        #[qinvokable]
        fn get_all_books_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_book_by_uid_json(self: &SuttaBridge, book_uid: &QString) -> QString;

        #[qinvokable]
        fn get_spine_items_for_book_json(self: &SuttaBridge, book_uid: &QString) -> QString;

        #[qinvokable]
        fn get_spine_item_uid_by_path(self: &SuttaBridge, book_uid: &QString, resource_path: &QString) -> QString;

        #[qinvokable]
        fn get_book_spine_html(self: &SuttaBridge, window_id: &QString, spine_item_uid: &QString) -> QString;

        #[qinvokable]
        fn check_book_uid_exists(self: &SuttaBridge, book_uid: &QString) -> bool;

        #[qinvokable]
        fn extract_document_metadata(self: &SuttaBridge, file_path: &QString) -> QString;

        #[qinvokable]
        fn copy_content_uri_to_temp(self: &SuttaBridge, content_uri: &QString) -> QString;

        #[qinvokable]
        fn delete_temp_import_folder(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn is_spine_item_pdf(self: &SuttaBridge, spine_item_uid: &QString) -> bool;

        #[qinvokable]
        fn get_book_uid_for_spine_item(self: &SuttaBridge, spine_item_uid: &QString) -> QString;

        #[qinvokable]
        fn import_document(self: Pin<&mut SuttaBridge>, file_path: &QString, book_uid: &QString, title: &QString, author: &QString, language: &QString, document_type: &QString, split_tag: &QString);

        #[qinvokable]
        fn rebuild_search_index(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn check_search_index_status(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_startup_db_report(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn remove_book(self: &SuttaBridge, book_uid: &QString) -> bool;

        #[qinvokable]
        fn get_book_metadata_json(self: &SuttaBridge, book_uid: &QString) -> QString;

        #[qinvokable]
        fn update_book_metadata(self: Pin<&mut SuttaBridge>, book_uid: &QString, title: &QString, author: &QString, language: &QString, enable_embedded_css: bool);

        #[qinvokable]
        fn set_provider_enabled(self: Pin<&mut SuttaBridge>, provider_name: &QString, enabled: bool);

        #[qinvokable]
        fn add_provider_model(self: Pin<&mut SuttaBridge>, provider_name: &QString, model_name: &QString);

        #[qinvokable]
        fn remove_provider_model(self: Pin<&mut SuttaBridge>, provider_name: &QString, model_name: &QString);

        #[qinvokable]
        fn set_provider_model_enabled(self: Pin<&mut SuttaBridge>, provider_name: &QString, model_name: &QString, enabled: bool);

        #[qinvokable]
        fn get_provider_for_model(self: &SuttaBridge, model_name: &QString) -> QString;

        #[qinvokable]
        fn get_ai_fallback_sequence_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_ai_fallback_sequence_json(self: Pin<&mut SuttaBridge>, entries_json: &QString);

        #[qinvokable]
        fn get_ai_parallel_prompts_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_ai_parallel_prompts_json(self: Pin<&mut SuttaBridge>, entries_json: &QString);

        #[qinvokable]
        fn get_anki_template_front(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_anki_template_front(self: Pin<&mut SuttaBridge>, template_str: &QString);

        #[qinvokable]
        fn get_anki_template_back(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_anki_template_back(self: Pin<&mut SuttaBridge>, template_str: &QString);

        #[qinvokable]
        fn get_anki_template_cloze_front(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_anki_template_cloze_front(self: Pin<&mut SuttaBridge>, template_str: &QString);

        #[qinvokable]
        fn get_anki_template_cloze_back(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_anki_template_cloze_back(self: Pin<&mut SuttaBridge>, template_str: &QString);

        #[qinvokable]
        fn get_anki_export_format(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_anki_export_format(self: Pin<&mut SuttaBridge>, format: &QString);

        #[qinvokable]
        fn get_anki_include_cloze(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_anki_include_cloze(self: Pin<&mut SuttaBridge>, include: bool);

        #[qinvokable]
        fn get_sample_vocabulary_data_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_dpd_headword_by_uid(self: &SuttaBridge, uid: &QString) -> QString;

        #[qinvokable]
        fn get_saved_theme(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_theme(self: &SuttaBridge, theme_name: &QString) -> QString;

        #[qinvokable]
        fn get_common_words_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn save_common_words_json(self: &SuttaBridge, words_json: &QString);

        // Gloss / Prompts history (shared, item_type-parameterised). All DB work
        // runs off the UI thread except the blocking flush used on app close.
        #[qinvokable]
        fn get_history_json_background(self: Pin<&mut SuttaBridge>, item_type: &QString);

        #[qinvokable]
        fn save_history_session_background(self: Pin<&mut SuttaBridge>, item_type: &QString, session_id: &QString, data_json: &QString);

        #[qinvokable]
        fn save_history_session_blocking(self: &SuttaBridge, item_type: &QString, session_id: &QString, data_json: &QString) -> QString;

        #[qinvokable]
        fn delete_history_item(self: Pin<&mut SuttaBridge>, item_type: &QString, id: i32);

        #[qinvokable]
        fn clear_history(self: Pin<&mut SuttaBridge>, item_type: &QString);

        #[qinvokable]
        fn save_anki_csv(self: &SuttaBridge, csv_content: &QString) -> QString;

        #[qinvokable]
        fn process_all_paragraphs_background(self: Pin<&mut SuttaBridge>, input_json: &QString);

        #[qinvokable]
        fn process_paragraph_background(self: Pin<&mut SuttaBridge>, paragraph_index: i32, input_json: &QString);

        #[qinvokable]
        fn save_file(self: &SuttaBridge, folder_url: &QUrl, filename: &QString, content: &QString) -> bool;

        #[qinvokable]
        fn export_gloss_docx(self: &SuttaBridge, folder_url: &QUrl, filename: &QString, gloss_json: &QString) -> bool;

        #[qinvokable]
        fn export_chat_docx(self: &SuttaBridge, folder_url: &QUrl, filename: &QString, chat_json: &QString) -> bool;

        #[qinvokable]
        fn gloss_export(self: &SuttaBridge, gloss_json: &QString, format: &QString) -> QString;

        #[qinvokable]
        fn gloss_paragraph_export(self: &SuttaBridge, paragraph_json: &QString, paragraph_number: i32, format: &QString) -> QString;

        #[qinvokable]
        fn chat_export(self: &SuttaBridge, chat_json: &QString, format: &QString) -> QString;

        #[qinvokable]
        fn chat_message_export(self: &SuttaBridge, message_json: &QString, format: &QString) -> QString;

        #[qinvokable]
        fn check_file_exists_in_folder(self: &SuttaBridge, folder_url: &QUrl, filename: &QString) -> bool;

        #[qinvokable]
        fn markdown_to_html(self: &SuttaBridge, markdown_text: &QString) -> QString;

        #[qinvokable]
        fn export_anki_csv_background(self: Pin<&mut SuttaBridge>, input_json: &QString);

        #[qinvokable]
        fn render_anki_preview_background(self: Pin<&mut SuttaBridge>, front_template: &QString, back_template: &QString);

        #[qinvokable]
        fn get_search_as_you_type(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_search_as_you_type(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_include_cst_commentary_in_translations(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_include_cst_commentary_in_translations(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_include_cst_mula_in_search_results(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_include_cst_mula_in_search_results(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_include_cst_commentary_in_search_results(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_include_cst_commentary_in_search_results(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_include_cst_mula_in_translations(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_include_cst_mula_in_translations(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_include_ms_mula_in_search_results(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_include_ms_mula_in_search_results(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_include_comm_bold_definitions_in_search_results(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_include_comm_bold_definitions_in_search_results(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_open_find_in_sutta_results(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_open_find_in_sutta_results(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_show_bottom_footnotes(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_show_bottom_footnotes(self: Pin<&mut SuttaBridge>, enabled: bool);

        #[qinvokable]
        fn get_sutta_language_labels(self: &SuttaBridge) -> QStringList;

        #[qinvokable]
        fn get_library_language_labels(self: &SuttaBridge) -> QStringList;

        #[qinvokable]
        fn get_dict_language_labels(self: &SuttaBridge) -> QStringList;

        #[qinvokable]
        fn get_language_filter_key(self: &SuttaBridge, area: &QString) -> QString;

        #[qinvokable]
        fn set_language_filter_key(self: &SuttaBridge, area: &QString, key: &QString);

        #[qinvokable]
        fn get_last_search_mode(self: &SuttaBridge, area: &QString) -> QString;

        #[qinvokable]
        fn set_last_search_mode(self: &SuttaBridge, area: &QString, mode: &QString);

        #[qinvokable]
        fn get_mobile_top_bar_margin(self: &SuttaBridge) -> i32;

        #[qinvokable]
        fn is_mobile_top_bar_margin_system(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn get_mobile_top_bar_margin_custom_value(self: &SuttaBridge) -> u32;

        #[qinvokable]
        fn set_mobile_top_bar_margin_system(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn set_mobile_top_bar_margin_custom(self: Pin<&mut SuttaBridge>, value: u32);

        #[qinvokable]
        fn get_sutta_language_labels_with_counts(self: &SuttaBridge) -> QStringList;

        #[qinvokable]
        fn search_reference(self: &SuttaBridge, query: &QString, field: &QString) -> QString;

        #[qinvokable]
        fn extract_uid_from_url(self: &SuttaBridge, url: &QString) -> QString;

        #[qinvokable]
        fn get_full_sutta_uid(self: &SuttaBridge, partial_uid: &QString) -> QString;

        #[qinvokable]
        fn get_sutta_reference_info(self: &SuttaBridge, uid: &QString) -> QString;

        // Update checker functions
        #[qinvokable]
        fn check_for_updates(self: Pin<&mut SuttaBridge>, include_no_updates: bool, screen_size: &QString, save_stats_behaviour: &QString);

        #[qinvokable]
        fn get_notify_about_simsapa_updates(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_notify_about_simsapa_updates(self: Pin<&mut SuttaBridge>, enabled: bool);

        // Keybindings management
        #[qinvokable]
        fn get_keybindings_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_default_keybindings_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_action_names_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_action_descriptions_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_keybinding(self: Pin<&mut SuttaBridge>, action_id: &QString, shortcuts_json: &QString);

        #[qinvokable]
        fn reset_keybinding(self: Pin<&mut SuttaBridge>, action_id: &QString);

        #[qinvokable]
        fn reset_all_keybindings(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn get_updates_checked(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_updates_checked(self: &SuttaBridge, checked: bool);

        #[qinvokable]
        fn prepare_for_database_upgrade(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn force_database_upgrade(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn get_import_me_dir_path(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_compatible_asset_version_tag(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_compatible_asset_github_repo(self: &SuttaBridge) -> QString;

        // Topic Index functions
        #[qinvokable]
        fn load_topic_index(self: Pin<&mut SuttaBridge>);

        #[qinvokable]
        fn is_topic_index_cached(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn get_topic_index_letters(self: &SuttaBridge) -> QStringList;

        #[qinvokable]
        fn get_topic_headwords_for_letter(self: &SuttaBridge, letter: &QString) -> QString;

        #[qinvokable]
        fn search_topic_headwords(self: &SuttaBridge, query: &QString) -> QString;

        #[qinvokable]
        fn get_topic_headword_by_id(self: &SuttaBridge, headword_id: &QString) -> QString;

        #[qinvokable]
        fn get_topic_letter_for_headword_id(self: &SuttaBridge, headword_id: &QString) -> QString;

        #[qinvokable]
        fn find_topic_headword_id_by_text(self: &SuttaBridge, target: &QString) -> QString;

        #[qinvokable]
        fn open_topic_index_window(self: &SuttaBridge);

        // Chanting Practice functions
        #[qinvokable]
        fn open_chanting_practice_window(self: &SuttaBridge, window_id: &QString);

        #[qinvokable]
        fn open_chanting_review_window(self: &SuttaBridge, window_id: &QString, section_uid: &QString);

        #[qinvokable]
        fn get_all_chanting_collections_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_chanting_section_detail_json(self: &SuttaBridge, section_uid: &QString) -> QString;

        #[qinvokable]
        fn get_chanting_recordings_dir(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn copy_file_to_chanting_recordings(self: &SuttaBridge, source_path: &QString, dest_filename: &QString) -> QString;

        #[qinvokable]
        fn check_file_exists(self: &SuttaBridge, file_path: &QString) -> bool;

        #[qinvokable]
        fn create_chanting_collection(self: &SuttaBridge, json: &QString) -> QString;

        #[qinvokable]
        fn update_chanting_collection(self: &SuttaBridge, json: &QString) -> QString;

        #[qinvokable]
        fn delete_chanting_collection(self: &SuttaBridge, collection_uid: &QString) -> QString;

        #[qinvokable]
        fn create_chanting_chant(self: &SuttaBridge, json: &QString) -> QString;

        #[qinvokable]
        fn update_chanting_chant(self: &SuttaBridge, json: &QString) -> QString;

        #[qinvokable]
        fn delete_chanting_chant(self: &SuttaBridge, chant_uid: &QString) -> QString;

        #[qinvokable]
        fn create_chanting_section(self: &SuttaBridge, json: &QString) -> QString;

        #[qinvokable]
        fn update_chanting_section(self: &SuttaBridge, json: &QString) -> QString;

        #[qinvokable]
        fn delete_chanting_section(self: &SuttaBridge, section_uid: &QString) -> QString;

        #[qinvokable]
        fn create_chanting_recording(self: &SuttaBridge, json: &QString) -> QString;

        #[qinvokable]
        fn delete_chanting_recording(self: &SuttaBridge, recording_uid: &QString) -> QString;

        #[qinvokable]
        fn update_recording_label(self: &SuttaBridge, recording_uid: &QString, label: &QString) -> QString;

        #[qinvokable]
        fn update_recording_markers(self: &SuttaBridge, recording_uid: &QString, markers_json: &QString) -> QString;

        #[qinvokable]
        fn update_recording_volume(self: &SuttaBridge, recording_uid: &QString, volume: f32) -> QString;

        #[qinvokable]
        fn update_recording_playback_position(self: &SuttaBridge, recording_uid: &QString, position_ms: i32) -> QString;

        #[qinvokable]
        fn generate_waveform_data(self: Pin<&mut SuttaBridge>, recording_uid: &QString, file_path: &QString, num_bars: i32);

        #[qinvokable]
        fn export_chanting_data(self: &SuttaBridge, json_selected_uids: &QString, dest_path: &QString) -> QString;

        #[qinvokable]
        fn import_chanting_data(self: &SuttaBridge, zip_path: &QString) -> QString;

        // Logger functions
        #[qinvokable]
        fn log_debug(self: &SuttaBridge, message: &QString);

        #[qinvokable]
        fn log_info(self: &SuttaBridge, message: &QString);

        #[qinvokable]
        fn log_warn(self: &SuttaBridge, message: &QString);

        #[qinvokable]
        fn log_error(self: &SuttaBridge, message: &QString);

        #[qinvokable]
        fn get_log_level(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn set_log_level(self: &SuttaBridge, level: &QString) -> bool;

        // --- Bookmark operations ---

        #[qinvokable]
        fn get_all_bookmark_folders_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_bookmark_items_for_folder_json(self: &SuttaBridge, folder_id: i32) -> QString;

        #[qinvokable]
        fn create_bookmark_folder(self: Pin<&mut SuttaBridge>, name: &QString) -> i32;

        #[qinvokable]
        fn create_bookmark_item(self: Pin<&mut SuttaBridge>, folder_id: i32, item_json: &QString) -> i32;

        #[qinvokable]
        fn update_bookmark_folder(self: Pin<&mut SuttaBridge>, folder_id: i32, name: &QString);

        #[qinvokable]
        fn update_bookmark_item(self: Pin<&mut SuttaBridge>, item_id: i32, item_json: &QString);

        #[qinvokable]
        fn delete_bookmark_folder(self: Pin<&mut SuttaBridge>, folder_id: i32);

        #[qinvokable]
        fn delete_bookmark_item(self: Pin<&mut SuttaBridge>, item_id: i32);

        #[qinvokable]
        fn reorder_bookmark_items(self: Pin<&mut SuttaBridge>, folder_id: i32, item_ids_json: &QString);

        #[qinvokable]
        fn reorder_bookmark_folders(self: Pin<&mut SuttaBridge>, folder_ids_json: &QString);

        #[qinvokable]
        fn move_bookmark_items_to_folder(self: Pin<&mut SuttaBridge>, item_ids_json: &QString, target_folder_id: i32);

        #[qinvokable]
        fn save_last_session(self: Pin<&mut SuttaBridge>, windows_json: &QString);

        #[qinvokable]
        fn get_last_session_json(self: &SuttaBridge) -> QString;

        #[qinvokable]
        fn get_restore_last_session(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_restore_last_session(self: Pin<&mut SuttaBridge>, value: bool);

        #[qinvokable]
        fn get_render_use_flat_results_background(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_render_use_flat_results_background(self: Pin<&mut SuttaBridge>, value: bool);

        #[qinvokable]
        fn get_render_disable_results_clip(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_render_disable_results_clip(self: Pin<&mut SuttaBridge>, value: bool);

        #[qinvokable]
        fn get_render_loop_basic(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_render_loop_basic(self: Pin<&mut SuttaBridge>, value: bool);

        // --- Snippet display settings ---

        #[qinvokable]
        fn get_snippet_chars_before(self: &SuttaBridge) -> u32;

        #[qinvokable]
        fn set_snippet_chars_before(self: Pin<&mut SuttaBridge>, value: u32);

        #[qinvokable]
        fn get_snippet_chars_after(self: &SuttaBridge) -> u32;

        #[qinvokable]
        fn set_snippet_chars_after(self: Pin<&mut SuttaBridge>, value: u32);

        #[qinvokable]
        fn get_snippet_all_chars_before(self: &SuttaBridge) -> u32;

        #[qinvokable]
        fn set_snippet_all_chars_before(self: Pin<&mut SuttaBridge>, value: u32);

        #[qinvokable]
        fn get_snippet_all_chars_after(self: &SuttaBridge) -> u32;

        #[qinvokable]
        fn set_snippet_all_chars_after(self: Pin<&mut SuttaBridge>, value: u32);

        #[qinvokable]
        fn get_item_height_use_default(self: &SuttaBridge) -> bool;

        #[qinvokable]
        fn set_item_height_use_default(self: Pin<&mut SuttaBridge>, value: bool);

        #[qinvokable]
        fn get_item_height_fixed(self: &SuttaBridge) -> u32;

        #[qinvokable]
        fn set_item_height_fixed(self: Pin<&mut SuttaBridge>, value: u32);
    }
}

/// Fold the startup report (recorded in `DbManager::new()` /
/// `ensure_no_empty_db_files()`) into a validation error message.
///
/// The three `*_first_query` validation functions own **both** invalidations
/// here, so the `database_validation_result` signal payload stays the single
/// source of truth and QML never has to post-mutate its results model.
///
/// - **File missing** beats everything: without it the downstream query reports
///   the misleading "Query returned 0 results" for a database that was never
///   there. A zero-byte stub also reads as missing (see
///   `docs/appdata-migration-mechanisms.md`).
/// - **Migration failed** otherwise. dpd has no migration folder, so its
///   outcome is always `NotApplicable` and only appdata/dictionaries can fail.
fn startup_report_error(kind: DbKind, label: &str) -> Option<String> {
    let report = get_startup_db_report();
    let entry = match kind {
        DbKind::Appdata => &report.appdata,
        DbKind::Dictionaries => &report.dictionaries,
        DbKind::Dpd => &report.dpd,
    };

    if entry.present_at_start == Some(false) {
        let msg = "Database file was missing".to_string();
        error(&format!("Database validation FAILED: {} - {}", label, msg));
        return Some(msg);
    }

    if let MigrationOutcome::Failed(err) = &entry.migration {
        let msg = format!("schema migration failed: {}", err);
        error(&format!("Database validation FAILED: {} - {}", label, msg));
        return Some(msg);
    }

    None
}

#[derive(Default)]
pub struct SuttaBridgeRust {
    db_loaded: bool,
    searcher_ready: bool,
    sutta_references_loaded: bool,
    /// Flag to track if topic index has been loaded
    topic_index_loaded: bool,
}


impl qobject::SuttaBridge {
    pub fn emit_update_window_title(self: Pin<&mut Self>, sutta_uid: QString, sutta_ref: QString, sutta_title: QString) {
        // info(&format!("emit_update_window_title(): {} {} {}", &sutta_uid.to_string(), &sutta_ref.to_string(), &sutta_title.to_string()));
        self.update_window_title(sutta_uid, sutta_ref, sutta_title);
    }

    pub fn emit_show_chapter_from_library(self: Pin<&mut Self>, window_id: QString, result_data_json: QString) {
        use crate::api::ffi;
        ffi::callback_show_chapter_in_sutta_window(window_id, result_data_json);
    }

    pub fn emit_show_sutta_from_reference_search(self: Pin<&mut Self>, window_id: QString, result_data_json: QString) {
        use crate::api::ffi;
        ffi::callback_show_sutta_from_reference_search(window_id, result_data_json);
    }

    pub fn load_db(self: Pin<&mut Self>) {
        info("SuttaBridge::load_db() start");
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            // FIXME: should init AppData if not alrerady
            // let r = db::rust_backend_init_db();
            let r = true;
            qt_thread.queue(move |mut qo| {
                qo.as_mut().set_db_loaded(r);
            }).unwrap();
            info("SuttaBridge::load_db() end");
        });
    }

    pub fn load_searcher(self: Pin<&mut Self>) {
        info("SuttaBridge::load_searcher() start");
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            // Opens the Tantivy indexes off the GUI thread. Idempotent —
            // `init_fulltext_searcher` checks `FULLTEXT_SEARCHER` is empty
            // before opening. `with_fulltext_searcher` returns `None` until
            // this completes, so callers that haven't gated on
            // `searcher_ready` still no-op safely.
            simsapa_backend::init_fulltext_searcher();
            qt_thread.queue(move |mut qo| {
                qo.as_mut().set_searcher_ready(true);
            }).unwrap();
            info("SuttaBridge::load_searcher() end");
        });
    }

    pub fn load_sutta_references(self: Pin<&mut Self>) {
        info("SuttaBridge::load_sutta_references() start");
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            simsapa_backend::init_sutta_references();
            qt_thread.queue(move |mut qo| {
                qo.as_mut().set_sutta_references_loaded(true);
            }).unwrap();
            info("SuttaBridge::load_sutta_references() end");
        });
    }

    /// Runs a db query so that db is cached from the disk. It should finish by
    /// the time the user types in the first actual query, and that will respond
    /// faster.
    pub fn appdata_first_query(self: Pin<&mut Self>) {
        info("SuttaBridge::appdata_first_query() start");

        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            // Check 0: was the file missing at startup, or did its schema
            // migrations fail? Both are folded into the result here.
            let mut error_message = startup_report_error(DbKind::Appdata, "Appdata").unwrap_or_default();

            if error_message.is_empty() {
                // Check 1: Database file exists (using try_exists() to avoid Android permission crashes)
                let db_path = get_app_globals().paths.appdata_db_path.clone();
                match db_path.try_exists() {
                    Ok(true) => {}, // File exists, continue
                    Ok(false) => {
                        error_message = "Database file not found".to_string();
                        error("Database validation FAILED: Appdata - Database file not found");
                    },
                    Err(e) => {
                        error_message = format!("Error checking file existence: {}", e);
                        error(&format!("Database validation FAILED: Appdata - Error checking file existence: {}", e));
                    }
                }
            }

            if error_message.is_empty() {
                // Check 2 & 3: Query executes and returns results
                let app_data = get_app_data();
                let params = SearchParams {
                    mode: SearchMode::ContainsMatch,
                    page_len: None,
                    lang: None,
                    lang_include: true,
                    source: None,
                    source_include: true,
                    enable_regex: false,
                    fuzzy_distance: 0,
                    include_cst_mula: true,
                    include_cst_commentary: true,
                    nikaya_prefix: None,
                    uid_prefix: None,
                    uid_suffix: None,
                    include_ms_mula: true,
                    include_comm_bold_definitions: true,
                    dict_source_uids: None,
                    show_all_snippets: false,
                    snippet_exclude: None,
                    deconstruction_selected_index: None,
                    deconstruction_locked: false,
                };

                let mut query_task = SearchQueryTask::new(
                    &app_data.dbm,
                    "dhamma".to_string(),
                    params,
                    SearchArea::Suttas,
                );

                match query_task.results_page(0) {
                    Ok(results) => {
                        if results.is_empty() {
                            error_message = "Query returned 0 results".to_string();
                            error("Database validation FAILED: Appdata - Query returned 0 results");
                        } else {
                            // Check 4: app_settings is readable from the same appdata DB.
                            // A corrupted row / unreadable connection here means the DB is
                            // compromised even if the sutta query above succeeded.
                            match app_data.dbm.appdata.get_conn() {
                                Ok(_) => {
                                    let _settings = app_data.dbm.appdata.get_app_settings();
                                    info("Database validation: Appdata OK");
                                }
                                Err(e) => {
                                    error_message = format!("App settings read error: {}", e);
                                    error(&format!("Database validation FAILED: Appdata - App settings read error: {}", e));
                                }
                            }
                        }
                    },
                    Err(e) => {
                        error_message = format!("Query error: {}", e);
                        error(&format!("Database validation FAILED: Appdata - Query error: {}", e));
                    }
                };
            }

            // Always emit signal with result (success or failure)
            let is_valid = error_message.is_empty();
            let message = if is_valid {
                QString::from("OK")
            } else {
                QString::from(error_message)
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().database_validation_result(QString::from("appdata"), is_valid, message);
            }).unwrap();

            info("SuttaBridge::appdata_first_query() end");
        });
    }

    pub fn dpd_first_query(self: Pin<&mut Self>) {
        info("SuttaBridge::dpd_first_query() start");

        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            // Check 0: was the file missing at startup? (dpd has no migration
            // folder, so its migration outcome is always NotApplicable.)
            let mut error_message = startup_report_error(DbKind::Dpd, "DPD").unwrap_or_default();

            if error_message.is_empty() {
                // Check 1: Database file exists (using try_exists() to avoid Android permission crashes)
                let db_path = get_app_globals().paths.dpd_db_path.clone();
                match db_path.try_exists() {
                    Ok(true) => {}, // File exists, continue
                    Ok(false) => {
                        error_message = "Database file not found".to_string();
                        error("Database validation FAILED: DPD - Database file not found");
                    },
                    Err(e) => {
                        error_message = format!("Error checking file existence: {}", e);
                        error(&format!("Database validation FAILED: DPD - Error checking file existence: {}", e));
                    }
                }
            }

            if error_message.is_empty() {
                // Check 2 & 3: Query executes and returns results
                let app_data = get_app_data();
                let json = app_data.dbm.dpd.dpd_lookup_json("dhamma");

                // dpd_lookup_json returns a JSON array string, check if it contains results
                if json == "[]" || json.is_empty() {
                    error_message = "Query returned 0 results".to_string();
                    error("Database validation FAILED: DPD - Query returned 0 results");
                } else {
                    info("Database validation: DPD OK");
                }
            }

            // Always emit signal with result (success or failure)
            let is_valid = error_message.is_empty();
            let message = if is_valid {
                QString::from("OK")
            } else {
                QString::from(error_message)
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().database_validation_result(QString::from("dpd"), is_valid, message);
            }).unwrap();

            info("SuttaBridge::dpd_first_query() end");
        });
    }

    pub fn dictionary_first_query(self: Pin<&mut Self>) {
        info("SuttaBridge::dictionary_first_query() start");

        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            // Check 0: was the file missing at startup, or did its schema
            // migrations fail? Both are folded into the result here.
            let mut error_message = startup_report_error(DbKind::Dictionaries, "Dictionaries").unwrap_or_default();

            if error_message.is_empty() {
                // Check 1: Database file exists (using try_exists() to avoid Android permission crashes)
                let db_path = get_app_globals().paths.dict_db_path.clone();
                match db_path.try_exists() {
                    Ok(true) => {}, // File exists, continue
                    Ok(false) => {
                        error_message = "Database file not found".to_string();
                        error("Database validation FAILED: Dictionaries - Database file not found");
                    },
                    Err(e) => {
                        error_message = format!("Error checking file existence: {}", e);
                        error(&format!("Database validation FAILED: Dictionaries - Error checking file existence: {}", e));
                    }
                }
            }

            if error_message.is_empty() {
                // Check 2 & 3: Query executes and returns results
                let app_data = get_app_data();
                let word = app_data.dbm.dictionaries.get_word("anidassana/dpd");

                match word {
                    Some(_) => {
                        info("Database validation: Dictionaries OK");
                    },
                    None => {
                        error_message = "Query returned 0 results".to_string();
                        error("Database validation FAILED: Dictionaries - Query returned 0 results");
                    },
                }
            }

            // Always emit signal with result (success or failure)
            let is_valid = error_message.is_empty();
            let message = if is_valid {
                QString::from("OK")
            } else {
                QString::from(error_message)
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().database_validation_result(QString::from("dictionaries"), is_valid, message);
            }).unwrap();

            info("SuttaBridge::dictionary_first_query() end");
        });
    }

    pub fn reset_app_settings_to_defaults(mut self: Pin<&mut Self>) -> bool {
        info("SuttaBridge::reset_app_settings_to_defaults() start");
        let app_data = get_app_data();
        match app_data.reset_app_settings_to_defaults() {
            Ok(_) => {
                self.as_mut().app_settings_reset();
                info("SuttaBridge::reset_app_settings_to_defaults() complete");
                true
            }
            Err(e) => {
                error(&format!("Failed to reset app settings: {}", e));
                false
            }
        }
    }

    pub fn query_text_to_uid_field_query(&self, query_text: &QString) -> QString {
        QString::from(query_text_to_uid_field_query(&query_text.to_string()))
    }

    /// Convert a verse reference (e.g., "dhp33", "thag50", "thig12") to its proper sutta UID.
    /// Returns the converted UID if it's a verse reference, otherwise returns the original string.
    pub fn convert_verse_ref_to_uid(&self, sutta_ref: &QString) -> QString {
        use simsapa_backend::helpers::verse_sutta_ref_to_uid;

        let ref_str = sutta_ref.to_string();

        // Try to convert verse reference to UID
        if let Some(converted_uid) = verse_sutta_ref_to_uid(&ref_str) {
            QString::from(converted_uid)
        } else {
            // Not a verse reference, return original
            sutta_ref.clone()
        }
    }

    pub fn results_page(self: Pin<&mut Self>, query: &QString, page_num: usize, search_area: &QString, params_json: &QString) {
        info(&format!("SuttaBridge::results_page() start - query='{}', page_num={}, search_area='{}'", query, page_num, search_area));
        let qt_thread = self.qt_thread();

        let query_text = query.to_string();
        let search_area_text = search_area.to_string();
        let params_json_text = params_json.to_string();
        info(&format!("params_json: {}", params_json_text));

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            // Combined+Dictionary is bridge-orchestrated: fan out DPD Lookup
            // and Fulltext Match in parallel and serve a merged virtual stream
            // through `fetch_combined_page`, with its own isolated cache.
            // Detect that case up front and dispatch separately so the
            // standard `RESULTS_PAGE_CACHE` flow stays single-mode.
            let parsed_params = serde_json::from_str::<SearchParams>(&params_json_text)
                .unwrap_or_default();
            let is_combined_dict = search_area_text == "Dictionary"
                && matches!(parsed_params.mode, SearchMode::Combined);

            // Grouped deconstructor break-downs for the original query, attached
            // to the result page on the Dictionary DPD Lookup / Combined-remap
            // path so FulltextResults can show a break-down selector. Cloned
            // into each SearchResultPage. `deconstructor_exact_only = false`
            // mirrors WordSummary's fuzzy break-down list. Empty for other
            // paths.
            //
            // This goes through the shared memo with the *same arguments* the
            // query task's `dpd_lookup_full()` uses — the normalized query text
            // and the raw uid_prefix / uid_suffix (dpd_lookup_grouped builds
            // its own LIKE patterns; pre-built ones would be double-wrapped).
            // The break-downs the user sees and the uids the backend lock
            // filter keeps must come from one and the same lookup, or the
            // selector can offer a break-down whose components were filtered
            // out of the results. The memo also collapses the 3–6 grouped
            // lookups a page request would otherwise run (selector, query task,
            // Combined DPD sub-query thread, every prefetched page) into one.
            let (page_deconstructions, page_direct_uids) = if search_area_text == "Dictionary"
                && matches!(parsed_params.mode, SearchMode::DpdLookup | SearchMode::Combined)
            {
                let app_data = get_app_data();
                let normalized_query = normalize_query_text(Some(query_text.clone()));
                match app_data.dbm.dpd.dpd_lookup_grouped_memo(
                    &normalized_query,
                    false,
                    true,
                    false,
                    parsed_params.uid_prefix.as_deref(),
                    parsed_params.uid_suffix.as_deref(),
                ) {
                    Ok(grouped) => (grouped.deconstructions.clone(), grouped.direct_uids.clone()),
                    Err(e) => {
                        error(&format!("dpd_lookup_grouped for result page failed: {}", e));
                        (Vec::new(), Vec::new())
                    }
                }
            } else {
                (Vec::new(), Vec::new())
            };

            if is_combined_dict {
                // PRD §6.6: distinct `|combined` suffix prevents any chance
                // of colliding with `RESULTS_PAGE_CACHE` keys.
                let cache_key = format!(
                    "{}|{}|{}|combined",
                    query_text, search_area_text, params_json_text
                );

                // Reset the combined cache cell if the key has changed (new
                // search) or if it was never initialized. `fetch_combined_page`
                // itself never resets — only this top-level entry does — so
                // stale prefetcher threads from a previous search can't clobber
                // the live cache while a cold-start join is in flight.
                //
                // The key embeds `params_json_text`, which now carries the
                // break-down selection index and lock state, so toggling the
                // lock or picking a different break-down changes the key and
                // resets the cell — the DPD side is rebuilt at its new,
                // filtered length. The same cache_key re-check that protects
                // against a previous *search*'s prefetch thread also covers a
                // previous *lock state*'s: both `fetch_combined_page` and
                // `fetch_and_cache_page` re-check the key after every unlocked
                // sub-query and return `Ok(None)` on mismatch.
                {
                    let mut guard = COMBINED_CACHE.lock().unwrap();
                    let needs_reset = match *guard {
                        Some(ref c) => c.cache_key != cache_key,
                        None => true,
                    };
                    if needs_reset {
                        *guard = Some(CombinedCache {
                            cache_key: cache_key.clone(),
                            page_len: 0,
                            dpd_buffer: Vec::new(),
                            dpd_total: None,
                            dpd_pages_fetched: 0,
                            ft_buffer: Vec::new(),
                            ft_total: None,
                            ft_pages_fetched: 0,
                        });
                    }
                }

                match fetch_combined_page(&cache_key, &query_text, &params_json_text, page_num) {
                    Ok(Some((results, total_hits, page_len))) => {
                        let results_page_data = SearchResultPage {
                            total_hits: total_hits as usize,
                            page_len,
                            page_num,
                            results,
                            deconstructions: page_deconstructions.clone(),
                            direct_uids: page_direct_uids.clone(),
                        };
                        let json = serde_json::to_string(&results_page_data).unwrap_or_default();
                        qt_thread.queue(move |mut qo| {
                            qo.as_mut().results_page_ready(QString::from(json));
                        }).unwrap();

                        let total_pages = if page_len > 0 {
                            (total_hits as usize).div_ceil(page_len)
                        } else {
                            0
                        };

                        if page_num + 1 < total_pages {
                            let _ = fetch_combined_page(
                                &cache_key,
                                &query_text,
                                &params_json_text,
                                page_num + 1,
                            );

                            if page_num + 2 < total_pages {
                                prefetch_pages(
                                    cache_key.clone(),
                                    query_text.clone(),
                                    search_area_text.clone(),
                                    params_json_text.clone(),
                                    page_num + 2,
                                    2,
                                    total_pages,
                                );
                            }
                        }

                        info("SuttaBridge::results_page() end (combined)");
                    }
                    Ok(None) => {
                        info("SuttaBridge::results_page() aborted (new search started; combined)");
                    }
                    Err(e) => {
                        error(&e.to_string());
                        // Only reset the cache if cold start never completed.
                        // On a top-up failure (both totals already populated),
                        // keep the partial buffers — the next user action
                        // retries the same top-up path and previously-served
                        // pages remain consistent.
                        {
                            let mut g = COMBINED_CACHE.lock().unwrap();
                            let cold_start_failed = match g.as_ref() {
                                Some(c) => c.cache_key != cache_key
                                    || c.dpd_total.is_none()
                                    || c.ft_total.is_none(),
                                None => true,
                            };
                            if cold_start_failed {
                                *g = None;
                            }
                        }
                        let error_json = serde_json::json!({"error": format!("{}", e)}).to_string();
                        qt_thread.queue(move |mut qo| {
                            qo.as_mut().results_page_ready(QString::from(error_json));
                        }).unwrap();
                    }
                }
                return;
            }

            // Build a cache key from the query, search area, and params.
            // CST mula/commentary settings are included in params_json, as are
            // show_all_snippets / snippet_exclude (they live on SearchParams),
            // so toggling either invalidates cached pages automatically — no
            // extra key plumbing. See docs/search-snippet-highlight-pipeline.md.
            let cache_key = format!("{}|{}|{}", query_text, search_area_text, params_json_text);

            // Check cache for a hit
            {
                let cache_guard = RESULTS_PAGE_CACHE.lock().unwrap();
                #[allow(clippy::collapsible_if)]
                if let Some(ref cache) = *cache_guard {
                    if cache.cache_key == cache_key {
                        if let Some(cached_results) = cache.pages.get(&page_num) {
                            info(&format!("Cache hit for page_num={}", page_num));
                            let results_page = SearchResultPage {
                                total_hits: cache.total_hits as usize,
                                page_len: cache.page_len,
                                page_num,
                                results: cached_results.clone(),
                                deconstructions: page_deconstructions.clone(),
                                direct_uids: page_direct_uids.clone(),
                            };
                            let json = serde_json::to_string(&results_page).unwrap_or_default();
                            qt_thread.queue(move |mut qo| {
                                qo.as_mut().results_page_ready(QString::from(json));
                            }).unwrap();

                            // If the user reached the highest cached page, prefetch the next 2
                            let max_cached = cache.pages.keys().max().copied().unwrap_or(0);
                            let total_pages = if cache.page_len > 0 {
                                // Manually:
                                // ((cache.total_hits as usize) + cache.page_len - 1) / cache.page_len
                                // Safer with .div_ceil():
                                (cache.total_hits as usize).div_ceil(cache.page_len)
                            } else { 0 };

                            if page_num >= max_cached && max_cached + 1 < total_pages {
                                prefetch_pages(
                                    cache_key.clone(),
                                    query_text.clone(),
                                    search_area_text.clone(),
                                    params_json_text.clone(),
                                    max_cached + 1,
                                    2,
                                    total_pages,
                                );
                            }

                            info("SuttaBridge::results_page() end (cached)");
                            return;
                        }
                    }
                }
            }

            // Cache miss — initialize cache for new search
            {
                let mut cache_guard = RESULTS_PAGE_CACHE.lock().unwrap();
                let needs_reset = match *cache_guard {
                    Some(ref cache) => cache.cache_key != cache_key,
                    None => true,
                };
                if needs_reset {
                    *cache_guard = Some(ResultsPageCache {
                        cache_key: cache_key.clone(),
                        pages: HashMap::new(),
                        total_hits: 0,
                        page_len: 0,
                    });
                }
            }

            // Fetch the requested page
            match fetch_and_cache_page(&cache_key, &query_text, &search_area_text, &params_json_text, page_num) {
                Ok(Some((results, total_hits, page_len))) => {
                    let results_page_data = SearchResultPage {
                        total_hits: total_hits as usize,
                        page_len,
                        page_num,
                        results,
                        deconstructions: page_deconstructions.clone(),
                        direct_uids: page_direct_uids.clone(),
                    };
                    let json = serde_json::to_string(&results_page_data).unwrap_or_default();
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().results_page_ready(QString::from(json));
                    }).unwrap();

                    // Prefetch: next page immediately (so page+1 is ready), then 2 more in background
                    let total_pages = if page_len > 0 {
                        // Manually:
                        // (total_hits as usize + page_len - 1) / page_len
                        // .div_ceil() implements this and is overflow-safe: 
                        (total_hits as usize).div_ceil(page_len)
                    } else {
                        0
                    };

                    if page_num + 1 < total_pages {
                        // Fetch the next page in this thread (so it's ready quickly)
                        let _ = fetch_and_cache_page(&cache_key, &query_text, &search_area_text, &params_json_text, page_num + 1);

                        // Prefetch 2 more pages in background
                        if page_num + 2 < total_pages {
                            prefetch_pages(
                                cache_key.clone(),
                                query_text.clone(),
                                search_area_text.clone(),
                                params_json_text.clone(),
                                page_num + 2,
                                2,
                                total_pages,
                            );
                        }
                    }

                    info("SuttaBridge::results_page() end");
                }
                Ok(None) => {
                    // Cache key changed during fetch (new search started), do nothing
                    info("SuttaBridge::results_page() aborted (new search started)");
                }
                Err(e) => {
                    error(&e.to_string());
                    let error_json = serde_json::json!({"error": format!("{}", e)}).to_string();
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().results_page_ready(QString::from(error_json));
                    }).unwrap();
                }
            }
        });
    }

    pub fn debug_query(self: Pin<&mut Self>, query: &QString, search_area: &QString, params_json: &QString) {
        let qt_thread = self.qt_thread();

        let query_text = query.to_string();
        let search_area_text = search_area.to_string();
        let params_json_text = params_json.to_string();

        thread::spawn(move || {
            let params: SearchParams = serde_json::from_str(&params_json_text).unwrap_or_default();

            let mode = &params.mode;
            let is_fulltext = matches!(mode, SearchMode::FulltextMatch | SearchMode::Combined);

            if !is_fulltext {
                // For non-fulltext modes, return a parameter summary
                let mode_name = format!("{:?}", mode);
                let debug_text = format!(
                    "Search Mode: {}\nSearch Area: {}\nQuery: {}\nLanguage: {} (include: {})\nSource: {} (include: {})\nRegex: {}\nFuzzy Distance: {}",
                    mode_name,
                    search_area_text,
                    query_text,
                    params.lang.as_deref().unwrap_or("(all)"),
                    params.lang_include,
                    params.source.as_deref().unwrap_or("(all)"),
                    params.source_include,
                    params.enable_regex,
                    params.fuzzy_distance,
                );

                let json = serde_json::json!({"debug_text": debug_text}).to_string();
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().debug_query_ready(QString::from(json));
                }).unwrap();
                return;
            }

            // Fulltext mode: use backend debug_query
            use simsapa_backend::search::searcher::SearchFilters;

            let filters = SearchFilters {
                lang: params.lang.clone(),
                lang_include: params.lang_include,
                source_uid: params.source.clone(),
                source_include: params.source_include,
                nikaya_prefix: params.nikaya_prefix.clone(),
                uid_prefix: params.uid_prefix.clone(),
                uid_suffix: params.uid_suffix.clone(),
                sutta_ref: None,
                include_cst_mula: true,
                include_cst_commentary: true,
                include_ms_mula: params.include_ms_mula,
                include_bold_definitions: params.include_comm_bold_definitions,
                dict_source_uids: params.dict_source_uids.clone(),
                show_all_snippets: false,
            };

            // Normalize the query the same way the live FulltextMatch search
            // does, so the syntax check parses the identical string (otherwise
            // a bare `'` in `day's` would report a tantivy Syntax Error here
            // even though the real search normalizes it away).
            let normalized_query = normalize_fulltext_query(&query_text);

            let result = with_fulltext_searcher(|searcher| {
                searcher.debug_query(&normalized_query, &filters)
            });

            let json = match result {
                Some(Ok(debug_result)) => {
                    let mut j = serde_json::json!({"debug_text": debug_result.debug_text});
                    if let Some(parse_err) = debug_result.parse_error {
                        j["error"] = serde_json::Value::String(parse_err);
                    }
                    j.to_string()
                }
                Some(Err(e)) => {
                    serde_json::json!({
                        "error": format!("{}", e),
                        "debug_text": format!("Error running debug query: {}", e),
                    }).to_string()
                }
                None => {
                    serde_json::json!({
                        "debug_text": "Fulltext search indexes not available.",
                    }).to_string()
                }
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().debug_query_ready(QString::from(json));
            }).unwrap();
        });
    }

    pub fn extract_words(&self, text: &QString) -> QStringList {
        let words = extract_words(&text.to_string());
        let mut res = QStringList::default();
        for i in words {
            res.append(QString::from(i));
        }
        res
    }

    pub fn normalize_query_text(&self, text: &QString) -> QString {
        QString::from(normalize_query_text(Some(text.to_string())))
    }

    pub fn dpd_deconstructor_list(&self, query: &QString) -> QStringList {
        let app_data = get_app_data();
        let list = app_data.dbm.dpd.dpd_deconstructor_list(&query.to_string());
        let mut res = QStringList::default();
        for i in list {
            res.append(QString::from(i));
        }
        res
    }

    pub fn dpd_lookup_json(&self, query: &QString) -> QString {
        let app_data = get_app_data();
        let s = app_data.dbm.dpd.dpd_lookup_json(&query.to_string());
        QString::from(s)
    }

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

    /// Grouped, break-down-aware DPD lookup (PRD FR-A1/FR-B3). WordSummary and
    /// FulltextResults consume the grouped structure to render the break-down
    /// selector and lock-filter the result list. `deconstructor_exact_only` is
    /// `false` here to preserve WordSummary's fuzzy deconstructor list behavior.
    pub fn dpd_lookup_grouped_json_async(self: Pin<&mut Self>, query_id: &QString, query: &QString) {
        info("SuttaBridge::dpd_lookup_grouped_json_async() start");
        let qt_thread = self.qt_thread();
        let query_id_string = query_id.to_string();
        let query_text = query.to_string();

        // Spawn a thread so Qt event loop is not blocked
        thread::spawn(move || {
            let app_data = get_app_data();
            let s = app_data.dbm.dpd.dpd_lookup_grouped_json(&query_text, false);
            let grouped_json = QString::from(s);
            let query_id_qstring = QString::from(query_id_string);

            qt_thread.queue(move |mut qo| {
                qo.as_mut().dpd_lookup_grouped_ready(query_id_qstring, grouped_json);
            }).unwrap();

            info("SuttaBridge::dpd_lookup_grouped_json_async() end");
        });
    }

    pub fn get_sutta_html(&self, window_id: &QString, uid: &QString) -> QString {
        let app_data = get_app_data();
        // Default to not showing references when called directly from QML
        let html = app_data.render_sutta_html_by_uid(&window_id.to_string(), &uid.to_string(), false);
        QString::from(html)
    }

    pub fn get_word_html(&self, window_id: &QString, uid: &QString) -> QString {
        let app_data = get_app_data();
        let html = app_data.render_word_html_by_uid(&window_id.to_string(), &uid.to_string());
        QString::from(html)
    }

    pub fn get_translations_data_json_for_sutta_uid(&self, sutta_uid: &QString) -> QString {
        let app_data = get_app_data();
        let include_cst_commentary = app_data.get_include_cst_commentary_in_translations();
        let include_cst_mula = app_data.get_include_cst_mula_in_translations();
        let r = app_data.dbm.appdata.get_translations_data_json_for_sutta_uid(
            &sutta_uid.to_string(),
            include_cst_commentary,
            include_cst_mula,
        );
        QString::from(r)
    }

    pub fn find_related_sutta_json(&self, sutta_uid: &QString, relation: &QString) -> QString {
        let app_data = get_app_data();
        let r = app_data.dbm.appdata.find_related_sutta_json(
            &sutta_uid.to_string(),
            &relation.to_string(),
        );
        QString::from(r)
    }

    pub fn qt_version(&self) -> QString {
        qobject::get_qt_version()
    }

    pub fn app_data_folder_path(&self) -> QString {
        let p = get_create_simsapa_dir().unwrap_or(PathBuf::from("."));
        let app_data_path = p.as_os_str();
        let s = app_data_path.to_str().unwrap_or("Path error");
        QString::from(s)
    }

    pub fn is_app_data_folder_writable(&self) -> bool {
        let p = get_create_simsapa_dir().unwrap_or(PathBuf::from("."));
        let md = match fs::metadata(p) {
            Ok(x) => x,
            Err(_) => return false,
        };
        let permissions = md.permissions();
        let read_only = permissions.readonly();
        !read_only
    }

    pub fn app_data_contents_html_table(&self) -> QString {
        let p = get_create_simsapa_dir().unwrap_or(PathBuf::from("."));
        let app_data_path = p.to_string_lossy();
        let app_data_folder_contents = generate_html_directory_listing(&app_data_path, 3).unwrap_or(String::from("Error"));
        QString::from(app_data_folder_contents)
    }

    pub fn app_data_contents_plain_table(&self) -> QString {
        let p = get_create_simsapa_dir().unwrap_or(PathBuf::from("."));
        let app_data_path = p.to_string_lossy();
        let app_data_folder_contents = generate_plain_directory_listing(&app_data_path, 3).unwrap_or(String::from("Error"));
        QString::from(app_data_folder_contents)
    }

    /// Get list of log files as JSON array of filenames
    pub fn get_log_files_list(&self) -> QString {
        let data_dir = match get_create_simsapa_dir() {
            Ok(d) => d,
            Err(_) => return QString::from("[]"),
        };

        // Find all log files (log.txt and log.*.txt)
        let mut log_files: Vec<String> = Vec::new();

        // Add current log.txt if it exists
        let current_log = data_dir.join("log.txt");
        if let Ok(true) = current_log.try_exists() { log_files.push("log.txt".to_string()) }

        // Find all rotated log files
        if let Ok(entries) = fs::read_dir(&data_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                    && filename.starts_with("log.") && filename.ends_with(".txt") && filename != "log.txt" {
                        log_files.push(filename.to_string());
                    }
            }
        }

        // Sort so current log.txt is first, then rotated logs in reverse chronological order
        log_files.sort_by(|a, b| {
            if a == "log.txt" {
                std::cmp::Ordering::Less
            } else if b == "log.txt" {
                std::cmp::Ordering::Greater
            } else {
                b.cmp(a) // Reverse sort for dated logs
            }
        });

        // Convert to JSON array
        let json = serde_json::to_string(&log_files).unwrap_or_else(|_| "[]".to_string());
        QString::from(json)
    }

    /// Read the contents of a log file
    pub fn get_log_file_contents(&self, file_name: &QString) -> QString {
        let file_name_str = file_name.to_string();

        // Security: only allow reading log files
        if !file_name_str.starts_with("log.") && file_name_str != "log.txt" {
            return QString::from("Invalid file name");
        }

        let data_dir = match get_create_simsapa_dir() {
            Ok(d) => d,
            Err(_) => return QString::from("Error: Could not get data directory"),
        };

        let file_path = data_dir.join(&file_name_str);

        match fs::read_to_string(&file_path) {
            Ok(contents) => QString::from(contents),
            Err(e) => QString::from(format!("Error reading file: {}", e)),
        }
    }

    /// Get the full path to a log file for opening with external apps
    pub fn get_log_file_path(&self, file_name: &QString) -> QString {
        let file_name_str = file_name.to_string();

        // Security: only allow log files
        if !file_name_str.starts_with("log.") && file_name_str != "log.txt" {
            return QString::from("");
        }

        let data_dir = match get_create_simsapa_dir() {
            Ok(d) => d,
            Err(_) => return QString::from(""),
        };

        let file_path = data_dir.join(&file_name_str);
        QString::from(file_path.to_string_lossy().as_ref())
    }

    /// Get the current theme setting, 'system', 'light', or 'dark'
    pub fn get_theme_name(&self) -> QString {
        let app_data = get_app_data();
        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        QString::from(app_settings.theme_name_as_string())
    }

    /// Save the theme setting in the db
    pub fn set_theme_name(self: Pin<&mut Self>, theme_name: &QString) {
        let app_data = get_app_data();
        app_data.set_theme_name(&theme_name.to_string());
    }

    /// Get the AI models auto retry setting
    pub fn get_ai_models_auto_retry(&self) -> bool {
        let app_data = get_app_data();
        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.ai_models_auto_retry
    }

    /// Save the AI models auto retry setting in the db
    pub fn set_ai_models_auto_retry(self: Pin<&mut Self>, auto_retry: bool) {
        let app_data = get_app_data();
        app_data.set_ai_models_auto_retry(auto_retry);
    }

    /// Get the auto-fallback-to-next-model setting
    pub fn get_ai_auto_fallback(&self) -> bool {
        let app_data = get_app_data();
        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.ai_auto_fallback
    }

    /// Save the auto-fallback-to-next-model setting in the db
    pub fn set_ai_auto_fallback(self: Pin<&mut Self>, auto_fallback: bool) {
        let app_data = get_app_data();
        app_data.set_ai_auto_fallback(auto_fallback);
    }

    /// Get the Gloss tab AI translation mode ("sequential_retry" | "parallel")
    pub fn get_gloss_ai_translate_mode(&self) -> QString {
        let app_data = get_app_data();
        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        QString::from(app_settings.gloss_ai_translate_mode.as_str())
    }

    /// Save the Gloss tab AI translation mode in the db
    pub fn set_gloss_ai_translate_mode(self: Pin<&mut Self>, mode: &QString) {
        let app_data = get_app_data();
        app_data.set_gloss_ai_translate_mode(AiRequestMode::from_str_or_default(&mode.to_string()));
    }

    /// Get the Prompts tab request mode ("sequential_retry" | "parallel")
    pub fn get_prompts_request_mode(&self) -> QString {
        let app_data = get_app_data();
        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        QString::from(app_settings.prompts_request_mode.as_str())
    }

    /// Save the Prompts tab request mode in the db
    pub fn set_prompts_request_mode(self: Pin<&mut Self>, mode: &QString) {
        let app_data = get_app_data();
        app_data.set_prompts_request_mode(AiRequestMode::from_str_or_default(&mode.to_string()));
    }

    /// Get a specific API key by name
    pub fn get_api_key(&self, key_name: &QString) -> QString {
        let app_data = get_app_data();
        let key = app_data.get_api_key(&key_name.to_string());
        QString::from(key)
    }

    /// Save API keys in the db as JSON
    pub fn set_api_keys(self: Pin<&mut Self>, api_keys_json: &QString) {
        let app_data = get_app_data();
        app_data.set_api_keys(&api_keys_json.to_string());
    }

    /// Get a specific system prompt by name
    pub fn get_system_prompt(&self, prompt_name: &QString) -> QString {
        let app_data = get_app_data();
        let prompt = app_data.get_system_prompt(&prompt_name.to_string());
        QString::from(prompt)
    }

    /// Get the built-in default text of a system prompt by name.
    /// Returns an empty string when the key has no built-in default.
    pub fn get_default_system_prompt(&self, prompt_name: &QString) -> QString {
        let prompts = simsapa_backend::app_settings::default_system_prompts();
        let prompt = prompts.get(&prompt_name.to_string()).cloned().unwrap_or_default();
        QString::from(prompt)
    }

    /// Save system prompts in the db as JSON
    pub fn set_system_prompts_json(self: Pin<&mut Self>, prompts_json: &QString) {
        let app_data = get_app_data();
        app_data.set_system_prompts_json(&prompts_json.to_string());
    }

    /// Get all system prompts as JSON
    pub fn get_system_prompts_json(&self) -> QString {
        let app_data = get_app_data();
        let prompts_json = app_data.get_system_prompts_json();
        QString::from(prompts_json)
    }

    /// Get the Gloss tab's AI word-selection settings as JSON
    /// (`{"enabled": bool, "provider": "...", "model": "..."}`).
    pub fn get_gloss_word_selection_settings_json(&self) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_gloss_word_selection_settings_json())
    }

    /// Save the Gloss tab's AI word-selection settings from JSON.
    pub fn set_gloss_word_selection_settings_json(self: Pin<&mut Self>, settings_json: &QString) {
        let app_data = get_app_data();
        app_data.set_gloss_word_selection_settings_json(&settings_json.to_string());
    }

    /// Save (upsert) a gloss word-selection cache row. `word` is the glossed
    /// surface form (`ProcessedWord.original_word`), `context_snippet` the
    /// word's context window (`example_sentence`, `<b>` markers allowed);
    /// key normalization and hashing happen here. Respects origin precedence
    /// (an `ai` write never downgrades a `user` or `built-in` row).
    pub fn save_gloss_word_cache(&self, word: &QString, context_snippet: &QString, selected_uid: &QString, origin: &QString) -> bool {
        use simsapa_backend::helpers::{gloss_cache_word_key, gloss_context_hash, normalize_gloss_context};
        let app_data = get_app_data();
        let word_key = gloss_cache_word_key(&word.to_string());
        let snippet = context_snippet.to_string();
        let hash = gloss_context_hash(&normalize_gloss_context(&snippet));
        match app_data.dbm.appdata.upsert_gloss_word_cache(
            &word_key,
            &hash,
            &snippet,
            &selected_uid.to_string(),
            &origin.to_string(),
        ) {
            Ok(written) => written,
            Err(e) => {
                error(&format!("save_gloss_word_cache(): {}", e));
                false
            }
        }
    }

    /// Save the compound's own cache row for a deconstructor-resolved word: the
    /// chosen break-down display string (`words_joined`) is stored in the
    /// `deconstruction` column with an empty `selected_uid`. `context_snippet`
    /// is the compound occurrence's context window (its hash keys the row, and
    /// the compound's component-sense rows share this hash). See PRD FR-C5.
    pub fn save_gloss_word_deconstruction_cache(&self, word: &QString, context_snippet: &QString, deconstruction: &QString, origin: &QString) -> bool {
        use simsapa_backend::helpers::{gloss_cache_word_key, gloss_context_hash, normalize_gloss_context};
        let app_data = get_app_data();
        let word_key = gloss_cache_word_key(&word.to_string());
        let snippet = context_snippet.to_string();
        let hash = gloss_context_hash(&normalize_gloss_context(&snippet));
        match app_data.dbm.appdata.upsert_gloss_word_deconstruction(
            &word_key,
            &hash,
            &snippet,
            &deconstruction.to_string(),
            &origin.to_string(),
        ) {
            Ok(written) => written,
            Err(e) => {
                error(&format!("save_gloss_word_deconstruction_cache(): {}", e));
                false
            }
        }
    }

    /// Delete the cache row for (word, context_hash). `word` may be the raw
    /// surface form (key-normalized here); `context_hash` is the stored hash.
    pub fn delete_gloss_word_cache(&self, word: &QString, context_hash: &QString) -> bool {
        use simsapa_backend::helpers::gloss_cache_word_key;
        let app_data = get_app_data();
        let word_key = gloss_cache_word_key(&word.to_string());
        match app_data.dbm.appdata.delete_gloss_word_cache(&word_key, &context_hash.to_string()) {
            Ok(()) => true,
            Err(e) => {
                error(&format!("delete_gloss_word_cache(): {}", e));
                false
            }
        }
    }

    /// Count of user-clearable (`ai` + `user`) word-selection cache rows.
    pub fn gloss_word_cache_count(&self) -> i32 {
        let app_data = get_app_data();
        app_data.dbm.appdata.count_gloss_word_cache() as i32
    }

    /// Bulk clear of `ai` + `user` cache rows (`built-in` rows and the phrase
    /// table are untouched).
    pub fn clear_gloss_word_cache(&self) -> bool {
        let app_data = get_app_data();
        match app_data.dbm.appdata.clear_gloss_word_cache() {
            Ok(()) => true,
            Err(e) => {
                error(&format!("clear_gloss_word_cache(): {}", e));
                false
            }
        }
    }

    /// Re-derive `resolution` / `selected_index` / `context_hash` for a
    /// restored session's words_data JSON from the current cache and phrase
    /// tables (see `simsapa_backend::helpers::annotate_gloss_words_json`).
    /// Returns the input unchanged when annotation fails.
    pub fn annotate_gloss_words_json(&self, words_data_json: &QString) -> QString {
        let app_data = get_app_data();
        let words_json = words_data_json.to_string();
        match simsapa_backend::helpers::annotate_gloss_words_json(&app_data.dbm.appdata, &words_json) {
            Ok(annotated) => QString::from(annotated),
            Err(e) => {
                error(&format!("annotate_gloss_words_json(): {}", e));
                words_data_json.clone()
            }
        }
    }

    /// Build the gloss session JSON export envelope (PRD §4.9 req 38): the
    /// session serialization the Gloss history saves, plus the word-selection
    /// cache rows referenced by the session's words. Returns an empty string
    /// on failure.
    pub fn export_gloss_session_json(&self, session_json: &QString) -> QString {
        let app_data = get_app_data();
        match simsapa_backend::helpers::build_gloss_session_export_json(
            &app_data.dbm.appdata,
            &session_json.to_string(),
        ) {
            Ok(json) => QString::from(json),
            Err(e) => {
                error(&format!("export_gloss_session_json(): {}", e));
                QString::from("")
            }
        }
    }

    /// Import word-selection cache entries (a session export's `word_cache`
    /// array) with the strictly-higher precedence rule (`user > built-in >
    /// ai`; equal precedence is a no-op). Returns
    /// `{"imported": n, "skipped": m}` or `{"error": "..."}`.
    pub fn import_gloss_word_cache(&self, entries_json: &QString) -> QString {
        use simsapa_backend::helpers::{import_gloss_word_cache_entries, GlossWordCacheExportEntry};
        let entries: Vec<GlossWordCacheExportEntry> =
            match serde_json::from_str(&entries_json.to_string()) {
                Ok(v) => v,
                Err(e) => {
                    return QString::from(
                        serde_json::json!({"error": format!("Invalid entries JSON: {}", e)}).to_string(),
                    );
                }
            };
        let app_data = get_app_data();
        let (imported, skipped) = import_gloss_word_cache_entries(&app_data.dbm.appdata, &entries);
        QString::from(serde_json::json!({"imported": imported, "skipped": skipped}).to_string())
    }

    /// Open a gloss session JSON export from a local file path ("Open JSON",
    /// PRD §4.9 reqs 39-41): validate the envelope, import its `word_cache`
    /// with the strict-precedence upsert, and return
    /// `{"ok": true, "session": {...}, "imported": n, "skipped": m}` or
    /// `{"error": "..."}`. A malformed or wrong-format file imports nothing.
    /// On Android the caller converts a `content://` URI to a temp file first
    /// (`copy_content_uri_to_temp`).
    pub fn open_gloss_session_export(&self, file_path: &QString) -> QString {
        let err_json =
            |msg: String| QString::from(serde_json::json!({"error": msg}).to_string());

        let path = file_path.to_string();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return err_json(format!("Failed to read the file: {}", e)),
        };

        let (session, word_cache) =
            match simsapa_backend::helpers::parse_gloss_session_export(&content) {
                Ok(v) => v,
                Err(e) => return err_json(e),
            };

        let app_data = get_app_data();
        let (imported, skipped) = simsapa_backend::helpers::import_gloss_word_cache_entries(
            &app_data.dbm.appdata,
            &word_cache,
        );

        QString::from(
            serde_json::json!({
                "ok": true,
                "session": session,
                "imported": imported,
                "skipped": skipped,
            })
            .to_string(),
        )
    }

    /// Parse and validate an AI word-selection response against the request's
    /// items array (see `simsapa_backend::helpers::parse_word_selection_response`;
    /// lenient mode — invalid entries are logged and skipped). Returns
    /// `{"selections": [{"id", "uid", "confidence", "note"?}]}` on success or
    /// `{"error": "..."}` on failure (incl. in-band `Error:` responses).
    pub fn parse_word_selection_response(&self, response: &QString, expected_items_json: &QString) -> QString {
        use simsapa_backend::helpers::WordSelectionParseMode;
        match simsapa_backend::helpers::parse_word_selection_response(
            &response.to_string(),
            &expected_items_json.to_string(),
            WordSelectionParseMode::Lenient,
        ) {
            Ok(entries) => {
                QString::from(serde_json::json!({"selections": entries}).to_string())
            }
            Err(e) => QString::from(serde_json::json!({"error": e}).to_string()),
        }
    }

    /// Build the shared `pali_word_selection` request items for the GlossTab
    /// network path (see `simsapa_backend::helpers::build_word_selection_items`).
    /// `paragraphs_json` is an array of `{paragraph_index, words_json}`
    /// objects; `forced` re-includes `ai-selected`-resolved words. Returns the
    /// items JSON array, or `"[]"` on error (logged).
    pub fn build_word_selection_items_json(&self, paragraphs_json: &QString, forced: bool) -> QString {
        use simsapa_backend::helpers::{build_word_selection_items, WordSelectionBuildMode, WordSelectionParagraphInput};
        let paragraphs: Vec<WordSelectionParagraphInput> =
            match serde_json::from_str(&paragraphs_json.to_string()) {
                Ok(p) => p,
                Err(e) => {
                    error(&format!("build_word_selection_items_json(): invalid paragraphs JSON: {}", e));
                    return QString::from("[]");
                }
            };
        match build_word_selection_items(&paragraphs, WordSelectionBuildMode::SkipResolved { forced }) {
            Ok(items) => QString::from(serde_json::json!(items).to_string()),
            Err(e) => {
                error(&format!("build_word_selection_items_json(): {}", e));
                QString::from("[]")
            }
        }
    }

    /// Get all providers as JSON
    pub fn get_providers_json(&self) -> QString {
        let app_data = get_app_data();
        let providers_json = app_data.get_providers_json();
        QString::from(providers_json)
    }

    /// Save providers in the db as JSON
    pub fn set_providers_json(self: Pin<&mut Self>, providers_json: &QString) {
        let app_data = get_app_data();
        app_data.set_providers_json(&providers_json.to_string());
    }

    /// Refresh every provider's model list from the keyless public sources on a
    /// background thread, save the result, and report via `modelListsUpdated`.
    ///
    /// The user's enabled set is authoritative here, so the default-enable
    /// heuristic is not applied (that is the CLI's job when regenerating the
    /// bundled `assets/providers.json`). See
    /// docs/ai-model-management-and-fallback.md.
    pub fn update_model_lists(self: Pin<&mut Self>) {
        info("SuttaBridge::update_model_lists() start");
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let app_data = get_app_data();
            let mut providers = app_data.get_providers();
            let report = update_all_provider_models(&mut providers, false);

            let success = !report.total_failure();
            if success {
                app_data.set_providers(providers);
            }

            let report_json = serde_json::to_string(&report).unwrap_or_default();
            info(&format!("SuttaBridge::update_model_lists() end: {}", report.summary()));

            let _ = qt_thread.queue(move |mut qo| {
                qo.as_mut().model_lists_updated(success, QString::from(&report_json));
            });
        });
    }

    /// Get API key for a specific provider
    pub fn get_provider_api_key(&self, provider_name: &QString) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_provider_api_key(&provider_name.to_string()))
    }

    /// Set API key for a specific provider
    pub fn set_provider_api_key(self: Pin<&mut Self>, provider_name: &QString, api_key: &QString) {
        let app_data = get_app_data();
        app_data.set_provider_api_key(&provider_name.to_string(), &api_key.to_string());
    }

    /// Get the API URL for the localhost server
    pub fn get_api_url(&self) -> QString {
        let app_data = get_app_data();
        QString::from(&app_data.api_url)
    }

    /// Get the status bar height in density-independent pixels (dp)
    /// Returns 0 on non-mobile platforms, actual height on Android
    pub fn get_status_bar_height(&self) -> i32 {
        use crate::api::ffi;
        ffi::get_status_bar_height()
    }

    /// Enable or disable a provider
    pub fn set_provider_enabled(self: Pin<&mut Self>, provider_name: &QString, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_provider_enabled(&provider_name.to_string(), enabled);
    }

    /// Add a new model to a provider (origin `user`)
    pub fn add_provider_model(self: Pin<&mut Self>, provider_name: &QString, model_name: &QString) {
        let app_data = get_app_data();
        app_data.add_provider_model(&provider_name.to_string(), &model_name.to_string());
    }

    /// Remove a user-added model from a provider
    pub fn remove_provider_model(self: Pin<&mut Self>, provider_name: &QString, model_name: &QString) {
        let app_data = get_app_data();
        app_data.remove_provider_model(&provider_name.to_string(), &model_name.to_string());
    }

    /// Set the enabled status of a specific model for a provider
    pub fn set_provider_model_enabled(self: Pin<&mut Self>, provider_name: &QString, model_name: &QString, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_provider_model_enabled(&provider_name.to_string(), &model_name.to_string(), enabled);
    }

    /// Get the provider name for a given model name
    pub fn get_provider_for_model(&self, model_name: &QString) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_provider_for_model(&model_name.to_string()))
    }

    /// The ordered "Fallback sequence" list. Seeded from the enabled models on
    /// first access. See docs/ai-model-management-and-fallback.md.
    pub fn get_ai_fallback_sequence_json(&self) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_ai_fallback_sequence_json())
    }

    /// Whole-list set, covering both reordering and the per-item toggles.
    pub fn set_ai_fallback_sequence_json(self: Pin<&mut Self>, entries_json: &QString) {
        let app_data = get_app_data();
        app_data.set_ai_fallback_sequence_json(&entries_json.to_string());
    }

    /// The unordered "Parallel prompts" list.
    pub fn get_ai_parallel_prompts_json(&self) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_ai_parallel_prompts_json())
    }

    pub fn set_ai_parallel_prompts_json(self: Pin<&mut Self>, entries_json: &QString) {
        let app_data = get_app_data();
        app_data.set_ai_parallel_prompts_json(&entries_json.to_string());
    }

    pub fn get_saved_theme(&self) -> QString {
        self.get_theme(&self.get_theme_name())
    }

    /// Get theme colors as JSON string
    pub fn get_theme(&self, theme_name: &QString) -> QString {
        let theme = theme_name.to_string();

        

        match theme.as_str() {
            "system" => qobject::get_system_palette_json(),
            "light" => QString::from(&ThemeColors::light_json()),
            "dark" => QString::from(&ThemeColors::dark_json()),
            _ => QString::from(serde_json::json!({}).to_string()),
        }
    }

    pub fn get_common_words_json(&self) -> QString {
        let app_data = get_app_data();
        let s = app_data.dbm.appdata.get_common_words_json();
        QString::from(s)
    }

    pub fn save_common_words_json(&self, words_json: &QString) {
        let app_data = get_app_data();
        match app_data.dbm.appdata.save_common_words_json(&words_json.to_string()) {
            Ok(_) => {},
            Err(e) => error(&format!("{}", e))
        }
    }

    // --- Gloss / Prompts history (shared, item_type-parameterised) ---
    //
    // The QML side passes `item_type` as a lowercase string ("gloss" |
    // "prompts"); we parse it to a `HistoryItemType` at the boundary so the rest
    // of the Rust path is type-safe. `data_json` is opaque text owned by the QML
    // tab. See tasks/2026-06-27-131935-prd---gloss-prompts-history.md.

    /// Builds the history list JSON: `[{id, modified, data}]` newest-first,
    /// where `data` is the stored `data_json` string (the GlossTab/PromptsTab
    /// `load_session` re-parses it). Emits `historyListReady(item_type, json)`.
    pub fn get_history_json_background(self: Pin<&mut Self>, item_type: &QString) {
        let item_type_str = item_type.to_string();
        let parsed: HistoryItemType = match item_type_str.parse() {
            Ok(t) => t,
            Err(e) => {
                error(&format!("get_history_json_background: {}", e));
                return;
            }
        };

        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let app_data = get_app_data();
            let items = app_data.dbm.appdata.get_history_for_type(parsed);

            let arr: Vec<serde_json::Value> = items
                .into_iter()
                .map(|it| {
                    let modified = it
                        .updated_at
                        .map(|t| t.to_string())
                        .unwrap_or_default();
                    serde_json::json!({
                        "id": it.id,
                        "modified": modified,
                        "data": it.data_json,
                    })
                })
                .collect();

            let json = serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string());
            let item_type_q = QString::from(&item_type_str);
            let json_q = QString::from(&json);
            qt_thread
                .queue(move |mut qo| {
                    qo.as_mut().history_list_ready(item_type_q, json_q);
                })
                .unwrap();
        });
    }

    /// INSERT (empty `session_id`) or UPDATE the current session off the UI
    /// thread. Skips genuinely empty `data_json`. Emits
    /// `historySaved(item_type, resolved_id)` with the resolved id as a string
    /// so QML keeps `current_session_id` as a string end-to-end.
    pub fn save_history_session_background(self: Pin<&mut Self>, item_type: &QString, session_id: &QString, data_json: &QString) {
        let item_type_str = item_type.to_string();
        let session_id_str = session_id.to_string();
        let data_json_str = data_json.to_string();

        let parsed: HistoryItemType = match item_type_str.parse() {
            Ok(t) => t,
            Err(e) => {
                error(&format!("save_history_session_background: {}", e));
                return;
            }
        };

        // Skip genuinely empty sessions so history is not polluted with blanks.
        if data_json_str.trim().is_empty() {
            return;
        }

        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let resolved_id = save_history_session_impl(parsed, &session_id_str, &data_json_str);
            // Always emit completion so the tab can clear its in-flight guard.
            // An empty resolved id signals failure (the tab keeps the session
            // dirty for the next retry and does not clobber current_session_id).
            let item_type_q = QString::from(&item_type_str);
            let id_q = QString::from(&resolved_id.unwrap_or_default());
            qt_thread
                .queue(move |mut qo| {
                    qo.as_mut().history_saved(item_type_q, id_q);
                })
                .unwrap();
        });
    }

    /// Synchronous INSERT/UPDATE for the app-close flush only (req 17): the
    /// write must complete before the process exits, so this runs on the caller
    /// thread and returns the resolved id directly (empty string on skip/error).
    pub fn save_history_session_blocking(&self, item_type: &QString, session_id: &QString, data_json: &QString) -> QString {
        let item_type_str = item_type.to_string();
        let session_id_str = session_id.to_string();
        let data_json_str = data_json.to_string();

        let parsed: HistoryItemType = match item_type_str.parse() {
            Ok(t) => t,
            Err(e) => {
                error(&format!("save_history_session_blocking: {}", e));
                return QString::from("");
            }
        };

        if data_json_str.trim().is_empty() {
            return QString::from("");
        }

        match save_history_session_impl(parsed, &session_id_str, &data_json_str) {
            Some(id_str) => QString::from(&id_str),
            None => QString::from(""),
        }
    }

    /// Deletes a single history row, then emits `historyChanged(item_type)` so
    /// the tab reloads its list.
    pub fn delete_history_item(self: Pin<&mut Self>, item_type: &QString, id: i32) {
        let item_type_str = item_type.to_string();
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let app_data = get_app_data();
            if let Err(e) = app_data.dbm.appdata.delete_history_item(id) {
                error(&format!("delete_history_item: {}", e));
            }
            let item_type_q = QString::from(&item_type_str);
            qt_thread
                .queue(move |mut qo| {
                    qo.as_mut().history_changed(item_type_q);
                })
                .unwrap();
        });
    }

    /// Clears all history rows for an item_type, then emits
    /// `historyChanged(item_type)`.
    pub fn clear_history(self: Pin<&mut Self>, item_type: &QString) {
        let item_type_str = item_type.to_string();
        let parsed: HistoryItemType = match item_type_str.parse() {
            Ok(t) => t,
            Err(e) => {
                error(&format!("clear_history: {}", e));
                return;
            }
        };

        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            let app_data = get_app_data();
            if let Err(e) = app_data.dbm.appdata.clear_history(parsed) {
                error(&format!("clear_history: {}", e));
            }
            let item_type_q = QString::from(&item_type_str);
            qt_thread
                .queue(move |mut qo| {
                    qo.as_mut().history_changed(item_type_q);
                })
                .unwrap();
        });
    }

    pub fn save_anki_csv(&self, _csv_content: &QString) -> QString {
        QString::from("file_name.csv")
    }

    pub fn save_file(&self,
                     folder_url: &QUrl,
                     filename: &QString,
                     content: &QString) -> bool {
        save_bytes_to_folder(folder_url, &filename.to_string(), content.to_string().as_bytes())
    }

    /// Generate a DOCX from the gloss export JSON and write it to the chosen
    /// folder (desktop path or Android SAF, same dispatch as `save_file`).
    pub fn export_gloss_docx(&self,
                             folder_url: &QUrl,
                             filename: &QString,
                             gloss_json: &QString) -> bool {
        let bytes = match simsapa_backend::docx_export::generate_gloss_docx(&gloss_json.to_string()) {
            Ok(bytes) => bytes,
            Err(e) => {
                error(&format!("export_gloss_docx failed to generate the document: {}", e));
                return false;
            }
        };
        save_bytes_to_folder(folder_url, &filename.to_string(), &bytes)
    }

    /// Generate a DOCX from the chat export JSON and write it to the chosen
    /// folder (desktop path or Android SAF, same dispatch as `save_file`).
    pub fn export_chat_docx(&self,
                            folder_url: &QUrl,
                            filename: &QString,
                            chat_json: &QString) -> bool {
        let bytes = match simsapa_backend::docx_export::generate_chat_docx(&chat_json.to_string()) {
            Ok(bytes) => bytes,
            Err(e) => {
                error(&format!("export_chat_docx failed to generate the document: {}", e));
                return false;
            }
        };
        save_bytes_to_folder(folder_url, &filename.to_string(), &bytes)
    }

    /// Render a full gloss export as text (`format`: "html" / "markdown" /
    /// "orgmode"). Returns an empty string on error.
    pub fn gloss_export(&self, gloss_json: &QString, format: &QString) -> QString {
        match simsapa_backend::text_export::gloss_export(&gloss_json.to_string(), &format.to_string()) {
            Ok(text) => QString::from(text),
            Err(e) => {
                error(&format!("gloss_export failed: {}", e));
                QString::from("")
            }
        }
    }

    /// Render a single gloss paragraph fragment as text (for per-paragraph
    /// "Copy As..."). Returns an empty string on error.
    pub fn gloss_paragraph_export(&self, paragraph_json: &QString, paragraph_number: i32, format: &QString) -> QString {
        let number = paragraph_number.max(0) as usize;
        match simsapa_backend::text_export::gloss_paragraph_export(&paragraph_json.to_string(), number, &format.to_string()) {
            Ok(text) => QString::from(text),
            Err(e) => {
                error(&format!("gloss_paragraph_export failed: {}", e));
                QString::from("")
            }
        }
    }

    /// Render a full chat export as text (`format`: "html" / "markdown" /
    /// "orgmode"). Returns an empty string on error.
    pub fn chat_export(&self, chat_json: &QString, format: &QString) -> QString {
        match simsapa_backend::text_export::chat_export(&chat_json.to_string(), &format.to_string()) {
            Ok(text) => QString::from(text),
            Err(e) => {
                error(&format!("chat_export failed: {}", e));
                QString::from("")
            }
        }
    }

    /// Render a single chat message fragment as text (for per-message
    /// "Copy As..."). Returns an empty string on error.
    pub fn chat_message_export(&self, message_json: &QString, format: &QString) -> QString {
        match simsapa_backend::text_export::chat_message_export(&message_json.to_string(), &format.to_string()) {
            Ok(text) => QString::from(text),
            Err(e) => {
                error(&format!("chat_message_export failed: {}", e));
                QString::from("")
            }
        }
    }

    pub fn check_file_exists_in_folder(&self,
                                       folder_url: &QUrl,
                                       filename: &QString) -> bool {
        // Android SAF: the folder is a content:// tree URI, not a path. Query
        // the ContentResolver so the Gloss/Prompts overwrite prompts work.
        #[cfg(target_os = "android")]
        {
            if folder_url.scheme().map(|s| s.to_string()).as_deref() == Some("content") {
                let tree_uri = String::from_utf8_lossy(folder_url.to_encoded().as_slice()).to_string();
                let fname = filename.to_string();
                return match simsapa_backend::android_saf::child_exists(&tree_uri, &fname) {
                    Ok(exists) => exists,
                    Err(e) => {
                        error(&format!("check_file_exists_in_folder SAF query failed for {}: {}", fname, e));
                        // On query failure, report "not present" rather than
                        // blocking the save; the write path handles overwrite.
                        false
                    }
                };
            }
        }

        let folder_path = PathBuf::from(qurl_to_local_path(folder_url));
        let output_path = folder_path.join(filename.to_string());

        check_file_exists_print_err(&output_path).unwrap_or(false)
    }

    pub fn markdown_to_html(&self, markdown_text: &QString) -> QString {
        QString::from(markdown_to_html(&markdown_text.to_string()))
    }

    pub fn run_gloss_in_sutta_window(&self, window_id: &QString, query_text: &QString) {
        use crate::api::ffi;
        ffi::callback_run_sutta_menu_action(
            window_id.clone(),
            QString::from("gloss-selection"),
            query_text.clone(),
        );
    }

    pub fn open_sutta_search_window(&self) {
        use crate::api::ffi;
        ffi::callback_open_sutta_search_window(QString::from(""));
    }

    pub fn open_sutta_search_window_with_result(&self, result_data_json: &QString) {
        use crate::api::ffi;
        ffi::callback_open_sutta_search_window(result_data_json.clone());
    }

    pub fn open_sutta_languages_window(&self) {
        use crate::api::ffi;
        ffi::callback_open_sutta_languages_window();
    }

    pub fn open_dictionaries_window(&self) {
        use crate::api::ffi;
        ffi::callback_open_dictionaries_window();
    }

    pub fn open_library_window(&self) {
        use crate::api::ffi;
        ffi::callback_open_library_window();
    }

    pub fn open_reference_search_window(&self) {
        use crate::api::ffi;
        ffi::callback_open_reference_search_window();
    }

    pub fn get_all_books_json(&self) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.get_all_books() {
            Ok(books) => {
                let json = serde_json::to_string(&books).unwrap_or_else(|_| "[]".to_string());
                QString::from(json)
            }
            Err(e) => {
                error(&format!("Failed to get books: {}", e));
                QString::from("[]")
            }
        }
    }

    pub fn get_book_by_uid_json(&self, book_uid: &QString) -> QString {
        let app_data = get_app_data();
        let uid = book_uid.to_string();
        match app_data.dbm.appdata.get_book_by_uid(&uid) {
            Ok(Some(book)) => {
                let json = serde_json::to_string(&book).unwrap_or_else(|_| "{}".to_string());
                QString::from(json)
            }
            Ok(None) => {
                error(&format!("Book not found: {}", uid));
                QString::from("{}")
            }
            Err(e) => {
                error(&format!("Failed to get book {}: {}", uid, e));
                QString::from("{}")
            }
        }
    }

    pub fn get_spine_items_for_book_json(&self, book_uid: &QString) -> QString {
        let app_data = get_app_data();
        let uid = book_uid.to_string();
        match app_data.dbm.appdata.get_spine_items_for_book(&uid) {
            Ok(items) => {
                let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
                QString::from(json)
            }
            Err(e) => {
                error(&format!("Failed to get spine items for book {}: {}", uid, e));
                QString::from("[]")
            }
        }
    }

    pub fn get_spine_item_uid_by_path(&self, book_uid: &QString, resource_path: &QString) -> QString {
        let app_data = get_app_data();
        let book_uid_str = book_uid.to_string();
        let resource_path_str = resource_path.to_string();

        match app_data.dbm.appdata.get_book_spine_item_by_path(&book_uid_str, &resource_path_str) {
            Ok(Some(item)) => QString::from(&item.spine_item_uid),
            Ok(None) => {
                info(&format!("No spine item found for book {} at path {}", book_uid_str, resource_path_str));
                QString::from("")
            }
            Err(e) => {
                error(&format!("Failed to get spine item by path for book {} at path {}: {}", book_uid_str, resource_path_str, e));
                QString::from("")
            }
        }
    }

    pub fn get_book_spine_html(&self, window_id: &QString, spine_item_uid: &QString) -> QString {
        let app_data = get_app_data();
        let html = app_data.render_book_spine_html_by_uid(&window_id.to_string(), &spine_item_uid.to_string());
        QString::from(html)
    }

    pub fn check_book_uid_exists(&self, book_uid: &QString) -> bool {
        let app_data = get_app_data();
        let uid = book_uid.to_string();
        match app_data.dbm.appdata.get_book_by_uid(&uid) {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(e) => {
                error(&format!("Failed to check book UID {}: {}", uid, e));
                false
            }
        }
    }

    /// Extract metadata (title and author) from a document file
    /// Returns a JSON string with "title" and "author" fields
    pub fn extract_document_metadata(&self, file_path: &QString) -> QString {
        use simsapa_backend::document_metadata;

        let path_str = file_path.to_string();
        let path = Path::new(&path_str);

        match document_metadata::extract_document_metadata(path) {
            Ok(metadata) => {
                let json = serde_json::json!({
                    "title": metadata.title,
                    "author": metadata.author
                });
                QString::from(json.to_string())
            }
            Err(e) => {
                error(&format!("Failed to extract metadata: {}", e));
                // Return empty metadata on error
                let json = serde_json::json!({
                    "title": "",
                    "author": ""
                });
                QString::from(json.to_string())
            }
        }
    }

    /// Copy content from a content:// URI to a temporary file (Android only)
    /// Returns the path to the temporary file, or empty string on error
    pub fn copy_content_uri_to_temp(&self, content_uri: &QString) -> QString {
        let uri_str = content_uri.to_string();

        // Only handle content:// URIs
        if !uri_str.starts_with("content://") {
            return QString::from("");
        }

        info(&format!("Copying content URI to temp file: {}", uri_str));

        // Call the C++ function to handle the actual copying
        let temp_path = qobject::copy_content_uri_to_temp_file(content_uri);

        if temp_path.is_empty() {
            error("Failed to copy content URI to temp file");
        } else {
            info(&format!("Successfully copied to: {}", temp_path));
        }

        temp_path
    }

    /// Delete the temporary import folder and all its contents
    /// Returns true if successful, false otherwise
    pub fn delete_temp_import_folder(&self) -> bool {
        let temp_dir = std::env::temp_dir().join("simsapa-imports");

        // Use try_exists() instead of exists() to avoid Android permission crashes
        match temp_dir.try_exists() {
            Ok(true) => {
                // Folder exists, try to remove it
                match fs::remove_dir_all(&temp_dir) {
                    Ok(_) => {
                        info(&format!("Deleted temp import folder: {}", temp_dir.display()));
                        true
                    }
                    Err(e) => {
                        error(&format!("Failed to delete temp import folder {}: {}", temp_dir.display(), e));
                        false
                    }
                }
            }
            Ok(false) => {
                // Folder doesn't exist, consider success
                true
            }
            Err(e) => {
                // Error checking if folder exists, log and return false
                error(&format!("Failed to check if temp import folder exists: {}", e));
                false
            }
        }
    }

    pub fn is_spine_item_pdf(&self, spine_item_uid: &QString) -> bool {
        let app_data = get_app_data();
        let uid = spine_item_uid.to_string();

        // Get the spine item
        match app_data.dbm.appdata.get_book_spine_item(&uid) {
            Ok(Some(spine_item)) => {
                // Get the book to check document_type
                match app_data.dbm.appdata.get_book_by_uid(&spine_item.book_uid) {
                    Ok(Some(book)) => book.document_type == "pdf",
                    _ => false,
                }
            },
            _ => false,
        }
    }

    /// Get the book_uid for a given spine_item_uid
    pub fn get_book_uid_for_spine_item(&self, spine_item_uid: &QString) -> QString {
        let app_data = get_app_data();
        let uid = spine_item_uid.to_string();

        // Get the spine item and return its book_uid
        match app_data.dbm.appdata.get_book_spine_item(&uid) {
            Ok(Some(spine_item)) => QString::from(&spine_item.book_uid),
            _ => QString::from(""),
        }
    }

    pub fn import_document(self: Pin<&mut Self>, file_path: &QString, book_uid: &QString, title: &QString, author: &QString, language: &QString, document_type: &QString, split_tag: &QString) {
        let path_str = file_path.to_string();
        let uid_str = book_uid.to_string();
        let title_str = title.to_string();
        let author_str = author.to_string();
        let language_str = language.to_string();
        let doc_type = document_type.to_string();
        let _split_tag_str = split_tag.to_string();

        info(&format!("import_document: {} as {} ({})", &path_str, &uid_str, &doc_type));

        let qt_thread = self.qt_thread();

        // Spawn thread for import
        thread::spawn(move || {
            let app_data = get_app_data();
            let path = Path::new(&path_str);

            // Convert custom title and author to Option<&str>
            let custom_title = if title_str.trim().is_empty() {
                None
            } else {
                Some(title_str.as_str())
            };
            let custom_author = if author_str.trim().is_empty() {
                None
            } else {
                Some(author_str.as_str())
            };
            let custom_language = if language_str.trim().is_empty() {
                None
            } else {
                Some(language_str.as_str())
            };

            let result = match doc_type.as_str() {
                "epub" => {
                    let progress_msg = QString::from("Importing EPUB...");
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().document_import_progress(progress_msg);
                    }).unwrap();

                    app_data.import_epub_to_db(path, &uid_str, custom_title, custom_author, custom_language, None, true)
                }
                "pdf" => {
                    let progress_msg = QString::from("Importing PDF...");
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().document_import_progress(progress_msg);
                    }).unwrap();

                    app_data.import_pdf_to_db(path, &uid_str, custom_title, custom_author, custom_language, None, true)
                }
                "html" => {
                    let progress_msg = QString::from("Importing HTML...");
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().document_import_progress(progress_msg);
                    }).unwrap();

                    // TODO: Pass split_tag parameter when html_import supports it
                    // For now, HTML is imported as a single spine item
                    app_data.import_html_to_db(path, &uid_str, custom_title, custom_author, custom_language, None, true)
                }
                _ => {
                    let error_msg = format!("Unknown document type: {}", doc_type);
                    error(&error_msg);
                    let error_qstr = QString::from(&error_msg);
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().document_import_completed(false, error_qstr);
                    }).unwrap();
                    return;
                }
            };

            match result {
                Ok(_) => {
                    info(&format!("Successfully imported {}", &uid_str));

                    // Build library index for the effective language of the imported book
                    let effective_lang = if language_str.trim().is_empty() { "en".to_string() } else { language_str.to_lowercase() };
                    let globals = simsapa_backend::get_app_globals();
                    let library_index_dir = &globals.paths.library_index_dir;

                    if let Err(e) = simsapa_backend::search::indexer::build_library_index(
                        &app_data.dbm.appdata, library_index_dir, &effective_lang
                    ) {
                        warn(&format!("Failed to build library index for language {}: {}", effective_lang, e));
                    }

                    simsapa_backend::reinit_fulltext_searcher();

                    let success_msg = QString::from(format!("Successfully imported '{}'", &title_str));
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().document_import_completed(true, success_msg);
                    }).unwrap();
                }
                Err(e) => {
                    let error_msg = format!("Failed to import: {}", e);
                    error(&error_msg);
                    let error_qstr = QString::from(&error_msg);
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().document_import_completed(false, error_qstr);
                    }).unwrap();
                }
            }
        });
    }

    /// Check the search index status.
    /// Returns a JSON string: {"exists": bool, "current": bool}
    /// - exists: whether the index directory exists
    /// - current: whether the VERSION file matches the expected version
    pub fn check_search_index_status(&self) -> QString {
        let globals = get_app_globals();
        let index_dir = &globals.paths.index_dir;

        let exists = matches!(index_dir.try_exists(), Ok(true));

        let current = if exists {
            simsapa_backend::search::indexer::is_index_current(index_dir)
        } else {
            false
        };

        let json = format!(r#"{{"exists": {}, "current": {}}}"#, exists, current);
        QString::from(&json)
    }

    /// Per-database startup report as JSON, for the Database Validation dialog's
    /// presentation rows. Shape per database (`appdata`, `dictionaries`, `dpd`):
    /// `{"present_at_start": bool|null, "migration_ok": bool|null, "migration_error": string|null}`.
    ///
    /// `migration_ok` is `null` where no migrations apply — always the case for
    /// dpd, which has no migration folder.
    ///
    /// This is **presentation only**. A failed migration is already folded into
    /// the `database_validation_result` signal by the `*_first_query` functions,
    /// so QML must not post-mutate its validation results from this report.
    pub fn get_startup_db_report(&self) -> QString {
        QString::from(&simsapa_backend::db::get_startup_db_report_json())
    }

    pub fn rebuild_search_index(self: Pin<&mut Self>) {
        info("rebuild_search_index: starting background rebuild");

        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            let progress_msg = QString::from("Rebuilding search index...");
            qt_thread.queue(move |mut qo| {
                qo.as_mut().rebuild_search_index_progress(progress_msg);
            }).unwrap();

            let app_data = get_app_data();
            let globals = get_app_globals();
            let paths = &globals.paths;

            // Delete existing index directories to rebuild from scratch
            if let Ok(true) = paths.index_dir.try_exists() {
                let msg = QString::from("Removing old index...");
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().rebuild_search_index_progress(msg);
                }).unwrap();

                if let Err(e) = std::fs::remove_dir_all(&paths.index_dir) {
                    let error_msg = format!("Failed to remove old index: {}", e);
                    error(&error_msg);
                    let error_qstr = QString::from(&error_msg);
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().rebuild_search_index_completed(false, error_qstr);
                    }).unwrap();
                    return;
                }
            }

            let msg = QString::from("Building fulltext indexes for all languages...");
            qt_thread.queue(move |mut qo| {
                qo.as_mut().rebuild_search_index_progress(msg);
            }).unwrap();

            match simsapa_backend::search::indexer::build_all_indexes(
                &app_data.dbm.appdata,
                &app_data.dbm.dictionaries,
                &app_data.dbm.dpd,
                paths,
            ) {
                Ok(()) => {
                    // Re-initialize the fulltext searcher with new indexes
                    simsapa_backend::reinit_fulltext_searcher();

                    info("rebuild_search_index: completed successfully");
                    let success_msg = QString::from("Search index rebuilt successfully.");
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().rebuild_search_index_completed(true, success_msg);
                    }).unwrap();
                }
                Err(e) => {
                    let error_msg = format!("Failed to rebuild search index: {}", e);
                    error(&error_msg);
                    let error_qstr = QString::from(&error_msg);
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().rebuild_search_index_completed(false, error_qstr);
                    }).unwrap();
                }
            }
        });
    }

    pub fn remove_book(&self, book_uid: &QString) -> bool {
        let uid = book_uid.to_string();
        info(&format!("remove_book: {}", &uid));

        let app_data = get_app_data();
        match app_data.dbm.appdata.delete_book_by_uid(&uid) {
            Ok(_) => {
                info(&format!("Successfully removed book: {}", &uid));
                // Refresh stats: a book delete cascades to its `book_spine_items`
                // (and via FTS triggers, the matching `book_spine_items_fts`
                // rows). See docs/user-data-and-sqlite-analyze.md.
                app_data.dbm.appdata.analyze("appdata");
                true
            }
            Err(e) => {
                error(&format!("Failed to remove book {}: {}", &uid, e));
                false
            }
        }
    }

    pub fn get_book_metadata_json(&self, book_uid: &QString) -> QString {
        let app_data = get_app_data();
        let uid = book_uid.to_string();

        match app_data.dbm.appdata.get_book_by_uid(&uid) {
            Ok(Some(book)) => {
                let json = serde_json::json!({
                    "title": book.title,
                    "author": book.author,
                    "language": book.language,
                    "document_type": book.document_type,
                    "enable_embedded_css": book.enable_embedded_css
                });
                QString::from(json.to_string())
            }
            Ok(None) => {
                error(&format!("Book not found: {}", uid));
                let json = serde_json::json!({
                    "title": "",
                    "author": "",
                    "language": "",
                    "document_type": "",
                    "enable_embedded_css": true
                });
                QString::from(json.to_string())
            }
            Err(e) => {
                error(&format!("Failed to get book metadata {}: {}", uid, e));
                let json = serde_json::json!({
                    "title": "",
                    "author": "",
                    "language": "",
                    "document_type": "",
                    "enable_embedded_css": true
                });
                QString::from(json.to_string())
            }
        }
    }

    pub fn update_book_metadata(self: Pin<&mut Self>, book_uid: &QString, title: &QString, author: &QString, language: &QString, enable_embedded_css: bool) {
        let uid = book_uid.to_string();
        let title_str = title.to_string();
        let author_str = author.to_string();
        let language_str = language.to_string();

        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            let app_data = get_app_data();

            // Get old language before updating
            let old_language = match app_data.dbm.appdata.get_book_by_uid(&uid) {
                Ok(Some(book)) => book.language.unwrap_or_default(),
                _ => String::new(),
            };

            match app_data.dbm.appdata.update_book_metadata(&uid, &title_str, &author_str, &language_str, enable_embedded_css) {
                Ok(_) => {
                    // Re-index library if language changed
                    let new_lang = if language_str.is_empty() { "en".to_string() } else { language_str.to_lowercase() };
                    let old_lang = if old_language.is_empty() { "en".to_string() } else { old_language.to_lowercase() };

                    if old_lang != new_lang {
                        // Rebuild both old and new language indexes to reflect the change
                        let globals = simsapa_backend::get_app_globals();
                        let library_index_dir = &globals.paths.library_index_dir;

                        if let Err(e) = simsapa_backend::search::indexer::build_library_index(
                            &app_data.dbm.appdata, library_index_dir, &old_lang
                        ) {
                            warn(&format!("Failed to rebuild library index for old language {}: {}", old_lang, e));
                        }

                        if let Err(e) = simsapa_backend::search::indexer::build_library_index(
                            &app_data.dbm.appdata, library_index_dir, &new_lang
                        ) {
                            warn(&format!("Failed to rebuild library index for new language {}: {}", new_lang, e));
                        }

                        // Reload the fulltext searcher
                        simsapa_backend::reinit_fulltext_searcher();
                    }

                    let success_msg = QString::from(format!("Successfully updated metadata for '{}'", &title_str));
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().book_metadata_updated(true, success_msg);
                    }).unwrap();
                }
                Err(e) => {
                    let error_msg = format!("Failed to update book metadata: {}", e);
                    error(&error_msg);
                    let error_qstr = QString::from(&error_msg);
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().book_metadata_updated(false, error_qstr);
                    }).unwrap();
                }
            }
        });
    }

    /// Helper function to create error response JSON for background processing
    fn create_error_response(error_message: &str) -> String {
        let error_response = simsapa_backend::types::BackgroundProcessingError {
            success: false,
            error: error_message.to_string(),
        };

        match serde_json::to_string(&error_response) {
            Ok(json) => json,
            Err(_) => {
                // Fallback to simple JSON if serialization fails
                format!(r#"{{"success":false,"error":"{}"}}"#, error_message.replace('"', r#"\""#))
            }
        }
    }

    /// Process all paragraphs in background thread
    pub fn process_all_paragraphs_background(self: Pin<&mut Self>, input_json: &QString) {
        let input_json = input_json.to_string();
        let self_ = self.qt_thread();

        thread::spawn(move || {
            // Parse input JSON directly into typed struct
            let input_data: simsapa_backend::types::AllParagraphsProcessingInput = match serde_json::from_str(&input_json) {
                Ok(data) => data,
                Err(e) => {
                    let error_response = Self::create_error_response(&format!("Failed to parse input JSON: {}", e));
                    self_.queue(move |mut qo| {
                        qo.as_mut().all_paragraphs_gloss_ready(QString::from(error_response));
                    }).unwrap();
                    return;
                }
            };

            // Get app data for DPD database access
            let app_data = simsapa_backend::get_app_data();

            // Delegate to the Qt-free backend core (shared with POST /gloss_text).
            let response = match simsapa_backend::helpers::process_all_paragraphs(
                &input_data,
                &app_data.dbm.appdata,
                &app_data.dbm.dpd,
            ) {
                Ok(response) => response,
                Err(e) => {
                    let error_response = Self::create_error_response(&e);
                    self_.queue(move |mut qo| {
                        qo.as_mut().all_paragraphs_gloss_ready(QString::from(error_response));
                    }).unwrap();
                    return;
                }
            };

            let response_json = match serde_json::to_string(&response) {
                Ok(json) => json,
                Err(e) => {
                    let error_response = Self::create_error_response(&format!("Failed to serialize response: {}", e));
                    self_.queue(move |mut qo| {
                        qo.as_mut().all_paragraphs_gloss_ready(QString::from(error_response));
                    }).unwrap();
                    return;
                }
            };

            self_.queue(move |mut qo| {
                qo.as_mut().all_paragraphs_gloss_ready(QString::from(response_json));
            }).unwrap();
        });
    }

    /// Process a single paragraph in background thread
    pub fn process_paragraph_background(self: Pin<&mut Self>, paragraph_index: i32, input_json: &QString) {
        let input_json = input_json.to_string();
        let self_ = self.qt_thread();

        thread::spawn(move || {
            // Parse input JSON directly into typed struct
            let input_data: simsapa_backend::types::SingleParagraphProcessingInput = match serde_json::from_str(&input_json) {
                Ok(data) => data,
                Err(e) => {
                    let error_response = Self::create_error_response(&format!("Failed to parse input JSON: {}", e));
                    self_.queue(move |mut qo| {
                        qo.as_mut().paragraph_gloss_ready(paragraph_index, QString::from(error_response));
                    }).unwrap();
                    return;
                }
            };

            // Get app data for DPD database access
            let app_data = simsapa_backend::get_app_data();

            // Delegate to the Qt-free backend core (shared with POST /gloss_text).
            let response = match simsapa_backend::helpers::process_single_paragraph(
                paragraph_index as usize,
                &input_data,
                &app_data.dbm.appdata,
                &app_data.dbm.dpd,
            ) {
                Ok(response) => response,
                Err(e) => {
                    let error_response = Self::create_error_response(&e);
                    self_.queue(move |mut qo| {
                        qo.as_mut().paragraph_gloss_ready(paragraph_index, QString::from(error_response));
                    }).unwrap();
                    return;
                }
            };

            let response_json = match serde_json::to_string(&response) {
                Ok(json) => json,
                Err(e) => {
                    let error_response = Self::create_error_response(&format!("Failed to serialize response: {}", e));
                    self_.queue(move |mut qo| {
                        qo.as_mut().paragraph_gloss_ready(paragraph_index, QString::from(error_response));
                    }).unwrap();
                    return;
                }
            };

            self_.queue(move |mut qo| {
                qo.as_mut().paragraph_gloss_ready(paragraph_index, QString::from(response_json));
            }).unwrap();
        });
    }

    /// Get Anki template for Front side
    pub fn get_anki_template_front(&self) -> QString {
        let app_data = get_app_data();
        let template = app_data.get_anki_template_front();
        QString::from(template)
    }

    /// Set Anki template for Front side
    pub fn set_anki_template_front(self: Pin<&mut Self>, template_str: &QString) {
        let app_data = get_app_data();
        app_data.set_anki_template_front(&template_str.to_string());
    }

    /// Get Anki template for Back side
    pub fn get_anki_template_back(&self) -> QString {
        let app_data = get_app_data();
        let template = app_data.get_anki_template_back();
        QString::from(template)
    }

    /// Set Anki template for Back side
    pub fn set_anki_template_back(self: Pin<&mut Self>, template_str: &QString) {
        let app_data = get_app_data();
        app_data.set_anki_template_back(&template_str.to_string());
    }

    /// Get Anki template for Cloze Front side
    pub fn get_anki_template_cloze_front(&self) -> QString {
        let app_data = get_app_data();
        let template = app_data.get_anki_template_cloze_front();
        QString::from(template)
    }

    /// Set Anki template for Cloze Front side
    pub fn set_anki_template_cloze_front(self: Pin<&mut Self>, template_str: &QString) {
        let app_data = get_app_data();
        app_data.set_anki_template_cloze_front(&template_str.to_string());
    }

    /// Get Anki template for Cloze Back side
    pub fn get_anki_template_cloze_back(&self) -> QString {
        let app_data = get_app_data();
        let template = app_data.get_anki_template_cloze_back();
        QString::from(template)
    }

    /// Set Anki template for Cloze Back side
    pub fn set_anki_template_cloze_back(self: Pin<&mut Self>, template_str: &QString) {
        let app_data = get_app_data();
        app_data.set_anki_template_cloze_back(&template_str.to_string());
    }

    /// Get Anki export format (Simple, Templated, DataCsv)
    pub fn get_anki_export_format(&self) -> QString {
        let app_data = get_app_data();
        let format = app_data.get_anki_export_format();
        QString::from(format)
    }

    /// Set Anki export format
    pub fn set_anki_export_format(self: Pin<&mut Self>, format: &QString) {
        let app_data = get_app_data();
        app_data.set_anki_export_format(&format.to_string());
    }

    /// Get whether to include cloze format in Anki export
    pub fn get_anki_include_cloze(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_anki_include_cloze()
    }

    /// Set whether to include cloze format in Anki export
    pub fn set_anki_include_cloze(self: Pin<&mut Self>, include: bool) {
        let app_data = get_app_data();
        app_data.set_anki_include_cloze(include);
    }

    /// Get sample vocabulary data for preview (hardcoded abhivādetvā)
    pub fn get_sample_vocabulary_data_json(&self) -> QString {
        let sample_json = simsapa_backend::anki_sample_data::get_sample_vocabulary_data_json();
        QString::from(sample_json)
    }

    /// Get DPD headword data by UID
    pub fn get_dpd_headword_by_uid(&self, uid: &QString) -> QString {
        let app_data = get_app_data();
        let uid_str = uid.to_string();

        match app_data.get_dpd_headword_by_uid(&uid_str) {
            Some(json) => QString::from(json),
            None => QString::from("{}"),
        }
    }

    pub fn export_anki_csv_background(self: Pin<&mut Self>, input_json: &QString) {
        info("SuttaBridge::export_anki_csv_background() start");
        let qt_thread = self.qt_thread();
        let input_json_str = input_json.to_string();

        thread::spawn(move || {
            let app_data = get_app_data();

            let input: simsapa_backend::types::AnkiCsvExportInput = match serde_json::from_str(&input_json_str) {
                Ok(data) => data,
                Err(e) => {
                    let error_response = simsapa_backend::types::AnkiCsvExportResult {
                        success: false,
                        files: vec![],
                        error: Some(format!("Failed to parse input JSON: {}", e)),
                    };
                    let error_json = serde_json::to_string(&error_response).unwrap_or_default();
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().anki_csv_export_ready(QString::from(error_json));
                    }).unwrap();
                    return;
                }
            };

            let result = match simsapa_backend::anki_export::export_anki_csv(input, app_data) {
                Ok(res) => res,
                Err(e) => simsapa_backend::types::AnkiCsvExportResult {
                    success: false,
                    files: vec![],
                    error: Some(format!("Export failed: {}", e)),
                },
            };

            let result_json = match serde_json::to_string(&result) {
                Ok(json) => json,
                Err(e) => {
                    let error_response = simsapa_backend::types::AnkiCsvExportResult {
                        success: false,
                        files: vec![],
                        error: Some(format!("Failed to serialize result: {}", e)),
                    };
                    serde_json::to_string(&error_response).unwrap_or_default()
                }
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().anki_csv_export_ready(QString::from(result_json));
            }).unwrap();

            info("SuttaBridge::export_anki_csv_background() end");
        });
    }

    pub fn render_anki_preview_background(self: Pin<&mut Self>, front_template: &QString, back_template: &QString) {
        info("SuttaBridge::render_anki_preview_background() start");
        let qt_thread = self.qt_thread();
        let front_template_str = front_template.to_string();
        let back_template_str = back_template.to_string();

        thread::spawn(move || {
            let app_data = get_app_data();
            let sample_json = simsapa_backend::anki_sample_data::get_sample_vocabulary_data_json();

            let preview_html = match simsapa_backend::anki_export::render_anki_preview(
                &sample_json,
                &front_template_str,
                &back_template_str,
                app_data,
            ) {
                Ok(html) => html,
                Err(e) => format!("<span style='color: red;'>Preview error: {}</span>", e),
            };

            qt_thread.queue(move |mut qo| {
                qo.as_mut().anki_preview_ready(QString::from(preview_html));
            }).unwrap();

            info("SuttaBridge::render_anki_preview_background() end");
        });
    }

    pub fn get_search_as_you_type(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_search_as_you_type()
    }

    pub fn set_search_as_you_type(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_search_as_you_type(enabled);
    }

    pub fn get_include_cst_commentary_in_translations(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_include_cst_commentary_in_translations()
    }

    pub fn set_include_cst_commentary_in_translations(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_include_cst_commentary_in_translations(enabled);
    }

    pub fn get_include_cst_mula_in_search_results(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_include_cst_mula_in_search_results()
    }

    pub fn set_include_cst_mula_in_search_results(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_include_cst_mula_in_search_results(enabled);
    }

    pub fn get_include_cst_commentary_in_search_results(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_include_cst_commentary_in_search_results()
    }

    pub fn set_include_cst_commentary_in_search_results(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_include_cst_commentary_in_search_results(enabled);
    }

    pub fn get_include_cst_mula_in_translations(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_include_cst_mula_in_translations()
    }

    pub fn set_include_cst_mula_in_translations(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_include_cst_mula_in_translations(enabled);
    }

    pub fn get_include_ms_mula_in_search_results(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_include_ms_mula_in_search_results()
    }

    pub fn set_include_ms_mula_in_search_results(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_include_ms_mula_in_search_results(enabled);
    }

    pub fn get_include_comm_bold_definitions_in_search_results(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_include_comm_bold_definitions_in_search_results()
    }

    pub fn set_include_comm_bold_definitions_in_search_results(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_include_comm_bold_definitions_in_search_results(enabled);
    }

    pub fn get_open_find_in_sutta_results(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_open_find_in_sutta_results()
    }

    pub fn set_open_find_in_sutta_results(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_open_find_in_sutta_results(enabled);
    }

    pub fn get_show_bottom_footnotes(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_show_bottom_footnotes()
    }

    pub fn set_show_bottom_footnotes(mut self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_show_bottom_footnotes(enabled);
        self.as_mut().show_bottom_footnotes_changed();
    }

    pub fn get_sutta_language_labels(&self) -> QStringList {
        let app_data = get_app_data();
        let languages = app_data.get_cached_sutta_languages();

        let mut res = QStringList::default();
        for lang in languages {
            res.append(QString::from(lang));
        }
        res
    }

    pub fn get_library_language_labels(&self) -> QStringList {
        let app_data = get_app_data();
        let languages = app_data.get_cached_library_languages();

        let mut res = QStringList::default();
        for lang in languages {
            res.append(QString::from(lang));
        }
        res
    }

    /// Distinct languages present in the dictionaries DB (e.g. "en", "pli"),
    /// used to populate the search bar's language filter for the Dictionary
    /// area. Mirrors `get_sutta_language_labels` / `get_library_language_labels`:
    /// returns exactly the distinct DB values with no fallback default (the
    /// built-in dictionaries include "en" sources such as DPPN, so a hardcoded
    /// "pli" default would be wrong).
    pub fn get_dict_language_labels(&self) -> QStringList {
        let app_data = get_app_data();
        let languages = app_data.get_cached_dict_languages();

        let mut res = QStringList::default();
        for lang in languages {
            res.append(QString::from(lang));
        }
        res
    }

    /// Get sutta languages with their counts in format "code|Name|Count"
    pub fn get_sutta_language_labels_with_counts(&self) -> QStringList {
        let app_data = get_app_data();
        let labels = app_data.dbm.get_sutta_language_labels_with_counts();

        let mut res = QStringList::default();
        for label in labels {
            res.append(QString::from(label));
        }
        res
    }

    pub fn get_language_filter_key(&self, area: &QString) -> QString {
        QString::from(&get_app_data().get_language_filter_key(&area.to_string()))
    }

    pub fn set_language_filter_key(&self, area: &QString, key: &QString) {
        get_app_data().set_language_filter_key(&area.to_string(), &key.to_string());
    }

    pub fn get_last_search_mode(&self, area: &QString) -> QString {
        QString::from(&get_app_data().get_last_search_mode(&area.to_string()))
    }

    pub fn set_last_search_mode(&self, area: &QString, mode: &QString) {
        get_app_data().set_last_search_mode(&area.to_string(), &mode.to_string());
    }

    /// Get the mobile top bar margin value
    /// Returns either system value (from get_status_bar_height) or custom value
    /// Returns a default value of 24 if APP_DATA is not yet initialized
    pub fn get_mobile_top_bar_margin(&self) -> i32 {
        // Return default value if APP_DATA is not yet initialized
        // This can happen when QML components load before init_app_data() is called
        let app_data = match try_get_app_data() {
            Some(data) => data,
            None => return 24,
        };

        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");

        use simsapa_backend::app_settings::MobileTopBarMargin;
        match app_settings.mobile_top_bar_margin {
            MobileTopBarMargin::SystemValue => {
                use crate::api::ffi;
                ffi::get_status_bar_height()
            }
            MobileTopBarMargin::CustomValue(value) => value as i32,
        }
    }

    pub fn is_mobile_top_bar_margin_system(&self) -> bool {
        // Return default (true for system value) if APP_DATA is not yet initialized
        let app_data = match try_get_app_data() {
            Some(data) => data,
            None => return true,
        };

        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.is_mobile_top_bar_margin_system()
    }

    pub fn get_mobile_top_bar_margin_custom_value(&self) -> u32 {
        // Return default custom value of 24 if APP_DATA is not yet initialized
        let app_data = match try_get_app_data() {
            Some(data) => data,
            None => return 24,
        };

        let app_settings = app_data.app_settings_cache.read().expect("Failed to read app settings");
        app_settings.get_mobile_top_bar_margin_custom_value()
    }

    pub fn set_mobile_top_bar_margin_system(self: Pin<&mut Self>) {
        let app_data = get_app_data();
        app_data.set_mobile_top_bar_margin_system();
    }

    pub fn set_mobile_top_bar_margin_custom(self: Pin<&mut Self>, value: u32) {
        let app_data = get_app_data();
        app_data.set_mobile_top_bar_margin_custom(value);
    }

    pub fn search_reference(&self, query: &QString, field: &QString) -> QString {
        use simsapa_backend::pts_reference_search;

        let results = pts_reference_search::search(
            &query.to_string(),
            &field.to_string()
        );

        match serde_json::to_string(&results) {
            Ok(json) => QString::from(json),
            Err(e) => {
                error(&format!("Failed to serialize reference search results: {}", e));
                QString::from("[]")
            }
        }
    }

    pub fn extract_uid_from_url(&self, url: &QString) -> QString {
        let url_str = url.to_string();

        // Extract UID from SuttaCentral URL
        // Examples:
        // - https://suttacentral.net/sn56.102 -> sn56.102
        // - https://suttacentral.net/dn1/en/sujato -> dn1
        // - sn56.102 -> sn56.102 (pass through if already just UID)

        if let Some(path_start) = url_str.find("suttacentral.net/") {
            let path = &url_str[path_start + 17..]; // Skip "suttacentral.net/"

            // Take everything up to the first '/' or end of string
            if let Some(slash_pos) = path.find('/') {
                QString::from(&path[..slash_pos])
            } else {
                QString::from(path)
            }
        } else {
            // If it's not a URL, return as-is (might already be a UID)
            QString::from(url_str)
        }
    }

    pub fn get_full_sutta_uid(&self, partial_uid: &QString) -> QString {
        let partial_str = partial_uid.to_string();
        let app_data = get_app_data();

        match app_data.dbm.appdata.get_full_sutta_uid(&partial_str) {
            Some(full_uid) => QString::from(full_uid),
            None => QString::from(""), // Return empty string if not found
        }
    }

    pub fn get_sutta_reference_info(&self, uid: &QString) -> QString {
        let uid_str = uid.to_string();
        let app_data = get_app_data();

        if let Some(sutta) = app_data.dbm.appdata.get_sutta(&uid_str) {
            let info = serde_json::json!({
                "uid": sutta.uid,
                "sutta_ref": sutta.sutta_ref,
                "title": sutta.title.clone().unwrap_or_default(),
                // FIXME: bootstrap issue: 'title_pali' is empty in the db, 'title' is used
                "title_pali": sutta.title.unwrap_or_default(),
            });
            QString::from(serde_json::to_string(&info).unwrap_or_else(|_| "{}".to_string()))
        } else {
            QString::from("{}")
        }
    }

    // ========================================================================
    // Update Checker Functions
    // ========================================================================

    /// Check for application and database updates.
    ///
    /// Spawns a background thread to perform the update check and emits
    /// appropriate signals when complete.
    ///
    /// # Arguments
    ///
    /// * `include_no_updates` - If true, emits noUpdatesAvailable signal when no updates found
    /// * `screen_size` - Screen resolution string (e.g., "1920 x 1080") for analytics, can be empty
    /// * `save_stats_behaviour` - Controls stats saving: "enabled", "disabled", or "determine"
    pub fn check_for_updates(self: Pin<&mut Self>, include_no_updates: bool, screen_size: &QString, save_stats_behaviour: &QString) {
        use simsapa_backend::update_checker::{self, SaveStatsBehaviour};

        info("SuttaBridge::check_for_updates() start");
        let qt_thread = self.qt_thread();

        // Convert screen_size to Option<String> for the background thread
        let screen_size_str = screen_size.to_string();
        let screen_size_opt: Option<String> = if screen_size_str.is_empty() {
            None
        } else {
            Some(screen_size_str)
        };

        // Parse save_stats_behaviour from string
        let stats_behaviour: SaveStatsBehaviour = save_stats_behaviour.to_string().parse().unwrap();

        thread::spawn(move || {
            // Get current app and db versions
            let app_version = update_checker::get_app_version();
            let db_version = update_checker::get_db_version();

            // First check if local db is obsolete (incompatible with app)
            if let Some(obsolete_info) = update_checker::is_local_db_obsolete(
                &app_version,
                db_version.as_deref(),
            ) {
                let json = serde_json::to_string(&obsolete_info).unwrap_or_default();
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().local_db_obsolete(QString::from(json));
                    qo.as_mut().releases_check_completed();
                }).unwrap();
                info("SuttaBridge::check_for_updates() - local db obsolete");
                return;
            }

            // Try to fetch release information
            let releases_info = match update_checker::fetch_releases_info(screen_size_opt.as_deref(), stats_behaviour) {
                Ok(info) => {
                    // Save the successfully fetched releases info to the global
                    simsapa_backend::set_releases_info(info.clone());
                    info
                },
                Err(e) => {
                    // The live fetch from the releases server failed (e.g. no
                    // network). Fall back to the embedded snapshot so the app can
                    // still resolve compatible asset download URLs for setup /
                    // language downloads. The actual asset download is attempted
                    // separately (AssetManager), and any network failure there is
                    // surfaced to the user from that path.
                    match update_checker::get_fallback_releases_info() {
                        Some(fallback_info) => {
                            warn(&format!(
                                "Failed to fetch releases info ({}), using embedded fallback",
                                e
                            ));
                            simsapa_backend::set_releases_info(fallback_info.clone());
                            fallback_info
                        }
                        None => {
                            // Neither the server nor the embedded fallback is
                            // usable: report the failure to the user.
                            let error_msg = format!("Failed to fetch updates: {}", e);
                            error(&error_msg);
                            qt_thread.queue(move |mut qo| {
                                qo.as_mut().update_check_error(QString::from(error_msg));
                                qo.as_mut().releases_check_completed();
                            }).unwrap();
                            info("SuttaBridge::check_for_updates() - fetch error, no fallback");
                            return;
                        }
                    }
                }
            };

            // Check for app update
            if let Some(mut app_update) = update_checker::has_app_update(&releases_info, &app_version) {
                // Convert release_notes from markdown to HTML
                if let Some(ref notes) = app_update.release_notes {
                    app_update.release_notes = Some(markdown_to_html(notes));
                }
                let json = serde_json::to_string(&app_update).unwrap_or_default();
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().app_update_available(QString::from(json));
                    qo.as_mut().releases_check_completed();
                }).unwrap();
                info("SuttaBridge::check_for_updates() - app update available");
                return;
            }

            // Check for db update
            if let Some(mut db_update) = update_checker::has_db_update(
                &releases_info,
                &app_version,
                db_version.as_deref(),
            ) {
                // Convert release_notes from markdown to HTML
                if let Some(ref notes) = db_update.release_notes {
                    db_update.release_notes = Some(markdown_to_html(notes));
                }
                let json = serde_json::to_string(&db_update).unwrap_or_default();
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().db_update_available(QString::from(json));
                    qo.as_mut().releases_check_completed();
                }).unwrap();
                info("SuttaBridge::check_for_updates() - db update available");
                return;
            }

            // No updates available
            if include_no_updates {
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().no_updates_available();
                    qo.as_mut().releases_check_completed();
                }).unwrap();
            } else {
                qt_thread.queue(move |mut qo| {
                    qo.as_mut().releases_check_completed();
                }).unwrap();
            }

            info("SuttaBridge::check_for_updates() - no updates available");
        });
    }

    /// Get whether to notify about Simsapa updates.
    pub fn get_notify_about_simsapa_updates(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_notify_about_simsapa_updates()
    }

    /// Set whether to notify about Simsapa updates.
    pub fn set_notify_about_simsapa_updates(self: Pin<&mut Self>, enabled: bool) {
        let app_data = get_app_data();
        app_data.set_notify_about_simsapa_updates(enabled);
    }

    /// Get the current keybindings as a JSON string.
    pub fn get_keybindings_json(&self) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_keybindings_json())
    }

    /// Get the default keybindings as a JSON string.
    pub fn get_default_keybindings_json(&self) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_default_keybindings_json())
    }

    /// Get the action names mapping as a JSON string.
    pub fn get_action_names_json(&self) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_action_names_json())
    }

    /// Get the action descriptions mapping as a JSON string.
    pub fn get_action_descriptions_json(&self) -> QString {
        let app_data = get_app_data();
        QString::from(app_data.get_action_descriptions_json())
    }

    /// Set the shortcuts for a specific action.
    /// shortcuts_json is a JSON array of strings, e.g. '["Ctrl+L", "Ctrl+K"]'
    pub fn set_keybinding(self: Pin<&mut Self>, action_id: &QString, shortcuts_json: &QString) {
        let app_data = get_app_data();
        let shortcuts: Vec<String> = serde_json::from_str(&shortcuts_json.to_string())
            .unwrap_or_default();
        app_data.set_keybinding(&action_id.to_string(), shortcuts);
    }

    /// Reset a single action's keybindings to default.
    pub fn reset_keybinding(self: Pin<&mut Self>, action_id: &QString) {
        let app_data = get_app_data();
        app_data.reset_keybinding(&action_id.to_string());
    }

    /// Reset all keybindings to their defaults.
    pub fn reset_all_keybindings(self: Pin<&mut Self>) {
        let app_data = get_app_data();
        app_data.reset_all_keybindings();
    }

    /// Get whether updates have already been checked in this session.
    pub fn get_updates_checked(&self) -> bool {
        use std::sync::atomic::Ordering;
        let globals = get_app_globals();
        globals.updates_checked.load(Ordering::Relaxed)
    }

    /// Set whether updates have been checked in this session.
    pub fn set_updates_checked(&self, checked: bool) {
        use std::sync::atomic::Ordering;
        let globals = get_app_globals();
        globals.updates_checked.store(checked, Ordering::Relaxed);
    }

    /// Prepare for database upgrade by exporting user data and creating marker files.
    ///
    /// First exports user data (app_settings, download_languages, user-imported books,
    /// bookmarks, chanting) to the import-me folder for restoration after the upgrade.
    ///
    /// If the export fails for any category, the marker files are **not** written.
    /// Instead, the `export_failed(reason)` signal is emitted with a human-readable
    /// summary of the per-category errors so the QML layer can show a confirmation
    /// dialog. The user can then cancel the upgrade (keeping the old DB) or call
    /// `force_database_upgrade()` to proceed anyway.
    ///
    /// Can't delete the db and index without triggering file-lock problems on Windows.
    /// On the happy path, two marker files are written to the simsapa directory:
    /// - `delete_files_for_upgrade.txt`: Signals the app to delete old database files on next startup
    /// - `auto_start_download.txt`: Signals the app to automatically start the download on next startup
    ///
    /// The user should quit the app after calling this function and restart it
    /// to begin the database download process.
    pub fn prepare_for_database_upgrade(self: Pin<&mut Self>) {
        // Run the export on a background thread so the UI thread is free to
        // repaint button-state/label bindings (e.g. "Exporting user data…")
        // while the work is in progress. Signals are emitted back on the Qt
        // thread via `qt_thread.queue`.
        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            let app_data = get_app_data();

            match app_data.export_user_data_to_assets() {
                Ok(()) => {
                    // Export succeeded; only emit exportSucceeded if marker
                    // files are also on disk (see PRD §11.3 — a silent marker
                    // failure would otherwise produce a no-op upgrade on
                    // restart).
                    match write_upgrade_marker_files() {
                        Ok(()) => {
                            if let Ok(mut guard) = LAST_EXPORT_FAILURE.lock() {
                                *guard = None;
                            }
                            qt_thread.queue(|mut qo| {
                                qo.as_mut().export_succeeded();
                            }).unwrap();
                        }
                        Err(marker_errors) => {
                            let reason = format_category_errors(&marker_errors);
                            error(&format!(
                                "prepare_for_database_upgrade: marker-file writes failed after successful export:\n{}",
                                reason
                            ));
                            if let Ok(mut guard) = LAST_EXPORT_FAILURE.lock() {
                                *guard = Some(reason.clone());
                            }
                            qt_thread.queue(move |mut qo| {
                                qo.as_mut().export_failed(QString::from(&reason));
                            }).unwrap();
                        }
                    }
                }
                Err(category_errors) => {
                    let reason = format_category_errors(&category_errors);
                    error(&format!(
                        "prepare_for_database_upgrade: export failed, not writing marker files:\n{}",
                        reason
                    ));
                    if let Ok(mut guard) = LAST_EXPORT_FAILURE.lock() {
                        *guard = Some(reason.clone());
                    }
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().export_failed(QString::from(&reason));
                    }).unwrap();
                }
            }
        });
    }

    /// Proceed with the database upgrade even though the export reported errors.
    ///
    /// Writes the two marker files unconditionally without re-running the export
    /// and without touching the existing `import-me/` folder — a partial export
    /// may still contain valid data for unaffected categories (bookmarks, books,
    /// etc.) that should be imported on the next run.
    ///
    /// Emits `exportSucceeded` when the markers land on disk, or
    /// `exportFailed(reason)` if marker I/O fails (PRD §11.3). Before writing,
    /// the errors the user chose to bypass are logged at `error` level so a
    /// post-mortem bug report can recover the context (PRD §11.6).
    pub fn force_database_upgrade(self: Pin<&mut Self>) {
        // Match the threading style of prepare_for_database_upgrade so callers
        // always observe the same async "button stays disabled until a signal
        // fires" pattern, even though the marker writes themselves are quick.
        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            // Log the export errors that the user chose to bypass so they are
            // available in the log file for later diagnosis.
            let bypassed_reason: Option<String> = LAST_EXPORT_FAILURE
                .lock()
                .ok()
                .and_then(|guard| guard.clone());
            if let Some(reason) = &bypassed_reason {
                error(&format!(
                    "force_database_upgrade(): user is bypassing the following export errors:\n{}",
                    reason
                ));
            } else {
                info("force_database_upgrade(): no stored export-failure reason (called without prior prepare_for_database_upgrade?)");
            }
            info("force_database_upgrade(): writing upgrade marker files after user opted to continue past export failure");

            match write_upgrade_marker_files() {
                Ok(()) => {
                    if let Ok(mut guard) = LAST_EXPORT_FAILURE.lock() {
                        *guard = None;
                    }
                    qt_thread.queue(|mut qo| {
                        qo.as_mut().export_succeeded();
                    }).unwrap();
                }
                Err(marker_errors) => {
                    let reason = format_category_errors(&marker_errors);
                    error(&format!(
                        "force_database_upgrade: marker-file writes failed:\n{}",
                        reason
                    ));
                    if let Ok(mut guard) = LAST_EXPORT_FAILURE.lock() {
                        *guard = Some(reason.clone());
                    }
                    qt_thread.queue(move |mut qo| {
                        qo.as_mut().export_failed(QString::from(&reason));
                    }).unwrap();
                }
            }
        });
    }

    /// Return the absolute path of the `import-me/` folder used to stage user data
    /// for database upgrade. The path is returned regardless of whether the folder
    /// currently exists, so QML can display it to the user for manual inspection.
    pub fn get_import_me_dir_path(&self) -> QString {
        let globals = get_app_globals();
        let path = globals.paths.app_assets_dir.join("import-me");
        QString::from(&path.to_string_lossy().to_string())
    }

    /// Get the version_tag of the highest compatible assets release.
    /// Returns empty string if no compatible release can be determined.
    pub fn get_compatible_asset_version_tag(&self) -> QString {
        match compatible_assets_release() {
            Some(release) => QString::from(&release.version_tag),
            None => QString::from(""),
        }
    }

    /// Get the github_repo of the highest compatible assets release.
    /// Returns empty string if no compatible release can be determined.
    pub fn get_compatible_asset_github_repo(&self) -> QString {
        match compatible_assets_release() {
            Some(release) => QString::from(&release.github_repo),
            None => QString::from(""),
        }
    }

    // =========================================================================
    // Topic Index Functions
    // =========================================================================

    /// Load the topic index data (CIPS general index).
    /// The data is cached after first load, so subsequent calls are fast.
    /// Emits topicIndexLoaded signal when complete.
    pub fn load_topic_index(self: Pin<&mut Self>) {
        info("SuttaBridge::load_topic_index() start");
        let qt_thread = self.qt_thread();
        thread::spawn(move || {
            // Load the topic index (this caches it for future use)
            let _ = topic_index::load_topic_index();
            qt_thread.queue(move |mut qo| {
                qo.as_mut().set_topic_index_loaded(true);
                qo.as_mut().topic_index_loaded_signal();
            }).unwrap();
            info("SuttaBridge::load_topic_index() end");
        });
    }

    /// Check if the topic index has been loaded and cached.
    pub fn is_topic_index_cached(&self) -> bool {
        topic_index::is_topic_index_loaded()
    }

    /// Get the list of available letters (A-Z) in the topic index.
    pub fn get_topic_index_letters(&self) -> QStringList {
        let letters = topic_index::get_letters();
        let mut qlist = QStringList::default();
        for letter in letters {
            qlist.append(QString::from(&letter));
        }
        qlist
    }

    /// Get all headwords for a specific letter as JSON.
    pub fn get_topic_headwords_for_letter(&self, letter: &QString) -> QString {
        let letter_str = letter.to_string();
        let headwords = topic_index::get_headwords_for_letter(&letter_str);
        match serde_json::to_string(&headwords) {
            Ok(json) => QString::from(&json),
            Err(e) => {
                error(&format!("Failed to serialize headwords: {}", e));
                QString::from("[]")
            }
        }
    }

    /// Search headwords and sub-entries with case-insensitive partial matching.
    /// Returns matching headwords as JSON.
    pub fn search_topic_headwords(&self, query: &QString) -> QString {
        let query_str = query.to_string();
        let results = topic_index::search_headwords(&query_str);
        match serde_json::to_string(&results) {
            Ok(json) => QString::from(&json),
            Err(e) => {
                error(&format!("Failed to serialize search results: {}", e));
                QString::from("[]")
            }
        }
    }

    /// Get a headword by its normalized ID as JSON.
    pub fn get_topic_headword_by_id(&self, headword_id: &QString) -> QString {
        let id_str = headword_id.to_string();
        match topic_index::get_headword_by_id(&id_str) {
            Some(headword) => {
                match serde_json::to_string(&headword) {
                    Ok(json) => QString::from(&json),
                    Err(e) => {
                        error(&format!("Failed to serialize headword: {}", e));
                        QString::from("{}")
                    }
                }
            }
            None => QString::from("{}")
        }
    }

    /// Get the letter section for a headword by its ID.
    pub fn get_topic_letter_for_headword_id(&self, headword_id: &QString) -> QString {
        let id_str = headword_id.to_string();
        match topic_index::get_letter_for_headword_id(&id_str) {
            Some(letter) => QString::from(&letter),
            None => QString::from("")
        }
    }

    /// Find a headword ID by its text (for xref navigation).
    pub fn find_topic_headword_id_by_text(&self, target: &QString) -> QString {
        let target_str = target.to_string();
        match topic_index::find_headword_id_by_text(&target_str) {
            Some(headword_id) => QString::from(&headword_id),
            None => QString::from("")
        }
    }

    /// Open the Topic Index window.
    pub fn open_topic_index_window(&self) {
        use crate::api::ffi;
        ffi::callback_open_topic_index_window();
    }

    // =========================================================================
    // Chanting Practice Functions
    // =========================================================================

    pub fn open_chanting_practice_window(&self, window_id: &QString) {
        use crate::api::ffi;
        ffi::callback_open_chanting_practice_window(window_id.clone());
    }

    pub fn open_chanting_review_window(&self, window_id: &QString, section_uid: &QString) {
        use crate::api::ffi;
        ffi::callback_open_chanting_review_window(window_id.clone(), section_uid.clone());
    }

    pub fn get_all_chanting_collections_json(&self) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.get_all_chanting_collections() {
            Ok(collections) => {
                let json = serde_json::to_string(&collections).unwrap_or_else(|_| "[]".to_string());
                QString::from(&json)
            }
            Err(e) => {
                error(&format!("get_all_chanting_collections_json(): {}", e));
                QString::from("[]")
            }
        }
    }

    pub fn get_chanting_section_detail_json(&self, section_uid: &QString) -> QString {
        let app_data = get_app_data();
        let uid_str = section_uid.to_string();
        match app_data.dbm.appdata.get_chanting_section_detail(&uid_str) {
            Ok(Some(detail)) => {
                let json = serde_json::to_string(&detail).unwrap_or_else(|_| "null".to_string());
                QString::from(&json)
            }
            Ok(None) => QString::from("null"),
            Err(e) => {
                error(&format!("get_chanting_section_detail_json(): {}", e));
                QString::from("null")
            }
        }
    }

    pub fn get_chanting_recordings_dir(&self) -> QString {
        let dir = simsapa_backend::get_chanting_recordings_dir();
        // Always return an absolute path for QML (MediaPlayer needs absolute file:// URLs)
        let abs_dir = if dir.is_absolute() {
            dir
        } else {
            std::env::current_dir().unwrap_or_default().join(&dir)
        };
        let canonical = abs_dir.canonicalize().unwrap_or(abs_dir);
        QString::from(&canonical.to_string_lossy().to_string())
    }

    pub fn copy_file_to_chanting_recordings(&self, source_path: &QString, dest_filename: &QString) -> QString {
        let src = std::path::PathBuf::from(source_path.to_string());
        let recordings_dir = simsapa_backend::get_chanting_recordings_dir();

        // Resolve to absolute path
        let abs_recordings_dir = if recordings_dir.is_absolute() {
            recordings_dir
        } else {
            std::env::current_dir().unwrap_or_default().join(&recordings_dir)
        };
        let dest = abs_recordings_dir.join(dest_filename.to_string());

        match src.try_exists() {
            Ok(true) => {},
            Ok(false) => return QString::from(&format!("{{\"error\": \"Source file not found: {}\"}}", src.display())),
            Err(e) => return QString::from(&format!("{{\"error\": \"Cannot check source file: {}\"}}", e)),
        }

        match std::fs::copy(&src, &dest) {
            Ok(_) => {
                let dest_str = dest.canonicalize()
                    .unwrap_or(dest.clone())
                    .to_string_lossy()
                    .to_string();
                QString::from(&format!("{{\"ok\": true, \"dest_path\": \"{}\"}}", dest_str))
            }
            Err(e) => QString::from(&format!("{{\"error\": \"Copy failed: {}\"}}", e)),
        }
    }

    pub fn check_file_exists(&self, file_path: &QString) -> bool {
        let path = std::path::PathBuf::from(file_path.to_string());
        // Try as-is first, then resolve relative to recordings dir
        if let Ok(true) = path.try_exists() { return true }
        // If relative, try resolving against recordings dir
        if !path.is_absolute() {
            let recordings_dir = simsapa_backend::get_chanting_recordings_dir();
            let abs_recordings_dir = if recordings_dir.is_absolute() {
                recordings_dir
            } else {
                std::env::current_dir().unwrap_or_default().join(&recordings_dir)
            };
            let resolved = abs_recordings_dir.join(&path);
            match resolved.try_exists() {
                Ok(exists) => return exists,
                Err(_) => return false,
            }
        }
        false
    }

    pub fn create_chanting_collection(&self, json: &QString) -> QString {
        use simsapa_backend::db::appdata_models::ChantingCollectionJson;
        let app_data = get_app_data();
        let data: ChantingCollectionJson = match serde_json::from_str(&json.to_string()) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        };
        match app_data.dbm.appdata.create_chanting_collection(&data) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn update_chanting_collection(&self, json: &QString) -> QString {
        use simsapa_backend::db::appdata_models::ChantingCollectionJson;
        let app_data = get_app_data();
        let data: ChantingCollectionJson = match serde_json::from_str(&json.to_string()) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        };
        match app_data.dbm.appdata.update_chanting_collection(&data) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn delete_chanting_collection(&self, collection_uid: &QString) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.delete_chanting_collection(&collection_uid.to_string()) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn create_chanting_chant(&self, json: &QString) -> QString {
        use simsapa_backend::db::appdata_models::ChantingChantJson;
        let app_data = get_app_data();
        let data: ChantingChantJson = match serde_json::from_str(&json.to_string()) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        };
        match app_data.dbm.appdata.create_chanting_chant(&data) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn update_chanting_chant(&self, json: &QString) -> QString {
        use simsapa_backend::db::appdata_models::ChantingChantJson;
        let app_data = get_app_data();
        let data: ChantingChantJson = match serde_json::from_str(&json.to_string()) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        };
        match app_data.dbm.appdata.update_chanting_chant(&data) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn delete_chanting_chant(&self, chant_uid: &QString) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.delete_chanting_chant(&chant_uid.to_string()) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn create_chanting_section(&self, json: &QString) -> QString {
        use simsapa_backend::db::appdata_models::ChantingSectionJson;
        let app_data = get_app_data();
        let data: ChantingSectionJson = match serde_json::from_str(&json.to_string()) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        };
        match app_data.dbm.appdata.create_chanting_section(&data) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn update_chanting_section(&self, json: &QString) -> QString {
        use simsapa_backend::db::appdata_models::ChantingSectionJson;
        let app_data = get_app_data();
        let data: ChantingSectionJson = match serde_json::from_str(&json.to_string()) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        };
        match app_data.dbm.appdata.update_chanting_section(&data) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn delete_chanting_section(&self, section_uid: &QString) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.delete_chanting_section(&section_uid.to_string()) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn create_chanting_recording(&self, json: &QString) -> QString {
        use simsapa_backend::db::appdata_models::ChantingRecordingJson;
        let app_data = get_app_data();
        let data: ChantingRecordingJson = match serde_json::from_str(&json.to_string()) {
            Ok(d) => d,
            Err(e) => return QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        };
        match app_data.dbm.appdata.create_chanting_recording(&data) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn delete_chanting_recording(&self, recording_uid: &QString) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.delete_chanting_recording(&recording_uid.to_string()) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn update_recording_label(&self, recording_uid: &QString, label: &QString) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.update_recording_label(&recording_uid.to_string(), &label.to_string()) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn update_recording_markers(&self, recording_uid: &QString, markers_json: &QString) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.update_recording_markers(&recording_uid.to_string(), &markers_json.to_string()) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn update_recording_volume(&self, recording_uid: &QString, volume: f32) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.update_recording_volume(&recording_uid.to_string(), volume) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn update_recording_playback_position(&self, recording_uid: &QString, position_ms: i32) -> QString {
        let app_data = get_app_data();
        match app_data.dbm.appdata.update_recording_playback_position(&recording_uid.to_string(), position_ms) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn generate_waveform_data(self: Pin<&mut Self>, recording_uid: &QString, file_path: &QString, num_bars: i32) {
        let uid_str = recording_uid.to_string();
        let path_str = file_path.to_string();
        let bars = if num_bars > 0 { num_bars as usize } else { 200 };
        let qt_thread = self.qt_thread();

        thread::spawn(move || {
            let waveform_json = match simsapa_backend::waveform::get_waveform_peaks(&path_str, bars) {
                Ok(peaks) => {
                    // Store as object with num_bars metadata
                    let obj = serde_json::json!({
                        "num_bars": bars,
                        "peaks": peaks
                    });
                    serde_json::to_string(&obj).unwrap_or_else(|_| "[]".to_string())
                }
                Err(e) => {
                    warn(&format!("generate_waveform_data error: {}", e));
                    "[]".to_string()
                }
            };

            // Save to database
            let app_data = get_app_data();
            if let Err(e) = app_data.dbm.appdata.update_recording_waveform(&uid_str, &waveform_json) {
                warn(&format!("Failed to save waveform data: {}", e));
            }

            // Emit signal back to QML
            let uid_qstr = QString::from(&uid_str);
            let json_qstr = QString::from(&waveform_json);
            qt_thread.queue(move |mut qo| {
                qo.as_mut().waveform_data_ready(uid_qstr, json_qstr);
            }).unwrap();
        });
    }

    // =========================================================================
    // Chanting Export / Import
    // =========================================================================

    pub fn export_chanting_data(&self, json_selected_uids: &QString, dest_path: &QString) -> QString {
        use simsapa_backend::db::chanting_export::export_chanting_to_zip;

        let json_str = json_selected_uids.to_string();
        let dest = dest_path.to_string();

        // Parse the JSON: { collections: [...], chants: [...], sections: [...] }
        let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(e) => return QString::from(&format!("{{\"error\": \"Invalid JSON: {}\"}}", e)),
        };

        let extract_strings = |key: &str| -> Vec<String> {
            parsed.get(key)
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };

        let collection_uids = extract_strings("collections");
        let chant_uids = extract_strings("chants");
        let section_uids = extract_strings("sections");

        let app_data = get_app_data();
        let dest_path = std::path::Path::new(&dest);

        match export_chanting_to_zip(
            &app_data.dbm.appdata,
            collection_uids,
            chant_uids,
            section_uids,
            dest_path,
        ) {
            Ok(_) => QString::from("{\"ok\": true}"),
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    pub fn import_chanting_data(&self, zip_path: &QString) -> QString {
        use simsapa_backend::db::chanting_export::import_chanting_from_zip;

        let path_str = zip_path.to_string();
        let zip_path = std::path::Path::new(&path_str);
        let recordings_dir = simsapa_backend::get_chanting_recordings_dir();

        let app_data = get_app_data();

        match import_chanting_from_zip(&app_data.dbm.appdata, zip_path, &recordings_dir) {
            Ok(result) => {
                let json = serde_json::json!({
                    "ok": true,
                    "imported": {
                        "collections": result.collections,
                        "chants": result.chants,
                        "sections": result.sections,
                        "recordings": result.recordings,
                    }
                });
                QString::from(&json.to_string())
            }
            Err(e) => QString::from(&format!("{{\"error\": \"{}\"}}", e)),
        }
    }

    // =========================================================================
    // Logger Functions
    // =========================================================================

    /// Log a debug message
    pub fn log_debug(&self, message: &QString) {
        debug(&message.to_string());
    }

    /// Log an info message
    pub fn log_info(&self, message: &QString) {
        info(&message.to_string());
    }

    /// Log a warning message
    pub fn log_warn(&self, message: &QString) {
        warn(&message.to_string());
    }

    /// Log an error message
    pub fn log_error(&self, message: &QString) {
        error(&message.to_string());
    }

    /// Get the current log level as a string
    pub fn get_log_level(&self) -> QString {
        QString::from(&get_log_level_str())
    }

    /// Set the log level from a string (case insensitive)
    /// Returns true if successful, false if the string is not a valid level
    pub fn set_log_level(&self, level: &QString) -> bool {
        set_log_level_str(&level.to_string())
    }

    // --- Bookmark operations ---

    pub fn get_all_bookmark_folders_json(&self) -> QString {
        let app_data = get_app_data();
        let folders = app_data.dbm.appdata.get_all_bookmark_folders();
        let json = serde_json::to_string(&folders).unwrap_or_else(|_| "[]".to_string());
        QString::from(json)
    }

    pub fn get_bookmark_items_for_folder_json(&self, folder_id: i32) -> QString {
        let app_data = get_app_data();
        let items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder_id);
        let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
        QString::from(json)
    }

    pub fn create_bookmark_folder(self: Pin<&mut Self>, name: &QString) -> i32 {
        let app_data = get_app_data();
        match app_data.dbm.appdata.create_bookmark_folder(&name.to_string(), false) {
            Ok(id) => id,
            Err(e) => {
                error(&format!("create_bookmark_folder(): {}", e));
                -1
            }
        }
    }

    pub fn create_bookmark_item(self: Pin<&mut Self>, folder_id: i32, item_json: &QString) -> i32 {
        use simsapa_backend::db::appdata_models::NewBookmarkItem;

        let json_str = item_json.to_string();
        let new_item: NewBookmarkItem = match serde_json::from_str(&json_str) {
            Ok(item) => item,
            Err(e) => {
                error(&format!("create_bookmark_item() parse error: {}", e));
                return -1;
            }
        };

        let app_data = get_app_data();
        match app_data.dbm.appdata.create_bookmark_item(&NewBookmarkItem {
            folder_id,
            ..new_item
        }) {
            Ok(id) => id,
            Err(e) => {
                error(&format!("create_bookmark_item(): {}", e));
                -1
            }
        }
    }

    pub fn update_bookmark_folder(self: Pin<&mut Self>, folder_id: i32, name: &QString) {
        let app_data = get_app_data();
        if let Err(e) = app_data.dbm.appdata.update_bookmark_folder(folder_id, &name.to_string()) {
            error(&format!("update_bookmark_folder(): {}", e));
        }
    }

    pub fn update_bookmark_item(self: Pin<&mut Self>, item_id: i32, item_json: &QString) {
        use simsapa_backend::db::appdata_models::BookmarkItemUpdate;

        let json_str = item_json.to_string();
        let update: BookmarkItemUpdate = match serde_json::from_str(&json_str) {
            Ok(u) => u,
            Err(e) => {
                error(&format!("update_bookmark_item() parse error: {}", e));
                return;
            }
        };

        let app_data = get_app_data();
        if let Err(e) = app_data.dbm.appdata.update_bookmark_item(item_id, &update) {
            error(&format!("update_bookmark_item(): {}", e));
        }
    }

    pub fn delete_bookmark_folder(self: Pin<&mut Self>, folder_id: i32) {
        let app_data = get_app_data();
        if let Err(e) = app_data.dbm.appdata.delete_bookmark_folder(folder_id) {
            error(&format!("delete_bookmark_folder(): {}", e));
        }
    }

    pub fn delete_bookmark_item(self: Pin<&mut Self>, item_id: i32) {
        let app_data = get_app_data();
        if let Err(e) = app_data.dbm.appdata.delete_bookmark_item(item_id) {
            error(&format!("delete_bookmark_item(): {}", e));
        }
    }

    pub fn reorder_bookmark_items(self: Pin<&mut Self>, folder_id: i32, item_ids_json: &QString) {
        let json_str = item_ids_json.to_string();
        let item_ids: Vec<i32> = match serde_json::from_str(&json_str) {
            Ok(ids) => ids,
            Err(e) => {
                error(&format!("reorder_bookmark_items() parse error: {}", e));
                return;
            }
        };

        let app_data = get_app_data();
        if let Err(e) = app_data.dbm.appdata.reorder_bookmark_items(folder_id, &item_ids) {
            error(&format!("reorder_bookmark_items(): {}", e));
        }
    }

    pub fn reorder_bookmark_folders(self: Pin<&mut Self>, folder_ids_json: &QString) {
        let json_str = folder_ids_json.to_string();
        let folder_ids: Vec<i32> = match serde_json::from_str(&json_str) {
            Ok(ids) => ids,
            Err(e) => {
                error(&format!("reorder_bookmark_folders() parse error: {}", e));
                return;
            }
        };

        let app_data = get_app_data();
        if let Err(e) = app_data.dbm.appdata.reorder_bookmark_folders(&folder_ids) {
            error(&format!("reorder_bookmark_folders(): {}", e));
        }
    }

    pub fn move_bookmark_items_to_folder(self: Pin<&mut Self>, item_ids_json: &QString, target_folder_id: i32) {
        let json_str = item_ids_json.to_string();
        let item_ids: Vec<i32> = match serde_json::from_str(&json_str) {
            Ok(ids) => ids,
            Err(e) => {
                error(&format!("move_bookmark_items_to_folder() parse error: {}", e));
                return;
            }
        };

        let app_data = get_app_data();
        if let Err(e) = app_data.dbm.appdata.move_bookmark_items_to_folder(&item_ids, target_folder_id) {
            error(&format!("move_bookmark_items_to_folder(): {}", e));
        }
    }

    pub fn save_last_session(self: Pin<&mut Self>, windows_json: &QString) {
        let json_str = windows_json.to_string();

        #[derive(serde::Deserialize)]
        struct SessionItem {
            item_uid: String,
            table_name: String,
            title: Option<String>,
            tab_group: String,
            #[serde(default)]
            scroll_position: f32,
            #[serde(default)]
            find_query: String,
            #[serde(default)]
            find_match_index: i32,
            #[serde(default)]
            sort_order: i32,
        }

        #[derive(serde::Deserialize)]
        struct SessionWindow {
            name: String,
            items: Vec<SessionItem>,
        }

        let windows: Vec<SessionWindow> = match serde_json::from_str(&json_str) {
            Ok(w) => w,
            Err(e) => {
                error(&format!("save_last_session() parse error: {}", e));
                return;
            }
        };

        let app_data = get_app_data();

        // Delete previous last session folders
        if let Err(e) = app_data.dbm.appdata.delete_last_session_folders() {
            error(&format!("save_last_session() delete old session: {}", e));
            return;
        }

        // Create new session folders and items
        for window in &windows {
            let folder_id = match app_data.dbm.appdata.create_bookmark_folder(&window.name, true) {
                Ok(id) => id,
                Err(e) => {
                    error(&format!("save_last_session() create folder: {}", e));
                    continue;
                }
            };

            for item in &window.items {
                let new_item = simsapa_backend::db::appdata_models::NewBookmarkItem {
                    folder_id,
                    item_uid: item.item_uid.clone(),
                    table_name: item.table_name.clone(),
                    title: item.title.clone(),
                    tab_group: item.tab_group.clone(),
                    scroll_position: item.scroll_position,
                    find_query: item.find_query.clone(),
                    find_match_index: item.find_match_index,
                    sort_order: item.sort_order,
                    is_user_added: true,
                };

                if let Err(e) = app_data.dbm.appdata.create_bookmark_item(&new_item) {
                    error(&format!("save_last_session() create item: {}", e));
                }
            }
        }
    }

    pub fn get_last_session_json(&self) -> QString {
        let app_data = get_app_data();
        let folders = app_data.dbm.appdata.get_last_session_folders();

        let mut result: Vec<serde_json::Value> = Vec::new();
        for folder in &folders {
            let items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder.id);
            result.push(serde_json::json!({
                "name": folder.name,
                "folder_id": folder.id,
                "items": items,
            }));
        }

        let json = serde_json::to_string(&result).unwrap_or_else(|_| "[]".to_string());
        QString::from(json)
    }

    pub fn get_restore_last_session(&self) -> bool {
        let app_data = get_app_data();
        app_data.get_restore_last_session()
    }

    pub fn set_restore_last_session(self: Pin<&mut Self>, value: bool) {
        let app_data = get_app_data();
        app_data.set_restore_last_session(value);
    }

    pub fn get_render_use_flat_results_background(&self) -> bool {
        get_app_data().get_render_use_flat_results_background()
    }

    pub fn set_render_use_flat_results_background(self: Pin<&mut Self>, value: bool) {
        get_app_data().set_render_use_flat_results_background(value);
    }

    pub fn get_render_disable_results_clip(&self) -> bool {
        get_app_data().get_render_disable_results_clip()
    }

    pub fn set_render_disable_results_clip(self: Pin<&mut Self>, value: bool) {
        get_app_data().set_render_disable_results_clip(value);
    }

    pub fn get_render_loop_basic(&self) -> bool {
        get_app_data().get_render_loop_basic()
    }

    pub fn set_render_loop_basic(self: Pin<&mut Self>, value: bool) {
        get_app_data().set_render_loop_basic(value);
    }

    // --- Snippet display settings ---

    pub fn get_snippet_chars_before(&self) -> u32 {
        get_app_data().get_snippet_chars_before() as u32
    }

    pub fn set_snippet_chars_before(self: Pin<&mut Self>, value: u32) {
        get_app_data().set_snippet_chars_before(value as usize);
    }

    pub fn get_snippet_chars_after(&self) -> u32 {
        get_app_data().get_snippet_chars_after() as u32
    }

    pub fn set_snippet_chars_after(self: Pin<&mut Self>, value: u32) {
        get_app_data().set_snippet_chars_after(value as usize);
    }

    pub fn get_snippet_all_chars_before(&self) -> u32 {
        get_app_data().get_snippet_all_chars_before() as u32
    }

    pub fn set_snippet_all_chars_before(self: Pin<&mut Self>, value: u32) {
        get_app_data().set_snippet_all_chars_before(value as usize);
    }

    pub fn get_snippet_all_chars_after(&self) -> u32 {
        get_app_data().get_snippet_all_chars_after() as u32
    }

    pub fn set_snippet_all_chars_after(self: Pin<&mut Self>, value: u32) {
        get_app_data().set_snippet_all_chars_after(value as usize);
    }

    pub fn get_item_height_use_default(&self) -> bool {
        get_app_data().get_item_height_use_default()
    }

    pub fn set_item_height_use_default(self: Pin<&mut Self>, value: bool) {
        get_app_data().set_item_height_use_default(value);
    }

    pub fn get_item_height_fixed(&self) -> u32 {
        get_app_data().get_item_height_fixed() as u32
    }

    pub fn set_item_height_fixed(self: Pin<&mut Self>, value: u32) {
        get_app_data().set_item_height_fixed(value as usize);
    }
}
