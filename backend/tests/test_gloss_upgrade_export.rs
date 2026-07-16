// Round-trip tests for the gloss categories of the appdata upgrade
// export/import cycle (docs/gloss-ai-word-selection.md §6): the user's own
// word selections and the saved Gloss/Prompts sessions must survive an appdata
// re-download.
//
// The "old" DB is exported to a temp import-me dir, then imported into a
// separate "fresh" DB standing in for the newly downloaded appdata.
//
// Uses throwaway temp appdata DBs only — never the real appdata DB.

use simsapa_backend::app_data::{
    export_gloss_prompts_history_to_dir, export_gloss_selections_to_dir,
    import_gloss_prompts_history_from_dir, import_gloss_selections_from_dir,
    GlossPromptsHistoryExport, GlossSelectionsExport,
};
use simsapa_backend::db::appdata::AppdataDbHandle;
use simsapa_backend::db::{DatabaseHandle, APPDATA_MIGRATIONS};
use simsapa_backend::db::appdata_models::HistoryItemType;
use diesel_migrations::MigrationHarness;

fn unique_suffix(tag: &str) -> String {
    format!(
        "{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn temp_appdata(tag: &str) -> AppdataDbHandle {
    let mut path = std::env::temp_dir();
    path.push(format!("simsapa_gloss_upgrade_test_{}.sqlite3", unique_suffix(tag)));
    let url = path.to_string_lossy().to_string();
    let handle = DatabaseHandle::new(&url).expect("create temp appdata handle");
    let mut conn = handle.get_conn().expect("get temp appdata conn");
    conn.run_pending_migrations(APPDATA_MIGRATIONS)
        .expect("run appdata migrations on temp db");
    handle
}

fn temp_import_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("simsapa_import_me_{}", unique_suffix(tag)));
    std::fs::create_dir_all(&dir).expect("create temp import-me dir");
    dir
}

// --- gloss_word_context_cache ---

#[test]
fn test_gloss_selections_export_skips_shipped_rows() {
    let old_db = temp_appdata("sel_export");

    old_db.upsert_gloss_word_cache("ārāme", "h1", "ctx1", "ārāma-4/dpd", "user-selected").unwrap();
    old_db.upsert_gloss_word_cache("cittaṁ", "h2", "ctx2", "citta-1/dpd", "ai-selected").unwrap();
    // Shipped rows: these arrive with the newly downloaded DB and must not be
    // exported.
    old_db.upsert_gloss_word_cache("dhammaṁ", "h3", "ctx3", "dhamma-1-01/dpd", "built-in-human-checked").unwrap();
    old_db.upsert_gloss_word_cache("bhikkhū", "h4", "ctx4", "bhikkhu-1/dpd", "built-in-agent-checked").unwrap();

    let import_dir = temp_import_dir("sel_export");
    export_gloss_selections_to_dir(&old_db, &import_dir).expect("export gloss selections");

    let json = std::fs::read_to_string(import_dir.join("gloss_selections.json"))
        .expect("read gloss_selections.json");
    let export: GlossSelectionsExport = serde_json::from_str(&json).expect("parse export");

    assert_eq!(export.format, "simsapa-gloss-selections");
    assert_eq!(export.format_version, 1);
    assert_eq!(export.word_cache.len(), 2, "only local rows are exported");

    let words: Vec<&str> = export.word_cache.iter().map(|e| e.word.as_str()).collect();
    assert!(words.contains(&"ārāme"));
    assert!(words.contains(&"cittaṁ"));
    assert!(!words.contains(&"dhammaṁ"), "built-in-human-checked row must not be exported");
    assert!(!words.contains(&"bhikkhū"), "built-in-agent-checked row must not be exported");

    // The agent-only fields belong to the session export format, not this one.
    for entry in &export.word_cache {
        assert!(entry.confidence.is_none());
        assert!(entry.note.is_none());
    }
}

#[test]
fn test_gloss_selections_round_trip_user_row_beats_shipped_row() {
    let old_db = temp_appdata("sel_rt_old");

    // The user's own choices, made against the previous DB.
    old_db.upsert_gloss_word_cache("ārāme", "h1", "ctx1", "ārāma-4/dpd", "user-selected").unwrap();
    old_db.upsert_gloss_word_cache("cittaṁ", "h2", "ctx2", "citta-1/dpd", "ai-selected").unwrap();

    let import_dir = temp_import_dir("sel_rt");
    export_gloss_selections_to_dir(&old_db, &import_dir).expect("export gloss selections");

    // The freshly downloaded DB ships its own curated rows for the same keys.
    let fresh_db = temp_appdata("sel_rt_fresh");
    fresh_db.upsert_gloss_word_cache("ārāme", "h1", "ctx1", "shipped-arama/dpd", "built-in-human-checked").unwrap();
    fresh_db.upsert_gloss_word_cache("cittaṁ", "h2", "ctx2", "shipped-citta/dpd", "built-in-agent-checked").unwrap();

    import_gloss_selections_from_dir(&fresh_db, &import_dir).expect("import gloss selections");

    // user-selected (4) outranks built-in-human-checked (3): the user's choice wins.
    let arame = fresh_db.get_gloss_word_cache("ārāme", "h1").expect("ārāme row");
    assert_eq!(arame.selected_uid, "ārāma-4/dpd");
    assert_eq!(arame.origin, "user-selected", "origin preserved, so the shield restores as full");

    // ai-selected (1) loses to built-in-agent-checked (2): the newly shipped
    // curation is the better guess.
    let cittam = fresh_db.get_gloss_word_cache("cittaṁ", "h2").expect("cittaṁ row");
    assert_eq!(cittam.selected_uid, "shipped-citta/dpd");
    assert_eq!(cittam.origin, "built-in-agent-checked");
}

