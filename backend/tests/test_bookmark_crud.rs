use diesel::prelude::*;
use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::db::appdata_models::*;

mod helpers;
use helpers as h;

/// Ensure bookmark tables exist in the test database by running the migration SQL.
fn ensure_bookmark_tables() {
    let app_data = get_app_data();

    // The 1.0.0 baseline creates the full appdata schema (incl. bookmark tables).
    // "already exists" errors are swallowed below when tables are present.
    let up_sql = include_str!("../migrations/appdata/2026-07-23-000000_initial_schema/up.sql");

    let mut db_conn = app_data.dbm.appdata.get_conn().expect("get conn");
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

/// Clean up all bookmark data between tests
fn cleanup_bookmark_data() {
    let app_data = get_app_data();

    let _ = app_data.dbm.appdata.do_write(|db_conn| {
        diesel::sql_query("DELETE FROM bookmark_items").execute(db_conn)?;
        diesel::sql_query("DELETE FROM bookmark_folders").execute(db_conn)?;
        Ok(())
    });
}

fn setup() {
    h::app_data_setup();
    ensure_bookmark_tables();
    cleanup_bookmark_data();
}

// --- Folder CRUD ---

#[test]
#[serial]
fn test_create_and_read_bookmark_folder() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("My Bookmarks", false)
        .expect("create folder");
    assert!(folder_id > 0);

    let folders = app_data.dbm.appdata.get_all_bookmark_folders();
    assert!(!folders.is_empty());

    let folder = folders.iter().find(|f| f.id == folder_id).expect("find folder");
    assert_eq!(folder.name, "My Bookmarks");
    assert!(!folder.is_last_session);
}

#[test]
#[serial]
fn test_create_folders_auto_sort_order() {
    setup();
    let app_data = get_app_data();

    let id1 = app_data.dbm.appdata.create_bookmark_folder("First", false).expect("create");
    let id2 = app_data.dbm.appdata.create_bookmark_folder("Second", false).expect("create");
    let id3 = app_data.dbm.appdata.create_bookmark_folder("Third", false).expect("create");

    let folders = app_data.dbm.appdata.get_all_bookmark_folders();
    let f1 = folders.iter().find(|f| f.id == id1).unwrap();
    let f2 = folders.iter().find(|f| f.id == id2).unwrap();
    let f3 = folders.iter().find(|f| f.id == id3).unwrap();

    assert!(f1.sort_order < f2.sort_order);
    assert!(f2.sort_order < f3.sort_order);
}

#[test]
#[serial]
fn test_update_bookmark_folder() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("Original", false)
        .expect("create");
    app_data.dbm.appdata.update_bookmark_folder(folder_id, "Renamed")
        .expect("update");

    let folders = app_data.dbm.appdata.get_all_bookmark_folders();
    let folder = folders.iter().find(|f| f.id == folder_id).expect("find");
    assert_eq!(folder.name, "Renamed");
}

#[test]
#[serial]
fn test_delete_bookmark_folder() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("To Delete", false)
        .expect("create");
    app_data.dbm.appdata.delete_bookmark_folder(folder_id).expect("delete");

    let folders = app_data.dbm.appdata.get_all_bookmark_folders();
    assert!(folders.iter().all(|f| f.id != folder_id));
}

// --- Item CRUD ---

#[test]
#[serial]
fn test_create_and_read_bookmark_items() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("Folder", false)
        .expect("create folder");

    let new_item = NewBookmarkItem {
        folder_id,
        item_uid: "mn1/en/sujato".to_string(),
        table_name: "suttas".to_string(),
        title: Some("The Root of All Things".to_string()),
        tab_group: "pinned".to_string(),
        scroll_position: 0.5,
        find_query: "".to_string(),
        find_match_index: 0,
        sort_order: 0,
        is_user_added: true,
    };

    let item_id = app_data.dbm.appdata.create_bookmark_item(&new_item).expect("create item");
    assert!(item_id > 0);

    let items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder_id);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_uid, "mn1/en/sujato");
    assert_eq!(items[0].title, Some("The Root of All Things".to_string()));
    assert_eq!(items[0].tab_group, "pinned");
    assert!((items[0].scroll_position - 0.5).abs() < 0.001);
}

