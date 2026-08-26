//! Does the lenient directory wrapper cost anything on a normal filesystem?
//!
//! The wrapper exists for volumes whose `flock(2)` answers `ENOSYS`. On an
//! ordinary ext4/btrfs developer machine the probe answers `Supported` and the
//! wrapper delegates straight to `MmapDirectory`, so the only theoretical costs
//! are: one probe per directory (cached for the process), one enum comparison
//! per `acquire_lock`, and one record pushed onto the route log.
//!
//! This test measures that claim instead of asserting it. Both arms run the
//! **identical** sequence — open → `Index::open` → `register_tokenizers` →
//! `reader()` with `ReloadPolicy::Manual` → the two diagnostic query terms —
//! against the **same** on-disk indexes, so the only variable is the
//! `Directory` implementation.
//!
//! Two rules this file follows deliberately:
//!
//! - **Ratios, never millisecond budgets.** The project has known drift in
//!   absolute-time assertions, and these numbers move with page cache state and
//!   machine load. The assertion compares the two arms measured in the same run
//!   on the same machine.
//! - **It never creates an index.** Every directory is opened with
//!   `Index::open`, and a machine without the dev index tree skips with a
//!   message rather than failing.
//!
//! Terms are `nirodha` and `cessation`, the same pair the Storage Diagnostics
//! report uses (`storage_diagnostics::QUERY_TERMS`), so the numbers here are
//! directly comparable with a user's section E.
//!
//! See `docs/fulltext-index-storage-and-file-locking.md`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tantivy::collector::Count;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::{Directory, Index, IndexReader, ReloadPolicy};

use simsapa_backend::search::lenient_directory::{
    flock_probe_count_for_dir, LenientLockMmapDirectory,
};
use simsapa_backend::search::tokenizer::register_tokenizers;
use simsapa_backend::storage_diagnostics::QUERY_TERMS;

/// Iterations per index per arm. The first is discarded (page cache, and the
/// one-time `flock` probe), so the reported figures average `ITERATIONS - 1`.
const ITERATIONS: usize = 6;

/// How much slower the wrapper is allowed to be than the bare directory,
/// aggregated across every index. Generous on purpose: the per-iteration open
/// and reader-build costs are sub-millisecond on this corpus (see the baseline
/// table in the task list), so their ratios are dominated by scheduling noise.
const MAX_RATIO: f64 = 1.25;

/// One arm's timings for one index, averaged over the kept iterations.
#[derive(Debug, Clone, Copy, Default)]
struct Timings {
    open: Duration,
    reader: Duration,
    search: Duration,
}

fn avg(total: Duration, n: usize) -> Duration {
    if n == 0 {
        Duration::ZERO
    } else {
        total / n as u32
    }
}

/// Locate the developer index tree without initialising the app.
///
/// `SIMSAPA_DIR` in the project `.env` is relative to the working directory
/// (`backend/` under `cargo test`), which is why the cwd-relative form is tried
/// first — the same fallback `get_create_simsapa_dir()` makes for the dev
/// workflow. Returns `None` rather than creating anything.
fn index_root() -> Option<PathBuf> {
    let _ = dotenvy::dotenv();
    let raw = std::env::var("SIMSAPA_DIR").ok()?;
    let candidate = PathBuf::from(&raw).join("app-assets").join("index");
    if candidate.try_exists().unwrap_or(false) {
        return Some(candidate);
    }
    None
}

