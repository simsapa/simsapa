//! Read-layer tests against a temp SQLite database created from the real
//! migration SQL, so a drift between `up.sql` and the diesel `table!`
//! declaration fails here rather than at runtime.

use diesel::connection::SimpleConnection;
use diesel::prelude::*;

use super::*;
use crate::db::appdata_schema::navigation_nodes;

const UP_SQL: &str =
    include_str!("../../migrations/appdata/2026-09-19-000000_navigation_tree/up.sql");

fn empty_db() -> SqliteConnection {
    SqliteConnection::establish(":memory:").expect("in-memory db")
}

fn migrated_db() -> SqliteConnection {
    let mut db_conn = empty_db();
    db_conn.batch_execute(UP_SQL).expect("up.sql applies");
    db_conn
}

fn node(
    node_uid: &str,
    parent_uid: Option<&str>,
    sort_index: i32,
    title_pali: &str,
    title_en: Option<&str>,
    ancestors: &[&str],
) -> NewNavigationNode {
    let ancestors: Vec<String> = ancestors.iter().map(|s| s.to_string()).collect();
    NewNavigationNode {
        node_uid: node_uid.to_string(),
        parent_uid: parent_uid.map(|s| s.to_string()),
        sort_index,
        node_type: "group".to_string(),
        tree_source: "sc".to_string(),
        title_pali: Some(title_pali.to_string()),
        title_en: title_en.map(|s| s.to_string()),
        blurb: None,
        sutta_uid: None,
        cst_file: None,
        has_children: false,
        child_count: 0,
        is_available: true,
        path: join_path(&ancestors),
    }
}

/// Sutta → Linked → SN → SN56 → sn56.11, plus a sibling root, inserted out of
/// sort order so the ordering assertions mean something.
fn seeded_db() -> SqliteConnection {
    let mut db_conn = migrated_db();

    let mut sn56_11 = node(
        "sn56.11",
        Some("sn56"),
        0,
        "Dhammacakkappavattanasutta",
        Some("Setting the Wheel of Dhamma Rolling"),
        &["sutta", "sutta/linked", "sn", "sn56"],
    );
    sn56_11.node_type = "sutta".to_string();
    sn56_11.sutta_uid = Some("sn56.11/pli/ms".to_string());
    sn56_11.blurb = Some("The Buddha's first discourse.".to_string());

    let rows = vec![
        node("vinaya", None, 1, "Vinaya", Some("Monastic Law"), &[]),
        node("sutta", None, 0, "Sutta", Some("Discourses"), &[]),
        node(
            "sutta/linked",
            Some("sutta"),
            2,
            "Saṁyutta",
            Some("Linked"),
            &["sutta"],
        ),
        node(
            "sutta/long",
            Some("sutta"),
            0,
            "Dīgha",
            Some("Long"),
            &["sutta"],
        ),
        node(
            "sn",
            Some("sutta/linked"),
            0,
            "Saṁyutta Nikāya",
            Some("Linked Discourses"),
            &["sutta", "sutta/linked"],
        ),
        node(
            "sn56",
            Some("sn"),
            0,
            "Saccasaṁyutta",
            Some("On the Truths"),
            &["sutta", "sutta/linked", "sn"],
        ),
        sn56_11,
    ];

    diesel::insert_into(navigation_nodes::table)
        .values(&rows)
        .execute(&mut db_conn)
        .expect("seed inserts");

    db_conn
}

#[test]
fn path_helpers_round_trip() {
    let ancestors = vec!["sutta".to_string(), "sutta/linked".to_string()];
    let path = join_path(&ancestors);
    assert_eq!(path, "|sutta|sutta/linked|");
    assert_eq!(split_path(&path), vec!["sutta", "sutta/linked"]);

    assert_eq!(join_path(&[]), "");
    assert!(split_path("").is_empty());
}

#[test]
fn root_level_is_the_null_parent_in_sort_order() {
    let mut db_conn = seeded_db();
    let roots = children_of_conn(&mut db_conn, None).unwrap();
    let uids: Vec<&str> = roots.iter().map(|n| n.node_uid.as_str()).collect();
    assert_eq!(uids, vec!["sutta", "vinaya"]);
}

#[test]
fn level_query_returns_one_level_in_sort_order() {
    let mut db_conn = seeded_db();
    let level = children_of_conn(&mut db_conn, Some("sutta")).unwrap();
    let uids: Vec<&str> = level.iter().map(|n| n.node_uid.as_str()).collect();
    // sort_index order, not insertion or alphabetical order, and the
    // grandchildren are not included.
    assert_eq!(uids, vec!["sutta/long", "sutta/linked"]);
}

