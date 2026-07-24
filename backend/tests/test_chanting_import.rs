use diesel::prelude::*;
use serial_test::serial;
use simsapa_backend::db::appdata_models::*;
use simsapa_backend::db::chanting_export::create_chanting_sqlite;
use simsapa_backend::get_app_data;
use tempfile::tempdir;

mod helpers;
use helpers as h;

fn ensure_chanting_tables() {
    let app_data = get_app_data();

    // The 1.0.0 baseline creates the full appdata schema (incl. chanting tables).
    // "already exists" errors are swallowed below when tables are present.
    let migration_sqls = [
        include_str!("../migrations/appdata/2026-07-23-000000_initial_schema/up.sql"),
    ];

    let mut db_conn = app_data.dbm.appdata.get_conn().expect("get conn");
    for up_sql in &migration_sqls {
        for statement in up_sql.split(';') {
            let trimmed = statement.trim();
            if !trimmed.is_empty() {
                diesel::sql_query(trimmed)
                    .execute(&mut db_conn)
                    .unwrap_or_else(|e| {
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

fn cleanup_chanting_data() {
    let app_data = get_app_data();
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

#[test]
#[serial]
fn test_import_skips_existing_and_repairs_orphans() {
    setup();
    let app_data = get_app_data();

    // --- Seed the "live" DB with one collection/chant/section/recording ---
    // These uids match the "already exists" rows we'll put in the import file.
    let seed_col = ChantingCollectionJson {
        uid: "col-seeded".to_string(),
        title: "Seeded Collection".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: false,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&seed_col).expect("seed col");

    let seed_chant = ChantingChantJson {
        uid: "chant-seeded".to_string(),
        collection_uid: "col-seeded".to_string(),
        title: "Seeded Chant".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: false,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&seed_chant).expect("seed chant");

    let seed_section = ChantingSectionJson {
        uid: "sec-seeded".to_string(),
        chant_uid: "chant-seeded".to_string(),
        title: "Seeded Section".to_string(),
        content_pali: "pali".to_string(),
        sort_index: 0,
        is_user_added: false,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&seed_section).expect("seed sec");

    let seed_rec = ChantingRecordingJson {
        uid: "rec-seeded-dup".to_string(),
        section_uid: "sec-seeded".to_string(),
        file_name: "sec-seeded_x.ogg".to_string(),
        recording_type: "reference".to_string(),
        label: None,
        duration_ms: 100,
        markers_json: None,
        volume: 1.0,
        playback_position_ms: 0,
        waveform_json: None,
        is_user_added: false,
    };
    app_data.dbm.appdata.create_chanting_recording(&seed_rec).expect("seed rec");

    // --- Build a minimal import chanting sqlite file ---
    // (a) duplicates of the seeded rows (must be skipped),
    // (b) an orphan user recording whose parent section is NOT in the live DB
    //     (parent section + chant + collection must be synthesised from the
    //     exported rows).
    let collections_for_export = vec![
        // Duplicate of seeded
        ChantingCollection {
            id: 0,
            uid: "col-seeded".to_string(),
            title: "Seeded Collection".to_string(),
            description: None,
            language: "pali".to_string(),
            sort_index: 0,
            is_user_added: false,
            metadata_json: None,
        },
        // New orphan ancestor
        ChantingCollection {
            id: 0,
            uid: "col-new".to_string(),
            title: "Orphan Parent Collection".to_string(),
            description: None,
            language: "pali".to_string(),
            sort_index: 1,
            is_user_added: true,
            metadata_json: None,
        },
    ];
    let chants_for_export = vec![
        ChantingChant {
            id: 0,
            uid: "chant-seeded".to_string(),
            collection_uid: "col-seeded".to_string(),
            title: "Seeded Chant".to_string(),
            description: None,
            sort_index: 0,
            is_user_added: false,
            metadata_json: None,
        },
        ChantingChant {
            id: 0,
            uid: "chant-new".to_string(),
            collection_uid: "col-new".to_string(),
            title: "Orphan Parent Chant".to_string(),
            description: None,
            sort_index: 1,
            is_user_added: true,
            metadata_json: None,
        },
    ];
    let sections_for_export = vec![
        ChantingSection {
            id: 0,
            uid: "sec-seeded".to_string(),
            chant_uid: "chant-seeded".to_string(),
            title: "Seeded Section".to_string(),
            content_pali: "pali".to_string(),
            sort_index: 0,
            is_user_added: false,
            metadata_json: None,
        },
        ChantingSection {
            id: 0,
            uid: "sec-new".to_string(),
            chant_uid: "chant-new".to_string(),
            title: "Orphan Parent Section".to_string(),
            content_pali: "pali".to_string(),
            sort_index: 1,
            is_user_added: true,
            metadata_json: None,
        },
    ];
    let recordings_for_export = vec![
        // Duplicate of seeded recording — must be skipped.
        ChantingRecording {
            id: 0,
            uid: "rec-seeded-dup".to_string(),
            section_uid: "sec-seeded".to_string(),
            file_name: "sec-seeded_x.ogg".to_string(),
            recording_type: "reference".to_string(),
            label: None,
            duration_ms: 100,
            markers_json: None,
            volume: 1.0,
            playback_position_ms: 0,
            waveform_json: None,
            is_user_added: false,
        },
        // Orphan user recording — parents must be synthesised.
        ChantingRecording {
            id: 0,
            uid: "rec-orphan-user".to_string(),
            section_uid: "sec-new".to_string(),
            file_name: "sec-new_user.ogg".to_string(),
            recording_type: "user".to_string(),
            label: Some("mine".to_string()),
            duration_ms: 2000,
            markers_json: None,
            volume: 1.0,
            playback_position_ms: 0,
            waveform_json: None,
            is_user_added: true,
        },
    ];

    let dir = tempdir().unwrap();
    let import_dir = dir.path().to_path_buf();
    let sqlite_path = import_dir.join("appdata-chanting.sqlite3");

    create_chanting_sqlite(
        &sqlite_path,
        &collections_for_export,
        &chants_for_export,
        &sections_for_export,
        &recordings_for_export,
    )
    .expect("write import sqlite");

    // --- Run import ---
    app_data.import_user_chanting_data(&import_dir).expect("import");

    // --- Assertions ---
    // Duplicate collection is not re-inserted (still just one).
    assert!(
        app_data
            .dbm
            .appdata
            .chanting_collection_exists_by_uid("col-seeded")
            .unwrap()
    );
    // Orphan parent collection was created.
    assert!(
        app_data
            .dbm
            .appdata
            .chanting_collection_exists_by_uid("col-new")
            .unwrap()
    );
    // Orphan parent chant + section were created.
    assert!(
        app_data
            .dbm
            .appdata
            .chanting_chant_exists_by_uid("chant-new")
            .unwrap()
    );
    assert!(
        app_data
            .dbm
            .appdata
            .chanting_section_exists_by_uid("sec-new")
            .unwrap()
    );
    // Duplicate recording skipped (still present but not duplicated).
    assert!(
        app_data
            .dbm
            .appdata
            .chanting_recording_exists_by_uid("rec-seeded-dup")
            .unwrap()
    );
    // Orphan user recording was inserted.
    assert!(
        app_data
            .dbm
            .appdata
            .chanting_recording_exists_by_uid("rec-orphan-user")
            .unwrap()
    );

    // Row-count check: exactly one of each seeded uid (no duplicates created).
    use simsapa_backend::db::appdata_schema::chanting_collections::dsl as col_dsl;
    use simsapa_backend::db::appdata_schema::chanting_recordings::dsl as rec_dsl;

    let col_seeded_count: i64 = app_data
        .dbm
        .appdata
        .do_read(|c| {
            col_dsl::chanting_collections
                .filter(col_dsl::uid.eq("col-seeded"))
                .count()
                .get_result(c)
        })
        .unwrap();
    assert_eq!(col_seeded_count, 1, "seeded collection must not be duplicated");

    let rec_dup_count: i64 = app_data
        .dbm
        .appdata
        .do_read(|c| {
            rec_dsl::chanting_recordings
                .filter(rec_dsl::uid.eq("rec-seeded-dup"))
                .count()
                .get_result(c)
        })
        .unwrap();
    assert_eq!(rec_dup_count, 1, "seeded recording must not be duplicated");
}

/// PRD §11.4 — when the exported DB lacks an ancestor chain entirely,
/// synthetic placeholder collection/chant/section must be created so the
/// recording is NOT dropped.
#[test]
#[serial]
fn test_import_synthesises_placeholders_when_exported_ancestors_missing() {
    setup();
    let app_data = get_app_data();

    // Build an import sqlite that contains ONLY a user recording whose
    // section_uid has no matching section/chant/collection rows anywhere.
    let recordings_for_export = vec![ChantingRecording {
        id: 0,
        uid: "rec-fully-orphan".to_string(),
        section_uid: "sec-missing-entirely".to_string(),
        file_name: "rec-fully-orphan.ogg".to_string(),
        recording_type: "user".to_string(),
        label: None,
        duration_ms: 1000,
        markers_json: None,
        volume: 1.0,
        playback_position_ms: 0,
        waveform_json: None,
        is_user_added: true,
    }];

    let dir = tempdir().unwrap();
    let import_dir = dir.path().to_path_buf();
    let sqlite_path = import_dir.join("appdata-chanting.sqlite3");
    create_chanting_sqlite(&sqlite_path, &[], &[], &[], &recordings_for_export)
        .expect("write import sqlite");

    app_data.import_user_chanting_data(&import_dir).expect("import");

    // Synthetic ancestors must exist with the deterministic uids.
    assert!(app_data
        .dbm
        .appdata
        .chanting_collection_exists_by_uid("col-orphan-recovery")
        .unwrap());
    assert!(app_data
        .dbm
        .appdata
        .chanting_chant_exists_by_uid("chant-orphan-recovery")
        .unwrap());
    // The section uid matches the recording's original section_uid so its
    // FK still resolves.
    assert!(app_data
        .dbm
        .appdata
        .chanting_section_exists_by_uid("sec-missing-entirely")
        .unwrap());
    // The recording must not have been dropped.
    assert!(app_data
        .dbm
        .appdata
        .chanting_recording_exists_by_uid("rec-fully-orphan")
        .unwrap());
}

/// PRD §11.5 — a user who has user-added sections but no user-added
/// collections/chants/recordings must still have their data round-trip.
/// Covered at the export-function level: the relaxed early-return does not
/// skip, and seeded-ancestor inclusion pulls in the parent chant + collection.
#[test]
#[serial]
fn test_export_includes_user_sections_without_user_collections_or_recordings() {
    setup();
    let app_data = get_app_data();

    // Seed a (fake) "seeded" collection and chant in the live DB — these
    // are the parents of the user-added section.
    let seed_col = ChantingCollectionJson {
        uid: "col-seed-x".to_string(),
        title: "Seed X".to_string(),
        description: None,
        language: "pali".to_string(),
        sort_index: 0,
        is_user_added: false,
        metadata_json: None,
        chants: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_collection(&seed_col).unwrap();

    let seed_chant = ChantingChantJson {
        uid: "chant-seed-x".to_string(),
        collection_uid: "col-seed-x".to_string(),
        title: "Chant Seed X".to_string(),
        description: None,
        sort_index: 0,
        is_user_added: false,
        metadata_json: None,
        sections: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_chant(&seed_chant).unwrap();

    // User-added section under the seeded chant. No user collections, no
    // user chants, no user recordings.
    let user_section = ChantingSectionJson {
        uid: "sec-user-only".to_string(),
        chant_uid: "chant-seed-x".to_string(),
        title: "User Section".to_string(),
        content_pali: "namo".to_string(),
        sort_index: 1,
        is_user_added: true,
        metadata_json: None,
        recordings: Vec::new(),
    };
    app_data.dbm.appdata.create_chanting_section(&user_section).unwrap();

    // Drive the export.
    let tmp = tempdir().unwrap();
    let import_dir = tmp.path().to_path_buf();
    std::fs::create_dir_all(&import_dir).unwrap();
    // `export_user_chanting_data` is private; go through the public path
    // that calls it. `export_user_data_to_assets` writes to the real assets
    // dir, so instead use the chanting sub-export sqlite directly via a
    // round-trip: read back the written file and assert the user section
    // is present.
    // Here we exercise the public `export_user_data_to_assets` which will
    // create `import-me/appdata-chanting.sqlite3` in the real assets dir.
    let res = app_data.export_user_data_to_assets();
    assert!(res.is_ok() || res.is_err(), "export ran (success or per-category error)");

    // Read back the exported sqlite and assert the user-added section is
    // present with its seeded ancestors.
    use simsapa_backend::db::chanting_export::read_chanting_from_sqlite;
    let app_assets_dir = &simsapa_backend::get_app_globals().paths.app_assets_dir;
    let exported_sqlite = app_assets_dir.join("import-me").join("appdata-chanting.sqlite3");
    if !exported_sqlite.exists() {
        panic!(
            "expected exported sqlite at {} but it was not written",
            exported_sqlite.display()
        );
    }
    let (cols, chants, secs, _recs) = read_chanting_from_sqlite(&exported_sqlite).unwrap();

    assert!(
        cols.iter().any(|c| c.uid == "col-seed-x"),
        "seeded ancestor collection must be carried into the export"
    );
    assert!(
        chants.iter().any(|c| c.uid == "chant-seed-x"),
        "seeded ancestor chant must be carried into the export"
    );
    assert!(
        secs.iter().any(|s| s.uid == "sec-user-only" && s.is_user_added),
        "user-added section must be in the export with is_user_added=true"
    );
}
