use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use diesel::prelude::*;

use stardict::{self, Ifo, WordDefinition};

use crate::get_app_data;
use crate::db;
use crate::db::dictionaries_models::NewDictWord;
use crate::helpers as h;
use crate::logger::{info, warn, error};

/// Stages emitted by the SQL-only StarDict import pipeline.
///
/// The startup re-indexing pass owns all FTS5 / Tantivy writes, so this enum
/// intentionally has no `IndexingFts5` / `IndexingTantivy` variants.
#[derive(Debug, Clone)]
pub enum StardictImportProgress {
    /// Unzipping the archive, entry by entry. `total` is the entry count and
    /// `done` how many have been written; both are `0` for the single "starting
    /// to extract" tick emitted before the archive is opened, which QML renders
    /// as an indeterminate bar.
    Extracting { done: usize, total: usize },
    Parsing,
    /// Reading the entries and writing them, a chunk at a time. `done` is the
    /// number of index entries read so far (inserted or skipped), `total` the
    /// number to read (the index size, or `limit`). Emitted once with
    /// `done: 0` before the first chunk, then after each chunk.
    InsertingWords { done: usize, total: usize },
    /// Emitted as soon as the dictionary's identity is known (after the
    /// `.ifo` is parsed and `stardict::no_cache` loads the dict). `total` is
    /// the raw index item count (`dict.idx.items.len()`), matching the
    /// `Importing … total entries` log line — NOT the later inserted count.
    /// `lang` is not carried here; QML already has it from `start_import`.
    Identified { title: String, total: usize },
    Done,
    Failed { msg: String },
    /// User-initiated abort: the dictionary row and the rows already
    /// inserted are left in the DB so the next startup reconcile picks them
    /// up. `inserted` is the count of rows committed before the abort.
    Aborted { inserted: usize },
}

/// Outcome of a successful `import_stardict_as_new` / `import_user_zip`
/// invocation. `cancelled = true` indicates the user aborted mid-import;
/// in that case `inserted` is the count of rows that survived in the DB
/// (the parent `dictionaries` row is also kept so the next reconcile
/// picks the partial import up).
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    pub dictionary_id: i32,
    pub inserted: usize,
    pub cancelled: bool,
}