#[test]
fn filter_matches_pali_title_english_title_and_uid() {
    let mut db_conn = seeded_db();

    let by_pali = filter_nodes_conn(&mut db_conn, "dhammacakka", 50).unwrap();
    assert_eq!(by_pali.len(), 1);
    assert_eq!(by_pali[0].node.node_uid, "sn56.11");

    let by_english = filter_nodes_conn(&mut db_conn, "Wheel of Dhamma", 50).unwrap();
    assert_eq!(by_english.len(), 1);
    assert_eq!(by_english[0].node.node_uid, "sn56.11");

    let by_node_uid = filter_nodes_conn(&mut db_conn, "sn56", 50).unwrap();
    let uids: Vec<&str> = by_node_uid
        .iter()
        .map(|m| m.node.node_uid.as_str())
        .collect();
    assert_eq!(uids, vec!["sn56", "sn56.11"]);

    // The sutta uid is matched too, and it is not the node uid.
    let by_sutta_uid = filter_nodes_conn(&mut db_conn, "sn56.11/pli/ms", 50).unwrap();
    assert_eq!(by_sutta_uid.len(), 1);
    assert_eq!(by_sutta_uid[0].node.node_uid, "sn56.11");
}

#[test]
fn filter_reconstructs_the_breadcrumb_path() {
    let mut db_conn = seeded_db();
    let hits = filter_nodes_conn(&mut db_conn, "dhammacakka", 50).unwrap();
    assert_eq!(
        hits[0].breadcrumbs,
        vec![
            "Sutta".to_string(),
            "Saṁyutta".to_string(),
            "Saṁyutta Nikāya".to_string(),
            "Saccasaṁyutta".to_string(),
        ]
    );

    // A root has no breadcrumbs at all.
    let roots = filter_nodes_conn(&mut db_conn, "Vinaya", 50).unwrap();
    assert!(roots[0].breadcrumbs.is_empty());
}

#[test]
fn a_missing_ancestor_is_carried_as_its_uid() {
    let mut db_conn = seeded_db();
    diesel::delete(navigation_nodes::table.filter(navigation_nodes::node_uid.eq("sn")))
        .execute(&mut db_conn)
        .unwrap();

    let hits = filter_nodes_conn(&mut db_conn, "dhammacakka", 50).unwrap();
    // The broken segment shows as its uid rather than the path silently
    // shortening, so the defect is visible.
    assert_eq!(hits[0].breadcrumbs.len(), 4);
    assert_eq!(hits[0].breadcrumbs[2], "sn");
}

#[test]
fn filter_honours_the_limit_and_ignores_an_empty_query() {
    let mut db_conn = seeded_db();
    assert!(filter_nodes_conn(&mut db_conn, "   ", 50).unwrap().is_empty());
    assert_eq!(filter_nodes_conn(&mut db_conn, "s", 2).unwrap().len(), 2);
}

#[test]
fn a_bare_wildcard_does_not_match_everything() {
    let mut db_conn = seeded_db();
    // '%' is stripped, so the query is empty after trimming and matches
    // nothing -- rather than returning the whole tree as a flat list.
    assert!(filter_nodes_conn(&mut db_conn, "%", 50).unwrap().is_empty());
}

#[test]
fn node_by_uid_finds_one_and_misses_cleanly() {
    let mut db_conn = seeded_db();
    let found = node_by_uid_conn(&mut db_conn, "sn56.11").unwrap().unwrap();
    assert_eq!(found.sutta_uid.as_deref(), Some("sn56.11/pli/ms"));
    assert_eq!(found.display_title(), "Dhammacakkappavattanasutta");
    assert!(node_by_uid_conn(&mut db_conn, "nope").unwrap().is_none());
}

#[test]
fn display_title_falls_back_through_english_to_the_uid() {
    let mut n = node("x", None, 0, "", None, &[]);
    n.title_pali = None;
    let row = NavigationNode {
        id: 1,
        node_uid: n.node_uid,
        parent_uid: None,
        sort_index: 0,
        node_type: n.node_type,
        tree_source: n.tree_source,
        title_pali: None,
        title_en: Some("English".to_string()),
        blurb: None,
        sutta_uid: None,
        cst_file: None,
        has_children: false,
        child_count: 0,
        is_available: true,
        path: String::new(),
    };
    assert_eq!(row.display_title(), "English");

    let bare = NavigationNode {
        title_en: None,
        ..row
    };
    assert_eq!(bare.display_title(), "x");
}

#[test]
fn availability_is_false_for_an_empty_table_and_true_once_populated() {
    let mut empty = migrated_db();
    assert!(!tree_is_available_conn(&mut empty).unwrap());

    let mut seeded = seeded_db();
    assert!(tree_is_available_conn(&mut seeded).unwrap());
}

#[test]
fn a_missing_table_is_an_error_not_a_panic() {
    // The table is absent when a migration failed, which is non-fatal by
    // design. The connection-level calls must report it as an ordinary query
    // error...
    let mut db_conn = empty_db();
    assert!(children_of_conn(&mut db_conn, None).is_err());
    assert!(node_by_uid_conn(&mut db_conn, "sutta").is_err());
    assert!(filter_nodes_conn(&mut db_conn, "sutta", 50).is_err());
    assert!(tree_is_available_conn(&mut db_conn).is_err());
}

#[test]
fn the_public_api_is_empty_without_app_data() {
    // ...and the public wrappers must answer empty rather than propagating it,
    // so the window shows "navigation index not available". There is no
    // `AppData` in a unit-test process, which is the same code path.
    assert!(children_of(None).is_empty());
    assert!(filter_nodes("sutta", 50).is_empty());
    assert!(node_by_uid("sutta").is_none());
    assert!(!tree_is_available());
}
