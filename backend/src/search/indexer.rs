use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use diesel::prelude::*;
use tantivy::{doc, Directory, DocSet, Index, IndexWriter, Term, TERMINATED};
use tantivy::schema::IndexRecordOption;

use crate::db::DatabaseHandle;
use crate::db::appdata_models::Sutta;
use crate::db::dictionaries_models::DictWord;
use crate::db::dpd_models::BoldDefinition;
use crate::logger::{info, warn};
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

/// Build fulltext index for suttas of a given language.
pub fn build_sutta_index(appdata_db: &DatabaseHandle, index_dir: &Path, lang: &str) -> Result<()> {
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

    let lang_clone = lang.to_string();
    let sutta_list: Vec<Sutta> = appdata_db.do_read(|db_conn| {
        suttas
            .filter(language.eq(&lang_clone))
            .select(Sutta::as_select())
            .load(db_conn)
    })?;

    info(&format!("Indexing {} suttas for language {}", sutta_list.len(), lang));

    let mut indexed_count = 0;
    for sutta in &sutta_list {
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

    Ok(())
}

/// Build fulltext index for dictionary words of a given language.
pub fn build_dict_index(dict_db: &DatabaseHandle, index_dir: &Path, lang: &str) -> Result<()> {
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

    let mut indexed_count = 0;
    for dw in &word_list {
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

    Ok(())
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

    let lang_clone = lang.to_string();
    let label_clone = dict_label_value.to_string();
    let word_list: Vec<DictWord> = dict_db.do_read(|db_conn| {
        dict_words
            .filter(language.eq(&lang_clone))
            .filter(dict_label.eq(&label_clone))
            .select(DictWord::as_select())
            .load(db_conn)
    })?;

    info(&format!(
        "Indexing {} dict_words for label '{}' / language {}",
        word_list.len(), dict_label_value, lang
    ));

    let mut indexed_count = 0;
    for dw in &word_list {
        let def = dw.definition_plain.as_deref().unwrap_or("");
        if def.is_empty() {
            continue;
        }

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

    let rows: Vec<BoldDefinition> = dpd_db.do_read(|db_conn| {
        bd_dsl::bold_definitions
            .select(BoldDefinition::as_select())
            .load(db_conn)
    })?;

    info(&format!("Indexing {} bold_definitions rows", rows.len()));

    let mut indexed_count = 0;
    for row in &rows {
        let plain = row.commentary_plain.as_str();
        if plain.is_empty() {
            continue;
        }

        let group_path = [
            row.nikaya.as_str(),
            row.book.as_str(),
            row.title.as_str(),
            row.subhead.as_str(),
        ]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<&str>>()
        .join(" / ");

        let uid_rev = reversed_lowercased(&row.uid);

        writer.add_document(doc!(
            uid_field => row.uid.as_str(),
            uid_rev_field => uid_rev.as_str(),
            word_field => row.bold.as_str(),
            synonyms_field => "",
            language_field => lang,
            source_uid_field => row.ref_code.as_str(),
            nikaya_group_path_field => group_path.as_str(),
            content_field => plain,
            content_exact_field => plain,
            is_bold_definition_field => true,
        ))?;

        indexed_count += 1;
    }

    // Finalize the index: see build_sutta_index for the rationale behind
    // wait_merging_threads() and sync_directory().
    writer.commit()?;
    writer.wait_merging_threads()?;
    index.directory().sync_directory()?;

    info(&format!("bold_definitions index committed: {} documents", indexed_count));

    Ok(())
}

/// Build fulltext index for library book chapters of a given language.
pub fn build_library_index(appdata_db: &DatabaseHandle, index_dir: &Path, lang: &str) -> Result<()> {
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

    let mut indexed_count = 0;
    for (spine_item, book) in &items {
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

    Ok(())
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

/// Build all fulltext indexes for all languages found in the databases.
pub fn build_all_indexes(
    appdata_db: &DatabaseHandle,
    dict_db: &DatabaseHandle,
    dpd_db: &DatabaseHandle,
    paths: &AppGlobalPaths,
) -> Result<()> {
    info("Building all fulltext indexes...");

    let sutta_langs = get_sutta_languages(appdata_db)?;
    info(&format!("Sutta languages: {:?}", sutta_langs));

    for lang in &sutta_langs {
        build_sutta_index(appdata_db, &paths.suttas_index_dir, lang)?;
    }

    let dict_langs = get_dict_word_languages(dict_db)?;
    info(&format!("Dict_word languages: {:?}", dict_langs));

    for lang in &dict_langs {
        build_dict_index(dict_db, &paths.dict_words_index_dir, lang)?;
    }

    let library_langs = get_library_languages(appdata_db)?;
    info(&format!("Library languages: {:?}", library_langs));

    for lang in &library_langs {
        build_library_index(appdata_db, &paths.library_index_dir, lang)?;
    }

    // DPD bold-definitions commentary is Pāli only; append into the
    // unified dict index (under the "pli" lang subdir).
    append_bold_definitions_to_dict_index(dpd_db, &paths.dict_words_index_dir, "pli")?;

    write_version_file(&paths.index_dir)?;

    info("All fulltext indexes built successfully.");
    Ok(())
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
pub const INDEX_VERSION: &str = "1.0";

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
}
