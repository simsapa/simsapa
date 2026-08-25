use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{BooleanQuery, Occur, QueryParser, RegexQuery, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, IndexReader, Term};

use crate::logger::{info, warn};
use crate::types::SearchResult;
use crate::highlight::{literal_ranges, wrap_ranges};
use crate::helpers::normalize_plain_text;
use crate::query_task::SearchQueryTask;
use crate::AppGlobalPaths;

use super::lenient_directory::LenientLockMmapDirectory;
use super::schema::{build_dict_schema, build_library_schema, build_sutta_schema};
use super::tokenizer::register_tokenizers;
pub use super::types::SearchFilters;

/// Identifies the type of index for schema selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexType {
    Sutta,
    Dict,
    Library,
}

/// How one search area (sutta / dict / library) fared at open time.
///
/// `dir_present` is recorded here rather than re-derived later because the two
/// zero-index states have different causes and different remedies: an **absent**
/// index directory means the user has not built or downloaded an index, while a
/// **present** one that yielded nothing means the files could not be read. See
/// `crate::fulltext_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FulltextAreaStatus {
    /// Per-language indexes successfully opened for this area.
    pub opened: usize,
    /// Whether the area's index directory exists at all.
    pub dir_present: bool,
}

/// The per-area open counts, captured when the searcher was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FulltextIndexCounts {
    pub sutta: FulltextAreaStatus,
    pub dict: FulltextAreaStatus,
    pub library: FulltextAreaStatus,
}

impl FulltextIndexCounts {
    pub fn total_opened(&self) -> usize {
        self.sutta.opened + self.dict.opened + self.library.opened
    }

    pub fn any_dir_present(&self) -> bool {
        self.sutta.dir_present || self.dict.dir_present || self.library.dir_present
    }
}

/// Holds open indexes for fulltext searching.
///
/// The dict index unifies dict_words rows and DPD bold-definition rows; both
/// kinds of doc share the dict schema and are distinguished by the
/// `is_bold_definition: bool` field.
pub struct FulltextSearcher {
    /// Map of language → (Index, IndexReader) for sutta indexes
    sutta_indexes: HashMap<String, (Index, IndexReader)>,
    /// Map of language → (Index, IndexReader) for dict_word indexes (also
    /// houses bold-definition docs under the "pli" key).
    dict_indexes: HashMap<String, (Index, IndexReader)>,
    /// Map of language → (Index, IndexReader) for library book chapter indexes
    library_indexes: HashMap<String, (Index, IndexReader)>,
    /// Captured at open time. The map lengths give the counts, but not whether
    /// a directory existed — and that distinction is the whole point.
    counts: FulltextIndexCounts,
}

/// Returned by `FulltextSearcher::debug_query()`: the formatted debug text
/// plus an optional parse-error message.
#[derive(Debug)]
pub struct DebugQueryResult {
    pub debug_text: String,
    pub parse_error: Option<String>,
}

impl FulltextSearcher {
    /// Reset the per-directory open-failure record kept for the storage
    /// diagnostics.
    ///
    /// Called by **both** constructors, before any index is opened. There are
    /// two, and both call `open_indexes()` three times — clearing in only one
    /// of them leaves the other appending to a list that is never reset, so an
    /// entry recorded before a storage recovery would be reported as a live
    /// fault forever.
    fn begin_open_session() {
        crate::clear_searcher_open_failures();
    }

    /// Open all available per-language indexes under the given paths.
    pub fn open(paths: &AppGlobalPaths) -> Result<Self> {
        Self::begin_open_session();
        let (sutta_indexes, sutta_dir) = Self::open_indexes(&paths.suttas_index_dir, IndexType::Sutta)?;
        let (dict_indexes, dict_dir) = Self::open_indexes(&paths.dict_words_index_dir, IndexType::Dict)?;
        let (library_indexes, library_dir) = Self::open_indexes(&paths.library_index_dir, IndexType::Library)?;

        let counts = FulltextIndexCounts {
            sutta: FulltextAreaStatus { opened: sutta_indexes.len(), dir_present: sutta_dir },
            dict: FulltextAreaStatus { opened: dict_indexes.len(), dir_present: dict_dir },
            library: FulltextAreaStatus { opened: library_indexes.len(), dir_present: library_dir },
        };

        info(&format!(
            "FulltextSearcher opened: {} sutta language indexes, {} dict language indexes, {} library language indexes",
            sutta_indexes.len(),
            dict_indexes.len(),
            library_indexes.len(),
        ));

        Ok(Self {
            sutta_indexes,
            dict_indexes,
            library_indexes,
            counts,
        })
    }

    /// The per-area open counts captured when this searcher was built, plus
    /// whether each area's index directory existed at all.
    ///
    /// Read through `crate::fulltext_index_counts()`, which is what the search
    /// UI, Database Validation and `/health` all go through — one source, so
    /// they cannot disagree.
    ///
    /// The `has_*_indexes()` predicates answer a different question — "is there
    /// at least one" — and are not a substitute where the count itself is the
    /// reported fact.
    pub fn index_counts(&self) -> FulltextIndexCounts {
        self.counts
    }

    /// Open indexes from explicit directory paths (without needing AppGlobalPaths).
    ///
    /// Useful for CLI tools or tests that manage index directories directly.
    /// Pass an empty or non-existent path to skip sutta, dict, or library indexes.
    pub fn open_from_dirs(suttas_index_dir: &Path, dict_words_index_dir: &Path, library_index_dir: Option<&Path>) -> Result<Self> {
        Self::begin_open_session();
        let (sutta_indexes, sutta_dir) = Self::open_indexes(suttas_index_dir, IndexType::Sutta)?;
        let (dict_indexes, dict_dir) = Self::open_indexes(dict_words_index_dir, IndexType::Dict)?;
        let (library_indexes, library_dir) = if let Some(dir) = library_index_dir {
            Self::open_indexes(dir, IndexType::Library)?
        } else {
            (HashMap::new(), false)
        };

        let counts = FulltextIndexCounts {
            sutta: FulltextAreaStatus { opened: sutta_indexes.len(), dir_present: sutta_dir },
            dict: FulltextAreaStatus { opened: dict_indexes.len(), dir_present: dict_dir },
            library: FulltextAreaStatus { opened: library_indexes.len(), dir_present: library_dir },
        };

        Ok(Self {
            sutta_indexes,
            dict_indexes,
            library_indexes,
            counts,
        })
    }

    /// Scan a directory for per-language subdirectories and open each as a
    /// Tantivy index.
    ///
    /// Returns the opened indexes **and whether the base directory existed** —
    /// an empty map means "nothing opened", which on its own cannot tell an
    /// absent index tree from one that would not open.
    fn open_indexes(base_dir: &Path, index_type: IndexType) -> Result<(HashMap<String, (Index, IndexReader)>, bool)> {
        let mut map = HashMap::new();

        match base_dir.try_exists() {
            Ok(true) => {}
            _ => return Ok((map, false)),
        }

        let entries = std::fs::read_dir(base_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let lang = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            match Self::open_single_index(&path, &lang, index_type) {
                Ok((index, reader)) => {
                    map.insert(lang.clone(), (index, reader));
                }
                Err(e) => {
                    warn(&format!("Failed to open index at {}: {}", path.display(), e));
                    // Same information, kept where the storage diagnostics
                    // report can read it back. No behaviour change.
                    crate::record_searcher_open_failure(
                        &path.display().to_string(),
                        &e.to_string(),
                    );
                }
            }
        }

        Ok((map, true))
    }

