use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use diesel::prelude::*;
use serde::Serialize;
use tantivy::{doc, Directory, DocSet, Index, IndexWriter, Term, TERMINATED};
use tantivy::schema::IndexRecordOption;

use crate::db::DatabaseHandle;
use crate::db::appdata_models::Sutta;
use crate::db::dictionaries_models::DictWord;
use crate::logger::{error, info, warn};
use crate::AppGlobalPaths;

use super::lenient_directory::LenientLockMmapDirectory;
use super::schema::{build_dict_schema, build_library_schema, build_sutta_schema};
use super::tokenizer::register_tokenizers;

/// Lowercase the input and reverse it character-by-character. Used to populate
/// the `*_rev` raw fields so a uid suffix query reduces to a prefix regex.
fn reversed_lowercased(s: &str) -> String {
    s.to_lowercase().chars().rev().collect()
}

/// Open or create a Tantivy index at the given directory path.
pub fn open_or_create_index(dir: &Path, schema: tantivy::schema::Schema, lang: &str) -> Result<Index> {
    match dir.try_exists() {
        Ok(true) => {}
        _ => {
            std::fs::create_dir_all(dir)?;
        }
    }

    // `LenientLockMmapDirectory`, not a bare `MmapDirectory`: on a volume whose
    // `flock(2)` answers ENOSYS every index open fails and the app holds zero
    // indexes. On a normal filesystem the wrapper delegates unchanged. See
    // `docs/fulltext-index-storage-and-file-locking.md`.
    //
    // `open_or_create` is correct here — this is a write path, and the six index
    // builders reach it before their index exists. The *search* path uses
    // `Index::open` instead (`searcher.rs`).
    let mmap_dir = LenientLockMmapDirectory::open(dir)?;
    let index = Index::open_or_create(mmap_dir, schema)?;

    register_tokenizers(&index, lang);

    Ok(index)
}

/// How often (in rows) a running step checks the cancel flag. Cancel is also
/// checked before each step. See `docs/fulltext-index-storage-and-file-locking.md`.
const CANCEL_CHECK_EVERY: usize = 1000;

/// What one build step did: the documents it added, or that it stopped on the
/// cancel flag without committing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepResult {
    Done(u64),
    Cancelled,
}

/// True at a cancel check point (every [`CANCEL_CHECK_EVERY`] rows) when the
/// flag is set.
fn cancel_requested(row: usize, cancel: &AtomicBool) -> bool {
    row % CANCEL_CHECK_EVERY == 0 && cancel.load(Ordering::Relaxed)
}

/// Build fulltext index for suttas of a given language.
pub fn build_sutta_index(appdata_db: &DatabaseHandle, index_dir: &Path, lang: &str) -> Result<()> {
    build_sutta_index_cancellable(appdata_db, index_dir, lang, &AtomicBool::new(false)).map(|_| ())
}

fn build_sutta_index_cancellable(appdata_db: &DatabaseHandle, index_dir: &Path, lang: &str, cancel: &AtomicBool) -> Result<StepResult> {
    use crate::db::appdata_schema::suttas::dsl::*;

    info(&format!("Building sutta index for language: {}", lang));

    let lang_index_dir = index_dir.join(lang);
    let schema = build_sutta_schema(lang);
    let index = open_or_create_index(&lang_index_dir, schema, lang)?;

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    // Clear existing documents before rebuilding
    writer.delete_all_documents()?;
    writer.commit()?;

    let schema = index.schema();
    let uid_field = schema.get_field("uid").unwrap();
    let uid_rev_field = schema.get_field("uid_rev").unwrap();
    let title_field = schema.get_field("title").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let source_uid_field = schema.get_field("source_uid").unwrap();
    let sutta_ref_field = schema.get_field("sutta_ref").unwrap();
    let nikaya_field = schema.get_field("nikaya").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();
    let is_mula_field = schema.get_field("is_mula").unwrap();
    let is_commentary_field = schema.get_field("is_commentary").unwrap();
    let has_other_edition_field = schema.get_field("has_other_edition").unwrap();

    // Only Pāli records have another edition. A table that cannot be read
    // leaves every record without one: search then shows both editions.
    let twins: HashSet<String> = if lang == "pli" {
        match appdata_db.get_conn().and_then(|mut db_conn| crate::pali_edition::load_twins(&mut db_conn)) {
            Ok(twins) => {
                info(&format!("build_sutta_index(): {} records in both Pāli editions", twins.len()));
                twins
            }
            Err(e) => {
                error(&format!("build_sutta_index(): pali_edition_twins could not be read, has_other_edition is false for every record: {}", e));
                HashSet::new()
            }
        }
    } else {
        HashSet::new()
    };

    let lang_clone = lang.to_string();
    let sutta_list: Vec<Sutta> = appdata_db.do_read(|db_conn| {
        suttas
            .filter(language.eq(&lang_clone))
            .select(Sutta::as_select())
            .load(db_conn)
    })?;

    info(&format!("Indexing {} suttas for language {}", sutta_list.len(), lang));

    let mut indexed_count: u64 = 0;
    for (row, sutta) in sutta_list.iter().enumerate() {
        if cancel_requested(row, cancel) {
            // Dropping the writer without commit() discards the added documents.
            info(&format!("Sutta index for {} cancelled after {} documents", lang, indexed_count));
            return Ok(StepResult::Cancelled);
        }

        let plain = sutta.content_plain.as_deref().unwrap_or("");
        if plain.is_empty() {
            continue;
        }

        // Prepend sutta_ref, title, and title_pali to content for better matching
        let sref = &sutta.sutta_ref;
        let t = sutta.title.as_deref().unwrap_or("");
        let tp = sutta.title_pali.as_deref().unwrap_or("");
        let content_text = format!("{} {} {} {}", sref, t, tp, plain);

        let source = sutta.source_uid.as_deref().unwrap_or("");

        let before_first_slash = sutta.uid.split('/').next().unwrap_or("");
        let is_commentary = before_first_slash.contains(".att") || before_first_slash.contains(".tik");
        let is_mula = !is_commentary;

        let uid_rev = reversed_lowercased(&sutta.uid);

        writer.add_document(doc!(
            uid_field => sutta.uid.as_str(),
            uid_rev_field => uid_rev.as_str(),
            title_field => t,
            language_field => sutta.language.as_str(),
            source_uid_field => source,
            sutta_ref_field => sref.as_str(),
            nikaya_field => sutta.nikaya.as_str(),
            content_field => content_text.as_str(),
            content_exact_field => content_text.as_str(),
            is_mula_field => is_mula,
            is_commentary_field => is_commentary,
            has_other_edition_field => twins.contains(&sutta.uid),
        ))?;

        indexed_count += 1;
    }

    // Finalize the index:
    // 1. commit() persists documents and makes them searchable.
    // 2. wait_merging_threads() blocks until background merges finish. Since it takes 'self',
    //    it consumes the writer and explicitly releases the INDEX_WRITER_LOCK.
    // 3. sync_directory() ensures the OS flushes directory metadata, preventing "file changed"
    //    errors during external archival (tar).
    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;

    info(&format!("Sutta index committed: {} documents for language {}", indexed_count, lang));

    Ok(StepResult::Done(indexed_count))
}

