// Round-trip test for the gloss session JSON export / Open JSON feature
// (PRD §4.9 reqs 38-41, docs/gloss-ai-word-selection.md): export a session
// with cache rows on one appdata DB, import it into a fresh DB (as Open JSON
// does), and verify the counts, the strict-precedence import rule, and that
// the restored words re-derive their resolution / checked state from the
// imported cache rows.
//
// Uses throwaway temp appdata DBs only — never the real appdata DB.

use simsapa_backend::db::appdata::AppdataDbHandle;
use simsapa_backend::db::{DatabaseHandle, APPDATA_MIGRATIONS};
use simsapa_backend::helpers::{
    build_gloss_session_export_json, gloss_cache_word_key, gloss_context_hash,
    import_gloss_word_cache_entries, normalize_gloss_context, parse_gloss_session_export,
    annotate_gloss_words_json,
    GLOSS_SESSION_EXPORT_FORMAT, GLOSS_SESSION_EXPORT_FORMAT_VERSION,
};
use diesel_migrations::MigrationHarness;

const ARAME_SENTENCE: &str = "jetavane anāthapiṇḍikassa <b>ārāme</b>";
const CITTAM_SENTENCE: &str = "evaṁ bahulamakāsi <b>cittaṁ</b> tathā tathā";

fn temp_appdata(tag: &str) -> AppdataDbHandle {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "simsapa_gloss_export_test_{}_{}_{}.sqlite3",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let url = path.to_string_lossy().to_string();
    let handle = DatabaseHandle::new(&url).expect("create temp appdata handle");
    let mut conn = handle.get_conn().expect("get temp appdata conn");
    conn.run_pending_migrations(APPDATA_MIGRATIONS)
        .expect("run appdata migrations on temp db");
    handle
}

fn session_json() -> String {
    // The minimal shape build_gloss_session_export_json /
    // annotate_gloss_words_json read: paragraphs[].words[] with
    // original_word, example_sentence and the ambiguous results array.
    serde_json::json!({
        "text": "test session",
        "paragraphs": [
            {
                "text": "…",
                "words": [
                    {
                        "original_word": "ārāme",
                        "example_sentence": ARAME_SENTENCE,
                        "selected_index": 0,
                        "results": [
                            {"uid": "ārāma-1/dpd", "word": "ārāma 1", "summary": ""},
                            {"uid": "ārāma-4/dpd", "word": "ārāma 4", "summary": ""}
                        ]
                    },
                    {
                        "original_word": "cittaṁ",
                        "example_sentence": CITTAM_SENTENCE,
                        "selected_index": 0,
                        "results": [
                            {"uid": "citta-1/dpd", "word": "citta 1", "summary": ""},
                            {"uid": "citta-2/dpd", "word": "citta 2", "summary": ""}
                        ]
                    }
                ]
            }
        ]
    })
    .to_string()
}