#[test]
fn test_gloss_selections_ai_row_restores_when_no_shipped_row() {
    let old_db = temp_appdata("sel_ai_old");
    old_db.upsert_gloss_word_cache("cittaṁ", "h2", "ctx2", "citta-1/dpd", "ai-selected").unwrap();

    let import_dir = temp_import_dir("sel_ai");
    export_gloss_selections_to_dir(&old_db, &import_dir).expect("export gloss selections");

    let fresh_db = temp_appdata("sel_ai_fresh");
    import_gloss_selections_from_dir(&fresh_db, &import_dir).expect("import gloss selections");

    let row = fresh_db.get_gloss_word_cache("cittaṁ", "h2").expect("cittaṁ row");
    assert_eq!(row.selected_uid, "citta-1/dpd");
    assert_eq!(row.origin, "ai-selected", "origin preserved, so the shield restores as half");
    assert_eq!(row.context_snippet, "ctx2");
}

#[test]
fn test_gloss_selections_export_noop_when_only_shipped_rows() {
    let old_db = temp_appdata("sel_noop");
    old_db.upsert_gloss_word_cache("dhammaṁ", "h3", "ctx3", "dhamma-1-01/dpd", "built-in-human-checked").unwrap();

    let import_dir = temp_import_dir("sel_noop");
    export_gloss_selections_to_dir(&old_db, &import_dir).expect("export gloss selections");

    assert!(
        !import_dir.join("gloss_selections.json").exists(),
        "nothing user-owned to export: no file is written"
    );

    // The import side tolerates the missing file.
    let fresh_db = temp_appdata("sel_noop_fresh");
    import_gloss_selections_from_dir(&fresh_db, &import_dir).expect("import with no export file");
}

// --- gloss_prompts_history ---

#[test]
fn test_gloss_prompts_history_round_trip_preserves_order_and_types() {
    let old_db = temp_appdata("hist_old");

    let gloss_id = old_db.save_new_history(HistoryItemType::Gloss, r#"{"text":"gloss session"}"#).unwrap();
    let prompts_id = old_db.save_new_history(HistoryItemType::Prompts, r#"{"text":"prompts session"}"#).unwrap();
    assert_ne!(gloss_id, prompts_id);

    // Make the two rows distinguishable by updated_at: the restored list is
    // ordered by it, so the export must carry the original timestamps.
    let gloss_rows = old_db.get_history_for_type(HistoryItemType::Gloss);
    let original_updated_at = gloss_rows[0].updated_at;

    let import_dir = temp_import_dir("hist");
    export_gloss_prompts_history_to_dir(&old_db, &import_dir).expect("export history");

    let json = std::fs::read_to_string(import_dir.join("gloss_prompts_history.json"))
        .expect("read gloss_prompts_history.json");
    let export: GlossPromptsHistoryExport = serde_json::from_str(&json).expect("parse export");
    assert_eq!(export.format, "simsapa-gloss-prompts-history");
    assert_eq!(export.items.len(), 2, "both tabs' sessions ride in one file");

    let fresh_db = temp_appdata("hist_fresh");
    import_gloss_prompts_history_from_dir(&fresh_db, &import_dir).expect("import history");

    let restored_gloss = fresh_db.get_history_for_type(HistoryItemType::Gloss);
    assert_eq!(restored_gloss.len(), 1);
    assert_eq!(restored_gloss[0].data_json, r#"{"text":"gloss session"}"#);
    assert_eq!(
        restored_gloss[0].updated_at, original_updated_at,
        "original timestamps preserved: the list sorts by updated_at"
    );

    let restored_prompts = fresh_db.get_history_for_type(HistoryItemType::Prompts);
    assert_eq!(restored_prompts.len(), 1);
    assert_eq!(restored_prompts[0].data_json, r#"{"text":"prompts session"}"#);
}

#[test]
fn test_gloss_prompts_history_import_is_idempotent() {
    let old_db = temp_appdata("hist_idem_old");
    old_db.save_new_history(HistoryItemType::Gloss, r#"{"text":"a"}"#).unwrap();
    old_db.save_new_history(HistoryItemType::Gloss, r#"{"text":"b"}"#).unwrap();

    let import_dir = temp_import_dir("hist_idem");
    export_gloss_prompts_history_to_dir(&old_db, &import_dir).expect("export history");

    let fresh_db = temp_appdata("hist_idem_fresh");
    import_gloss_prompts_history_from_dir(&fresh_db, &import_dir).expect("first import");
    import_gloss_prompts_history_from_dir(&fresh_db, &import_dir).expect("retried import");

    let rows = fresh_db.get_history_for_type(HistoryItemType::Gloss);
    assert_eq!(rows.len(), 2, "a retried import must not duplicate the list");
}

#[test]
fn test_gloss_prompts_history_export_noop_when_empty() {
    let old_db = temp_appdata("hist_empty");
    let import_dir = temp_import_dir("hist_empty");

    export_gloss_prompts_history_to_dir(&old_db, &import_dir).expect("export empty history");
    assert!(!import_dir.join("gloss_prompts_history.json").exists());

    let fresh_db = temp_appdata("hist_empty_fresh");
    import_gloss_prompts_history_from_dir(&fresh_db, &import_dir).expect("import with no export file");
}