/// Build fulltext index for dictionary words of a given language.
pub fn build_dict_index(dict_db: &DatabaseHandle, index_dir: &Path, lang: &str) -> Result<()> {
    build_dict_index_cancellable(dict_db, index_dir, lang, &AtomicBool::new(false)).map(|_| ())
}

fn build_dict_index_cancellable(dict_db: &DatabaseHandle, index_dir: &Path, lang: &str, cancel: &AtomicBool) -> Result<StepResult> {
    use crate::db::dictionaries_schema::dict_words::dsl::*;

    info(&format!("Building dict_words index for language: {}", lang));

    let lang_index_dir = index_dir.join(lang);
    let schema = build_dict_schema(lang);
    let index = open_or_create_index(&lang_index_dir, schema, lang)?;

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    // Clear existing documents before rebuilding
    writer.delete_all_documents()?;
    writer.commit()?;

    let schema = index.schema();
    let uid_field = schema.get_field("uid").unwrap();
    let uid_rev_field = schema.get_field("uid_rev").unwrap();
    let word_field = schema.get_field("word").unwrap();
    let synonyms_field = schema.get_field("synonyms").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let source_uid_field = schema.get_field("source_uid").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();
    let is_bold_definition_field = schema.get_field("is_bold_definition").unwrap();

    let lang_clone = lang.to_string();
    let word_list: Vec<DictWord> = dict_db.do_read(|db_conn| {
        dict_words
            .filter(language.eq(&lang_clone))
            .select(DictWord::as_select())
            .load(db_conn)
    })?;

    info(&format!("Indexing {} dict_words for language {}", word_list.len(), lang));

    let mut indexed_count: u64 = 0;
    for (row, dw) in word_list.iter().enumerate() {
        if cancel_requested(row, cancel) {
            info(&format!("Dict_words index for {} cancelled after {} documents", lang, indexed_count));
            return Ok(StepResult::Cancelled);
        }

        let def = dw.definition_plain.as_deref().unwrap_or("");
        if def.is_empty() {
            continue;
        }

        // Prepend word and synonyms to content for better matching
        let w = &dw.word;
        let syn = dw.synonyms.as_deref().unwrap_or("");
        let content_text = format!("{} {} {}", w, syn, def);

        let lang_val = dw.language.as_deref().unwrap_or("");

        let uid_rev = reversed_lowercased(&dw.uid);

        writer.add_document(doc!(
            uid_field => dw.uid.as_str(),
            uid_rev_field => uid_rev.as_str(),
            word_field => w.as_str(),
            synonyms_field => syn,
            language_field => lang_val,
            source_uid_field => dw.dict_label.as_str(),
            content_field => content_text.as_str(),
            content_exact_field => content_text.as_str(),
            is_bold_definition_field => false,
        ))?;

        indexed_count += 1;
    }

    // Finalize the index:
    // See build_sutta_index for details on why wait_merging_threads and sync_directory are used.
    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;

    info(&format!("Dict_words index committed: {} documents for language {}", indexed_count, lang));

    Ok(StepResult::Done(indexed_count))
}

/// Append the rows for a single `dict_label` to the per-language dict index
/// without rebuilding it.
///
/// Used by the user-facing StarDict import path so a freshly imported
/// dictionary becomes searchable without rescanning every other dictionary's
/// rows. Any previously indexed docs whose `source_uid` equals `dict_label`
/// are deleted first, so re-imports of the same label don't duplicate docs.
/// Bold-definition docs are untouched (their `source_uid` is the per-row
/// `ref_code`, not a dictionary label).
pub fn append_dict_label_to_dict_index(
    dict_db: &DatabaseHandle,
    dict_words_index_dir: &Path,
    lang: &str,
    dict_label_value: &str,
) -> Result<()> {
    use crate::db::dictionaries_schema::dict_words::dsl::*;

    info(&format!(
        "Appending dict_label '{}' to dict index for language: {}",
        dict_label_value, lang
    ));

    let lang_index_dir = dict_words_index_dir.join(lang);
    let schema = build_dict_schema(lang);
    let index = open_or_create_index(&lang_index_dir, schema, lang)?;

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let schema = index.schema();
    let uid_field = schema.get_field("uid").unwrap();
    let uid_rev_field = schema.get_field("uid_rev").unwrap();
    let word_field = schema.get_field("word").unwrap();
    let synonyms_field = schema.get_field("synonyms").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let source_uid_field = schema.get_field("source_uid").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();
    let is_bold_definition_field = schema.get_field("is_bold_definition").unwrap();

    // Drop any prior docs for this label so a re-import doesn't duplicate.
    // `source_uid` uses the raw tokenizer (see schema.rs), so the label is
    // stored as a single term and matches via `delete_term`.
    writer.delete_term(Term::from_field_text(source_uid_field, dict_label_value));

    // Only the indexed columns: the DPD's `definition_html` alone is ~880 MB,
    // which would otherwise be loaded and never read.
    let lang_clone = lang.to_string();
    let label_clone = dict_label_value.to_string();
    #[allow(clippy::type_complexity)]
    let word_list: Vec<(String, String, Option<String>, Option<String>, Option<String>, String)> = dict_db.do_read(|db_conn| {
        dict_words
            .filter(language.eq(&lang_clone))
            .filter(dict_label.eq(&label_clone))
            .select((uid, word, synonyms, definition_plain, language, dict_label))
            .load(db_conn)
    })?;

    info(&format!(
        "Indexing {} dict_words for label '{}' / language {}",
        word_list.len(), dict_label_value, lang
    ));

    let mut indexed_count = 0;
    for (dw_uid, w, dw_synonyms, dw_plain, dw_language, dw_label) in &word_list {
        let def = dw_plain.as_deref().unwrap_or("");
        if def.is_empty() {
            continue;
        }

        let syn = dw_synonyms.as_deref().unwrap_or("");
        let content_text = format!("{} {} {}", w, syn, def);

        let lang_val = dw_language.as_deref().unwrap_or("");

        let uid_rev = reversed_lowercased(dw_uid);

        writer.add_document(doc!(
            uid_field => dw_uid.as_str(),
            uid_rev_field => uid_rev.as_str(),
            word_field => w.as_str(),
            synonyms_field => syn,
            language_field => lang_val,
            source_uid_field => dw_label.as_str(),
            content_field => content_text.as_str(),
            content_exact_field => content_text.as_str(),
            is_bold_definition_field => false,
        ))?;

        indexed_count += 1;
    }

    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;

    info(&format!(
        "dict_label append committed: {} documents for '{}' / {}",
        indexed_count, dict_label_value, lang
    ));

    Ok(())
}