/// Every `<area>/<lang>` directory under the index root that actually holds an
/// index, enumerated rather than hard-coded: this machine's tree has
/// `suttas/hu` and no `suttas/san`, the reporting user's is the other way
/// round.
fn discover_indexes(root: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();

    let areas = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return found,
    };

    for area in areas.flatten() {
        let area_path = area.path();
        if !area_path.is_dir() {
            continue;
        }
        let langs = match std::fs::read_dir(&area_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for lang in langs.flatten() {
            let lang_path = lang.path();
            if !lang_path.is_dir() {
                continue;
            }
            let directory = match MmapDirectory::open(&lang_path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if !Index::exists(&directory).unwrap_or(false) {
                continue;
            }
            let label = format!(
                "{}/{}",
                area_path.file_name().unwrap_or_default().to_string_lossy(),
                lang_path.file_name().unwrap_or_default().to_string_lossy(),
            );
            found.push((label, lang_path));
        }
    }

    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// The measured sequence, parameterised only by which `Directory` opens the
/// index. Returns the three timings plus the document count, so a nonsense
/// result (an empty index) is visible in the printed table.
fn run_once<D, F>(dir: &Path, lang: &str, open_directory: F) -> Option<(Timings, u64)>
where
    D: Directory,
    F: FnOnce(&Path) -> Option<D>,
{
    let started = Instant::now();
    let directory = open_directory(dir)?;
    let index = Index::open(directory).ok()?;
    register_tokenizers(&index, lang);
    let open = started.elapsed();

    let started = Instant::now();
    let reader: IndexReader = index
        .reader_builder()
        // The same policy both arms, and the one the app now ships. A default
        // reader would also spawn a `meta.json` polling thread per index, which
        // would poison `no_meta_file_watcher_threads_with_manual_reload` in this
        // same test binary.
        .reload_policy(ReloadPolicy::Manual)
        .try_into()
        .ok()?;
    let reader_build = started.elapsed();

    let searcher = reader.searcher();
    let num_docs = searcher.num_docs();
    let content_field = index.schema().get_field("content").ok()?;
    let parser = QueryParser::for_index(&index, vec![content_field]);

    let started = Instant::now();
    for term in QUERY_TERMS {
        let query = parser.parse_query(term).ok()?;
        searcher.search(&query, &Count).ok()?;
    }
    let search = started.elapsed();

    Some((
        Timings {
            open,
            reader: reader_build,
            search,
        },
        num_docs,
    ))
}

fn measure<D, F>(dir: &Path, lang: &str, mut open_directory: F) -> Option<(Timings, u64)>
where
    D: Directory,
    F: FnMut(&Path) -> Option<D>,
{
    let mut totals = Timings::default();
    let mut kept = 0usize;
    let mut docs = 0u64;

    for i in 0..ITERATIONS {
        let (t, num_docs) = run_once(dir, lang, &mut open_directory)?;
        docs = num_docs;
        // Discard the first: cold page cache, and the one-time `flock` probe
        // that the wrapper caches for the rest of the process.
        if i == 0 {
            continue;
        }
        totals.open += t.open;
        totals.reader += t.reader;
        totals.search += t.search;
        kept += 1;
    }

    Some((
        Timings {
            open: avg(totals.open, kept),
            reader: avg(totals.reader, kept),
            search: avg(totals.search, kept),
        },
        docs,
    ))
}

fn ratio(wrapper: Duration, bare: Duration) -> f64 {
    if bare.is_zero() {
        return 1.0;
    }
    wrapper.as_secs_f64() / bare.as_secs_f64()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[test]
fn the_wrapper_costs_nothing_measurable_on_a_normal_filesystem() {
    let Some(root) = index_root() else {
        println!(
            "SKIPPED: no fulltext index tree found. Set SIMSAPA_DIR to a data \
             directory containing app-assets/index/ to run this benchmark. \
             This test never creates an index."
        );
        return;
    };

    let indexes = discover_indexes(&root);
    if indexes.is_empty() {
        println!(
            "SKIPPED: {} contains no <area>/<lang> directory holding an index.",
            root.display()
        );
        return;
    }

    let mut bare_total = Timings::default();
    let mut wrapper_total = Timings::default();

    println!(
        "\nLenientLockMmapDirectory vs MmapDirectory — {} iterations per index, first discarded",
        ITERATIONS
    );
    println!("index root: {}", root.display());
    println!(
        "{:<18} {:>9} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>8} {:>8} {:>8}",
        "index",
        "num_docs",
        "open_b_ms",
        "open_w_ms",
        "read_b_ms",
        "read_w_ms",
        "srch_b_ms",
        "srch_w_ms",
        "open_x",
        "read_x",
        "srch_x",
    );

    for (label, dir) in &indexes {
        let lang = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let bare = measure(dir, &lang, |p| MmapDirectory::open(p).ok());
        let wrapper = measure(dir, &lang, |p| LenientLockMmapDirectory::open(p).ok());

        let (Some((bare, docs)), Some((wrapper, _))) = (bare, wrapper) else {
            println!("{:<18} could not be measured (open or query failed)", label);
            continue;
        };

        println!(
            "{:<18} {:>9} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>8.2} {:>8.2} {:>8.2}",
            label,
            docs,
            ms(bare.open),
            ms(wrapper.open),
            ms(bare.reader),
            ms(wrapper.reader),
            ms(bare.search),
            ms(wrapper.search),
            ratio(wrapper.open, bare.open),
            ratio(wrapper.reader, bare.reader),
            ratio(wrapper.search, bare.search),
        );

        bare_total.open += bare.open;
        bare_total.reader += bare.reader;
        bare_total.search += bare.search;
        wrapper_total.open += wrapper.open;
        wrapper_total.reader += wrapper.reader;
        wrapper_total.search += wrapper.search;
    }

    assert!(
        !bare_total.search.is_zero(),
        "no index could be measured under {}",
        root.display()
    );

    let open_ratio = ratio(wrapper_total.open, bare_total.open);
    let reader_ratio = ratio(wrapper_total.reader, bare_total.reader);
    let search_ratio = ratio(wrapper_total.search, bare_total.search);

    println!(
        "\naggregate over {} indexes: open {:.3} ms → {:.3} ms ({:.2}×), reader {:.3} ms → {:.3} ms ({:.2}×), search {:.3} ms → {:.3} ms ({:.2}×)",
        indexes.len(),
        ms(bare_total.open),
        ms(wrapper_total.open),
        open_ratio,
        ms(bare_total.reader),
        ms(wrapper_total.reader),
        reader_ratio,
        ms(bare_total.search),
        ms(wrapper_total.search),
        search_ratio,
    );

    // Aggregated, not per index: one index's sub-millisecond reader build is
    // noise, the sum across all of them is signal.
    assert!(
        search_ratio <= MAX_RATIO,
        "search through the wrapper was {search_ratio:.2}× the bare directory (limit {MAX_RATIO})"
    );
    assert!(
        reader_ratio <= MAX_RATIO,
        "reader build through the wrapper was {reader_ratio:.2}× the bare directory (limit {MAX_RATIO})"
    );

    // The one cost that could scale with query volume if the cache ever broke:
    // `flock_support_for_dir` must answer from its map after the first call, so
    // however many times the wrapper opened each index above, the probe ran
    // once. Per directory, not the process-global counter — `cargo test` runs
    // test binaries in parallel and this assertion must not depend on what
    // another test opened.
    for (label, dir) in &indexes {
        let probes = flock_probe_count_for_dir(dir);
        assert!(
            probes <= 1,
            "flock probe ran {probes} times for {label} across {ITERATIONS} wrapper opens; \
             the per-directory support cache is not holding"
        );
    }
}

/// `ReloadPolicy::Manual` must leave no `meta.json` polling threads behind.
///
/// The default `OnCommitWithDelay` spawns one thread per index that re-reads
/// and CRC32s `meta.json` every 500 ms for the life of the process — six
/// threads and ~12 reads/s against the user's storage volume, growing with
/// every downloaded language.
///
/// The thread is named `thread-tantivy-meta-file-watcher`
/// (`tantivy-0.25.0/src/directory/file_watcher.rs:44`), which Linux truncates
/// to 15 bytes in `/proc/<pid>/task/<tid>/comm`, hence the prefix match. The
/// assertion is on that **name**, never on a thread count: other tests in this
/// binary run in the same process.
#[test]
#[cfg(target_os = "linux")]
fn no_meta_file_watcher_threads_with_manual_reload() {
    let Some(root) = index_root() else {
        println!("SKIPPED: no fulltext index tree found (see the benchmark test).");
        return;
    };
    let indexes = discover_indexes(&root);
    if indexes.is_empty() {
        println!("SKIPPED: no index directories under {}", root.display());
        return;
    }

    // Held for the duration of the check: a dropped reader takes its watcher
    // thread with it, which would make this pass for the wrong reason.
    let mut readers: Vec<IndexReader> = Vec::new();
    for (_, dir) in &indexes {
        let lang = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
        let Ok(directory) = LenientLockMmapDirectory::open(dir) else {
            continue;
        };
        let Ok(index) = Index::open(directory) else {
            continue;
        };
        register_tokenizers(&index, &lang);
        let reader: IndexReader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .expect("reader build");
        readers.push(reader);
    }

    assert!(
        !readers.is_empty(),
        "no index could be opened under {}",
        root.display()
    );

    let mut watchers: Vec<String> = Vec::new();
    if let Ok(tasks) = std::fs::read_dir("/proc/self/task") {
        for task in tasks.flatten() {
            if let Ok(comm) = std::fs::read_to_string(task.path().join("comm")) {
                let name = comm.trim().to_string();
                if name.starts_with("thread-tantivy-") {
                    watchers.push(name);
                }
            }
        }
    }

    assert!(
        watchers.is_empty(),
        "{} tantivy watcher thread(s) present after opening {} indexes with \
         ReloadPolicy::Manual: {:?}. If this is flaky, re-run with \
         --test-threads=1 — another test building a default-policy reader in \
         this process would produce the same names.",
        watchers.len(),
        readers.len(),
        watchers,
    );
}
