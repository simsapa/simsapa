use diesel::prelude::*;
use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::db::appdata_models::*;

mod helpers;
use helpers as h;

/// Ensure chanting tables exist in the test database by running the migration SQL.
/// This is idempotent (uses IF NOT EXISTS pattern via CREATE TABLE).
fn ensure_chanting_tables() {
    let app_data = get_app_data();

    // The 1.0.0 baseline creates the full appdata schema (incl. chanting tables).
    // Splitting on ';' is safe: the baseline has no trigger bodies, and
    // "already exists" errors are swallowed below when tables are present.
    let migration_sqls = [
        include_str!("../migrations/appdata/2026-07-23-000000_initial_schema/up.sql"),
    ];

    // Execute each statement separately (SQLite doesn't support multi-statement exec)
    let mut db_conn = app_data.dbm.appdata.get_conn().expect("get conn");
    for up_sql in &migration_sqls {
        for statement in up_sql.split(';') {
            let trimmed = statement.trim();
            if !trimmed.is_empty() {
                diesel::sql_query(trimmed)
                    .execute(&mut db_conn)
                    .unwrap_or_else(|e| {
                        // Table/index/column already exists is OK
                        let msg = e.to_string();
                        if !msg.contains("already exists") && !msg.contains("duplicate column") {
                            eprintln!("Warning running migration SQL: {}", e);
                        }
                        0
                    });
            }
        }
    }
}

/// Clean up all chanting data between tests
fn cleanup_chanting_data() {
    let app_data = get_app_data();

    // Delete in reverse order (children first) to respect FK constraints
    let _ = app_data.dbm.appdata.do_write(|db_conn| {
        diesel::sql_query("DELETE FROM chanting_recordings").execute(db_conn)?;
        diesel::sql_query("DELETE FROM chanting_sections").execute(db_conn)?;
        diesel::sql_query("DELETE FROM chanting_chants").execute(db_conn)?;
        diesel::sql_query("DELETE FROM chanting_collections").execute(db_conn)?;
        Ok(())
    });
}

fn setup() {
    h::app_data_setup();
    ensure_chanting_tables();
    cleanup_chanting_data();
}

// --- Collection CRUD ---

#[test]
#[serial]
fn test_create_and_read_chanting_collection() {
    setup();
    let app_data = get_app_data();

    let data = ChantingCollectionJson {
        uid: "test-collection-1".to_string(),
        title: "Test Collection".to_string(),
        description: Some("A test collection".to_string()),
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };

    app_data.dbm.appdata.create_chanting_collection(&data).expect("create collection");

    let collections = app_data.dbm.appdata.get_all_chanting_collections().expect("get collections");
    assert!(!collections.is_empty(), "Should have at least one collection");

    let col = collections.iter().find(|c| c.uid == "test-collection-1").expect("find collection");
    assert_eq!(col.title, "Test Collection");
    assert_eq!(col.description, Some("A test collection".to_string()));
    assert_eq!(col.language, "pali");
}

#[test]
#[serial]
fn test_update_chanting_collection() {
    setup();
    let app_data = get_app_data();

    let data = ChantingCollectionJson {
        uid: "test-collection-upd".to_string(),
        title: "Original Title".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };

    app_data.dbm.appdata.create_chanting_collection(&data).expect("create");

    let updated = ChantingCollectionJson {
        uid: "test-collection-upd".to_string(),
        title: "Updated Title".to_string(),
        description: Some("Now with description".to_string()),
        language: "en".to_string(),
        sort_index: 5,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };

    app_data.dbm.appdata.update_chanting_collection(&updated).expect("update");

    let collections = app_data.dbm.appdata.get_all_chanting_collections().expect("get");
    let col = collections.iter().find(|c| c.uid == "test-collection-upd").expect("find");
    assert_eq!(col.title, "Updated Title");
    assert_eq!(col.description, Some("Now with description".to_string()));
}