/// Append DPD bold-definition rows to the unified dict index.
///
/// Bold-definition docs share the dict schema (distinguished by
/// `is_bold_definition = true`) and live in the `lang`-keyed subdir of
/// `dict_words_index_dir` so a single tantivy query against the dict index
/// returns both kinds of doc with internally-consistent BM25.
///
/// Must be called *after* `build_dict_index` for the same language, since
/// `build_dict_index` calls `delete_all_documents()` first; this function
/// only appends and never clears.
///
/// Commentary text is Pāli; call sites pass `lang = "pli"` so the index uses
/// the Pāli tokenizers matching DPD dictionary entries.
pub fn append_bold_definitions_to_dict_index(
    dpd_db: &DatabaseHandle,
    dict_words_index_dir: &Path,
    lang: &str,
) -> Result<()> {
    append_bold_definitions_cancellable(dpd_db, dict_words_index_dir, lang, &AtomicBool::new(false)).map(|_| ())
}

fn append_bold_definitions_cancellable(
    dpd_db: &DatabaseHandle,
    dict_words_index_dir: &Path,
    lang: &str,
    cancel: &AtomicBool,
) -> Result<StepResult> {
    use crate::db::dpd_schema::bold_definitions::dsl as bd_dsl;

    info(&format!("Appending bold_definitions to dict index for language: {}", lang));

    let lang_index_dir = dict_words_index_dir.join(lang);
    let schema = build_dict_schema(lang);
    let index = open_or_create_index(&lang_index_dir, schema, lang)?;

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let schema = index.schema();
    let uid_field = schema.get_field("uid").unwrap();
    let uid_rev_field = schema.get_field("uid_rev").unwrap();
    let word_field = schema.get_field("word").unwrap();
    let synonyms_field = schema.get_field("synonyms").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let source_uid_field = schema.get_field("source_uid").unwrap();
    let nikaya_group_path_field = schema.get_field("nikaya_group_path").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();
    let is_bold_definition_field = schema.get_field("is_bold_definition").unwrap();

    // Read in id-ordered pages of only the columns the document uses: the
    // whole table with every column is ~370k rows and hundreds of MB.
    type BoldRow = (i32, String, String, String, String, String, String, String, String);
    const PAGE: i64 = 10_000;
    let mut last_id: i32 = 0;
    let mut read_count = 0;
    let mut indexed_count: u64 = 0;
    loop {
        // PAGE is a multiple of CANCEL_CHECK_EVERY, so the row index restarts
        // at a check point on every page.
        if cancel.load(Ordering::Relaxed) {
            info(&format!("bold_definitions append cancelled after {} documents", indexed_count));
            return Ok(StepResult::Cancelled);
        }

        let rows: Vec<BoldRow> = dpd_db.do_read(|db_conn| {
            bd_dsl::bold_definitions
                .select((bd_dsl::id, bd_dsl::uid, bd_dsl::bold, bd_dsl::ref_code, bd_dsl::nikaya,
                         bd_dsl::book, bd_dsl::title, bd_dsl::subhead, bd_dsl::commentary_plain))
                .filter(bd_dsl::id.gt(last_id))
                .order(bd_dsl::id.asc())
                .limit(PAGE)
                .load(db_conn)
        })?;
        let Some(last) = rows.last() else { break };
        last_id = last.0;
        read_count += rows.len();

        for (row, (_id, uid, bold, ref_code, nikaya, book, title, subhead, plain)) in rows.iter().enumerate() {
            if row > 0 && cancel_requested(row, cancel) {
                info(&format!("bold_definitions append cancelled after {} documents", indexed_count));
                return Ok(StepResult::Cancelled);
            }

            if plain.is_empty() {
                continue;
            }

            let group_path = [nikaya.as_str(), book.as_str(), title.as_str(), subhead.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<&str>>()
                .join(" / ");

            let uid_rev = reversed_lowercased(uid);

            writer.add_document(doc!(
                uid_field => uid.as_str(),
                uid_rev_field => uid_rev.as_str(),
                word_field => bold.as_str(),
                synonyms_field => "",
                language_field => lang,
                source_uid_field => ref_code.as_str(),
                nikaya_group_path_field => group_path.as_str(),
                content_field => plain.as_str(),
                content_exact_field => plain.as_str(),
                is_bold_definition_field => true,
            ))?;

            indexed_count += 1;
        }
    }
    info(&format!("Indexed {} of {} bold_definitions rows", indexed_count, read_count));

    // Finalize the index: see build_sutta_index for the rationale behind
    // wait_merging_threads() and sync_directory().
    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;

    info(&format!("bold_definitions index committed: {} documents", indexed_count));

    Ok(StepResult::Done(indexed_count))
}

/// Build fulltext index for library book chapters of a given language.
pub fn build_library_index(appdata_db: &DatabaseHandle, index_dir: &Path, lang: &str) -> Result<()> {
    build_library_index_cancellable(appdata_db, index_dir, lang, &AtomicBool::new(false)).map(|_| ())
}

fn build_library_index_cancellable(appdata_db: &DatabaseHandle, index_dir: &Path, lang: &str, cancel: &AtomicBool) -> Result<StepResult> {
    use crate::db::appdata_schema::book_spine_items::dsl as spine_dsl;
    use crate::db::appdata_schema::books::dsl as books_dsl;

    info(&format!("Building library index for language: {}", lang));

    let lang_index_dir = index_dir.join(lang);
    let schema = build_library_schema(lang);
    let index = open_or_create_index(&lang_index_dir, schema, lang)?;

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    // Clear existing documents before rebuilding
    writer.delete_all_documents()?;
    writer.commit()?;

    let schema = index.schema();
    let spine_item_uid_field = schema.get_field("spine_item_uid").unwrap();
    let spine_item_uid_rev_field = schema.get_field("spine_item_uid_rev").unwrap();
    let book_uid_field = schema.get_field("book_uid").unwrap();
    let book_title_field = schema.get_field("book_title").unwrap();
    let author_field = schema.get_field("author").unwrap();
    let title_field = schema.get_field("title").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();

    // Load all spine items joined with their books
    let items: Vec<(crate::db::appdata_models::BookSpineItem, crate::db::appdata_models::Book)> = appdata_db.do_read(|db_conn| {
        spine_dsl::book_spine_items
            .inner_join(books_dsl::books.on(books_dsl::id.eq(spine_dsl::book_id)))
            .select((crate::db::appdata_models::BookSpineItem::as_select(), crate::db::appdata_models::Book::as_select()))
            .load(db_conn)
    })?;

    info(&format!("Found {} book spine items total", items.len()));

    let mut indexed_count: u64 = 0;
    for (row, (spine_item, book)) in items.iter().enumerate() {
        if cancel_requested(row, cancel) {
            info(&format!("Library index for {} cancelled after {} documents", lang, indexed_count));
            return Ok(StepResult::Cancelled);
        }

        // Determine effective language: spine_item.language > book.language > "en"
        let effective_lang = spine_item.language.as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| book.language.as_deref().filter(|s| !s.is_empty()))
            .unwrap_or("en")
            .to_lowercase();

        if effective_lang != lang {
            continue;
        }

        let plain = spine_item.content_plain.as_deref().unwrap_or("");
        if plain.is_empty() {
            continue;
        }

        let book_title = book.title.as_deref().unwrap_or("");
        let chapter_title = spine_item.title.as_deref().unwrap_or("");
        let author = book.author.as_deref().unwrap_or("");

        // Prepend book_title, chapter title, and author to content for better matching
        let content_text = format!("{} {} {} {}", book_title, chapter_title, author, plain);

        let spine_item_uid_rev = reversed_lowercased(&spine_item.spine_item_uid);

        writer.add_document(doc!(
            spine_item_uid_field => spine_item.spine_item_uid.as_str(),
            spine_item_uid_rev_field => spine_item_uid_rev.as_str(),
            book_uid_field => spine_item.book_uid.as_str(),
            book_title_field => book_title,
            author_field => author,
            title_field => chapter_title,
            language_field => effective_lang.as_str(),
            content_field => content_text.as_str(),
            content_exact_field => content_text.as_str(),
        ))?;

        indexed_count += 1;
    }

    // Finalize the index
    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;

    info(&format!("Library index committed: {} documents for language {}", indexed_count, lang));

    Ok(StepResult::Done(indexed_count))
}