    fn open_single_index(dir: &Path, lang: &str, index_type: IndexType) -> Result<(Index, IndexReader)> {
        let schema = match index_type {
            IndexType::Sutta => build_sutta_schema(lang),
            IndexType::Dict => build_dict_schema(lang),
            IndexType::Library => build_library_schema(lang),
        };

        // `LenientLockMmapDirectory`, not a bare `MmapDirectory`: on a volume
        // whose `flock(2)` answers ENOSYS the reader build below fails on every
        // index and the searcher ends up holding none. On a normal filesystem
        // the wrapper delegates to `MmapDirectory` unchanged. See
        // `docs/fulltext-index-storage-and-file-locking.md`.
        let mmap_dir = LenientLockMmapDirectory::open(dir)?;

        // Read path: open what is there, and only create when the directory
        // genuinely holds no index. Creating an index from the *search* path is
        // never correct — it would leave an empty index behind and report
        // success. `Index::open_or_create` stays in `indexer.rs`'s write paths.
        //
        // Neither `Index::exists` nor `Index::open` takes a lock (both are
        // `load_metas` over the directory), so this is hygiene, not part of the
        // lock fix.
        let index = if Index::exists(&mmap_dir)? {
            Index::open(mmap_dir)?
        } else {
            Index::open_or_create(mmap_dir, schema)?
        };
        register_tokenizers(&index, lang);

        // `ReloadPolicy::Manual`, not the default `OnCommitWithDelay`. This is
        // an **independent improvement, not an alternative to the wrapper**:
        // `open_segment_readers` takes `META_LOCK` whatever the policy, so the
        // lenient directory above is still what makes the open succeed.
        //
        // The default spawns one polling thread per index that re-reads and
        // CRC32s `meta.json` every 500 ms for the life of the process
        // (`directory/file_watcher.rs`). With six indexes open that is six
        // threads and ~12 file reads per second against the user's storage
        // volume, growing with every downloaded language. Nothing depends on the
        // auto-reload: every index mutation is followed by an explicit
        // `crate::reinit_fulltext_searcher()`.
        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::Manual)
            .try_into()?;
        Ok((index, reader))
    }

    /// Run the named tokenizer on `text` and return the resulting tokens as a
    /// comma-separated string.
    pub fn tokenize_to_string(index: &Index, tokenizer_name: &str, text: &str) -> Result<String> {
        let mut tokenizer = index
            .tokenizers()
            .get(tokenizer_name)
            .ok_or_else(|| anyhow::anyhow!("tokenizer '{}' not registered", tokenizer_name))?;

        let mut stream = tokenizer.token_stream(text);
        let mut tokens: Vec<String> = Vec::new();
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }
        Ok(tokens.join(", "))
    }

    /// Build a human-readable debug report for the given query.
    ///
    /// For each relevant language index (respecting `filters.lang`) the report
    /// includes:
    /// - tokenization results for both `{lang}_stem` and `{lang}_normalize`
    /// - whether stemming changed any tokens
    /// - parsed query ASTs for `content` and `content_exact` fields
    /// - total document count
    ///
    /// Parse errors are captured but do **not** short-circuit: partial results
    /// (tokens, doc count) are still included.
    pub fn debug_query(&self, query_text: &str, filters: &SearchFilters) -> Result<DebugQueryResult> {
        let mut out = String::new();
        let mut first_parse_error: Option<String> = None;

        let indexes = &self.sutta_indexes;
        if indexes.is_empty() {
            return Ok(DebugQueryResult {
                debug_text: "No sutta indexes available.".to_string(),
                parse_error: None,
            });
        }

        // Determine which languages to search (same logic as search_indexes)
        let langs_to_search: Vec<&String> = if let Some(ref lang) = filters.lang {
            if filters.lang_include && !lang.is_empty() && lang != "Language" {
                indexes.keys().filter(|k| *k == lang).collect()
            } else {
                indexes.keys().collect()
            }
        } else {
            indexes.keys().collect()
        };

        let mut sorted_langs: Vec<&String> = langs_to_search;
        sorted_langs.sort();

        for lang in sorted_langs {
            let Some((index, reader)) = indexes.get(lang) else {
                continue;
            };

            writeln!(out, "=== Language: {} ===", lang)?;
            writeln!(out)?;

            // --- Tokenization ---
            let stem_name = format!("{}_stem", lang);
            let norm_name = format!("{}_normalize", lang);

            let stem_tokens = Self::tokenize_to_string(index, &stem_name, query_text)
                .unwrap_or_else(|e| format!("(error: {})", e));
            let norm_tokens = Self::tokenize_to_string(index, &norm_name, query_text)
                .unwrap_or_else(|e| format!("(error: {})", e));

            writeln!(out, "Tokens ({}_stem):      {}", lang, stem_tokens)?;
            writeln!(out, "Tokens ({}_normalize): {}", lang, norm_tokens)?;

            // Stemming effect analysis
            if stem_tokens != norm_tokens {
                writeln!(out, "Stemming effect: stemmed differs from exact")?;
            } else {
                writeln!(out, "Stemming effect: no change (stemmed == exact)")?;
            }
            writeln!(out)?;

            // --- Parsed queries ---
            let schema = index.schema();
            if let Ok(content_field) = schema.get_field("content") {
                let parser = QueryParser::for_index(index, vec![content_field]);
                match parser.parse_query(query_text) {
                    Ok(q) => writeln!(out, "Parsed query (content):\n{:#?}", q)?,
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        writeln!(out, "Parsed query (content): ERROR: {}", err_msg)?;
                        if first_parse_error.is_none() {
                            first_parse_error = Some(err_msg);
                        }
                    }
                }
                writeln!(out)?;
            }

            if let Ok(content_exact_field) = schema.get_field("content_exact") {
                let parser = QueryParser::for_index(index, vec![content_exact_field]);
                match parser.parse_query(query_text) {
                    Ok(q) => writeln!(out, "Parsed query (content_exact):\n{:#?}", q)?,
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        writeln!(out, "Parsed query (content_exact): ERROR: {}", err_msg)?;
                        if first_parse_error.is_none() {
                            first_parse_error = Some(err_msg);
                        }
                    }
                }
                writeln!(out)?;
            }

            // --- Doc count ---
            let num_docs = reader.searcher().num_docs();
            writeln!(out, "Total docs in index: {}", num_docs)?;
            writeln!(out)?;
        }

        Ok(DebugQueryResult {
            debug_text: out,
            parse_error: first_parse_error,
        })
    }


    /// Check if any sutta indexes are available.
    pub fn has_sutta_indexes(&self) -> bool {
        !self.sutta_indexes.is_empty()
    }

    /// Check if any dict indexes are available.
    pub fn has_dict_indexes(&self) -> bool {
        !self.dict_indexes.is_empty()
    }

    /// Search sutta indexes, returning (total_hits, results).
    pub fn search_suttas_with_count(
        &self,
        query_text: &str,
        filters: &SearchFilters,
        page_len: usize,
        page_num: usize,
    ) -> Result<(usize, Vec<SearchResult>)> {
        self.search_indexes(query_text, filters, page_len, page_num, &self.sutta_indexes, IndexType::Sutta, true)
    }

    /// Search dict_word indexes, returning (total_hits, results).
    pub fn search_dict_words_with_count(
        &self,
        query_text: &str,
        filters: &SearchFilters,
        page_len: usize,
        page_num: usize,
    ) -> Result<(usize, Vec<SearchResult>)> {
        self.search_indexes(query_text, filters, page_len, page_num, &self.dict_indexes, IndexType::Dict, true)
    }

    /// Search library indexes, returning (total_hits, results).
    pub fn search_library_with_count(
        &self,
        query_text: &str,
        filters: &SearchFilters,
        page_len: usize,
        page_num: usize,
    ) -> Result<(usize, Vec<SearchResult>)> {
        self.search_indexes(query_text, filters, page_len, page_num, &self.library_indexes, IndexType::Library, true)
    }

    /// Check if any library indexes are available.
    pub fn has_library_indexes(&self) -> bool {
        !self.library_indexes.is_empty()
    }

    #[allow(clippy::too_many_arguments)]
    fn search_indexes(
        &self,
        query_text: &str,
        filters: &SearchFilters,
        page_len: usize,
        page_num: usize,
        indexes: &HashMap<String, (Index, IndexReader)>,
        index_type: IndexType,
        with_count: bool,
    ) -> Result<(usize, Vec<SearchResult>)> {
        if indexes.is_empty() {
            return Ok((0, Vec::new()));
        }

        // Determine which languages to search
        let langs_to_search: Vec<&String> = if let Some(ref lang) = filters.lang {
            if filters.lang_include && !lang.is_empty() && lang != "Language" {
                // Only search the specified language
                indexes.keys().filter(|k| *k == lang).collect()
            } else {
                indexes.keys().collect()
            }
        } else {
            indexes.keys().collect()
        };

        // Fetch enough results from each index to cover all pages up to the requested one
        let limit = (page_num + 1) * page_len;

        // Collect results from all matching languages with scores. Each scored
        // entry carries its source language key + tantivy DocAddress so a sliced
        // record can be re-associated with its own (index, reader) to re-fetch
        // the stored content for per-occurrence expansion (Show All Snippets).
        let mut all_scored: Vec<(f32, String, tantivy::DocAddress, SearchResult)> = Vec::new();
        let mut total_hits: usize = 0;

        for lang in langs_to_search {
            if let Some((index, reader)) = indexes.get(lang) {
                match self.search_single_index(query_text, filters, limit, index, reader, index_type, with_count) {
                    Ok((count, scored_results)) => {
                        total_hits += count;
                        for (score, addr, r) in scored_results {
                            all_scored.push((score, lang.clone(), addr, r));
                        }
                    }
                    Err(e) => {
                        warn(&format!("Fulltext search error for lang {}: {}", lang, e));
                    }
                }
            }
        }

        // Sort by score descending (interleaved by score, not grouped by language)
        all_scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Slice the requested page at the **record** level so a page always
        // holds at most `page_len` records, regardless of snippet expansion.
        let sliced: Vec<(f32, String, tantivy::DocAddress, SearchResult)> = all_scored
            .into_iter()
            .skip(page_num * page_len)
            .take(page_len)
            .collect();

        // Per-occurrence expansion happens **after** the slice so its cost is
        // bounded to `page_len` records. Only Suttas + Library honour it; the
        // Dict index stays single-snippet.
        let expand = filters.show_all_snippets
            && matches!(index_type, IndexType::Sutta | IndexType::Library);

        let (all_chars_before, all_chars_after) = match crate::try_get_app_data() {
            Some(app_data) => (app_data.get_snippet_all_chars_before(), app_data.get_snippet_all_chars_after()),
            None => (30, 200),
        };
        let results: Vec<SearchResult> = if expand {
            let mut out: Vec<SearchResult> = Vec::with_capacity(sliced.len());
            for (_score, lang, addr, base) in sliced {
                match indexes.get(&lang) {
                    Some((index, reader)) => {
                        match Self::expand_doc_occurrences(query_text, &lang, index, reader, addr, &base, all_chars_before, all_chars_after) {
                            Ok(mut rows) => out.append(&mut rows),
                            Err(e) => {
                                warn(&format!("Snippet expansion error for lang {}: {}", lang, e));
                                out.push(base);
                            }
                        }
                    }
                    None => out.push(base),
                }
            }
            out
        } else {
            sliced.into_iter().map(|(_, _, _, r)| r).collect()
        };

        Ok((total_hits, results))
    }

    /// Enumerate every match byte range of the query terms in `content` by
    /// re-tokenizing it with the index's `{lang}_stem` analyzer and keeping
    /// each token whose stem equals a query term's stem. This is what surfaces
    /// inflected forms (query `pajahati` → content `pajahitvā`) and, for AND
    /// queries, covers every term. Returns ranges in document order. See
    /// docs/search-snippet-highlight-pipeline.md.
    fn enumerate_match_ranges(
        index: &Index,
        lang: &str,
        content: &str,
        query_text: &str,
    ) -> Result<Vec<std::ops::Range<usize>>> {
        let tokenizer_name = format!("{lang}_stem");
        let mut tokenizer = index
            .tokenizers()
            .get(&tokenizer_name)
            .ok_or_else(|| anyhow::anyhow!("tokenizer '{}' not registered", tokenizer_name))?;

        // Collect the set of query stems.
        let mut query_stems: std::collections::HashSet<String> = std::collections::HashSet::new();
        {
            let mut qs = tokenizer.token_stream(query_text);
            while qs.advance() {
                query_stems.insert(qs.token().text.clone());
            }
        }
        if query_stems.is_empty() {
            return Ok(Vec::new());
        }

        // One range per content token whose stem matches a query stem. Token
        // offsets are byte offsets into the original `content`.
        let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
        let mut cs = tokenizer.token_stream(content);
        while cs.advance() {
            let tok = cs.token();
            if query_stems.contains(&tok.text) {
                ranges.push(tok.offset_from..tok.offset_to);
            }
        }
        Ok(ranges)
    }

    /// Expand one matched record into one focal-highlighted `SearchResult` per
    /// matched occurrence in its stored `content`. Each occurrence gets its own
    /// window (`fragment_around_offset`) and only that occurrence is highlighted
    /// (`wrap_ranges` with the focal range), satisfying the focal-only rule.
    /// `is_snippet` is set so QML can group rows by record. If no occurrence is
    /// found (analyzer/normalization edge case) the single best snippet (`base`)
    /// is emitted so the record still appears. See
    /// docs/search-snippet-highlight-pipeline.md.
    fn expand_doc_occurrences(
        query_text: &str,
        lang: &str,
        index: &Index,
        reader: &IndexReader,
        doc_address: tantivy::DocAddress,
        base: &SearchResult,
        all_chars_before: usize,
        all_chars_after: usize,
    ) -> Result<Vec<SearchResult>> {
        let searcher = reader.searcher();
        let schema = index.schema();
        let doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;
        let content = Self::get_text_field(&doc, &schema, "content");
        if content.is_empty() {
            return Ok(vec![base.clone()]);
        }

        let ranges = Self::enumerate_match_ranges(index, lang, &content, query_text)?;
        if ranges.is_empty() {
            return Ok(vec![base.clone()]);
        }

        let mut out: Vec<SearchResult> = Vec::with_capacity(ranges.len());
        for r in ranges {
            let (window, focal) = SearchQueryTask::fragment_around_offset(&content, r.start, r.end - r.start, all_chars_before, all_chars_after);
            let mut row = base.clone();
            row.snippet = wrap_ranges(&window, &[focal]);
            row.is_snippet = true;
            out.push(row);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn search_single_index(
        &self,
        query_text: &str,
        filters: &SearchFilters,
        page_len: usize,
        index: &Index,
        reader: &IndexReader,
        index_type: IndexType,
        with_count: bool,
    ) -> Result<(usize, Vec<(f32, tantivy::DocAddress, SearchResult)>)> {
        let searcher = reader.searcher();
        let schema = index.schema();

        let content_field = schema.get_field("content")?;
        let content_exact_field = schema.get_field("content_exact")?;

        // Build dual-field query: content (Must) + content_exact (Should, boosted)
        let content_parser = QueryParser::for_index(index, vec![content_field]);
        let content_exact_parser = QueryParser::for_index(index, vec![content_exact_field]);

        let content_query = content_parser.parse_query(query_text)?;
        let content_exact_query = content_exact_parser.parse_query(query_text)?;

        let boosted_exact = tantivy::query::BoostQuery::new(
            Box::new(content_exact_query),
            2.0,
        );

        let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = vec![
            (Occur::Must, Box::new(content_query)),
            (Occur::Should, Box::new(boosted_exact)),
        ];

        // Add filter term queries
        match index_type {
            IndexType::Sutta => Self::add_sutta_filters(&mut subqueries, filters, &schema)?,
            IndexType::Dict => Self::add_dict_filters(&mut subqueries, filters, &schema)?,
            IndexType::Library => Self::add_library_filters(&mut subqueries, filters, &schema)?,
        }

        let combined_query = BooleanQuery::new(subqueries);

        let (top_docs, count) = if with_count {
            searcher.search(&combined_query, &(TopDocs::with_limit(page_len), Count))?
        } else {
            let top_docs = searcher.search(&combined_query, &TopDocs::with_limit(page_len))?;
            (top_docs, 0)
        };

        // Build a single SnippetGenerator and reuse across all docs in this
        // call. Previously we constructed one per doc, which dominated runtime
        // for queries with hundreds-to-thousands of hits (e.g. fulltext +
        // suffix-filter where every candidate must be materialized for the
        // Rust-side post-filter).
        let snippet_gen = {
            let parser = QueryParser::for_index(index, vec![content_field]);
            let parsed = parser.parse_query(query_text)?;
            let mut g = tantivy::snippet::SnippetGenerator::create(&searcher, &parsed, content_field)?;
            g.set_max_num_chars(200);
            g
        };

        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;

            let result = match index_type {
                IndexType::Sutta => self.sutta_doc_to_result(&doc, &schema, score, &snippet_gen, query_text)?,
                IndexType::Dict => {
                    // Dict index unifies dict_words + bold-definition rows;
                    // dispatch per-doc so bold rows render via their own
                    // projection (group path, ref_code, etc.).
                    let is_bold = schema
                        .get_field("is_bold_definition")
                        .ok()
                        .and_then(|f| doc.get_first(f))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if is_bold {
                        self.bold_definition_doc_to_result(&doc, &schema, score, &snippet_gen, query_text)?
                    } else {
                        self.dict_doc_to_result(&doc, &schema, score, &snippet_gen, query_text)?
                    }
                }
                IndexType::Library => self.library_doc_to_result(&doc, &schema, score, &snippet_gen, query_text)?,
            };

            results.push((score, doc_address, result));
        }

        Ok((count, results))
    }

    /// Push down uid prefix and suffix filters as exact regex queries against
    /// the `raw`-tokenized uid + reversed-uid fields. Both reduce to anchored
    /// prefix-on-some-field, so the term dictionary's btree handles them in
    /// O(log N) on number of unique terms — no full-corpus scan, no
    /// over-match. Stored uids are lowercase by invariant; we lowercase the
    /// user input to match.
    fn add_uid_filters(
        subqueries: &mut Vec<(Occur, Box<dyn tantivy::query::Query>)>,
        filters: &SearchFilters,
        schema: &tantivy::schema::Schema,
        uid_field_name: &str,
        uid_rev_field_name: &str,
    ) -> Result<()> {
        if let Some(ref uid_prefix) = filters.uid_prefix
            && !uid_prefix.is_empty()
        {
            let field = schema.get_field(uid_field_name)?;
            let pattern = format!("{}.*", regex::escape(&uid_prefix.to_lowercase()));
            subqueries.push((Occur::Must, Box::new(RegexQuery::from_pattern(&pattern, field)?)));
        }

        if let Some(ref uid_suffix) = filters.uid_suffix
            && !uid_suffix.is_empty()
        {
            let field = schema.get_field(uid_rev_field_name)?;
            let reversed: String = uid_suffix.to_lowercase().chars().rev().collect();
            let pattern = format!("{}.*", regex::escape(&reversed));
            subqueries.push((Occur::Must, Box::new(RegexQuery::from_pattern(&pattern, field)?)));
        }

        Ok(())
    }

    fn add_sutta_filters(
        subqueries: &mut Vec<(Occur, Box<dyn tantivy::query::Query>)>,
        filters: &SearchFilters,
        schema: &tantivy::schema::Schema,
    ) -> Result<()> {
        if let Some(ref source) = filters.source_uid
            && filters.source_include && !source.is_empty()
        {
            let field = schema.get_field("source_uid")?;
            let term = Term::from_field_text(field, source);
            subqueries.push((Occur::Must, Box::new(TermQuery::new(term, IndexRecordOption::Basic))));
        }

        if let Some(ref nikaya) = filters.nikaya_prefix
            && !nikaya.is_empty()
        {
            let field = schema.get_field("nikaya")?;
            let pattern = format!("{}.*", regex::escape(&nikaya.to_lowercase()));
            let regex_query = RegexQuery::from_pattern(&pattern, field)?;
            subqueries.push((Occur::Must, Box::new(regex_query)));
        }

        Self::add_uid_filters(subqueries, filters, schema, "uid", "uid_rev")?;

        if let Some(ref sutta_ref) = filters.sutta_ref
            && !sutta_ref.is_empty()
        {
            let field = schema.get_field("sutta_ref")?;
            let term = Term::from_field_text(field, sutta_ref);
            subqueries.push((Occur::Must, Box::new(TermQuery::new(term, IndexRecordOption::Basic))));
        }

        // CST mula/commentary filtering: only exclude CST-sourced texts, not all sources.
        // This matches the SQL behavior in ContainsMatch which filters on uid LIKE '%/cst'.
        if !filters.include_cst_mula {
            let is_mula_field = schema.get_field("is_mula")?;
            let source_field = schema.get_field("source_uid")?;
            // Build: is_mula=true AND source_uid="cst"
            let cst_mula_query = BooleanQuery::new(vec![
                (Occur::Must, Box::new(TermQuery::new(Term::from_field_bool(is_mula_field, true), IndexRecordOption::Basic)) as Box<dyn tantivy::query::Query>),
                (Occur::Must, Box::new(TermQuery::new(Term::from_field_text(source_field, "cst"), IndexRecordOption::Basic))),
            ]);
            subqueries.push((Occur::MustNot, Box::new(cst_mula_query)));
        }

        // MS Mūla exclusion: exclude is_mula=true AND source_uid="ms" texts.
        if !filters.include_ms_mula {
            let is_mula_field = schema.get_field("is_mula")?;
            let source_field = schema.get_field("source_uid")?;
            let ms_mula_query = BooleanQuery::new(vec![
                (Occur::Must, Box::new(TermQuery::new(Term::from_field_bool(is_mula_field, true), IndexRecordOption::Basic)) as Box<dyn tantivy::query::Query>),
                (Occur::Must, Box::new(TermQuery::new(Term::from_field_text(source_field, "ms"), IndexRecordOption::Basic))),
            ]);
            subqueries.push((Occur::MustNot, Box::new(ms_mula_query)));
        }

        if !filters.include_cst_commentary {
            let is_commentary_field = schema.get_field("is_commentary")?;
            let source_field = schema.get_field("source_uid")?;
            // Build: is_commentary=true AND source_uid="cst"
            let cst_commentary_query = BooleanQuery::new(vec![
                (Occur::Must, Box::new(TermQuery::new(Term::from_field_bool(is_commentary_field, true), IndexRecordOption::Basic)) as Box<dyn tantivy::query::Query>),
                (Occur::Must, Box::new(TermQuery::new(Term::from_field_text(source_field, "cst"), IndexRecordOption::Basic))),
            ]);
            subqueries.push((Occur::MustNot, Box::new(cst_commentary_query)));
        }

        Ok(())
    }

    fn add_dict_filters(
        subqueries: &mut Vec<(Occur, Box<dyn tantivy::query::Query>)>,
        filters: &SearchFilters,
        schema: &tantivy::schema::Schema,
    ) -> Result<()> {
        if let Some(ref source) = filters.source_uid
            && filters.source_include && !source.is_empty()
        {
            let field = schema.get_field("source_uid")?;
            let term = Term::from_field_text(field, source);
            subqueries.push((Occur::Must, Box::new(TermQuery::new(term, IndexRecordOption::Basic))));
        }

        Self::add_uid_filters(subqueries, filters, schema, "uid", "uid_rev")?;

        // Combine the bold-definition gate with the dict_source_uids
        // inclusion set. Bold rows are gated by `include_bold_definitions`;
        // non-bold dict_words rows are restricted by `dict_source_uids`
        // when supplied. The two are OR-ed under a Must clause so that:
        //   - bold-only:    Some([]),          include_bold = true   -> only bold rows
        //   - non-bold-only: Some([labels..]), include_bold = false  -> only those labels
        //   - both: Some([labels..]),         include_bold = true   -> bold OR labels
        //   - neither: Some([]),              include_bold = false  -> nothing matches
        // When `dict_source_uids` is `None`, only the legacy bold gate applies.
        let is_bold_field = schema.get_field("is_bold_definition")?;
        let source_uid_field = schema.get_field("source_uid")?;

        match &filters.dict_source_uids {
            None => {
                if !filters.include_bold_definitions {
                    let term = Term::from_field_bool(is_bold_field, true);
                    subqueries.push((
                        Occur::MustNot,
                        Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
                    ));
                }
            }
            Some(set) => {
                // Build a Should-of-MustClauses: at least one branch must match.
                let mut shoulds: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

                if !set.is_empty() {
                    // Branch A: NOT bold AND source_uid IN (set)
                    let mut uid_disj: Vec<(Occur, Box<dyn tantivy::query::Query>)> =
                        Vec::with_capacity(set.len());
                    for src in set {
                        if src.is_empty() { continue; }
                        let term = Term::from_field_text(source_uid_field, src);
                        uid_disj.push((
                            Occur::Should,
                            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
                        ));
                    }
                    if !uid_disj.is_empty() {
                        let non_bold = vec![
                            (
                                Occur::Must,
                                Box::new(TermQuery::new(
                                    Term::from_field_bool(is_bold_field, false),
                                    IndexRecordOption::Basic,
                                )) as Box<dyn tantivy::query::Query>,
                            ),
                            (Occur::Must, Box::new(BooleanQuery::new(uid_disj))),
                        ];
                        shoulds.push((Occur::Should, Box::new(BooleanQuery::new(non_bold))));
                    }
                }

                if filters.include_bold_definitions {
                    // Branch B: is_bold = true
                    let term = Term::from_field_bool(is_bold_field, true);
                    shoulds.push((
                        Occur::Should,
                        Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
                    ));
                }

                if shoulds.is_empty() {
                    // No branch — force zero results via a contradiction.
                    let term = Term::from_field_bool(is_bold_field, true);
                    let must_bold = TermQuery::new(term.clone(), IndexRecordOption::Basic);
                    let must_not_bold = TermQuery::new(term, IndexRecordOption::Basic);
                    subqueries.push((Occur::Must, Box::new(must_bold)));
                    subqueries.push((Occur::MustNot, Box::new(must_not_bold)));
                } else {
                    subqueries.push((Occur::Must, Box::new(BooleanQuery::new(shoulds))));
                }
            }
        }

        Ok(())
    }

    fn add_library_filters(
        subqueries: &mut Vec<(Occur, Box<dyn tantivy::query::Query>)>,
        filters: &SearchFilters,
        schema: &tantivy::schema::Schema,
    ) -> Result<()> {
        Self::add_uid_filters(subqueries, filters, schema, "spine_item_uid", "spine_item_uid_rev")?;

        Ok(())
    }

    fn sutta_doc_to_result(
        &self,
        doc: &tantivy::TantivyDocument,
        schema: &tantivy::schema::Schema,
        score: f32,
        snippet_gen: &tantivy::snippet::SnippetGenerator,
        query_text: &str,
    ) -> Result<SearchResult> {
        let uid = Self::get_text_field(doc, schema, "uid");
        let title = Self::get_text_field(doc, schema, "title");
        let language = Self::get_text_field(doc, schema, "language");
        let source_uid = Self::get_text_field(doc, schema, "source_uid");
        let sutta_ref = Self::get_text_field(doc, schema, "sutta_ref");
        let nikaya = Self::get_text_field(doc, schema, "nikaya");

        let snippet = Self::render_snippet(snippet_gen, doc, query_text);

        Ok(SearchResult {
            uid,
            schema_name: "appdata".to_string(),
            table_name: "suttas".to_string(),
            source_uid: Some(source_uid).filter(|s| !s.is_empty()),
            title,
            sutta_ref: Some(sutta_ref).filter(|s| !s.is_empty()),
            nikaya: Some(nikaya).filter(|s| !s.is_empty()),
            author: None,
            lang: Some(language).filter(|s| !s.is_empty()),
            snippet,
            page_number: None,
            score: Some(score),
            rank: None,
            is_section_header: false,
            is_snippet: false,
        })
    }

    fn dict_doc_to_result(
        &self,
        doc: &tantivy::TantivyDocument,
        schema: &tantivy::schema::Schema,
        score: f32,
        snippet_gen: &tantivy::snippet::SnippetGenerator,
        query_text: &str,
    ) -> Result<SearchResult> {
        let uid = Self::get_text_field(doc, schema, "uid");
        let word = Self::get_text_field(doc, schema, "word");
        let language = Self::get_text_field(doc, schema, "language");
        let source_uid = Self::get_text_field(doc, schema, "source_uid");

        let snippet = Self::render_snippet(snippet_gen, doc, query_text);

        Ok(SearchResult {
            uid,
            schema_name: "appdata".to_string(),
            table_name: "dict_words".to_string(),
            source_uid: Some(source_uid).filter(|s| !s.is_empty()),
            title: word,
            sutta_ref: None,
            nikaya: None,
            author: None,
            lang: Some(language).filter(|s| !s.is_empty()),
            snippet,
            page_number: None,
            score: Some(score),
            rank: None,
            is_section_header: false,
            is_snippet: false,
        })
    }

    fn library_doc_to_result(
        &self,
        doc: &tantivy::TantivyDocument,
        schema: &tantivy::schema::Schema,
        score: f32,
        snippet_gen: &tantivy::snippet::SnippetGenerator,
        query_text: &str,
    ) -> Result<SearchResult> {
        let uid = Self::get_text_field(doc, schema, "spine_item_uid");
        let title = Self::get_text_field(doc, schema, "title");
        let book_title = Self::get_text_field(doc, schema, "book_title");
        let author = Self::get_text_field(doc, schema, "author");
        let language = Self::get_text_field(doc, schema, "language");

        // Use book_title as source_uid for display purposes
        let display_title = if !book_title.is_empty() && !title.is_empty() {
            format!("{} — {}", book_title, title)
        } else if !title.is_empty() {
            title
        } else {
            book_title.clone()
        };

        let snippet = Self::render_snippet(snippet_gen, doc, query_text);

        Ok(SearchResult {
            uid,
            schema_name: "appdata".to_string(),
            table_name: "book_spine_items".to_string(),
            source_uid: Some(book_title).filter(|s| !s.is_empty()),
            title: display_title,
            sutta_ref: None,
            nikaya: None,
            author: Some(author).filter(|s| !s.is_empty()),
            lang: Some(language).filter(|s| !s.is_empty()),
            snippet,
            page_number: None,
            score: Some(score),
            rank: None,
            is_section_header: false,
            is_snippet: false,
        })
    }

    fn bold_definition_doc_to_result(
        &self,
        doc: &tantivy::TantivyDocument,
        schema: &tantivy::schema::Schema,
        score: f32,
        snippet_gen: &tantivy::snippet::SnippetGenerator,
        query_text: &str,
    ) -> Result<SearchResult> {
        let uid = Self::get_text_field(doc, schema, "uid");
        let bold = Self::get_text_field(doc, schema, "word");
        let ref_code = Self::get_text_field(doc, schema, "source_uid");
        let group_path = Self::get_text_field(doc, schema, "nikaya_group_path");

        let snippet = Self::render_snippet(snippet_gen, doc, query_text);

        Ok(SearchResult {
            uid,
            schema_name: "dpd".to_string(),
            table_name: "bold_definitions".to_string(),
            // In bold_definitions, the equivalent of source_uid is the ref_code field (e.g. vina, mna, vvt)
            source_uid: Some(ref_code.clone()),
            title: bold,
            // ref_code also serves as the sutta_ref (Vinaya, Majjhima, etc. origin)
            sutta_ref: Some(ref_code),
            // The constructed nikaya / book / title / subhead path is surfaced via the
            // `nikaya` slot for display; the bare nikaya is no longer indexed
            // separately.
            nikaya: Some(group_path).filter(|s| !s.is_empty()),
            author: None,
            lang: Some("pli".to_string()),
            snippet,
            page_number: None,
            score: Some(score),
            rank: None,
            is_section_header: false,
            is_snippet: false,
        })
    }

    fn get_text_field(doc: &tantivy::TantivyDocument, schema: &tantivy::schema::Schema, field_name: &str) -> String {
        schema
            .get_field(field_name)
            .ok()
            .and_then(|f| doc.get_first(f))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    /// Render a single document's snippet using a pre-built `SnippetGenerator`.
    /// The generator is constructed once per `search_single_index` call —
    /// constructing it per-doc dominated runtime for queries returning many
    /// hits.
    ///
    /// Highlighting is **producer-owned and range-based** (see
    /// docs/search-snippet-highlight-pipeline.md): we take tantivy's stemmed
    /// match ranges (`Snippet::highlighted()`) unioned with literal occurrences
    /// of the normalized query, then emit exactly one `<span class='match'>`
    /// per merged range via `wrap_ranges`. This is **non-nested by
    /// construction** — it replaces the old `.to_html()` path that, combined
    /// with the central `highlight_row` pass, produced nested match spans for
    /// the literal query term.
    fn render_snippet(
        snippet_gen: &tantivy::snippet::SnippetGenerator,
        doc: &tantivy::TantivyDocument,
        query_text: &str,
    ) -> String {
        let snippet = snippet_gen.snippet_from_doc(doc);
        let fragment = snippet.fragment();
        if fragment.is_empty() {
            return String::new();
        }

        // Stemmed ranges from tantivy (e.g. pajahati → pajahitvā) ∪ literal
        // occurrences of the normalized query (so an exact typed form is also
        // wrapped). wrap_ranges merges + emits non-nested spans.
        let mut ranges = snippet.highlighted().to_vec();
        let norm_query = normalize_plain_text(query_text);
        ranges.extend(literal_ranges(fragment, &norm_query));
        wrap_ranges(fragment, &ranges)
    }
}

/// Get the path to the sutta index dir for inspection (e.g., checking if indexes exist).
pub fn sutta_index_dir(paths: &AppGlobalPaths) -> &PathBuf {
    &paths.suttas_index_dir
}

/// Get the path to the dict_words index dir.
pub fn dict_index_dir(paths: &AppGlobalPaths) -> &PathBuf {
    &paths.dict_words_index_dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::schema::build_sutta_schema;
    use super::super::tokenizer::register_tokenizers;
    use tantivy::doc;

    /// Create a temporary in-memory sutta index with one document for the given language.
    fn create_test_index(lang: &str) -> (Index, IndexReader) {
        let schema = build_sutta_schema(lang);
        let index = Index::create_in_ram(schema.clone());
        register_tokenizers(&index, lang);

        let mut writer = index.writer_with_num_threads(1, 15_000_000).unwrap();

        let uid = schema.get_field("uid").unwrap();
        let title = schema.get_field("title").unwrap();
        let language = schema.get_field("language").unwrap();
        let source_uid = schema.get_field("source_uid").unwrap();
        let sutta_ref = schema.get_field("sutta_ref").unwrap();
        let nikaya = schema.get_field("nikaya").unwrap();
        let content = schema.get_field("content").unwrap();
        let content_exact = schema.get_field("content_exact").unwrap();

        writer
            .add_document(doc!(
                uid => "sn12.2/pli/ms",
                title => "Vibhaṅgasutta",
                language => lang,
                source_uid => "ms",
                sutta_ref => "SN 12.2",
                nikaya => "sn",
                content => "Katamo ca bhikkhave jarāmaraṇaṁ. Yā tesaṁ tesaṁ sattānaṁ.",
                content_exact => "Katamo ca bhikkhave jarāmaraṇaṁ. Yā tesaṁ tesaṁ sattānaṁ."
            ))
            .unwrap();

        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        (index, reader)
    }

    /// Create an in-RAM sutta index with one document of the given content.
    fn create_test_index_with_content(lang: &str, doc_uid: &str, text: &str) -> (Index, IndexReader) {
        let schema = build_sutta_schema(lang);
        let index = Index::create_in_ram(schema.clone());
        register_tokenizers(&index, lang);

        let mut writer = index.writer_with_num_threads(1, 15_000_000).unwrap();
        let uid = schema.get_field("uid").unwrap();
        let title = schema.get_field("title").unwrap();
        let language = schema.get_field("language").unwrap();
        let source_uid = schema.get_field("source_uid").unwrap();
        let sutta_ref = schema.get_field("sutta_ref").unwrap();
        let nikaya = schema.get_field("nikaya").unwrap();
        let content = schema.get_field("content").unwrap();
        let content_exact = schema.get_field("content_exact").unwrap();

        writer
            .add_document(doc!(
                uid => doc_uid,
                title => "Test",
                language => lang,
                source_uid => "ms",
                sutta_ref => "CND 8",
                nikaya => "cnd",
                content => text,
                content_exact => text
            ))
            .unwrap();
        writer.commit().unwrap();
        let reader = index.reader().unwrap();
        (index, reader)
    }

    /// render_snippet must wrap both the stemmed match (pajahati → pajahitvā)
    /// and the literal query occurrence, each exactly once, with no nested
    /// spans. This is the regression guard for the old double-highlight bug.
    #[test]
    fn test_render_snippet_stemmed_and_literal_non_nested() {
        let text = "pajahati na upādiyati pajahitvā ṭhito";
        let (index, reader) = create_test_index_with_content("pli", "cnd8/pli/ms", text);
        let searcher = reader.searcher();
        let schema = index.schema();
        let content_field = schema.get_field("content").unwrap();

        let query_text = "pajahati";
        let parser = QueryParser::for_index(&index, vec![content_field]);
        let parsed = parser.parse_query(query_text).unwrap();
        let mut snippet_gen = tantivy::snippet::SnippetGenerator::create(&searcher, &parsed, content_field).unwrap();
        snippet_gen.set_max_num_chars(200);

        let top = searcher.search(&parsed, &TopDocs::with_limit(1)).unwrap();
        assert_eq!(top.len(), 1, "expected the doc to match query 'pajahati'");
        let doc: tantivy::TantivyDocument = searcher.doc(top[0].1).unwrap();

        let snippet = FulltextSearcher::render_snippet(&snippet_gen, &doc, query_text);

        assert!(
            !snippet.contains("class='match'><span"),
            "snippet has nested match spans: {snippet}"
        );
        assert_eq!(
            snippet.matches("class='match'").count(),
            2,
            "expected exactly two highlighted occurrences: {snippet}"
        );
        assert!(
            snippet.contains("<span class='match'>pajahati</span>"),
            "literal 'pajahati' not highlighted: {snippet}"
        );
        assert!(
            snippet.contains("<span class='match'>pajahitvā</span>"),
            "stemmed 'pajahitvā' not highlighted: {snippet}"
        );
    }

    /// With `show_all_snippets` on, a record containing two matched
    /// occurrences (one literal `pajahati`, one inflected `pajahitvā` that
    /// stems to the same root) expands into two `SearchResult` rows, each
    /// `is_snippet: true` and each highlighting **only its own** occurrence
    /// (single non-nested span). With the flag off, exactly one row.
    #[test]
    fn test_show_all_snippets_expands_per_occurrence() {
        let text = "pajahati na upādiyati pajahitvā ṭhito";
        let (index, reader) = create_test_index_with_content("pli", "cnd8/pli/ms", text);
        let mut sutta_indexes = HashMap::new();
        sutta_indexes.insert("pli".to_string(), (index, reader));
        let searcher = FulltextSearcher {
            sutta_indexes,
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        // Flag off: one row per record, not flagged as a snippet.
        let off = SearchFilters { show_all_snippets: false, ..SearchFilters::default() };
        let (count_off, res_off) = searcher.search_suttas_with_count("pajahati", &off, 10, 0).unwrap();
        assert_eq!(count_off, 1, "record count is unaffected by expansion");
        assert_eq!(res_off.len(), 1, "flag off yields one row per record");
        assert!(!res_off[0].is_snippet);

        // Flag on: two rows, one per occurrence.
        let on = SearchFilters { show_all_snippets: true, ..SearchFilters::default() };
        let (count_on, res_on) = searcher.search_suttas_with_count("pajahati", &on, 10, 0).unwrap();
        assert_eq!(count_on, 1, "record count stays the record total, not the snippet count");
        assert_eq!(res_on.len(), 2, "two occurrences → two snippet rows");

        for r in &res_on {
            assert!(r.is_snippet, "expanded rows are flagged is_snippet");
            assert_eq!(r.uid, "cnd8/pli/ms");
            assert!(
                !r.snippet.contains("class='match'><span"),
                "no nested spans: {}",
                r.snippet
            );
            assert_eq!(
                r.snippet.matches("class='match'").count(),
                1,
                "each expanded snippet highlights exactly its focal occurrence: {}",
                r.snippet
            );
        }

        // Focal-only: one row highlights pajahati (not pajahitvā), the other
        // highlights pajahitvā (not pajahati).
        let hl_pajahati = res_on.iter().any(|r| {
            r.snippet.contains("<span class='match'>pajahati</span>")
                && !r.snippet.contains("<span class='match'>pajahitvā</span>")
        });
        let hl_pajahitva = res_on.iter().any(|r| {
            r.snippet.contains("<span class='match'>pajahitvā</span>")
                && !r.snippet.contains("<span class='match'>pajahati</span>")
        });
        assert!(hl_pajahati, "one snippet must focal-highlight pajahati only: {res_on:?}");
        assert!(hl_pajahitva, "one snippet must focal-highlight pajahitvā only: {res_on:?}");
    }

    #[test]
    fn test_tokenize_to_string_stem() {
        let (index, _reader) = create_test_index("pli");
        let result = FulltextSearcher::tokenize_to_string(&index, "pli_stem", "bhikkhūnaṁ dhammo").unwrap();
        assert_eq!(result, "bhikkhu, dhamma");
    }

    #[test]
    fn test_tokenize_to_string_normalize() {
        let (index, _reader) = create_test_index("pli");
        let result = FulltextSearcher::tokenize_to_string(&index, "pli_normalize", "bhikkhūnaṁ dhammo").unwrap();
        // normalize: lowercase + niggahita norm + ascii fold, but no stemming
        assert!(result.contains("bhikkhunam"));
        assert!(result.contains("dhammo"));
    }

    #[test]
    fn test_tokenize_to_string_unknown_tokenizer() {
        let (index, _reader) = create_test_index("pli");
        let result = FulltextSearcher::tokenize_to_string(&index, "nonexistent", "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not registered"));
    }

    #[test]
    fn test_debug_query_basic() {
        let (index, reader) = create_test_index("pli");
        let mut sutta_indexes = HashMap::new();
        sutta_indexes.insert("pli".to_string(), (index, reader));

        let searcher = FulltextSearcher {
            sutta_indexes,
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        let filters = SearchFilters {
            lang: None,
            lang_include: false,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        let result = searcher.debug_query("bhikkhave", &filters).unwrap();

        assert!(result.debug_text.contains("=== Language: pli ==="));
        assert!(result.debug_text.contains("Tokens (pli_stem):"));
        assert!(result.debug_text.contains("Tokens (pli_normalize):"));
        assert!(result.debug_text.contains("Parsed query (content):"));
        assert!(result.debug_text.contains("Parsed query (content_exact):"));
        assert!(result.debug_text.contains("Total docs in index: 1"));
        assert!(result.parse_error.is_none());
    }

    #[test]
    fn test_debug_query_invalid_query_partial_results() {
        let (index, reader) = create_test_index("pli");
        let mut sutta_indexes = HashMap::new();
        sutta_indexes.insert("pli".to_string(), (index, reader));

        let searcher = FulltextSearcher {
            sutta_indexes,
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        let filters = SearchFilters {
            lang: None,
            lang_include: false,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        // Unbalanced quotes should cause a parse error but still return partial results
        let result = searcher.debug_query("\"unclosed quote", &filters).unwrap();

        // Tokens should still be present even if query parsing fails
        assert!(result.debug_text.contains("Tokens (pli_stem):"));
        assert!(result.debug_text.contains("Total docs in index: 1"));
        // Parse error should be reported
        assert!(result.parse_error.is_some());
    }

    #[test]
    fn test_debug_query_stemming_effect() {
        let (index, reader) = create_test_index("pli");
        let mut sutta_indexes = HashMap::new();
        sutta_indexes.insert("pli".to_string(), (index, reader));

        let searcher = FulltextSearcher {
            sutta_indexes,
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        let filters = SearchFilters {
            lang: None,
            lang_include: false,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        // "bhikkhūnaṁ" should stem differently than normalize
        let result = searcher.debug_query("bhikkhūnaṁ", &filters).unwrap();
        assert!(result.debug_text.contains("Stemming effect: stemmed differs from exact"));
    }

    #[test]
    fn test_ascii_query_matches_pali_text() {
        // ASCII "bhikkhave" should match Pāli "bhikkhave" in the test document
        let (index, reader) = create_test_index("pli");
        let mut sutta_indexes = HashMap::new();
        sutta_indexes.insert("pli".to_string(), (index, reader));

        let searcher = FulltextSearcher {
            sutta_indexes,
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        let filters = SearchFilters {
            lang: Some("pli".to_string()),
            lang_include: true,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        // "sattanam" is the ASCII-folded form of "sattānaṁ" in the test document.
        // The stemmer now operates on ASCII input, so it stems "sattanam" → "satta"
        // matching the indexed stem of "sattānaṁ" → fold → "sattanam" → stem → "satta".
        let (count, results) = searcher.search_suttas_with_count("sattanam", &filters, 10, 0).unwrap();
        assert!(count > 0, "ASCII query 'sattanam' should match Pāli 'sattānaṁ'");
        assert!(!results.is_empty());
        assert_eq!(results[0].uid, "sn12.2/pli/ms");
    }

    #[test]
    fn test_ascii_query_jaramaranam() {
        // "jaramaranam" should match "jarāmaraṇaṁ"
        let (index, reader) = create_test_index("pli");
        let mut sutta_indexes = HashMap::new();
        sutta_indexes.insert("pli".to_string(), (index, reader));

        let searcher = FulltextSearcher {
            sutta_indexes,
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        let filters = SearchFilters {
            lang: Some("pli".to_string()),
            lang_include: true,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        let (count, results) = searcher.search_suttas_with_count("jaramaranam", &filters, 10, 0).unwrap();
        assert!(count > 0, "ASCII query 'jaramaranam' should match Pāli 'jarāmaraṇaṁ'");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_ascii_query_with_declensions() {
        // "vinnanam" should match documents containing multiple declensions of viññāṇa
        let lang = "pli";
        let schema = build_sutta_schema(lang);
        let index = Index::create_in_ram(schema.clone());
        register_tokenizers(&index, lang);

        let mut writer = index.writer_with_num_threads(1, 15_000_000).unwrap();

        let uid = schema.get_field("uid").unwrap();
        let title = schema.get_field("title").unwrap();
        let language = schema.get_field("language").unwrap();
        let source_uid = schema.get_field("source_uid").unwrap();
        let sutta_ref = schema.get_field("sutta_ref").unwrap();
        let nikaya = schema.get_field("nikaya").unwrap();
        let content = schema.get_field("content").unwrap();
        let content_exact = schema.get_field("content_exact").unwrap();

        let text = "viññāṇaṁ viññāṇena viññāṇassa viññāṇānaṁ";
        writer
            .add_document(doc!(
                uid => "sn12.1/pli/ms",
                title => "Test",
                language => lang,
                source_uid => "ms",
                sutta_ref => "SN 12.1",
                nikaya => "sn",
                content => text,
                content_exact => text
            ))
            .unwrap();

        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        let mut sutta_indexes = HashMap::new();
        sutta_indexes.insert("pli".to_string(), (index, reader));

        let searcher = FulltextSearcher {
            sutta_indexes,
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        let filters = SearchFilters {
            lang: Some("pli".to_string()),
            lang_include: true,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        let (count, results) = searcher.search_suttas_with_count("vinnanam", &filters, 10, 0).unwrap();
        assert!(count > 0, "ASCII query 'vinnanam' should match documents with viññāṇa declensions");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_debug_query_no_indexes() {
        let searcher = FulltextSearcher {
            sutta_indexes: HashMap::new(),
            dict_indexes: HashMap::new(),
            library_indexes: HashMap::new(),
            // These tests build a searcher by hand to exercise the search
            // methods; the open-time reporting counts are not what they are
            // about.
            counts: FulltextIndexCounts::default(),
        };

        let filters = SearchFilters {
            lang: None,
            lang_include: false,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: None,
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        let result = searcher.debug_query("test", &filters).unwrap();
        assert_eq!(result.debug_text, "No sutta indexes available.");
        assert!(result.parse_error.is_none());
    }

    /// Build an in-memory sutta index containing one doc per uid in `uids`.
    /// Each doc carries the same content so a content match doesn't filter
    /// any out — the only differentiator is `uid` / `uid_rev`.
    fn create_uid_test_index(uids: &[&str]) -> (Index, IndexReader) {
        let lang = "en";
        let schema = build_sutta_schema(lang);
        let index = Index::create_in_ram(schema.clone());
        register_tokenizers(&index, lang);

        let mut writer = index.writer_with_num_threads(1, 15_000_000).unwrap();

        let uid_field = schema.get_field("uid").unwrap();
        let uid_rev_field = schema.get_field("uid_rev").unwrap();
        let title_field = schema.get_field("title").unwrap();
        let language_field = schema.get_field("language").unwrap();
        let source_uid_field = schema.get_field("source_uid").unwrap();
        let sutta_ref_field = schema.get_field("sutta_ref").unwrap();
        let nikaya_field = schema.get_field("nikaya").unwrap();
        let content_field = schema.get_field("content").unwrap();
        let content_exact_field = schema.get_field("content_exact").unwrap();

        for u in uids {
            let lower = u.to_lowercase();
            let rev: String = lower.chars().rev().collect();
            writer
                .add_document(doc!(
                    uid_field => *u,
                    uid_rev_field => rev.as_str(),
                    title_field => "T",
                    language_field => lang,
                    source_uid_field => "ms",
                    sutta_ref_field => "",
                    nikaya_field => "",
                    content_field => "lorem ipsum dolor sit amet",
                    content_exact_field => "lorem ipsum dolor sit amet",
                ))
                .unwrap();
        }
        writer.commit().unwrap();

        let reader = index.reader().unwrap();
        (index, reader)
    }

    #[test]
    fn test_add_uid_filters_prefix_and_suffix_exact() {
        // Mixed uids; only "an1.1/..." rows match prefix "an" AND suffix "1.1".
        let uids = [
            "an1.1/en/sujato",      // matches both
            "an1.1/pli/ms",         // suffix is "ms" — no
            "an1.10/en/sujato",     // prefix yes, suffix no
            "sn12.1/en/sujato",     // prefix no
            "an2.1/en/sujato",      // prefix yes, suffix no (ends in "sujato")
            "dn1.1/en/sujato",      // prefix no
        ];

        let (index, reader) = create_uid_test_index(&uids);
        let schema = index.schema();

        let filters = SearchFilters {
            lang: None,
            lang_include: false,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: Some("an".to_string()),
            uid_suffix: Some("1.1".to_string()),
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        // suffix "1.1" matches uids ending in "1.1" — only "an1.1/en/sujato" if we
        // require both prefix and suffix to apply to the uid as a whole. But our
        // suffix matches against the uid_rev — i.e. the original uid ends with
        // the suffix string. None of these uids actually *end* with "1.1"
        // (they end with "/sujato" etc.), so the right test suffix is one that
        // some uids actually end with. Adjust:
        let filters = SearchFilters {
            uid_suffix: Some("sujato".to_string()),
            ..filters
        };

        let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
        // Add a match-all so the BooleanQuery has a positive clause.
        subqueries.push((
            Occur::Must,
            Box::new(tantivy::query::AllQuery) as Box<dyn tantivy::query::Query>,
        ));
        FulltextSearcher::add_uid_filters(&mut subqueries, &filters, &schema, "uid", "uid_rev")
            .unwrap();
        let query = BooleanQuery::new(subqueries);

        let s = reader.searcher();
        let count = s.search(&query, &Count).unwrap();
        // prefix "an" AND suffix "sujato": an1.1/en/sujato, an1.10/en/sujato, an2.1/en/sujato
        assert_eq!(count, 3, "expected exactly the an*-prefixed, sujato-suffixed uids");
    }

    #[test]
    fn test_add_uid_filters_no_overmatch() {
        // raw uid field means a regex against the uid is anchored to the full
        // term, not against tokens — so "an" prefix must NOT match "san1.1".
        let uids = ["an1.1/en/sujato", "san1.1/en/sujato"];
        let (index, reader) = create_uid_test_index(&uids);
        let schema = index.schema();

        let filters = SearchFilters {
            lang: None,
            lang_include: false,
            source_uid: None,
            source_include: false,
            nikaya_prefix: None,
            uid_prefix: Some("an".to_string()),
            uid_suffix: None,
            sutta_ref: None,
            include_cst_mula: true,
            include_cst_commentary: true,
            include_ms_mula: true,
            include_bold_definitions: true,
            dict_source_uids: None,
            show_all_snippets: false,
        };

        let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
        subqueries.push((Occur::Must, Box::new(tantivy::query::AllQuery)));
        FulltextSearcher::add_uid_filters(&mut subqueries, &filters, &schema, "uid", "uid_rev")
            .unwrap();
        let query = BooleanQuery::new(subqueries);

        let count = reader.searcher().search(&query, &Count).unwrap();
        assert_eq!(count, 1, "prefix 'an' must not match 'san1.1/...'");
    }
}