#[test]
#[serial]
fn test_delete_chanting_collection() {
    setup();
    let app_data = get_app_data();

    let data = ChantingCollectionJson {
        uid: "test-collection-del".to_string(),
        title: "To Delete".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };

    app_data.dbm.appdata.create_chanting_collection(&data).expect("create");
    app_data.dbm.appdata.delete_chanting_collection("test-collection-del").expect("delete");

    let collections = app_data.dbm.appdata.get_all_chanting_collections().expect("get");
    assert!(collections.iter().all(|c| c.uid != "test-collection-del"), "Collection should be deleted");
}

// --- Chant CRUD ---

#[test]
#[serial]
fn test_create_and_read_chanting_chant() {
    setup();
    let app_data = get_app_data();

    // Create parent collection first
    let col_data = ChantingCollectionJson {
        uid: "test-col-for-chant".to_string(),
        title: "Parent Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col_data).expect("create collection");

    let chant_data = ChantingChantJson {
        uid: "test-chant-1".to_string(),
        collection_uid: "test-col-for-chant".to_string(),
        title: "Morning Chanting".to_string(),
        description: Some("Morning chants".to_string()),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant_data).expect("create chant");

    let collections = app_data.dbm.appdata.get_all_chanting_collections().expect("get");
    let col = collections.iter().find(|c| c.uid == "test-col-for-chant").expect("find collection");
    assert_eq!(col.chants.len(), 1);
    assert_eq!(col.chants[0].title, "Morning Chanting");
}

// --- Section CRUD ---

#[test]
#[serial]
fn test_create_and_read_chanting_section() {
    setup();
    let app_data = get_app_data();

    // Create parent hierarchy
    let col_data = ChantingCollectionJson {
        uid: "test-col-for-sec".to_string(),
        title: "Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col_data).expect("create collection");

    let chant_data = ChantingChantJson {
        uid: "test-chant-for-sec".to_string(),
        collection_uid: "test-col-for-sec".to_string(),
        title: "Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant_data).expect("create chant");

    let sec_data = ChantingSectionJson {
        uid: "test-section-1".to_string(),
        chant_uid: "test-chant-for-sec".to_string(),
        title: "Homage to the Buddha".to_string(),
        content_pali: "Namo tassa bhagavato arahato sammāsambuddhassa".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec_data).expect("create section");

    let detail = app_data.dbm.appdata.get_chanting_section_detail("test-section-1")
        .expect("get detail")
        .expect("section should exist");

    assert_eq!(detail.title, "Homage to the Buddha");
    assert_eq!(detail.content_pali, "Namo tassa bhagavato arahato sammāsambuddhassa");
    assert!(detail.recordings.is_empty());
}

// --- Recording CRUD ---

#[test]
#[serial]
fn test_create_and_read_chanting_recording() {
    setup();
    let app_data = get_app_data();

    // Create parent hierarchy
    let col_data = ChantingCollectionJson {
        uid: "test-col-for-rec".to_string(),
        title: "Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col_data).expect("create collection");

    let chant_data = ChantingChantJson {
        uid: "test-chant-for-rec".to_string(),
        collection_uid: "test-col-for-rec".to_string(),
        title: "Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant_data).expect("create chant");

    let sec_data = ChantingSectionJson {
        uid: "test-sec-for-rec".to_string(),
        chant_uid: "test-chant-for-rec".to_string(),
        title: "Section".to_string(),
        content_pali: "Pali text".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec_data).expect("create section");

    let rec_data = ChantingRecordingJson {
        uid: "test-recording-1".to_string(),
        section_uid: "test-sec-for-rec".to_string(),
        file_name: "test-sec-for-rec_20260324.ogg".to_string(),
        recording_type: "user".to_string(),
        label: Some("Attempt 1".to_string()),
        duration_ms: 5000,
        markers_json: None,
        volume: 1.0,
        playback_position_ms: 0,
        waveform_json: None,
        is_user_added: true,
    };
    app_data.dbm.appdata.create_chanting_recording(&rec_data).expect("create recording");

    let detail = app_data.dbm.appdata.get_chanting_section_detail("test-sec-for-rec")
        .expect("get detail")
        .expect("section should exist");

    assert_eq!(detail.recordings.len(), 1);
    assert_eq!(detail.recordings[0].label, Some("Attempt 1".to_string()));
    assert_eq!(detail.recordings[0].recording_type, "user");
    assert_eq!(detail.recordings[0].duration_ms, 5000);
}

/// Re-recording an attempt that is already saved: the row must move to the new
/// file and drop everything derived from the old take, and the superseded file
/// must go. Before `replace_recording_file` existed the QML side had nothing to
/// call, so the new take played in the panel while the row kept the old file.
#[test]
#[serial]
fn test_replace_recording_file() {
    setup();
    let app_data = get_app_data();

    let col_data = ChantingCollectionJson {
        uid: "test-col-for-replace".to_string(),
        title: "Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col_data).expect("create collection");

    let chant_data = ChantingChantJson {
        uid: "test-chant-for-replace".to_string(),
        collection_uid: "test-col-for-replace".to_string(),
        title: "Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant_data).expect("create chant");

    let sec_data = ChantingSectionJson {
        uid: "test-sec-for-replace".to_string(),
        chant_uid: "test-chant-for-replace".to_string(),
        title: "Section".to_string(),
        content_pali: "Pali text".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec_data).expect("create section");

    // Two stand-in files in the real recordings dir: the point of the test is
    // that one of them is gone afterwards.
    let recordings_dir = simsapa_backend::get_chanting_recordings_dir();
    let old_name = "test-replace-old.flac";
    let new_name = "test-replace-new.flac";
    let old_path = recordings_dir.join(old_name);
    let new_path = recordings_dir.join(new_name);
    std::fs::write(&old_path, b"not audio").expect("write old file");
    std::fs::write(&new_path, b"not audio either").expect("write new file");

    let rec_data = ChantingRecordingJson {
        uid: "test-recording-replace".to_string(),
        section_uid: "test-sec-for-replace".to_string(),
        file_name: old_name.to_string(),
        recording_type: "user".to_string(),
        label: Some("Attempt 1".to_string()),
        duration_ms: 5000,
        markers_json: Some("[{\"id\":\"m1\",\"type\":\"position\",\"position_ms\":1200}]".to_string()),
        volume: 0.8,
        playback_position_ms: 1234,
        waveform_json: Some("{\"num_bars\":2,\"peaks\":[0.1,0.2]}".to_string()),
        is_user_added: true,
    };
    app_data.dbm.appdata.create_chanting_recording(&rec_data).expect("create recording");

    app_data.dbm.appdata
        .replace_recording_file("test-recording-replace", new_name)
        .expect("replace recording file");

    let detail = app_data.dbm.appdata.get_chanting_section_detail("test-sec-for-replace")
        .expect("get detail")
        .expect("section should exist");
    assert_eq!(detail.recordings.len(), 1);
    let rec = &detail.recordings[0];

    assert_eq!(rec.file_name, new_name);
    // Re-probed, not carried over: these stand-ins are not decodable, so 0.
    assert_eq!(rec.duration_ms, 0);
    // Everything that indexed the old take's timeline is cleared.
    assert_eq!(rec.markers_json, Some("[]".to_string()));
    assert_eq!(rec.waveform_json, None);
    assert_eq!(rec.playback_position_ms, 0);
    // The label and volume are the user's settings, not derived from the audio.
    assert_eq!(rec.label, Some("Attempt 1".to_string()));
    assert!((rec.volume - 0.8).abs() < 1e-6);

    assert!(!old_path.try_exists().expect("check old file"), "superseded file should be deleted");
    assert!(new_path.try_exists().expect("check new file"), "new file should be kept");

    // An unknown uid is an error, not a silent no-op -- the QML side logs it.
    assert!(app_data.dbm.appdata
        .replace_recording_file("test-recording-does-not-exist", new_name)
        .is_err());
    // And an empty file name is refused before anything is written.
    assert!(app_data.dbm.appdata
        .replace_recording_file("test-recording-replace", "")
        .is_err());
    // So is a file that is not there -- otherwise the row would be pointed at
    // nothing and the delete would take the last remaining audio with it.
    assert!(app_data.dbm.appdata
        .replace_recording_file("test-recording-replace", "test-replace-missing.flac")
        .is_err());
    let after = app_data.dbm.appdata.get_chanting_section_detail("test-sec-for-replace")
        .expect("get detail")
        .expect("section should exist");
    assert_eq!(after.recordings[0].file_name, new_name);
    assert!(new_path.try_exists().expect("check new file"), "new file should survive a refused replace");

    let _ = std::fs::remove_file(&new_path);
}

// --- Cascade delete ---

#[test]
#[serial]
fn test_cascade_delete_collection_removes_chants_and_sections() {
    setup();
    let app_data = get_app_data();

    // Create hierarchy: collection -> chant -> section -> recording
    let col_data = ChantingCollectionJson {
        uid: "test-cascade-col".to_string(),
        title: "Cascade Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col_data).expect("create");

    let chant_data = ChantingChantJson {
        uid: "test-cascade-chant".to_string(),
        collection_uid: "test-cascade-col".to_string(),
        title: "Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant_data).expect("create");

    let sec_data = ChantingSectionJson {
        uid: "test-cascade-sec".to_string(),
        chant_uid: "test-cascade-chant".to_string(),
        title: "Section".to_string(),
        content_pali: "Text".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec_data).expect("create");

    let rec_data = ChantingRecordingJson {
        uid: "test-cascade-rec".to_string(),
        section_uid: "test-cascade-sec".to_string(),
        file_name: "cascade_test.ogg".to_string(),
        recording_type: "user".to_string(),
        label: None,
        duration_ms: 0,
        markers_json: None,
        volume: 1.0,
        playback_position_ms: 0,
        waveform_json: None,
        is_user_added: true,
    };
    app_data.dbm.appdata.create_chanting_recording(&rec_data).expect("create");

    // Delete the collection - should cascade to chants, sections, recordings
    app_data.dbm.appdata.delete_chanting_collection("test-cascade-col").expect("delete");

    // Verify all children are gone
    use simsapa_backend::db::appdata_schema::*;

    let chant_count: i64 = app_data.dbm.appdata.do_read(|db_conn| {
        chanting_chants::table
            .filter(chanting_chants::uid.eq("test-cascade-chant"))
            .count()
            .get_result(db_conn)
    }).expect("query");
    assert_eq!(chant_count, 0, "Chant should be cascade deleted");

    let sec_count: i64 = app_data.dbm.appdata.do_read(|db_conn| {
        chanting_sections::table
            .filter(chanting_sections::uid.eq("test-cascade-sec"))
            .count()
            .get_result(db_conn)
    }).expect("query");
    assert_eq!(sec_count, 0, "Section should be cascade deleted");

    let rec_count: i64 = app_data.dbm.appdata.do_read(|db_conn| {
        chanting_recordings::table
            .filter(chanting_recordings::uid.eq("test-cascade-rec"))
            .count()
            .get_result(db_conn)
    }).expect("query");
    assert_eq!(rec_count, 0, "Recording should be cascade deleted");
}

#[test]
#[serial]
fn test_cascade_delete_chant_removes_sections() {
    setup();
    let app_data = get_app_data();

    let col_data = ChantingCollectionJson {
        uid: "test-cascade2-col".to_string(),
        title: "Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col_data).expect("create");

    let chant_data = ChantingChantJson {
        uid: "test-cascade2-chant".to_string(),
        collection_uid: "test-cascade2-col".to_string(),
        title: "Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant_data).expect("create");

    let sec_data = ChantingSectionJson {
        uid: "test-cascade2-sec".to_string(),
        chant_uid: "test-cascade2-chant".to_string(),
        title: "Section".to_string(),
        content_pali: "Text".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec_data).expect("create");

    // Delete the chant
    app_data.dbm.appdata.delete_chanting_chant("test-cascade2-chant").expect("delete");

    use simsapa_backend::db::appdata_schema::*;

    let sec_count: i64 = app_data.dbm.appdata.do_read(|db_conn| {
        chanting_sections::table
            .filter(chanting_sections::uid.eq("test-cascade2-sec"))
            .count()
            .get_result(db_conn)
    }).expect("query");
    assert_eq!(sec_count, 0, "Section should be cascade deleted");

    // Collection should still exist
    let col_count: i64 = app_data.dbm.appdata.do_read(|db_conn| {
        chanting_collections::table
            .filter(chanting_collections::uid.eq("test-cascade2-col"))
            .count()
            .get_result(db_conn)
    }).expect("query");
    assert_eq!(col_count, 1, "Collection should still exist");
}

// --- Markers ---

#[test]
#[serial]
fn test_update_recording_markers() {
    setup();
    let app_data = get_app_data();

    // Create hierarchy
    let col_data = ChantingCollectionJson {
        uid: "test-markers-col".to_string(),
        title: "Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col_data).expect("create");

    let chant_data = ChantingChantJson {
        uid: "test-markers-chant".to_string(),
        collection_uid: "test-markers-col".to_string(),
        title: "Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant_data).expect("create");

    let sec_data = ChantingSectionJson {
        uid: "test-markers-sec".to_string(),
        chant_uid: "test-markers-chant".to_string(),
        title: "Section".to_string(),
        content_pali: "Text".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec_data).expect("create");

    let rec_data = ChantingRecordingJson {
        uid: "test-markers-rec".to_string(),
        section_uid: "test-markers-sec".to_string(),
        file_name: "markers_test.ogg".to_string(),
        recording_type: "user".to_string(),
        label: None,
        duration_ms: 10000,
        markers_json: None,
        volume: 1.0,
        playback_position_ms: 0,
        waveform_json: None,
        is_user_added: true,
    };
    app_data.dbm.appdata.create_chanting_recording(&rec_data).expect("create");

    // Update markers
    let markers = r#"[{"id":"m1","type":"position","label":"Problem spot","position_ms":5000}]"#;
    app_data.dbm.appdata.update_recording_markers("test-markers-rec", markers).expect("update markers");

    // Verify markers were saved
    let detail = app_data.dbm.appdata.get_chanting_section_detail("test-markers-sec")
        .expect("get detail")
        .expect("section should exist");

    assert_eq!(detail.recordings.len(), 1);
    assert_eq!(detail.recordings[0].markers_json, Some(markers.to_string()));

    // Verify JSON is valid
    let parsed: serde_json::Value = serde_json::from_str(
        detail.recordings[0].markers_json.as_ref().unwrap()
    ).expect("markers should be valid JSON");
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 1);
}

// --- Tree structure ---

#[test]
#[serial]
fn test_get_all_collections_returns_nested_tree() {
    setup();
    let app_data = get_app_data();

    // Create a full hierarchy
    let col = ChantingCollectionJson {
        uid: "tree-col".to_string(),
        title: "Tree Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&col).expect("create");

    let chant1 = ChantingChantJson {
        uid: "tree-chant-1".to_string(),
        collection_uid: "tree-col".to_string(),
        title: "First Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant1).expect("create");

    let chant2 = ChantingChantJson {
        uid: "tree-chant-2".to_string(),
        collection_uid: "tree-col".to_string(),
        title: "Second Chant".to_string(),
        description: None,
        sort_index: 1,
        is_user_added: true,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&chant2).expect("create");

    let sec1 = ChantingSectionJson {
        uid: "tree-sec-1".to_string(),
        chant_uid: "tree-chant-1".to_string(),
        title: "Section A".to_string(),
        content_pali: "Pali A".to_string(),
        sort_index: 0,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec1).expect("create");

    let sec2 = ChantingSectionJson {
        uid: "tree-sec-2".to_string(),
        chant_uid: "tree-chant-1".to_string(),
        title: "Section B".to_string(),
        content_pali: "Pali B".to_string(),
        sort_index: 1,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&sec2).expect("create");

    // Get the tree
    let collections = app_data.dbm.appdata.get_all_chanting_collections().expect("get");
    let tree_col = collections.iter().find(|c| c.uid == "tree-col").expect("find collection");

    assert_eq!(tree_col.chants.len(), 2);
    assert_eq!(tree_col.chants[0].title, "First Chant");
    assert_eq!(tree_col.chants[1].title, "Second Chant");
    assert_eq!(tree_col.chants[0].sections.len(), 2);
    assert_eq!(tree_col.chants[0].sections[0].title, "Section A");
    assert_eq!(tree_col.chants[0].sections[1].title, "Section B");
    assert_eq!(tree_col.chants[1].sections.len(), 0);
}