/// Read the optional `description=` line from a StarDict `.ifo` file.
///
/// Returns the trimmed value when present and non-empty; otherwise `None`.
/// The underlying `stardict` crate already parses the `.ifo` description
/// field (see `stardict::Ifo::description`), so this is a thin convenience
/// wrapper used by the runtime importer.
pub fn read_ifo_description(unzipped_dir: &Path, physical_stem: &str) -> Option<String> {
    let ifo_path = unzipped_dir.join(format!("{}.ifo", physical_stem));
    let ifo = Ifo::new(ifo_path).ok()?;
    let trimmed = ifo.description.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

/// Replaces bword:// links with ssp:// links.
fn parse_bword_links_to_ssp(definition: &str) -> String {
    // QueryType.words
    let words_route_path = "words";
    definition
        .replace("bword://localhost/", &format!("ssp://{}/", words_route_path))
        .replace("bword://", &format!("ssp://{}/", words_route_path))
}

/// Holds the parsed data extracted from WordDefinition segments.
#[derive(Debug, Default)]
struct DictEntry {
    word: String,
    definition_plain: Option<String>,
    definition_html: Option<String>,
    synonyms: Vec<String>,
}

fn db_entries(x: &DictEntry,
              dictionary_id: i32,
              dictionary_label: &str,
              lang: &str) -> NewDictWord {
    // TODO should we check for conflicting uids? generate with meaning count?
    // NOTE: sanitizing dict word uid causes duplicates because of punctuation, and DPD stardict fails to import to db.
    let uid = h::word_uid(&x.word, dictionary_label);

    // add a Latinized lowercase synonym
    let mut syn = x.synonyms.clone();
    let latin = h::latinize(&x.word).to_lowercase();
    if !syn.contains(&latin) {
        syn.push(latin);
    }

    NewDictWord {
        // copy values
        word: x.word.clone(),
        word_ascii: h::pali_to_ascii(Some(&x.word)),
        language: Some(lang.to_string()),
        definition_plain: x.definition_plain.clone(),
        definition_html: x.definition_html.clone(),
        synonyms: Some(syn.join(", ")),
        // add missing data
        uid,
        dict_label: dictionary_label.to_string(),
        dictionary_id,
        word_nom_sg: None,
        inflections: None,
        phonetic: None,
        transliteration: None,
        meaning_order: None,
        summary: None,
        antonyms: None,
        homonyms: None,
        also_written_as: None,
        see_also: None,
    }
}

/// Parses a WordDefinition, extracting plain text and HTML definitions
/// based on segment types ('m' and 'h').
///
/// It processes the *first* segment of type 'm' or 'h' found.
fn parse_word(word_def: &WordDefinition) -> DictEntry {
    let mut parsed_data = DictEntry {
        // Apply consistent_niggahita to the main word itself
        word: h::consistent_niggahita(Some(word_def.word.clone())),
        ..Default::default()
    };

    // Iterate through segments to find the first relevant one ('m' or 'h')
    for segment in &word_def.segments {
        let clean_text = h::consistent_niggahita(Some(segment.text.clone()));

        match segment.types.as_str() {
            "m" => {
                // Found plain text definition (already owned String)
                parsed_data.definition_plain = Some(clean_text);
                // Stop after finding the first 'm' segment
                // TODO: Check if there are more
                break;
            }
            "h" => {
                // Found HTML definition (parse returns owned String)
                let html_def = parse_bword_links_to_ssp(&clean_text);
                parsed_data.definition_html = Some(html_def);
                // Also generate a plain text version
                parsed_data.definition_plain = Some(h::compact_rich_text(&clean_text));
                // Stop after finding the first 'h' segment
                // TODO: Check if there are more
                break;
            }
            _ => {
                warn(&format!(
                    "Segment type '{}' is not handled for word '{}'",
                    segment.types, parsed_data.word
                ));
            }
        }
    }

    if parsed_data.definition_plain.is_none() && parsed_data.definition_html.is_none() {
        warn(&format!(
            "No 'm' or 'h' type definition found for word: {}",
            parsed_data.word
        ));
    }

    // TODO: handle synonyms
    parsed_data.synonyms = Vec::new();

    parsed_data
}

/// Entries per chunk: the insert transaction, the progress tick and the
/// cancel checkpoint all use this cadence.
const CHUNK_SIZE: usize = 1000;

/// A loaded StarDict and the `.dict` file behind it, so that the `.dict`
/// reader can be reopened.
///
/// The `stardict` crate's dictzip reader keeps every chunk it has inflated
/// for the life of the reader, which for DPD is the whole uncompressed
/// `.dict` (~1 GB). `import_entries()` reopens it after each chunk of
/// entries, which drops that cache.
struct LoadedStardict {
    sd: stardict::StarDictStd,
    dict_file: std::path::PathBuf,
    dict_dz: bool,
}

/// How an `import_entries()` run ended.
#[derive(Debug, Clone, Copy)]
struct InsertOutcome {
    inserted: usize,
    cancelled: bool,
}

/// Read all (or `limit`) index entries and hand them to `insert` in chunks of
/// `chunk_size`, so that at most one chunk of parsed pages is in memory (the
/// DPD pages are ~880 MB of HTML). The entries are read in the order of their
/// offset in the `.dict`, so that each compressed chunk of a `.dict.dz` is
/// inflated once even though the reader is reopened after every chunk of
/// entries (see `LoadedStardict`).
///
/// Colliding uids are disambiguated as each entry is parsed, with a map of the
/// uids seen so far that lives across chunks. Two index entries produce the
/// same uid when they differ only in characters normalized away by
/// `consistent_niggahita` (ṃ/ṁ/ŋ) or by `word_uid_sanitize`. The 2nd, 3rd, …
/// occurrence gets ` N` before the `/{label}` suffix, the shape DPD uses for
/// multi-meaning headwords (`dhamma 1.01/dpd`), matched by the user-query
/// disambiguator in `helpers.rs`. The entry that comes first in the `.dict`
/// keeps the plain uid.
///
/// `cancel` is checked before each chunk: the chunks already inserted stay
/// and `cancelled` is set. `InsertingWords` reports the index entries read
/// so far (inserted or skipped) of the total, after each chunk. An `insert`
/// error emits `Failed` and is returned; deleting anything is the caller's
/// decision.
fn import_entries(
    loaded: &mut LoadedStardict,
    dictionary_id: i32,
    dict_label: &str,
    lang: &str,
    limit: Option<usize>,
    chunk_size: usize,
    progress: &dyn Fn(StardictImportProgress),
    cancel: &AtomicBool,
    insert: &mut dyn FnMut(&[NewDictWord]) -> Result<(), String>,
) -> Result<InsertOutcome, String> {
    let idx = &loaded.sd.idx;
    let ifo = &loaded.sd.ifo;
    let reader = &mut loaded.sd.dict;
    let (dict_file, dict_dz) = (&loaded.dict_file, loaded.dict_dz);

    let mut entries: Vec<(&String, &stardict::idx::IdxEntry)> = idx.items.iter().collect();
    entries.sort_by(|(wa, a), (wb, b)| {
        let offset = |e: &stardict::idx::IdxEntry| e.blocks.first().map_or(usize::MAX, |b| b.offset);
        offset(a).cmp(&offset(b)).then_with(|| wa.cmp(wb))
    });

    let total = limit.map_or(entries.len(), |n| n.min(entries.len()));
    progress(StardictImportProgress::InsertingWords { done: 0, total });

    let label_suffix = format!("/{}", dict_label);
    let mut seen: HashMap<String, u32> = HashMap::with_capacity(total);
    let mut collisions: u32 = 0;

    let mut chunk: Vec<NewDictWord> = Vec::with_capacity(chunk_size);
    let mut read: usize = 0;
    let mut inserted: usize = 0;

    let mut flush = |chunk: &mut Vec<NewDictWord>,
                     reader: &mut stardict::dict::Dict,
                     read: usize,
                     inserted: &mut usize| -> Result<(), String> {
        // Drop the reader's cache of inflated chunks.
        *reader = stardict::dict::Dict::new(dict_file.clone(), dict_dz).map_err(|e| {
            let msg = format!("Error reopening {}: {}", dict_file.display(), e);
            progress(StardictImportProgress::Failed { msg: msg.clone() });
            msg
        })?;
        if !chunk.is_empty() {
            if let Err(e) = insert(chunk) {
                let msg = format!("Batch insertion failed: {}", e);
                progress(StardictImportProgress::Failed { msg: msg.clone() });
                return Err(msg);
            }
            *inserted += chunk.len();
            chunk.clear();
        }
        progress(StardictImportProgress::InsertingWords { done: read, total });
        Ok(())
    };

    for (word, idx_entry) in entries.into_iter() {
        if read >= total {
            break;
        }
        if read % chunk_size == 0 && cancel.load(Ordering::Relaxed) {
            return Ok(InsertOutcome { inserted, cancelled: true });
        }
        read += 1;

        match reader.get_definition(idx_entry, ifo) {
            Ok(Some(def)) => {
                let mut nw = db_entries(&parse_word(&def), dictionary_id, dict_label, lang);
                let count = seen.entry(nw.uid.clone()).or_insert(0);
                *count += 1;
                if *count > 1 {
                    collisions += 1;
                    let base = nw.uid.strip_suffix(&label_suffix).unwrap_or(&nw.uid).to_string();
                    let new_uid = format!("{} {}{}", base, *count, label_suffix);
                    warn(&format!(
                        "Stardict uid collision: '{}' (occurrence {}) -> '{}'",
                        nw.uid, *count, new_uid
                    ));
                    nw.uid = new_uid;
                }
                chunk.push(nw);
            }
            Ok(None) => {
                warn(&format!("No definitions found for index entry: {}", word));
            }
            Err(e) => {
                error(&format!("Failed to get definition for {}: {}", word, e));
            }
        }

        if read % chunk_size == 0 {
            flush(&mut chunk, reader, read, &mut inserted)?;
        }
    }
    if read % chunk_size != 0 || read == 0 {
        flush(&mut chunk, reader, read, &mut inserted)?;
    }

    if collisions > 0 {
        info(&format!(
            "Disambiguated {} colliding stardict uid(s) for '{}'",
            collisions, dict_label
        ));
    }

    Ok(InsertOutcome { inserted, cancelled: false })
}

/// Insert one chunk in its own transaction, so that on cancel the chunks
/// already inserted stay in the database.
fn insert_chunk(db_conn: &mut SqliteConnection, chunk: &[NewDictWord]) -> Result<(), String> {
    db_conn
        .transaction::<_, diesel::result::Error, _>(|tx| db::dictionaries::create_dict_words_batch(tx, chunk))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Load the StarDict behind `<unzipped_dir>/<physical_stem>.ifo` and emit
/// `Identified`. Emits `Failed` on an error.
fn load_stardict(
    unzipped_dir: &Path,
    physical_stem: &str,
    progress: &dyn Fn(StardictImportProgress),
) -> Result<LoadedStardict, String> {
    let fail = |msg: String| {
        progress(StardictImportProgress::Failed { msg: msg.clone() });
        msg
    };
    let ifo_path = unzipped_dir.join(format!("{}.ifo", physical_stem));
    let sd = stardict::no_cache(ifo_path)
        .map_err(|e| fail(format!("Error loading stardict dictionary: {}", e)))?;

    // The same choice `stardict::no_cache` makes: `.dict`, else `.dict.dz`.
    let plain = unzipped_dir.join(format!("{}.dict", physical_stem));
    let (dict_file, dict_dz) = match plain.try_exists() {
        Ok(true) => (plain, false),
        Ok(false) => (unzipped_dir.join(format!("{}.dict.dz", physical_stem)), true),
        Err(e) => return Err(fail(format!("Cannot access {}: {}", plain.display(), e))),
    };

    info(&format!("Importing {}, {} total entries ...", &sd.ifo.bookname, sd.idx.items.len()));
    progress(StardictImportProgress::Identified {
        title: sd.ifo.bookname.clone(),
        total: sd.idx.items.len(),
    });
    Ok(LoadedStardict { sd, dict_file, dict_dz })
}

/// SQL-only StarDict import.
///
/// - Parses the `.ifo` + index and inserts the `dict_words` rows a chunk at a
///   time as they are read (`import_entries()`).
/// - Does NOT touch FTS5 / Tantivy. Index writes are owned by the startup
///   reconciliation pass (see `dict_index_reconcile`).
/// - When `description` is `Some`, that value is stored on the new
///   `dictionaries` row. When `description` is `None` and `is_user_imported`
///   is true, the value is taken from the `.ifo` `description=` field if
///   present (trimmed); otherwise NULL.
/// - `progress` receives stage updates; pass `|_| {}` for no-op.
/// - `indexed_at` is always set to `NULL` so the next startup picks the
///   dictionary up for re-indexing.
/// - `physical_stem` is the basename of the `.ifo`/`.idx`/`.dict` files inside
///   `unzipped_dir` (i.e. how the StarDict archive names them).
/// - `new_dict_label` is the logical label stored on the `dictionaries` row and used as the
///   `{word}/{label}` uid suffix; the two may differ when the user picks a custom label at import time.
/// - The dictionaries write lock is taken for each chunk insert, not for the
///   whole run (a large dictionary takes minutes), so other writers are not
///   blocked meanwhile.
pub fn import_stardict_as_new(
    unzipped_dir: &Path,
    lang: &str,
    physical_stem: &str,
    new_dict_label: &str,
    _ignore_synonyms: bool,
    delete_if_exists: bool,
    limit: Option<usize>,
    is_user_imported: bool,
    description: Option<&str>,
    progress: &dyn Fn(StardictImportProgress),
    cancel: &AtomicBool,
) -> Result<ImportOutcome, String> {
    use crate::db::dictionaries_models::NewDictionary;

    let app_data = get_app_data();

    progress(StardictImportProgress::Parsing);

    let ifo_path = unzipped_dir.join(format!("{}.ifo", physical_stem));
    let ifo = match Ifo::new(ifo_path.clone()) {
        Ok(x) => x,
        Err(e) => {
            let msg = format!("Error parsing ifo: {}, {}", ifo_path.to_string_lossy(), e);
            progress(StardictImportProgress::Failed { msg: msg.clone() });
            return Err(msg);
        }
    };

    if delete_if_exists {
        // Delete dictionary. Associated words are dropped due to cascade.
        match app_data.dbm.dictionaries.delete_dictionary_by_label(new_dict_label) {
            Ok(n) => info(&format!("Deleted {} dictionary.", n)),
            Err(e) => {
                let msg = format!("Error deleting: {}", e);
                progress(StardictImportProgress::Failed { msg: msg.clone() });
                return Err(msg);
            }
        };
    }

    // Resolve description: caller-supplied wins; else fall back to the
    // `.ifo` description for user-imported dictionaries.
    let ifo_description: Option<String> = if description.is_some() {
        None
    } else if is_user_imported {
        let t = ifo.description.trim();
        if t.is_empty() { None } else { Some(t.to_string()) }
    } else {
        None
    };

    // Add the dictionary, store id
    let lang_opt = if lang.is_empty() { None } else { Some(lang) };
    let new_dict = NewDictionary {
        label: new_dict_label,
        title: &ifo.bookname,
        dict_type: "stardict",
        creator: None, // TODO: parse from ifo
        description: description.or(ifo_description.as_deref()),
        feedback_email: None,
        feedback_url: None,
        version: None,
        is_user_imported,
        // Store the language on the dictionary row regardless of
        // `is_user_imported` so the built-in DPD stardict import gets `pli`.
        // `dict_words.language` is set from `lang` in `db_entries`
        // independently.
        language: lang_opt,
        indexed_at: None,
    };

    let dictionary_id = match app_data.dbm.dictionaries.create_dictionary(new_dict) {
        Ok(x) => x.id,
        Err(e) => {
            let msg = format!("{}", e);
            progress(StardictImportProgress::Failed { msg: msg.clone() });
            return Err(msg);
        }
    };

    let mut dict = match load_stardict(unzipped_dir, physical_stem, progress) {
        Ok(d) => d,
        Err(msg) => {
            // Roll back the dictionaries row we just inserted so the DB stays clean.
            let _ = app_data.dbm.dictionaries.delete_dictionary_by_label(new_dict_label);
            return Err(msg);
        }
    };

    let dbh = &app_data.dbm.dictionaries;
    let mut insert = |chunk: &[NewDictWord]| -> Result<(), String> {
        let _lock = dbh.write_lock.lock();
        let mut db_conn = dbh.get_conn().map_err(|e| e.to_string())?;
        insert_chunk(&mut db_conn, chunk)
    };
    let result = import_entries(&mut dict, dictionary_id, new_dict_label, lang, limit, CHUNK_SIZE,
                                progress, cancel, &mut insert);

    match result {
        // On cancel the partial rows and the `dictionaries` row stay, so the
        // next startup reconcile indexes what is there. (On a cancel before
        // the first chunk the bridge removes the empty row.)
        Ok(outcome) => finish_import(&dict.sd.ifo.bookname, dictionary_id, outcome, progress),
        Err(msg) => {
            // A failed chunk is unrecoverable: drop the parent row (and any
            // prior chunks) so a half-imported dictionary does not remain.
            let _ = app_data.dbm.dictionaries.delete_dictionary_by_label(new_dict_label);
            Err(msg)
        }
    }
}

/// Import a StarDict folder's entries into the dictionaries database behind
/// `db_conn`, under an existing `dictionary_id`. Creates no `dictionaries` row
/// and deletes nothing, so it can write into a staging database. The row with
/// `dictionary_id` must exist: `dict_words.dictionary_id` is a foreign key.
///
/// On cancel the rows already inserted are kept and `cancelled` is set.
pub fn import_stardict_into(
    db_conn: &mut SqliteConnection,
    unzipped_dir: &Path,
    physical_stem: &str,
    dict_label: &str,
    lang: &str,
    dictionary_id: i32,
    limit: Option<usize>,
    progress: &dyn Fn(StardictImportProgress),
    cancel: &AtomicBool,
) -> Result<ImportOutcome, String> {
    progress(StardictImportProgress::Parsing);
    let mut dict = load_stardict(unzipped_dir, physical_stem, progress)?;
    let mut insert = |chunk: &[NewDictWord]| insert_chunk(db_conn, chunk);
    let outcome = import_entries(&mut dict, dictionary_id, dict_label, lang, limit, CHUNK_SIZE,
                                 progress, cancel, &mut insert)?;
    finish_import(&dict.sd.ifo.bookname, dictionary_id, outcome, progress)
}

fn finish_import(
    bookname: &str,
    dictionary_id: i32,
    outcome: InsertOutcome,
    progress: &dyn Fn(StardictImportProgress),
) -> Result<ImportOutcome, String> {
    if outcome.cancelled {
        info(&format!("Import aborted for '{}' after {} entries.", bookname, outcome.inserted));
        progress(StardictImportProgress::Aborted { inserted: outcome.inserted });
    } else {
        info(&format!("Import finished for '{}'.", bookname));
        progress(StardictImportProgress::Done);
    }
    Ok(ImportOutcome { dictionary_id, inserted: outcome.inserted, cancelled: outcome.cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Write a StarDict triplet whose index holds `words` (sorted by bytes),
    /// each with a one-word definition.
    fn write_stardict(dir: &Path, stem: &str, words: &[&str]) {
        let mut words: Vec<&str> = words.to_vec();
        words.sort();
        let mut dict_bytes: Vec<u8> = Vec::new();
        let mut idx_bytes: Vec<u8> = Vec::new();
        for w in &words {
            let def = format!("def of {}", w);
            idx_bytes.extend_from_slice(w.as_bytes());
            idx_bytes.push(0);
            idx_bytes.extend_from_slice(&(dict_bytes.len() as u32).to_be_bytes());
            idx_bytes.extend_from_slice(&(def.len() as u32).to_be_bytes());
            dict_bytes.extend_from_slice(def.as_bytes());
        }
        std::fs::File::create(dir.join(format!("{}.dict", stem))).unwrap().write_all(&dict_bytes).unwrap();
        std::fs::write(dir.join(format!("{}.idx", stem)), &idx_bytes).unwrap();
        let ifo = format!(
            "StarDict's dict ifo file\nversion=2.4.2\nwordcount={}\nidxfilesize={}\nbookname=Chunk Test\nsametypesequence=m\n",
            words.len(), idx_bytes.len(),
        );
        std::fs::write(dir.join(format!("{}.ifo", stem)), ifo).unwrap();
    }

    /// Colliding entries that fall in different chunks are still
    /// disambiguated: with one entry per chunk, `kaṃ` and `kaṁ` (one uid
    /// after `consistent_niggahita`) come out as `kaṁ/lbl` and `kaṁ 2/lbl`.
    #[test]
    fn colliding_uids_in_different_chunks_are_disambiguated() {
        let tmp = tempfile::tempdir().unwrap();
        write_stardict(tmp.path(), "t", &["kaṃ", "kaṁ", "other"]);
        let mut dict = load_stardict(tmp.path(), "t", &|_| {}).unwrap();

        let mut chunks: Vec<Vec<String>> = Vec::new();
        let mut insert = |chunk: &[NewDictWord]| -> Result<(), String> {
            chunks.push(chunk.iter().map(|w| w.uid.clone()).collect());
            Ok(())
        };
        let cancel = AtomicBool::new(false);
        let outcome = import_entries(&mut dict, 1, "lbl", "pli", None, 1, &|_| {}, &cancel, &mut insert)
            .unwrap();

        assert_eq!(outcome.inserted, 3);
        assert!(!outcome.cancelled);
        assert_eq!(chunks.len(), 3, "one entry per chunk");
        let mut uids: Vec<String> = chunks.into_iter().flatten().collect();
        uids.sort();
        let base = h::word_uid("kaṁ", "lbl");
        let second = format!("{} 2/lbl", base.strip_suffix("/lbl").unwrap());
        let mut expected = vec![base, second, h::word_uid("other", "lbl")];
        expected.sort();
        assert_eq!(uids, expected);
    }

    /// `limit` caps the entries read, and the progress ticks report entries
    /// read of the capped total, ending at the total.
    #[test]
    fn limit_and_progress_cover_the_entries_read() {
        let tmp = tempfile::tempdir().unwrap();
        write_stardict(tmp.path(), "t", &["a1", "a2", "a3", "a4", "a5"]);
        let mut dict = load_stardict(tmp.path(), "t", &|_| {}).unwrap();

        let ticks = std::cell::RefCell::new(Vec::new());
        let progress = |p: StardictImportProgress| {
            if let StardictImportProgress::InsertingWords { done, total } = p {
                ticks.borrow_mut().push((done, total));
            }
        };
        let mut inserted_rows = 0;
        let mut insert = |chunk: &[NewDictWord]| -> Result<(), String> {
            inserted_rows += chunk.len();
            Ok(())
        };
        let cancel = AtomicBool::new(false);
        let outcome = import_entries(&mut dict, 1, "lbl", "pli", Some(3), 2, &progress, &cancel, &mut insert)
            .unwrap();

        assert_eq!(outcome.inserted, 3);
        assert_eq!(inserted_rows, 3);
        assert_eq!(*ticks.borrow(), vec![(0, 3), (2, 3), (3, 3)]);
    }
}