#[test]
fn test_gloss_session_export_round_trip() {
    let arame_hash = gloss_context_hash(&normalize_gloss_context(ARAME_SENTENCE));
    let cittam_hash = gloss_context_hash(&normalize_gloss_context(CITTAM_SENTENCE));

    // --- Export side: a DB holding a user row and an ai row for the
    // session's two words (plus an unrelated row that must NOT be exported).
    let db_a = temp_appdata("a");
    db_a.upsert_gloss_word_cache(
        &gloss_cache_word_key("ārāme"), &arame_hash, ARAME_SENTENCE, "ārāma-4/dpd", "user-selected")
        .unwrap();
    db_a.upsert_gloss_word_cache(
        &gloss_cache_word_key("cittaṁ"), &cittam_hash, CITTAM_SENTENCE, "citta-2/dpd", "ai-selected")
        .unwrap();
    db_a.upsert_gloss_word_cache("unrelated", "other-hash", "ctx", "x/dpd", "user-selected")
        .unwrap();

    let export = build_gloss_session_export_json(&db_a, &session_json()).unwrap();

    // The envelope validates and carries exactly the session's rows.
    let (session, word_cache) = parse_gloss_session_export(&export).unwrap();
    assert_eq!(session.get("text").unwrap().as_str().unwrap(), "test session");
    assert_eq!(word_cache.len(), 2, "only rows referenced by the session are exported");
    let value: serde_json::Value = serde_json::from_str(&export).unwrap();
    assert_eq!(value.get("format").unwrap().as_str().unwrap(), GLOSS_SESSION_EXPORT_FORMAT);
    assert_eq!(
        value.get("format_version").unwrap().as_u64().unwrap(),
        GLOSS_SESSION_EXPORT_FORMAT_VERSION
    );
    assert!(value.get("app_version").unwrap().as_str().unwrap().len() > 0);

    // --- Import side (Open JSON): a fresh DB where the user has already
    // made their own choice for cittaṁ — the imported ai row must not
    // overwrite it; the imported user row for ārāme lands.
    let db_b = temp_appdata("b");
    db_b.upsert_gloss_word_cache(
        &gloss_cache_word_key("cittaṁ"), &cittam_hash, CITTAM_SENTENCE, "citta-1/dpd", "user-selected")
        .unwrap();

    let (imported, skipped) = import_gloss_word_cache_entries(&db_b, &word_cache);
    assert_eq!(imported, 1, "the ārāme user row is imported");
    assert_eq!(skipped, 1, "the cittaṁ ai row loses to the local user row");

    let row = db_b
        .get_gloss_word_cache(&gloss_cache_word_key("ārāme"), &arame_hash)
        .expect("imported row exists");
    assert_eq!(row.selected_uid, "ārāma-4/dpd");
    assert_eq!(row.origin, "user-selected");
    let row = db_b
        .get_gloss_word_cache(&gloss_cache_word_key("cittaṁ"), &cittam_hash)
        .expect("local row survives");
    assert_eq!(row.selected_uid, "citta-1/dpd");
    assert_eq!(row.origin, "user-selected");

    // --- Restore side: the annotate pass (used by load_session) re-derives
    // resolution / selected_index from the imported cache table.
    let words = session.get("paragraphs").unwrap()[0].get("words").unwrap().to_string();
    let annotated = annotate_gloss_words_json(&db_b, &words).unwrap();
    let annotated: Vec<serde_json::Value> = serde_json::from_str(&annotated).unwrap();

    // ārāme: imported user row → ārāma-4/dpd (index 1), checked "user-selected".
    assert_eq!(annotated[0].get("resolution").unwrap().as_str().unwrap(), "user-selected");
    assert_eq!(annotated[0].get("selected_index").unwrap().as_i64().unwrap(), 1);
    // cittaṁ: the local user's own row → citta-1/dpd (index 0), not the
    // exported ai selection.
    assert_eq!(annotated[1].get("resolution").unwrap().as_str().unwrap(), "user-selected");
    assert_eq!(annotated[1].get("selected_index").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_import_skips_invalid_entries() {
    use simsapa_backend::helpers::GlossWordCacheExportEntry;

    let db = temp_appdata("invalid");
    let entries = vec![
        // Unknown origin.
        GlossWordCacheExportEntry {
            word: "w1".into(), context_hash: "h1".into(), context_snippet: "c".into(),
            selected_uid: "u/dpd".into(), origin: "bogus".into(),
            ..Default::default()
        },
        // Empty fields.
        GlossWordCacheExportEntry {
            word: "".into(), context_hash: "h1".into(), context_snippet: "c".into(),
            selected_uid: "u/dpd".into(), origin: "ai-selected".into(),
            ..Default::default()
        },
        GlossWordCacheExportEntry {
            word: "w2".into(), context_hash: "".into(), context_snippet: "c".into(),
            selected_uid: "u/dpd".into(), origin: "ai-selected".into(),
            ..Default::default()
        },
        // Valid; word is key-normalized on import (Dhammaṁ → dhammaṁ).
        GlossWordCacheExportEntry {
            word: "Dhammaṃ".into(), context_hash: "h2".into(), context_snippet: "c".into(),
            selected_uid: "dhamma-1/dpd".into(), origin: "built-in-human-checked".into(),
            ..Default::default()
        },
    ];
    let (imported, skipped) = import_gloss_word_cache_entries(&db, &entries);
    assert_eq!(imported, 1);
    assert_eq!(skipped, 3);
    assert!(db.get_gloss_word_cache(&gloss_cache_word_key("dhammaṁ"), "h2").is_some());
}
