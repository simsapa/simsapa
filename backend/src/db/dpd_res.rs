//! The `dpd_res` table of `dpd.sqlite3`: the static files (CSS, fonts, page
//! scripts and the `family_*_json.js` data) that DPD's rendered pages load.
//!
//! The files come from the `res/` folder of the same DPD release as the
//! `dict_words` pages and the structured data, so the three can never come from
//! different releases. The DPD migration imports them; `GET /dpd_res/<path..>`
//! serves them from memory (`DPD_RES_CACHE`), loaded once per process. See
//! docs/dpd-upgrades.md.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context, Result};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Binary, Integer, Text};

use crate::helpers::consistent_niggahita;
use crate::logger::info;

/// How many DPD `dict_words` rows `check_dpd_link_hrefs()` reads, spread over
/// the whole id range.
const LINK_CHECK_SAMPLE_ROWS: i64 = 2000;

/// The mime type a `res/` file is stored and served with, by extension.
pub fn dpd_res_mime_type(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// A text file gets the niggahīta normalisation; every other file is stored
/// byte for byte.
fn is_text_mime(mime: &str) -> bool {
    matches!(mime, "text/css" | "application/javascript" | "application/json")
}

/// Drop and create the `dpd_res` table.
pub fn create_dpd_res_table(conn: &mut SqliteConnection) -> Result<()> {
    conn.batch_execute(
        "DROP TABLE IF EXISTS dpd_res;
         CREATE TABLE dpd_res (
             path TEXT PRIMARY KEY NOT NULL,
             mime_type TEXT NOT NULL,
             content BLOB NOT NULL
         );",
    )
    .context("Failed to create the dpd_res table")
}

/// Every regular file under `res_dir`, as (path relative to `res_dir` with
/// forward slashes, absolute path), sorted by the relative path.
fn list_res_files(res_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    let mut dirs = vec![res_dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = fs::read_dir(&dir)
            .with_context(|| format!("Cannot read the DPD res folder {}", dir.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("Cannot read the DPD res folder {}", dir.display()))?;
            let path = entry.path();
            // Follows symlinks, so a linked file or folder counts as what it names.
            let meta = fs::metadata(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?;
            if meta.is_dir() {
                dirs.push(path);
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(res_dir)
                    .with_context(|| format!("{} is not under {}", path.display(), res_dir.display()))?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                files.push((rel, path));
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Create the `dpd_res` table and import every regular file of `res_dir` into
/// it. Text files (`.css`, `.js`, `.json`) go through `consistent_niggahita()`
/// (ṃ → ṁ, ŋ → ṁ), like the rest of the DPD data; other files are stored byte
/// for byte. The CSS is otherwise stored as DPD ships it: its relative font
/// `url()`s resolve against the CSS's own `/dpd_res/` URL, on whatever port the
/// server runs. Returns the number of files.
pub fn import_dpd_res_dir(conn: &mut SqliteConnection, res_dir: &Path) -> Result<usize> {
    match res_dir.try_exists() {
        Ok(true) => {}
        Ok(false) => anyhow::bail!("DPD res folder not found: {}", res_dir.display()),
        Err(e) => anyhow::bail!("Cannot access the DPD res folder {}: {}", res_dir.display(), e),
    }
    let files = list_res_files(res_dir)?;
    if files.is_empty() {
        anyhow::bail!("DPD res folder is empty: {}", res_dir.display());
    }

    create_dpd_res_table(conn)?;

    let mut total_bytes: usize = 0;
    conn.transaction::<_, anyhow::Error, _>(|conn| {
        for (rel, path) in &files {
            let mime = dpd_res_mime_type(rel);
            let raw = fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
            let content = if is_text_mime(mime) {
                let text = String::from_utf8(raw)
                    .with_context(|| format!("DPD res file is not UTF-8 text: {}", path.display()))?;
                consistent_niggahita(Some(text)).into_bytes()
            } else {
                raw
            };
            total_bytes += content.len();
            sql_query("INSERT INTO dpd_res (path, mime_type, content) VALUES (?, ?, ?)")
                .bind::<Text, _>(rel)
                .bind::<Text, _>(mime)
                .bind::<Binary, _>(&content)
                .execute(conn)
                .with_context(|| format!("Failed to store {} in dpd_res", rel))?;
        }
        Ok(())
    })?;

    info(&format!(
        "import_dpd_res_dir(): {} files, {} bytes from {}",
        files.len(),
        total_bytes,
        res_dir.display()
    ));
    Ok(files.len())
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    n: i64,
}

#[derive(QueryableByName)]
struct DpdResRow {
    #[diesel(sql_type = Text)]
    mime_type: String,
    #[diesel(sql_type = Binary)]
    content: Vec<u8>,
}

#[derive(QueryableByName)]
struct PathRow {
    #[diesel(sql_type = Text)]
    path: String,
}

fn has_dpd_res_table(conn: &mut SqliteConnection) -> QueryResult<bool> {
    let row: CountRow = sql_query(
        "SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'table' AND name = 'dpd_res'",
    )
    .get_result(conn)?;
    Ok(row.n > 0)
}

/// One `res/` file: `(mime_type, content)`, or `None` when the path is not
/// stored or the database has no `dpd_res` table.
pub fn get_dpd_res(conn: &mut SqliteConnection, path: &str) -> QueryResult<Option<(String, Vec<u8>)>> {
    if !has_dpd_res_table(conn)? {
        return Ok(None);
    }
    let row: Option<DpdResRow> = sql_query("SELECT mime_type, content FROM dpd_res WHERE path = ?")
        .bind::<Text, _>(path)
        .get_result(conn)
        .optional()?;
    Ok(row.map(|r| (r.mime_type, r.content)))
}

/// Every path stored in `dpd_res` (empty when there is no table).
pub fn list_dpd_res_paths(conn: &mut SqliteConnection) -> QueryResult<BTreeSet<String>> {
    if !has_dpd_res_table(conn)? {
        return Ok(BTreeSet::new());
    }
    let rows: Vec<PathRow> = sql_query("SELECT path FROM dpd_res").load(conn)?;
    Ok(rows.into_iter().map(|r| r.path).collect())
}

/// One `res/` file held in memory.
#[derive(Debug, Clone)]
pub struct DpdResFile {
    pub mime_type: String,
    pub content: Arc<[u8]>,
}

/// Every `res/` file of one `dpd.sqlite3`, by path.
pub type DpdResFiles = HashMap<String, DpdResFile>;

#[derive(QueryableByName)]
struct DpdResFullRow {
    #[diesel(sql_type = Text)]
    path: String,
    #[diesel(sql_type = Text)]
    mime_type: String,
    #[diesel(sql_type = Binary)]
    content: Vec<u8>,
}

/// Read every row of `dpd_res` (an empty map when there is no table).
pub fn load_all_dpd_res(conn: &mut SqliteConnection) -> QueryResult<DpdResFiles> {
    if !has_dpd_res_table(conn)? {
        return Ok(HashMap::new());
    }
    let rows: Vec<DpdResFullRow> = sql_query("SELECT path, mime_type, content FROM dpd_res").load(conn)?;
    Ok(rows
        .into_iter()
        .map(|r| (r.path, DpdResFile { mime_type: r.mime_type, content: Arc::from(r.content) }))
        .collect())
}

/// The `res/` files in memory, so the page route never reads the database: a
/// DPD headword or root page loads 13 or 14 of them (about 13 MB, mostly the
/// `family_*_json.js` data).
///
/// Loaded once per process, on a background thread after startup
/// (`DpdDbHandle::warm_dpd_res_cache()`), or by the first request if that has
/// not finished. A load that fails is not kept, so the next request tries
/// again; a `dpd.sqlite3` without the table is kept as an empty set, since the
/// table cannot appear while the process runs. Never reloaded: `dpd.sqlite3` is
/// only replaced while the app is not running (the DPD upgrade runs before the
/// app's databases are opened, or from the CLI while the app is closed).
pub struct DpdResCache {
    files: RwLock<Option<Arc<DpdResFiles>>>,
    /// Held while loading, so concurrent first requests and the warm-up load
    /// the table once.
    load_lock: Mutex<()>,
}

impl DpdResCache {
    pub const fn new() -> Self {
        Self { files: RwLock::new(None), load_lock: Mutex::new(()) }
    }

    fn loaded(&self) -> Option<Arc<DpdResFiles>> {
        self.files.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded().is_some()
    }

    /// The loaded files, or the result of `load` stored for later calls.
    pub fn get_or_load(&self, load: impl FnOnce() -> Result<DpdResFiles>) -> Result<Arc<DpdResFiles>> {
        if let Some(files) = self.loaded() {
            return Ok(files);
        }
        let _guard = self.load_lock.lock().unwrap_or_else(|e| e.into_inner());
        // Another caller may have loaded it while this one waited.
        if let Some(files) = self.loaded() {
            return Ok(files);
        }
        let files = Arc::new(load()?);
        *self.files.write().unwrap_or_else(|e| e.into_inner()) = Some(files.clone());
        Ok(files)
    }
}

impl Default for DpdResCache {
    fn default() -> Self {
        Self::new()
    }
}

/// The process-wide copy of the installed `dpd.sqlite3`'s `res/` files.
pub static DPD_RES_CACHE: DpdResCache = DpdResCache::new();

#[derive(QueryableByName)]
struct HtmlRow {
    #[diesel(sql_type = Text)]
    definition_html: String,
}

#[derive(QueryableByName)]
struct RangeRow {
    #[diesel(sql_type = BigInt)]
    n: i64,
}

/// Check that every `<link href="…">` in a sample of the DPD `dict_words`
/// pages (`dictionary_id`) names a path stored in `dpd_res`. The page renderer
/// points each such href at `/dpd_res/<href>` as written, so the check takes
/// the href verbatim. A missing file is an error naming it: reported, not
/// repaired. Returns the number of distinct hrefs found.
pub fn check_dpd_link_hrefs(
    dict_conn: &mut SqliteConnection,
    dpd_dictionary_id: i32,
    dpd_conn: &mut SqliteConnection,
) -> Result<usize> {
    lazy_static::lazy_static! {
        static ref RE_LINK_HREF: regex::Regex =
            regex::Regex::new(r#"<link +[^>]*href=['"]([^'"]+)['"]"#).unwrap();
    }

    let stored = list_dpd_res_paths(dpd_conn).context("Failed to list dpd_res")?;
    if stored.is_empty() {
        anyhow::bail!("dpd_res is empty or missing");
    }

    let total: RangeRow = sql_query("SELECT COUNT(*) AS n FROM dict_words WHERE dictionary_id = ?")
        .bind::<Integer, _>(dpd_dictionary_id)
        .get_result(dict_conn)
        .context("Failed to count the DPD dict_words rows")?;
    if total.n == 0 {
        anyhow::bail!("No DPD dict_words rows (dictionary_id {}) to check the <link href>s of", dpd_dictionary_id);
    }
    let stride = (total.n / LINK_CHECK_SAMPLE_ROWS).max(1);

    let rows: Vec<HtmlRow> = sql_query(
        "SELECT COALESCE(definition_html, '') AS definition_html FROM dict_words \
         WHERE dictionary_id = ? AND id % ? = 0",
    )
    .bind::<Integer, _>(dpd_dictionary_id)
    .bind::<BigInt, _>(stride)
    .load(dict_conn)
    .context("Failed to read the DPD dict_words sample")?;

    let mut hrefs: BTreeSet<String> = BTreeSet::new();
    for r in &rows {
        for caps in RE_LINK_HREF.captures_iter(&r.definition_html) {
            hrefs.insert(caps[1].to_string());
        }
    }

    let missing: Vec<&String> = hrefs.iter().filter(|h| !stored.contains(*h)).collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "DPD pages link to res/ files that are not in dpd_res: {}",
            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        );
    }

    info(&format!(
        "check_dpd_link_hrefs(): {} rows sampled, {} distinct hrefs, all in dpd_res",
        rows.len(),
        hrefs.len()
    ));
    Ok(hrefs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_conn() -> SqliteConnection {
        SqliteConnection::establish(":memory:").unwrap()
    }

    fn write(dir: &Path, rel: &str, bytes: &[u8]) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, bytes).unwrap();
    }

    /// Bytes that contain the UTF-8 encoding of ṃ (E1 B9 83), so a text
    /// conversion applied by mistake would change them.
    const FONT_BYTES: &[u8] = &[0x00, 0x01, 0xE1, 0xB9, 0x83, 0xFF, 0xFE, 0x00, 0x80];

    #[test]
    fn niggahita_in_text_files_only() {
        let tmp = tempfile::tempdir().unwrap();
        let res = tmp.path();
        write(res, "dpd-css-and-fonts.css", "/* saṃgha */ @font-face { src: url('Inter.ttf'); }".as_bytes());
        write(res, "family_idiom_json.js", "var x = {\"saṃvega\": \"ŋ\"};".as_bytes());
        write(res, "feedback_template.js", "const t = `dhammaṃ`;".as_bytes());
        write(res, "Inter.ttf", FONT_BYTES);
        write(res, "sub/extra.js", "aṃ".as_bytes());

        let mut conn = mem_conn();
        assert_eq!(import_dpd_res_dir(&mut conn, res).unwrap(), 5);

        let (mime, css) = get_dpd_res(&mut conn, "dpd-css-and-fonts.css").unwrap().unwrap();
        assert_eq!(mime, "text/css");
        let css = String::from_utf8(css).unwrap();
        assert!(css.contains("saṁgha") && !css.contains('ṃ'));
        // The font URL is stored as DPD ships it.
        assert!(css.contains("url('Inter.ttf')"));

        let (mime, js) = get_dpd_res(&mut conn, "family_idiom_json.js").unwrap().unwrap();
        assert_eq!(mime, "application/javascript");
        assert_eq!(String::from_utf8(js).unwrap(), "var x = {\"saṁvega\": \"ṁ\"};");

        let (_, tpl) = get_dpd_res(&mut conn, "feedback_template.js").unwrap().unwrap();
        assert_eq!(String::from_utf8(tpl).unwrap(), "const t = `dhammaṁ`;");

        let (mime, font) = get_dpd_res(&mut conn, "Inter.ttf").unwrap().unwrap();
        assert_eq!(mime, "font/ttf");
        assert_eq!(font, FONT_BYTES);

        let (_, sub) = get_dpd_res(&mut conn, "sub/extra.js").unwrap().unwrap();
        assert_eq!(String::from_utf8(sub).unwrap(), "aṁ");

        assert!(get_dpd_res(&mut conn, "absent.js").unwrap().is_none());
    }

    #[test]
    fn reimport_replaces_the_table() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "a.js", b"1");
        write(tmp.path(), "b.js", b"2");
        let mut conn = mem_conn();
        import_dpd_res_dir(&mut conn, tmp.path()).unwrap();

        fs::remove_file(tmp.path().join("b.js")).unwrap();
        import_dpd_res_dir(&mut conn, tmp.path()).unwrap();
        let paths = list_dpd_res_paths(&mut conn).unwrap();
        assert_eq!(paths.into_iter().collect::<Vec<_>>(), vec!["a.js".to_string()]);
    }

    #[test]
    fn missing_and_empty_folders_are_errors_naming_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let mut conn = mem_conn();

        let missing = tmp.path().join("no-such-res");
        let err = import_dpd_res_dir(&mut conn, &missing).unwrap_err().to_string();
        assert!(err.contains("not found") && err.contains("no-such-res"), "{err}");

        let empty = tmp.path().join("empty-res");
        fs::create_dir(&empty).unwrap();
        let err = import_dpd_res_dir(&mut conn, &empty).unwrap_err().to_string();
        assert!(err.contains("empty") && err.contains("empty-res"), "{err}");
    }

    #[test]
    fn no_table_reads_as_absent() {
        let mut conn = mem_conn();
        assert!(get_dpd_res(&mut conn, "main.js").unwrap().is_none());
        assert!(list_dpd_res_paths(&mut conn).unwrap().is_empty());
    }

    fn dict_conn_with_pages(pages: &[&str]) -> SqliteConnection {
        let mut conn = mem_conn();
        conn.batch_execute(
            "CREATE TABLE dict_words (id INTEGER PRIMARY KEY, dictionary_id INTEGER, definition_html TEXT);",
        )
        .unwrap();
        for (i, html) in pages.iter().enumerate() {
            sql_query("INSERT INTO dict_words (id, dictionary_id, definition_html) VALUES (?, 7, ?)")
                .bind::<Integer, _>(i as i32 + 1)
                .bind::<Text, _>(*html)
                .execute(&mut conn)
                .unwrap();
        }
        conn
    }

    #[test]
    fn link_href_check() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "dpd-css-and-fonts.css", b"x");
        write(tmp.path(), "main.js", b"x");
        let mut dpd_conn = mem_conn();
        import_dpd_res_dir(&mut dpd_conn, tmp.path()).unwrap();

        let ok_page = r#"<head><link href="dpd-css-and-fonts.css" rel="stylesheet"><link href="main.js" class="load_js" rel="preload"as="script"></head>"#;
        let mut dict_conn = dict_conn_with_pages(&[ok_page, ok_page]);
        assert_eq!(check_dpd_link_hrefs(&mut dict_conn, 7, &mut dpd_conn).unwrap(), 2);

        let bad_page = r#"<head><link href="family_set_template.js" class="load_js"></head>"#;
        let mut dict_conn = dict_conn_with_pages(&[ok_page, bad_page]);
        let err = check_dpd_link_hrefs(&mut dict_conn, 7, &mut dpd_conn).unwrap_err().to_string();
        assert!(err.contains("family_set_template.js"), "{err}");

        // No rows for the dictionary is an error, not a pass.
        let mut dict_conn = dict_conn_with_pages(&[]);
        assert!(check_dpd_link_hrefs(&mut dict_conn, 7, &mut dpd_conn).is_err());
    }

    #[test]
    fn load_all_reads_every_row_and_tolerates_no_table() {
        let mut conn = mem_conn();
        assert!(load_all_dpd_res(&mut conn).unwrap().is_empty());

        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "main.js", "aṃ".as_bytes());
        write(tmp.path(), "Inter.ttf", FONT_BYTES);
        import_dpd_res_dir(&mut conn, tmp.path()).unwrap();

        let files = load_all_dpd_res(&mut conn).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files["main.js"].mime_type, "application/javascript");
        assert_eq!(&*files["main.js"].content, "aṁ".as_bytes());
        assert_eq!(&*files["Inter.ttf"].content, FONT_BYTES);
    }

    #[test]
    fn cache_loads_once_and_retries_after_a_failure() {
        let cache = DpdResCache::new();
        assert!(!cache.is_loaded());

        assert!(cache.get_or_load(|| anyhow::bail!("disk I/O error")).is_err());
        assert!(!cache.is_loaded(), "a failed load is not kept");

        let one = |name: &str| {
            let mut m = DpdResFiles::new();
            m.insert(name.to_string(), DpdResFile { mime_type: "text/css".into(), content: Arc::from(&b"x"[..]) });
            m
        };
        let files = cache.get_or_load(|| Ok(one("a.css"))).unwrap();
        assert!(files.contains_key("a.css"));
        // A second load is never run.
        let again = cache.get_or_load(|| Ok(one("b.css"))).unwrap();
        assert!(again.contains_key("a.css") && !again.contains_key("b.css"));
    }

    #[test]
    fn mime_types() {
        assert_eq!(dpd_res_mime_type("a.CSS"), "text/css");
        assert_eq!(dpd_res_mime_type("x/a.js"), "application/javascript");
        assert_eq!(dpd_res_mime_type("Inter-VariableFont_opsz,wght.ttf"), "font/ttf");
        assert_eq!(dpd_res_mime_type("a.woff2"), "font/woff2");
        assert_eq!(dpd_res_mime_type("dpd.ico"), "image/x-icon");
        assert_eq!(dpd_res_mime_type("README"), "application/octet-stream");
    }
}