/// Get distinct effective languages across all library book spine items.
///
/// Uses the fallback chain: spine_item.language > book.language > "en".
pub fn get_library_languages(appdata_db: &DatabaseHandle) -> Result<Vec<String>> {
    use crate::db::appdata_schema::book_spine_items::dsl as spine_dsl;
    use crate::db::appdata_schema::books::dsl as books_dsl;

    let items: Vec<(Option<String>, Option<String>)> = appdata_db.do_read(|db_conn| {
        spine_dsl::book_spine_items
            .inner_join(books_dsl::books.on(books_dsl::id.eq(spine_dsl::book_id)))
            .select((spine_dsl::language, books_dsl::language))
            .load(db_conn)
    })?;

    let mut langs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (spine_lang, book_lang) in items {
        let effective = spine_lang.as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| book_lang.as_deref().filter(|s| !s.is_empty()))
            .unwrap_or("en")
            .to_lowercase();
        langs.insert(effective);
    }

    Ok(langs.into_iter().collect())
}

/// Get distinct sutta languages from the appdata database.
pub fn get_sutta_languages(appdata_db: &DatabaseHandle) -> Result<Vec<String>> {
    use crate::db::appdata_schema::suttas::dsl::*;

    let langs = appdata_db.do_read(|db_conn| {
        suttas
            .select(language)
            .distinct()
            .load::<String>(db_conn)
    })?;

    Ok(langs
        .into_iter()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_lowercase())
        .collect())
}

/// Get distinct dict_word languages from the dictionaries database.
pub fn get_dict_word_languages(dict_db: &DatabaseHandle) -> Result<Vec<String>> {
    use crate::db::dictionaries_schema::dict_words::dsl::*;

    let langs = dict_db.do_read(|db_conn| {
        dict_words
            .select(language)
            .filter(language.is_not_null())
            .distinct()
            .load::<Option<String>>(db_conn)
    })?;

    Ok(langs
        .into_iter()
        .flatten()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_lowercase())
        .collect())
}

// ---------------------------------------------------------------------------
// Building every index, step by step
// ---------------------------------------------------------------------------

/// The area one build step writes. Not `cli_support::IndexArea`: that one is
/// the CLI's `--area` filter and has no bold definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStepArea {
    Suttas,
    DictWords,
    Library,
    BoldDefinitions,
}

impl IndexStepArea {
    /// All areas, in build order.
    pub const ALL: [IndexStepArea; 4] = [
        IndexStepArea::Suttas,
        IndexStepArea::DictWords,
        IndexStepArea::Library,
        IndexStepArea::BoldDefinitions,
    ];

    /// The machine name used in the build summary JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            IndexStepArea::Suttas => "suttas",
            IndexStepArea::DictWords => "dict_words",
            IndexStepArea::Library => "library",
            IndexStepArea::BoldDefinitions => "bold_definitions",
        }
    }

    /// The name shown to the user.
    pub fn label(&self) -> &'static str {
        match self {
            IndexStepArea::Suttas => "Suttas",
            IndexStepArea::DictWords => "Dictionary words",
            IndexStepArea::Library => "Library",
            IndexStepArea::BoldDefinitions => "DPD bold definitions",
        }
    }
}

/// One step of an index build: one area, one language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStep {
    pub area: IndexStepArea,
    pub lang: String,
}

impl IndexStep {
    /// E.g. `Suttas (pli)`, `DPD bold definitions (pli)`.
    pub fn label(&self) -> String {
        format!("{} ({})", self.area.label(), self.lang)
    }
}

/// The DPD bold definitions are Pāli commentary text, appended into the `pli`
/// dictionary index.
pub const BOLD_DEFINITIONS_LANG: &str = "pli";

/// The whole build plan, worked out before the first step runs so the total is
/// known from the start: suttas per language, then dictionary words per
/// language, then library per language, then the DPD bold definitions. The bold
/// definitions come after the dictionary words because `build_dict_index`
/// clears the `pli` index that they are appended to.
pub fn plan_index_steps(sutta_langs: &[String], dict_langs: &[String], library_langs: &[String]) -> Vec<IndexStep> {
    let mut steps = Vec::new();
    let mut push = |area: IndexStepArea, langs: &[String]| {
        for lang in langs {
            steps.push(IndexStep { area, lang: lang.clone() });
        }
    };
    push(IndexStepArea::Suttas, sutta_langs);
    push(IndexStepArea::DictWords, dict_langs);
    push(IndexStepArea::Library, library_langs);
    steps.push(IndexStep { area: IndexStepArea::BoldDefinitions, lang: BOLD_DEFINITIONS_LANG.to_string() });
    steps
}

/// The documents and languages one area received in a build.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct IndexBuildAreaSummary {
    /// `suttas` | `dict_words` | `library` | `bold_definitions`
    pub area: String,
    pub documents: u64,
    pub languages: Vec<String>,
}

