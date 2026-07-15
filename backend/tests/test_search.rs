use std::path::PathBuf;

use simsapa_backend::search::indexer::{
    open_or_create_index, write_version_file, read_version_file,
    is_index_current, INDEX_VERSION,
};
use simsapa_backend::search::schema::{build_sutta_schema, build_dict_schema};
use simsapa_backend::search::searcher::{FulltextSearcher, SearchFilters};
use simsapa_backend::AppGlobalPaths;

use tantivy::doc;
use tantivy::schema::Value;

/// Helper: create a temporary directory for test indexes.
fn temp_index_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("simsapa_test_search").join(name);
    // Clean up any previous test run
    if let Ok(true) = dir.try_exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    dir
}

#[test]
fn test_open_or_create_sutta_index() {
    let dir = temp_index_dir("test_sutta_index");
    let lang = "pli";
    let schema = build_sutta_schema(lang);

    let index = open_or_create_index(&dir, schema, lang).expect("Failed to create sutta index");

    // Verify the index directory was created
    assert!(dir.try_exists().unwrap_or(false), "Index directory should exist");

    // Verify we can get a reader
    let _reader = index.reader().expect("Should be able to create reader");

    // Clean up
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_open_or_create_dict_index() {
    let dir = temp_index_dir("test_dict_index");
    let lang = "en";
    let schema = build_dict_schema(lang);

    let index = open_or_create_index(&dir, schema, lang).expect("Failed to create dict index");

    // Verify the index directory was created
    assert!(dir.try_exists().unwrap_or(false), "Index directory should exist");

    let _reader = index.reader().expect("Should be able to create reader");

    // Clean up
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_sutta_index_add_and_search() {
    let dir = temp_index_dir("test_sutta_add_search");
    let lang = "pli";
    let schema = build_sutta_schema(lang);

    let index = open_or_create_index(&dir, schema, lang).expect("Failed to create index");

    let schema = index.schema();
    let uid_field = schema.get_field("uid").unwrap();
    let title_field = schema.get_field("title").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let source_uid_field = schema.get_field("source_uid").unwrap();
    let sutta_ref_field = schema.get_field("sutta_ref").unwrap();
    let nikaya_field = schema.get_field("nikaya").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();

    let mut writer = index.writer(15_000_000).expect("Failed to create writer");

    writer.add_document(doc!(
        uid_field => "dn1/pli/ms",
        title_field => "Brahmajālasutta",
        language_field => "pli",
        source_uid_field => "ms",
        sutta_ref_field => "DN 1",
        nikaya_field => "dn",
        content_field => "DN 1 Brahmajālasutta evaṁ me sutaṁ bhikkhūnaṁ dhammo",
        content_exact_field => "DN 1 Brahmajālasutta evaṁ me sutaṁ bhikkhūnaṁ dhammo",
    )).expect("Failed to add document");

    writer.commit().expect("Failed to commit");
    drop(writer);

    let reader = index.reader().expect("Failed to create reader");
    let searcher = reader.searcher();

    // Search using the stemmed content field
    let query_parser = tantivy::query::QueryParser::for_index(&index, vec![content_field]);
    let query = query_parser.parse_query("bhikkhu").expect("Failed to parse query");

    let top_docs = searcher
        .search(&query, &tantivy::collector::TopDocs::with_limit(10))
        .expect("Failed to search");

    // "bhikkhūnaṁ" should match "bhikkhu" through Pali stemming
    assert!(!top_docs.is_empty(), "Should find documents matching stemmed 'bhikkhu' against 'bhikkhūnaṁ'");

    // Verify stored fields
    let (_, doc_address) = top_docs[0];
    let doc = searcher.doc::<tantivy::TantivyDocument>(doc_address).expect("Failed to get doc");
    let uid_val = doc.get_first(uid_field).unwrap().as_str().unwrap();
    assert_eq!(uid_val, "dn1/pli/ms");

    // Clean up
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_dict_index_add_and_search() {
    let dir = temp_index_dir("test_dict_add_search");
    let lang = "en";
    let schema = build_dict_schema(lang);

    let index = open_or_create_index(&dir, schema, lang).expect("Failed to create index");

    let schema = index.schema();
    let uid_field = schema.get_field("uid").unwrap();
    let word_field = schema.get_field("word").unwrap();
    let synonyms_field = schema.get_field("synonyms").unwrap();
    let language_field = schema.get_field("language").unwrap();
    let source_uid_field = schema.get_field("source_uid").unwrap();
    let content_field = schema.get_field("content").unwrap();
    let content_exact_field = schema.get_field("content_exact").unwrap();

    let mut writer = index.writer(15_000_000).expect("Failed to create writer");

    writer.add_document(doc!(
        uid_field => "dhamma/pts",
        word_field => "dhamma",
        synonyms_field => "dharma",
        language_field => "en",
        source_uid_field => "pts",
        content_field => "dhamma dharma the teaching of the Buddha, truth, righteousness, nature, phenomenon",
        content_exact_field => "dhamma dharma the teaching of the Buddha, truth, righteousness, nature, phenomenon",
    )).expect("Failed to add document");

    writer.commit().expect("Failed to commit");
    drop(writer);

    let reader = index.reader().expect("Failed to create reader");
    let searcher = reader.searcher();

    let query_parser = tantivy::query::QueryParser::for_index(&index, vec![content_field]);
    let query = query_parser.parse_query("teaching").expect("Failed to parse query");

    let top_docs = searcher
        .search(&query, &tantivy::collector::TopDocs::with_limit(10))
        .expect("Failed to search");

    assert!(!top_docs.is_empty(), "Should find documents matching 'teaching'");

    // Clean up
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_version_file() {
    let dir = temp_index_dir("test_version_file");
    std::fs::create_dir_all(&dir).expect("Failed to create dir");

    // Write version file
    write_version_file(&dir).expect("Failed to write version file");

    // Read it back
    let version = read_version_file(&dir).expect("Failed to read version file");
    assert_eq!(version, INDEX_VERSION);

    // Check is_index_current
    assert!(is_index_current(&dir), "Index should be current");

    // Write a different version
    let version_path = dir.join("VERSION");
    std::fs::write(&version_path, "0.0").expect("Failed to write");
    assert!(!is_index_current(&dir), "Index should not be current with old version");

    // Non-existent directory
    let fake_dir = temp_index_dir("nonexistent_version_test");
    assert!(!is_index_current(&fake_dir), "Non-existent directory should not be current");

    // Clean up
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_register_tokenizers_for_different_languages() {
    // Verify that register_tokenizers doesn't panic for various languages
    for lang in &["pli", "en", "de", "fr", "san"] {
        let dir = temp_index_dir(&format!("test_tokenizer_{}", lang));
        let schema = build_sutta_schema(lang);
        let index = open_or_create_index(&dir, schema, lang)
            .unwrap_or_else(|_| panic!("Failed to create index for {}", lang));

        // Verify we can use the tokenizer by adding a document
        let schema = index.schema();
        let content_field = schema.get_field("content").unwrap();

        let mut writer = index.writer(15_000_000).expect("Failed to create writer");
        writer.add_document(doc!(
            schema.get_field("uid").unwrap() => "test",
            schema.get_field("title").unwrap() => "test",
            schema.get_field("language").unwrap() => *lang,
            schema.get_field("source_uid").unwrap() => "test",
            schema.get_field("sutta_ref").unwrap() => "TEST 1",
            schema.get_field("nikaya").unwrap() => "test",
            content_field => "test content for tokenizer verification",
            schema.get_field("content_exact").unwrap() => "test content for tokenizer verification",
        )).unwrap_or_else(|_| panic!("Failed to add document for {}", lang));

        writer.commit().unwrap_or_else(|_| panic!("Failed to commit for {}", lang));
        drop(writer);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Helper: create a test index directory structure that FulltextSearcher can open,
/// and populate it with test documents.
fn create_test_indexes(base_name: &str) -> (PathBuf, AppGlobalPaths) {
    let base = std::env::temp_dir().join("simsapa_test_search").join(base_name);
    if let Ok(true) = base.try_exists() {
        let _ = std::fs::remove_dir_all(&base);
    }

    let suttas_dir = base.join("index").join("suttas");
    let dict_dir = base.join("index").join("dict_words");

    // Create Pali sutta index with test data
    {
        let pli_dir = suttas_dir.join("pli");
        let schema = build_sutta_schema("pli");
        let index = open_or_create_index(&pli_dir, schema, "pli").unwrap();
        let schema = index.schema();
        let mut writer = index.writer(15_000_000).unwrap();

        writer.add_document(doc!(
            schema.get_field("uid").unwrap() => "dn1/pli/ms",
            schema.get_field("title").unwrap() => "Brahmajālasutta",
            schema.get_field("language").unwrap() => "pli",
            schema.get_field("source_uid").unwrap() => "ms",
            schema.get_field("sutta_ref").unwrap() => "DN 1",
            schema.get_field("nikaya").unwrap() => "dn",
            schema.get_field("content").unwrap() => "DN 1 Brahmajālasutta evaṁ me sutaṁ bhikkhūnaṁ dhammo",
            schema.get_field("content_exact").unwrap() => "DN 1 Brahmajālasutta evaṁ me sutaṁ bhikkhūnaṁ dhammo",
        )).unwrap();

        writer.add_document(doc!(
            schema.get_field("uid").unwrap() => "mn1/pli/ms",
            schema.get_field("title").unwrap() => "Mūlapariyāyasutta",
            schema.get_field("language").unwrap() => "pli",
            schema.get_field("source_uid").unwrap() => "ms",
            schema.get_field("sutta_ref").unwrap() => "MN 1",
            schema.get_field("nikaya").unwrap() => "mn",
            schema.get_field("content").unwrap() => "MN 1 Mūlapariyāyasutta sabbadhammaṁ abhiññā",
            schema.get_field("content_exact").unwrap() => "MN 1 Mūlapariyāyasutta sabbadhammaṁ abhiññā",
        )).unwrap();

        writer.commit().unwrap();
        drop(writer);
    }

    // Create English sutta index
    {
        let en_dir = suttas_dir.join("en");
        let schema = build_sutta_schema("en");
        let index = open_or_create_index(&en_dir, schema, "en").unwrap();
        let schema = index.schema();
        let mut writer = index.writer(15_000_000).unwrap();

        writer.add_document(doc!(
            schema.get_field("uid").unwrap() => "dn1/en/bodhi",
            schema.get_field("title").unwrap() => "The All-embracing Net of Views",
            schema.get_field("language").unwrap() => "en",
            schema.get_field("source_uid").unwrap() => "bodhi",
            schema.get_field("sutta_ref").unwrap() => "DN 1",
            schema.get_field("nikaya").unwrap() => "dn",
            schema.get_field("content").unwrap() => "DN 1 The All-embracing Net of Views Thus have I heard the monks suffering",
            schema.get_field("content_exact").unwrap() => "DN 1 The All-embracing Net of Views Thus have I heard the monks suffering",
        )).unwrap();

        writer.commit().unwrap();
        drop(writer);
    }

    // Create English dict index
    {
        let en_dict_dir = dict_dir.join("en");
        let schema = build_dict_schema("en");
        let index = open_or_create_index(&en_dict_dir, schema, "en").unwrap();
        let schema = index.schema();
        let mut writer = index.writer(15_000_000).unwrap();

        writer.add_document(doc!(
            schema.get_field("uid").unwrap() => "dhamma/pts",
            schema.get_field("word").unwrap() => "dhamma",
            schema.get_field("synonyms").unwrap() => "dharma",
            schema.get_field("language").unwrap() => "en",
            schema.get_field("source_uid").unwrap() => "pts",
            schema.get_field("content").unwrap() => "dhamma dharma the teaching of the Buddha truth righteousness",
            schema.get_field("content_exact").unwrap() => "dhamma dharma the teaching of the Buddha truth righteousness",
        )).unwrap();

        writer.commit().unwrap();
        drop(writer);
    }

    // Build AppGlobalPaths-like struct with just the index paths
    // We use a minimal mock since we only need the index paths
    let paths = AppGlobalPaths {
        simsapa_dir: base.clone(),
        simsapa_api_port_path: base.join("api-port.txt"),
        download_temp_folder: base.join("temp-download"),
        extract_temp_folder: base.join("temp-extract"),
        app_assets_dir: base.join("app-assets"),
        appdata_db_path: base.join("appdata.sqlite3"),
        appdata_abs_path: base.join("appdata.sqlite3"),
        appdata_database_url: String::new(),
        dict_db_path: base.join("dictionaries.sqlite3"),
        dict_abs_path: base.join("dictionaries.sqlite3"),
        dict_database_url: String::new(),
        dpd_db_path: base.join("dpd.sqlite3"),
        dpd_abs_path: base.join("dpd.sqlite3"),
        dpd_database_url: String::new(),
        index_dir: base.join("index"),
        suttas_index_dir: suttas_dir,
        dict_words_index_dir: dict_dir,
        library_index_dir: base.join("index").join("library"),
        download_languages_marker: base.join("download_languages.txt"),
        auto_start_download_marker: base.join("auto_start_download.txt"),
        delete_files_for_upgrade_marker: base.join("delete_files_for_upgrade.txt"),
        download_select_sanskrit_bundle_marker: base.join("download_select_sanskrit_bundle.txt"),
        remove_lang_index_dirs_marker: base.join("remove_lang_index_dirs.txt"),
    };

    (base, paths)
}

#[test]
fn test_fulltext_searcher_open_and_search_suttas() {
    let (base, paths) = create_test_indexes("test_ft_search_suttas");

    let searcher = FulltextSearcher::open(&paths).expect("Failed to open searcher");
    assert!(searcher.has_sutta_indexes());

    // Search for stemmed term: "bhikkhu" should match "bhikkhūnaṁ"
    let filters = SearchFilters::default();
    let (_count, results) = searcher.search_suttas_with_count("bhikkhu", &filters, 10, 0).expect("Search failed");
    assert!(!results.is_empty(), "Should find results for stemmed 'bhikkhu'");
    assert_eq!(results[0].uid, "dn1/pli/ms");

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn test_fulltext_searcher_dual_field_boost() {
    let (base, paths) = create_test_indexes("test_ft_boost");

    let searcher = FulltextSearcher::open(&paths).expect("Failed to open searcher");

    // Search for "dhammo" - should find results (stemmed to dhamma)
    let filters = SearchFilters::default();
    let (_count, results) = searcher.search_suttas_with_count("dhammo", &filters, 10, 0).expect("Search failed");
    assert!(!results.is_empty(), "Should find results for 'dhammo'");

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn test_fulltext_searcher_language_filter() {
    let (base, paths) = create_test_indexes("test_ft_lang_filter");

    let searcher = FulltextSearcher::open(&paths).expect("Failed to open searcher");

    // Search all languages
    let filters = SearchFilters::default();
    let (_count, all_results) = searcher.search_suttas_with_count("DN", &filters, 10, 0).expect("Search failed");

    // Search Pali only
    let pli_filters = SearchFilters {
        lang: Some("pli".to_string()),
        lang_include: true,
        ..Default::default()
    };
    let (_count, pli_results) = searcher.search_suttas_with_count("DN", &pli_filters, 10, 0).expect("Search failed");

    // Pali results should be a subset
    assert!(pli_results.len() <= all_results.len(),
        "Filtered results ({}) should be <= all results ({})", pli_results.len(), all_results.len());

    // Verify all pli_results have lang=pli
    for r in &pli_results {
        assert_eq!(r.lang.as_deref(), Some("pli"), "Filtered result should be pli");
    }

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn test_fulltext_searcher_dict_words() {
    let (base, paths) = create_test_indexes("test_ft_dict_words");

    let searcher = FulltextSearcher::open(&paths).expect("Failed to open searcher");
    assert!(searcher.has_dict_indexes());

    let filters = SearchFilters::default();
    let (_count, results) = searcher.search_dict_words_with_count("teaching", &filters, 10, 0).expect("Search failed");
    assert!(!results.is_empty(), "Should find dict results for 'teaching'");
    assert_eq!(results[0].table_name, "dict_words");

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn test_fulltext_searcher_snippet_html() {
    let (base, paths) = create_test_indexes("test_ft_snippet");

    let searcher = FulltextSearcher::open(&paths).expect("Failed to open searcher");

    let filters = SearchFilters::default();
    let (_count, results) = searcher.search_suttas_with_count("bhikkhu", &filters, 10, 0).expect("Search failed");
    assert!(!results.is_empty());

    // Verify snippet uses <span class='match'> instead of <b>
    let snippet = &results[0].snippet;
    assert!(!snippet.contains("<b>"), "Snippet should not contain <b> tags");
    assert!(!snippet.contains("</b>"), "Snippet should not contain </b> tags");
    // The snippet may or may not have highlights depending on Tantivy's snippet generator
    // but if it does, it should use span tags
    if snippet.contains("match") {
        assert!(snippet.contains("<span class='match'>"), "Snippet should use span class='match'");
    }

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn test_fulltext_searcher_empty_indexes() {
    // Test with non-existent paths
    let base = std::env::temp_dir().join("simsapa_test_search").join("test_ft_empty");
    if let Ok(true) = base.try_exists() {
        let _ = std::fs::remove_dir_all(&base);
    }

    let paths = AppGlobalPaths {
        simsapa_dir: base.clone(),
        simsapa_api_port_path: base.join("api-port.txt"),
        download_temp_folder: base.join("temp-download"),
        extract_temp_folder: base.join("temp-extract"),
        app_assets_dir: base.join("app-assets"),
        appdata_db_path: base.join("appdata.sqlite3"),
        appdata_abs_path: base.join("appdata.sqlite3"),
        appdata_database_url: String::new(),
        dict_db_path: base.join("dictionaries.sqlite3"),
        dict_abs_path: base.join("dictionaries.sqlite3"),
        dict_database_url: String::new(),
        dpd_db_path: base.join("dpd.sqlite3"),
        dpd_abs_path: base.join("dpd.sqlite3"),
        dpd_database_url: String::new(),
        index_dir: base.join("index"),
        suttas_index_dir: base.join("index").join("suttas"),
        dict_words_index_dir: base.join("index").join("dict_words"),
        library_index_dir: base.join("index").join("library"),
        download_languages_marker: base.join("download_languages.txt"),
        auto_start_download_marker: base.join("auto_start_download.txt"),
        delete_files_for_upgrade_marker: base.join("delete_files_for_upgrade.txt"),
        download_select_sanskrit_bundle_marker: base.join("download_select_sanskrit_bundle.txt"),
        remove_lang_index_dirs_marker: base.join("remove_lang_index_dirs.txt"),
    };

    let searcher = FulltextSearcher::open(&paths).expect("Should handle missing dirs gracefully");
    assert!(!searcher.has_sutta_indexes());
    assert!(!searcher.has_dict_indexes());

    let filters = SearchFilters::default();
    let (_count, results) = searcher.search_suttas_with_count("test", &filters, 10, 0).expect("Should return empty");
    assert!(results.is_empty());
}