#[test]
#[serial]
fn test_update_bookmark_item() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("Folder", false)
        .expect("create folder");

    let new_item = NewBookmarkItem {
        folder_id,
        item_uid: "dn1/en/sujato".to_string(),
        table_name: "suttas".to_string(),
        title: Some("Original".to_string()),
        tab_group: "results".to_string(),
        scroll_position: 0.0,
        find_query: "".to_string(),
        find_match_index: 0,
        sort_order: 0,
        is_user_added: true,
    };

    let item_id = app_data.dbm.appdata.create_bookmark_item(&new_item).expect("create");

    let update = BookmarkItemUpdate {
        item_uid: None,
        title: Some("Updated Title".to_string()),
        tab_group: Some("pinned".to_string()),
        find_query: Some("dhamma".to_string()),
        find_match_index: Some(3),
    };

    app_data.dbm.appdata.update_bookmark_item(item_id, &update).expect("update");

    let items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder_id);
    let item = items.iter().find(|i| i.id == item_id).expect("find");
    assert_eq!(item.title, Some("Updated Title".to_string()));
    assert_eq!(item.tab_group, "pinned");
    assert_eq!(item.find_query, "dhamma");
    assert_eq!(item.find_match_index, 3);
    // Unchanged field
    assert_eq!(item.item_uid, "dn1/en/sujato");
}

#[test]
#[serial]
fn test_delete_bookmark_item() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("Folder", false)
        .expect("create folder");

    let new_item = NewBookmarkItem {
        folder_id,
        item_uid: "sn56.11/en/sujato".to_string(),
        table_name: "suttas".to_string(),
        title: None,
        tab_group: "results".to_string(),
        scroll_position: 0.0,
        find_query: "".to_string(),
        find_match_index: 0,
        sort_order: 0,
        is_user_added: true,
    };

    let item_id = app_data.dbm.appdata.create_bookmark_item(&new_item).expect("create");
    app_data.dbm.appdata.delete_bookmark_item(item_id).expect("delete");

    let items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder_id);
    assert!(items.is_empty());
}

// --- Reorder ---

#[test]
#[serial]
fn test_reorder_bookmark_items() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("Folder", false)
        .expect("create folder");

    let make_item = |uid: &str| NewBookmarkItem {
        folder_id,
        item_uid: uid.to_string(),
        table_name: "suttas".to_string(),
        title: None,
        tab_group: "results".to_string(),
        scroll_position: 0.0,
        find_query: "".to_string(),
        find_match_index: 0,
        sort_order: 0,
        is_user_added: true,
    };

    let id1 = app_data.dbm.appdata.create_bookmark_item(&make_item("a")).expect("create");
    let id2 = app_data.dbm.appdata.create_bookmark_item(&make_item("b")).expect("create");
    let id3 = app_data.dbm.appdata.create_bookmark_item(&make_item("c")).expect("create");

    // Reverse the order
    app_data.dbm.appdata.reorder_bookmark_items(folder_id, &[id3, id2, id1])
        .expect("reorder");

    let items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder_id);
    assert_eq!(items[0].id, id3);
    assert_eq!(items[1].id, id2);
    assert_eq!(items[2].id, id1);
}

#[test]
#[serial]
fn test_reorder_bookmark_folders() {
    setup();
    let app_data = get_app_data();

    let id1 = app_data.dbm.appdata.create_bookmark_folder("A", false).expect("create");
    let id2 = app_data.dbm.appdata.create_bookmark_folder("B", false).expect("create");
    let id3 = app_data.dbm.appdata.create_bookmark_folder("C", false).expect("create");

    // Reverse
    app_data.dbm.appdata.reorder_bookmark_folders(&[id3, id1, id2]).expect("reorder");

    let folders = app_data.dbm.appdata.get_all_bookmark_folders();
    assert_eq!(folders[0].id, id3);
    assert_eq!(folders[1].id, id1);
    assert_eq!(folders[2].id, id2);
}