/// What a finished build did. Every area is listed, in build order, also when
/// it had no languages (0 documents).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct IndexBuildSummary {
    pub elapsed_secs: u64,
    pub areas: Vec<IndexBuildAreaSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexBuildOutcome {
    Completed(IndexBuildSummary),
    /// Stopped on the cancel flag. The step that was running was not committed,
    /// no `VERSION` was written and the `INCOMPLETE` marker is left in place.
    Cancelled,
}

/// Run one step against its database and index folder.
fn run_index_step(
    step: &IndexStep,
    appdata_db: &DatabaseHandle,
    dict_db: &DatabaseHandle,
    dpd_db: &DatabaseHandle,
    paths: &AppGlobalPaths,
    cancel: &AtomicBool,
) -> Result<StepResult> {
    let lang = step.lang.as_str();
    match step.area {
        IndexStepArea::Suttas => build_sutta_index_cancellable(appdata_db, &paths.suttas_index_dir, lang, cancel),
        IndexStepArea::DictWords => build_dict_index_cancellable(dict_db, &paths.dict_words_index_dir, lang, cancel),
        IndexStepArea::Library => build_library_index_cancellable(appdata_db, &paths.library_index_dir, lang, cancel),
        IndexStepArea::BoldDefinitions => append_bold_definitions_cancellable(dpd_db, &paths.dict_words_index_dir, lang, cancel),
    }
}

/// Run a step plan in order, reporting each step before it starts and checking
/// `cancel` before each step and inside each step (see [`CANCEL_CHECK_EVERY`]).
///
/// Writes the `INCOMPLETE` marker in `paths.index_dir` before the first step
/// and, on success, writes `VERSION` and then removes the marker. A cancel or
/// an error leaves the marker, so the fulltext status reports the index as
/// incomplete. Used for the full build and for the CLI's filtered builds.
///
/// `full_plan` says whether `steps` is every step of the build. A filtered
/// build that finishes leaves a marker it found in place: building one area or
/// language does not complete an index that an earlier build left unfinished.
#[allow(clippy::too_many_arguments)]
pub fn build_steps_with_progress(
    steps: &[IndexStep],
    full_plan: bool,
    appdata_db: &DatabaseHandle,
    dict_db: &DatabaseHandle,
    dpd_db: &DatabaseHandle,
    paths: &AppGlobalPaths,
    on_step: &mut dyn FnMut(usize, usize, &IndexStep),
    cancel: &AtomicBool,
) -> Result<IndexBuildOutcome> {
    let started = Instant::now();
    let total = steps.len();

    let was_incomplete = is_index_incomplete(&paths.index_dir);
    write_incomplete_marker(&paths.index_dir)?;

    let mut areas: Vec<IndexBuildAreaSummary> = IndexStepArea::ALL
        .iter()
        .map(|a| IndexBuildAreaSummary { area: a.as_str().to_string(), documents: 0, languages: Vec::new() })
        .collect();

    for (i, step) in steps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            info(&format!("Index build cancelled before step {}/{}", i + 1, total));
            return Ok(IndexBuildOutcome::Cancelled);
        }

        on_step(i + 1, total, step);
        info(&format!("Index build step {}/{}: {}", i + 1, total, step.label()));

        match run_index_step(step, appdata_db, dict_db, dpd_db, paths, cancel)? {
            StepResult::Cancelled => return Ok(IndexBuildOutcome::Cancelled),
            StepResult::Done(documents) => {
                let idx = IndexStepArea::ALL.iter().position(|a| *a == step.area).unwrap_or(0);
                let summary = &mut areas[idx];
                summary.documents += documents;
                if !summary.languages.contains(&step.lang) {
                    summary.languages.push(step.lang.clone());
                }
            }
        }
    }

    write_version_file(&paths.index_dir)?;
    if full_plan || !was_incomplete {
        remove_incomplete_marker(&paths.index_dir)?;
    } else {
        info("Filtered index build finished; the index was already incomplete, so the INCOMPLETE marker stays");
    }

    Ok(IndexBuildOutcome::Completed(IndexBuildSummary {
        elapsed_secs: started.elapsed().as_secs(),
        areas,
    }))
}

/// Build every index for every language found in the databases, reporting
/// progress and stopping on `cancel`.
///
/// `on_step(step, total, &step)` is called once before each step, with a
/// 1-based step number. The whole plan is read before the first step, so
/// `total` is fixed from the first call.
pub fn build_all_indexes_with_progress(
    appdata_db: &DatabaseHandle,
    dict_db: &DatabaseHandle,
    dpd_db: &DatabaseHandle,
    paths: &AppGlobalPaths,
    on_step: &mut dyn FnMut(usize, usize, &IndexStep),
    cancel: &AtomicBool,
) -> Result<IndexBuildOutcome> {
    info("Building all fulltext indexes...");

    let sutta_langs = get_sutta_languages(appdata_db)?;
    info(&format!("Sutta languages: {:?}", sutta_langs));
    let dict_langs = get_dict_word_languages(dict_db)?;
    info(&format!("Dict_word languages: {:?}", dict_langs));
    let library_langs = get_library_languages(appdata_db)?;
    info(&format!("Library languages: {:?}", library_langs));

    let steps = plan_index_steps(&sutta_langs, &dict_langs, &library_langs);

    let outcome = build_steps_with_progress(&steps, true, appdata_db, dict_db, dpd_db, paths, on_step, cancel)?;
    match &outcome {
        IndexBuildOutcome::Completed(summary) => {
            info(&format!("All fulltext indexes built successfully in {}s.", summary.elapsed_secs));
        }
        IndexBuildOutcome::Cancelled => info("Building all fulltext indexes was cancelled."),
    }
    Ok(outcome)
}