// --- Move items between folders ---

#[test]
#[serial]
fn test_move_bookmark_items_to_folder() {
    setup();
    let app_data = get_app_data();

    let folder1 = app_data.dbm.appdata.create_bookmark_folder("Source", false).expect("create");
    let folder2 = app_data.dbm.appdata.create_bookmark_folder("Target", false).expect("create");

    let new_item = NewBookmarkItem {
        folder_id: folder1,
        item_uid: "mn1/en/sujato".to_string(),
        table_name: "suttas".to_string(),
        title: None,
        tab_group: "results".to_string(),
        scroll_position: 0.0,
        find_query: "".to_string(),
        find_match_index: 0,
        sort_order: 0,
        is_user_added: true,
    };

    let item_id = app_data.dbm.appdata.create_bookmark_item(&new_item).expect("create");

    app_data.dbm.appdata.move_bookmark_items_to_folder(&[item_id], folder2).expect("move");

    let source_items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder1);
    let target_items = app_data.dbm.appdata.get_bookmark_items_for_folder(folder2);

    assert!(source_items.is_empty());
    assert_eq!(target_items.len(), 1);
    assert_eq!(target_items[0].id, item_id);
}

// --- Cascade delete ---

#[test]
#[serial]
fn test_cascade_delete_folder_removes_items() {
    setup();
    let app_data = get_app_data();

    let folder_id = app_data.dbm.appdata.create_bookmark_folder("Folder", false)
        .expect("create folder");

    let new_item = NewBookmarkItem {
        folder_id,
        item_uid: "mn1/en/sujato".to_string(),
        table_name: "suttas".to_string(),
        title: None,
        tab_group: "results".to_string(),
        scroll_position: 0.0,
        find_query: "".to_string(),
        find_match_index: 0,
        sort_order: 0,
        is_user_added: true,
    };

    let item_id = app_data.dbm.appdata.create_bookmark_item(&new_item).expect("create item");

    app_data.dbm.appdata.delete_bookmark_folder(folder_id).expect("delete folder");

    use simsapa_backend::db::appdata_schema::*;

    let item_count: i64 = app_data.dbm.appdata.do_read(|db_conn| {
        bookmark_items::table
            .filter(bookmark_items::id.eq(item_id))
            .count()
            .get_result(db_conn)
    }).expect("query");
    assert_eq!(item_count, 0, "Items should be cascade deleted with folder");
}

// --- Last session lifecycle ---

#[test]
#[serial]
fn test_last_session_lifecycle() {
    setup();
    let app_data = get_app_data();

    // Create a last session folder
    let session_id = app_data.dbm.appdata.create_bookmark_folder("Window 1", true)
        .expect("create session folder");

    let new_item = NewBookmarkItem {
        folder_id: session_id,
        item_uid: "dn2/en/sujato".to_string(),
        table_name: "suttas".to_string(),
        title: Some("Sāmaññaphala".to_string()),
        tab_group: "pinned".to_string(),
        scroll_position: 0.3,
        find_query: "".to_string(),
        find_match_index: 0,
        sort_order: 0,
        is_user_added: true,
    };
    app_data.dbm.appdata.create_bookmark_item(&new_item).expect("create item");

    // Create a regular folder too
    let regular_id = app_data.dbm.appdata.create_bookmark_folder("Regular", false)
        .expect("create regular folder");

    // Verify last session folders
    let session_folders = app_data.dbm.appdata.get_last_session_folders();
    assert_eq!(session_folders.len(), 1);
    assert_eq!(session_folders[0].name, "Window 1");

    // Delete last session folders
    app_data.dbm.appdata.delete_last_session_folders().expect("delete last session");

    // Last session folders should be gone
    let session_folders = app_data.dbm.appdata.get_last_session_folders();
    assert!(session_folders.is_empty());

    // Regular folder should still exist
    let all_folders = app_data.dbm.appdata.get_all_bookmark_folders();
    assert!(all_folders.iter().any(|f| f.id == regular_id));
}