/// Build all fulltext indexes for all languages found in the databases.
pub fn build_all_indexes(
    appdata_db: &DatabaseHandle,
    dict_db: &DatabaseHandle,
    dpd_db: &DatabaseHandle,
    paths: &AppGlobalPaths,
) -> Result<()> {
    let never = AtomicBool::new(false);
    build_all_indexes_with_progress(appdata_db, dict_db, dpd_db, paths, &mut |_, _, _| {}, &never)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The INCOMPLETE marker
// ---------------------------------------------------------------------------

/// A file in `index_dir` that is present from the first build step until
/// `VERSION` has been written. A cancelled, failed or interrupted build leaves
/// it, and the fulltext status then reports the index as incomplete. A missing
/// `VERSION` alone could not tell that apart from an index an older Simsapa
/// built.
pub const INCOMPLETE_MARKER: &str = "INCOMPLETE";

pub fn write_incomplete_marker(index_dir: &Path) -> Result<()> {
    match index_dir.try_exists() {
        Ok(true) => {}
        _ => std::fs::create_dir_all(index_dir)?,
    }
    std::fs::write(index_dir.join(INCOMPLETE_MARKER), "")?;
    Ok(())
}

pub fn remove_incomplete_marker(index_dir: &Path) -> Result<()> {
    let marker = index_dir.join(INCOMPLETE_MARKER);
    if let Ok(true) = marker.try_exists() {
        std::fs::remove_file(&marker)?;
    }
    Ok(())
}

/// Whether the last build into `index_dir` did not finish.
pub fn is_index_incomplete(index_dir: &Path) -> bool {
    matches!(index_dir.join(INCOMPLETE_MARKER).try_exists(), Ok(true))
}

// ---------------------------------------------------------------------------
// Index versioning
// ---------------------------------------------------------------------------

/// **Bump this whenever anything in `super::schema` changes.**
///
/// It is the only thing that detects a stale index now. The search path uses
/// `Index::open`, which takes whatever schema is on disk; the
/// `Index::open_or_create` it replaced compared schemas and refused a mismatch.
/// Without a bump, an index built by an older Simsapa opens silently and fails
/// per query against fields that are not in it. `is_index_current()` reads this
/// against the `VERSION` file and the app offers a rebuild. See
/// `docs/fulltext-index-storage-and-file-locking.md`.
pub const INDEX_VERSION: &str = "1.1";

/// Write a VERSION file to the index directory.
pub fn write_version_file(index_dir: &Path) -> Result<()> {
    match index_dir.try_exists() {
        Ok(true) => {}
        _ => {
            std::fs::create_dir_all(index_dir)?;
        }
    }

    let version_path = index_dir.join("VERSION");
    std::fs::write(&version_path, INDEX_VERSION)?;

    info(&format!("Wrote index VERSION file: {}", version_path.display()));
    Ok(())
}

/// Remove the VERSION file, if there is one. The rebuild does this before it
/// deletes the index folder, so a deletion that stops part way leaves an index
/// that reads as outdated rather than current.
pub fn remove_version_file(index_dir: &Path) -> Result<()> {
    let version_path = index_dir.join("VERSION");
    if let Ok(true) = version_path.try_exists() {
        std::fs::remove_file(&version_path)?;
    }
    Ok(())
}

/// Read the VERSION file from the index directory.
pub fn read_version_file(index_dir: &Path) -> Result<String> {
    let version_path = index_dir.join("VERSION");
    let version = std::fs::read_to_string(version_path)?;
    Ok(version.trim().to_string())
}

/// Check if the index is current (VERSION file matches INDEX_VERSION).
pub fn is_index_current(index_dir: &Path) -> bool {
    match index_dir.try_exists() {
        Ok(true) => {}
        _ => return false,
    }

    match read_version_file(index_dir) {
        Ok(version) => version == INDEX_VERSION,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Per-dictionary index helpers (used by the startup reconciliation pass).
//
// These helpers operate on a single language subdirectory of the unified
// dict index and use the `source_uid` raw-text field as the dict-label
// term, mirroring the bootstrap StarDict import path.
// ---------------------------------------------------------------------------

/// Append `dict_words` (already loaded from SQL) into the per-language dict index.
///
/// This is purely additive: callers should first
/// [`delete_from_dict_index_by_source_uid`] for the same label to make the
/// operation idempotent.
///
/// `on_progress` is called as `(done, total)` at chunk boundaries (and once at
/// the end with the final count). `on_finalize` is called once after the last
/// word is queued but before the (potentially slow) commit/merge/sync, so the
/// caller can tell the user that disk-writing is in progress and the app is not
/// stuck.
pub fn index_dict_words_into_dict_index<F, G>(
    index_dir: &Path,
    lang: &str,
    words: &[crate::db::dictionaries_models::DictWord],
    on_progress: F,
    on_finalize: G,
) -> Result<()>
where
    F: Fn(usize, usize),
    G: Fn(),
{
    let lang_index_dir = index_dir.join(lang);
    let schema = build_dict_schema(lang);
    let index = open_or_create_index(&lang_index_dir, schema, lang)?;

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let schema = index.schema();
    let uid_field = schema.get_field("uid").unwrap();
    let uid_rev_field = schema.get_field("uid_rev").unwrap();
    let word_field = schema.get_field("word").unwrap();
    let synonyms_field = schema.get_field("synonyms").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let source_uid_field = schema.get_field("source_uid").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();
    let is_bold_definition_field = schema.get_field("is_bold_definition").unwrap();

    let total = words.len();
    let chunk = 1000usize;

    let mut indexed = 0usize;
    for (i, dw) in words.iter().enumerate() {
        let def = dw.definition_plain.as_deref().unwrap_or("");
        if def.is_empty() {
            continue;
        }
        let w = &dw.word;
        let syn = dw.synonyms.as_deref().unwrap_or("");
        let content_text = format!("{} {} {}", w, syn, def);
        let lang_val = dw.language.as_deref().unwrap_or(lang);
        let uid_rev = reversed_lowercased(&dw.uid);

        writer.add_document(doc!(
            uid_field => dw.uid.as_str(),
            uid_rev_field => uid_rev.as_str(),
            word_field => w.as_str(),
            synonyms_field => syn,
            language_field => lang_val,
            source_uid_field => dw.dict_label.as_str(),
            content_field => content_text.as_str(),
            content_exact_field => content_text.as_str(),
            is_bold_definition_field => false,
        ))?;
        indexed += 1;

        if (i + 1) % chunk == 0 {
            on_progress(i + 1, total);
        }
    }

    // Show the final count (including the trailing partial chunk) before the
    // commit, then signal the finalize phase so the UI can switch to an
    // indeterminate "Finalizing index" indication during the slow commit/merge.
    on_progress(total, total);
    on_finalize();

    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;

    info(&format!(
        "index_dict_words_into_dict_index: indexed {} of {} words for lang={}",
        indexed, total, lang
    ));
    Ok(())
}

/// Delete every bold-definition document (`is_bold_definition = true`) from
/// one language's dict index folder (`index/dict_words/<lang>`). Their
/// `source_uid` is the row's `ref_code`, so a delete by dictionary label does
/// not reach them; `append_bold_definitions_to_dict_index()` only appends, so
/// a re-index calls this first. A missing folder is not an error.
pub fn delete_bold_definitions_from_dict_index(lang_index_dir: &Path, lang: &str) -> Result<()> {
    match lang_index_dir.try_exists() {
        Ok(true) => {}
        _ => return Ok(()),
    }
    let schema = build_dict_schema(lang);
    let index = open_or_create_index(lang_index_dir, schema, lang)?;
    let mut writer: IndexWriter = index.writer(50_000_000)?;
    let field = index.schema().get_field("is_bold_definition").unwrap();
    writer.delete_term(Term::from_field_bool(field, true));
    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;
    info(&format!("Deleted the bold-definition documents from {}", lang_index_dir.display()));
    Ok(())
}

/// Delete all dict-index documents whose `source_uid` equals `label`,
/// across every per-language subdir under `index_dir`.
pub fn delete_from_dict_index_by_source_uid(index_dir: &Path, label: &str) -> Result<()> {
    match index_dir.try_exists() {
        Ok(true) => {}
        _ => return Ok(()),
    }

    for entry in std::fs::read_dir(index_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let lang = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let schema = build_dict_schema(&lang);
        let mmap_dir = match LenientLockMmapDirectory::open(&path) {
            Ok(d) => d,
            Err(e) => {
                warn(&format!("delete_from_dict_index: open {}: {}", path.display(), e));
                continue;
            }
        };
        let index = match Index::open_or_create(mmap_dir, schema) {
            Ok(i) => i,
            Err(e) => {
                warn(&format!("delete_from_dict_index: open index {}: {}", path.display(), e));
                continue;
            }
        };
        super::tokenizer::register_tokenizers(&index, &lang);

        let mut writer: IndexWriter = index.writer(50_000_000)?;
        let source_uid_field = index.schema().get_field("source_uid").unwrap();
        let term = tantivy::Term::from_field_text(source_uid_field, label);
        writer.delete_term(term);
        writer.commit()?;
        writer.wait_merging_threads()?;
        index.directory().sync_directory()?;
    }

    Ok(())
}

/// Enumerate distinct `source_uid` term values across every per-language
/// subdir of the unified dict index.
pub fn list_indexed_source_uids_in_dict_index(index_dir: &Path) -> Result<HashSet<String>> {
    let mut out: HashSet<String> = HashSet::new();

    match index_dir.try_exists() {
        Ok(true) => {}
        _ => return Ok(out),
    }

    for entry in std::fs::read_dir(index_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let lang = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let schema = build_dict_schema(&lang);
        let mmap_dir = match LenientLockMmapDirectory::open(&path) {
            Ok(d) => d,
            Err(e) => {
                warn(&format!("list_indexed_source_uids: open {}: {}", path.display(), e));
                continue;
            }
        };
        let index = match Index::open_or_create(mmap_dir, schema) {
            Ok(i) => i,
            Err(e) => {
                warn(&format!("list_indexed_source_uids: open index {}: {}", path.display(), e));
                continue;
            }
        };
        super::tokenizer::register_tokenizers(&index, &lang);

        let reader = match index.reader() {
            Ok(r) => r,
            Err(e) => {
                warn(&format!("list_indexed_source_uids: reader {}: {}", path.display(), e));
                continue;
            }
        };
        let searcher = reader.searcher();
        let source_uid_field = index.schema().get_field("source_uid").unwrap();

        for segment_reader in searcher.segment_readers() {
            let inv = match segment_reader.inverted_index(source_uid_field) {
                Ok(i) => i,
                Err(_) => continue,
            };
            // Documents are removed with `delete_term` (a tombstone) but the
            // term itself survives in the term dictionary until a segment merge
            // garbage-collects it. Enumerating terms alone would therefore keep
            // reporting a deleted dictionary's `source_uid` forever, so the
            // startup reconcile would treat it as an orphan on EVERY launch.
            // Only report a term if at least one of its documents is still
            // alive (not tombstoned) in this segment.
            let alive = segment_reader.alive_bitset();
            let mut stream = match inv.terms().stream() {
                Ok(s) => s,
                Err(_) => continue,
            };
            while stream.advance() {
                let key = stream.key();
                let s = match std::str::from_utf8(key) {
                    Ok(s) if !s.is_empty() => s.to_string(),
                    _ => continue,
                };
                // Cheap skip: already confirmed alive in another segment.
                if out.contains(&s) {
                    continue;
                }
                let term_info = stream.value();
                let mut postings = match inv.read_postings_from_terminfo(term_info, IndexRecordOption::Basic) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let mut has_alive = false;
                let mut doc = postings.doc();
                while doc != TERMINATED {
                    let is_alive = alive.map_or(true, |a| a.is_alive(doc));
                    if is_alive {
                        has_alive = true;
                        break;
                    }
                    doc = postings.advance();
                }
                if has_alive {
                    out.insert(s);
                }
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy::doc;

    /// Regression: a dictionary deleted from the index via `delete_term`
    /// leaves a tombstone — the term survives in the term dictionary until a
    /// segment merge. `list_indexed_source_uids_in_dict_index` must NOT report
    /// such tombstoned-only `source_uid`s, otherwise the startup reconcile
    /// keeps treating the deleted dictionary as an orphan and pops the indexing
    /// window on every launch.
    #[test]
    fn list_indexed_excludes_tombstoned_source_uids() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let index_dir = tmp.path().to_path_buf();
        let lang_dir = index_dir.join("en");
        std::fs::create_dir_all(&lang_dir).unwrap();

        let schema = build_dict_schema("en");
        let source_uid_field = schema.get_field("source_uid").unwrap();

        let mmap = tantivy::directory::MmapDirectory::open(&lang_dir).unwrap();
        let index = Index::open_or_create(mmap, schema).unwrap();
        register_tokenizers(&index, "en");
        {
            let mut writer: IndexWriter = index.writer(50_000_000).unwrap();
            writer.add_document(doc!(source_uid_field => "keep_label")).unwrap();
            writer.add_document(doc!(source_uid_field => "remove_label")).unwrap();
            writer.commit().unwrap();
        }

        let before = list_indexed_source_uids_in_dict_index(&index_dir).unwrap();
        assert!(before.contains("keep_label"), "keep_label should be indexed");
        assert!(before.contains("remove_label"), "remove_label should be indexed");

        // Tombstone every doc for remove_label (does not remove the term).
        delete_from_dict_index_by_source_uid(&index_dir, "remove_label").unwrap();

        let after = list_indexed_source_uids_in_dict_index(&index_dir).unwrap();
        assert!(after.contains("keep_label"), "live label must still be reported");
        assert!(!after.contains("remove_label"),
            "tombstoned-only label must NOT be reported (regression: perpetual reconcile popup)");
    }

    fn langs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plan_index_steps_order_total_and_labels() {
        let steps = plan_index_steps(&langs(&["pli", "en"]), &langs(&["en", "pli"]), &langs(&["en"]));
        let labels: Vec<String> = steps.iter().map(|s| s.label()).collect();
        assert_eq!(labels, vec![
            "Suttas (pli)",
            "Suttas (en)",
            "Dictionary words (en)",
            "Dictionary words (pli)",
            "Library (en)",
            "DPD bold definitions (pli)",
        ]);
        assert_eq!(steps[5].area, IndexStepArea::BoldDefinitions);
    }

    #[test]
    fn plan_index_steps_with_no_languages_still_has_the_bold_definitions() {
        let steps = plan_index_steps(&[], &[], &[]);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].label(), "DPD bold definitions (pli)");
    }

    #[test]
    fn incomplete_marker_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let index_dir = tmp.path().join("index");
        assert!(!is_index_incomplete(&index_dir), "no folder: not incomplete");

        write_incomplete_marker(&index_dir).unwrap();
        assert!(is_index_incomplete(&index_dir));

        remove_incomplete_marker(&index_dir).unwrap();
        assert!(!is_index_incomplete(&index_dir));
        // Removing a missing marker is not an error.
        remove_incomplete_marker(&index_dir).unwrap();
    }

    #[test]
    fn remove_version_file_makes_the_index_not_current() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let index_dir = tmp.path().join("index");
        write_version_file(&index_dir).unwrap();
        assert!(is_index_current(&index_dir));

        remove_version_file(&index_dir).unwrap();
        assert!(!is_index_current(&index_dir));
        // Removing a missing VERSION is not an error.
        remove_version_file(&index_dir).unwrap();
    }

    /// Paths from the dist `SIMSAPA_DIR` (see `backend/.env`) with every index
    /// folder moved into `tmp`, and handles for the three dist databases.
    /// `None` when the dist databases are absent.
    fn dist_dbs_with_temp_index(tmp: &Path) -> Option<(AppGlobalPaths, DatabaseHandle, DatabaseHandle, DatabaseHandle)> {
        dotenvy::dotenv().ok();
        let mut paths = AppGlobalPaths::new();
        for db in [&paths.appdata_db_path, &paths.dict_db_path, &paths.dpd_db_path] {
            if !matches!(db.try_exists(), Ok(true)) {
                return None;
            }
        }
        paths.index_dir = tmp.join("index");
        paths.suttas_index_dir = paths.index_dir.join("suttas");
        paths.dict_words_index_dir = paths.index_dir.join("dict_words");
        paths.library_index_dir = paths.index_dir.join("library");

        let appdata = DatabaseHandle::new(&paths.appdata_database_url).ok()?;
        let dict = DatabaseHandle::new(&paths.dict_database_url).ok()?;
        let dpd = DatabaseHandle::new(&paths.dpd_database_url).ok()?;
        Some((paths, appdata, dict, dpd))
    }

    #[test]
    fn build_all_indexes_with_cancel_already_set_builds_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let Some((paths, appdata, dict, dpd)) = dist_dbs_with_temp_index(tmp.path()) else {
            return;
        };

        let cancel = AtomicBool::new(true);
        let mut calls = 0;
        let outcome = build_all_indexes_with_progress(
            &appdata, &dict, &dpd, &paths, &mut |_, _, _| calls += 1, &cancel,
        ).unwrap();

        assert_eq!(outcome, IndexBuildOutcome::Cancelled);
        assert_eq!(calls, 0, "no step may start");
        assert!(is_index_incomplete(&paths.index_dir), "the marker stays after a cancel");
        assert!(!matches!(paths.index_dir.join("VERSION").try_exists(), Ok(true)), "no VERSION after a cancel");
    }

    #[test]
    fn cancel_set_during_step_one_stops_before_step_two() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let Some((paths, appdata, dict, dpd)) = dist_dbs_with_temp_index(tmp.path()) else {
            return;
        };

        // A library language no book has keeps step 1 small. Step 2 would be
        // a full Pāli sutta build if the cancel were missed.
        let steps = vec![
            IndexStep { area: IndexStepArea::Library, lang: "dummy-none".to_string() },
            IndexStep { area: IndexStepArea::Suttas, lang: "pli".to_string() },
        ];

        let cancel = AtomicBool::new(false);
        let mut started: Vec<(usize, usize)> = Vec::new();
        let outcome = build_steps_with_progress(
            &steps, false, &appdata, &dict, &dpd, &paths,
            &mut |i, total, _step| {
                started.push((i, total));
                cancel.store(true, Ordering::Relaxed);
            },
            &cancel,
        ).unwrap();

        assert_eq!(outcome, IndexBuildOutcome::Cancelled);
        assert_eq!(started, vec![(1, 2)], "only step 1 may start");
        assert!(is_index_incomplete(&paths.index_dir));
        assert!(!matches!(paths.index_dir.join("VERSION").try_exists(), Ok(true)), "no VERSION after a cancel");
        assert!(!matches!(paths.suttas_index_dir.join("pli").try_exists(), Ok(true)), "step 2 must not touch its index");
    }

    #[test]
    fn a_finished_plan_writes_version_removes_the_marker_and_lists_every_area() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let Some((paths, appdata, dict, dpd)) = dist_dbs_with_temp_index(tmp.path()) else {
            return;
        };

        let steps = vec![IndexStep { area: IndexStepArea::Library, lang: "dummy-none".to_string() }];
        let cancel = AtomicBool::new(false);
        let outcome = build_steps_with_progress(&steps, false, &appdata, &dict, &dpd, &paths, &mut |_, _, _| {}, &cancel).unwrap();

        let IndexBuildOutcome::Completed(summary) = outcome else { panic!("expected Completed") };
        let names: Vec<&str> = summary.areas.iter().map(|a| a.area.as_str()).collect();
        assert_eq!(names, vec!["suttas", "dict_words", "library", "bold_definitions"]);
        assert_eq!(summary.areas[2].documents, 0);
        assert_eq!(summary.areas[2].languages, vec!["dummy-none".to_string()]);
        assert!(summary.areas[0].languages.is_empty());
        assert!(!is_index_incomplete(&paths.index_dir));
        assert!(is_index_current(&paths.index_dir));
    }

    #[test]
    fn a_filtered_build_keeps_the_marker_of_an_unfinished_build() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let Some((paths, appdata, dict, dpd)) = dist_dbs_with_temp_index(tmp.path()) else {
            return;
        };

        // An earlier full build stopped part way.
        write_incomplete_marker(&paths.index_dir).unwrap();

        let steps = vec![IndexStep { area: IndexStepArea::Library, lang: "dummy-none".to_string() }];
        let cancel = AtomicBool::new(false);

        let outcome = build_steps_with_progress(&steps, false, &appdata, &dict, &dpd, &paths, &mut |_, _, _| {}, &cancel).unwrap();
        assert!(matches!(outcome, IndexBuildOutcome::Completed(_)));
        assert!(is_index_incomplete(&paths.index_dir), "a filtered build must not clear the marker");

        // A build of the whole plan does clear it.
        let outcome = build_steps_with_progress(&steps, true, &appdata, &dict, &dpd, &paths, &mut |_, _, _| {}, &cancel).unwrap();
        assert!(matches!(outcome, IndexBuildOutcome::Completed(_)));
        assert!(!is_index_incomplete(&paths.index_dir), "a full build clears the marker");
    }
}
