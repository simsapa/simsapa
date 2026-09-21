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
        // Filled by `seeded_db` once the row is finished.
        search_text: String::new(),
        path: join_path(&ancestors),
        // Filled by `seeded_db`, which is the only place that can see every
        // ancestor's `sort_index`.
        sort_path: String::new(),
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

    // Recompute `search_text` from the finished row, exactly as the bootstrap's
    // single `flatten_node` pass does. Building it inside `node()` would miss
    // every field the caller sets afterwards -- `sn56_11.sutta_uid` is one --
    // and a fixture that folds less than the writer does would let a real
    // one-sided fold pass its own test.
    let index_of: std::collections::HashMap<String, i32> = rows
        .iter()
        .map(|r| (r.node_uid.clone(), r.sort_index))
        .collect();

    let rows: Vec<NewNavigationNode> = rows
        .into_iter()
        .map(|mut row| {
            row.search_text = build_search_text(
                row.title_pali.as_deref(),
                row.title_en.as_deref(),
                &row.node_uid,
                row.sutta_uid.as_deref(),
            );
            // Resolve the ancestors' `sort_index`es the way `flatten_node` has
            // them on hand during its walk. Taking the shortcut of an empty
            // ancestor list here orders the fixture by each node's own index
            // alone, which is not document order -- and a fixture ordered
            // differently from the writer would let a real ordering defect pass.
            let ancestor_indexes: Vec<i32> = split_path(&row.path)
                .iter()
                .map(|uid| index_of[*uid])
                .collect();
            row.sort_path = join_sort_path(&ancestor_indexes, row.sort_index);
            row
        })
        .collect();

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
        search_text: String::new(),
        path: String::new(),
        sort_path: String::new(),
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

#[test]
fn the_filter_folds_accents_case_and_the_niggahita() {
    let mut db_conn = seeded_db();

    // The seeded titles are `Saccasaṁyutta` and `Saṁyutta Nikāya`. A reader on
    // a phone keyboard types none of those characters.
    for typed in ["saccasamyutta", "Saccasaṃyutta", "SACCASAMYUTTA", "saccasaṁyutta"] {
        let hits = filter_nodes_conn(&mut db_conn, typed, 50).unwrap();
        assert_eq!(
            hits.iter().map(|m| m.node.node_uid.as_str()).collect::<Vec<_>>(),
            vec!["sn56"],
            "{typed:?} should reach Saccasaṁyutta"
        );
    }

    // `Dīgha` and `Saṁyutta Nikāya` -- a long vowel and a capital with a
    // diacritic, which SQLite's ASCII-only LIKE case folding cannot reach.
    assert_eq!(
        filter_nodes_conn(&mut db_conn, "digha", 50).unwrap().len(),
        1
    );
    assert_eq!(
        filter_nodes_conn(&mut db_conn, "SAMYUTTA NIKAYA", 50).unwrap().len(),
        1
    );
}

#[test]
fn the_filter_matches_level_names_as_well_as_suttas() {
    let mut db_conn = seeded_db();

    // A reader who already knows where they are going types the level name.
    // Branch nodes are matched on equal terms with leaves -- that is what makes
    // the filter a shortcut past the drill-down rather than a sutta lookup.
    let hits = filter_nodes_conn(&mut db_conn, "linked", 50).unwrap();
    let uids: Vec<&str> = hits.iter().map(|m| m.node.node_uid.as_str()).collect();
    assert_eq!(uids, vec!["sutta/linked", "sn"]);
    assert!(hits.iter().all(|m| m.node.sutta_uid.is_none()));
}

#[test]
fn normalize_for_search_is_idempotent() {
    // The column is folded once at build time and the query once per keystroke;
    // if the two ever ran a different number of times the results must not move.
    let once = normalize_for_search("Saccasaṁyuttaṃ");
    assert_eq!(once, "saccasamyuttam");
    assert_eq!(normalize_for_search(&once), once);
}

#[test]
fn folding_reaches_every_non_ascii_form_the_corpus_contains() {
    // The six characters measured as surviving `pali_to_ascii` over the built
    // tree, plus the two a table can never catch. A reader types the right-hand
    // side; if any of these stops folding, those titles become unsearchable and
    // nothing reports it.
    for (stored, typed) in [
        ("the lion\u{2019}s roar", "lion's roar"),   // curly apostrophe, 203 titles
        ("\u{201C}quoted\u{201D}", "\"quoted\""), // curly double quotes
        ("one\u{2013}two", "one-two"),               // en dash
        ("and so on\u{2026}", "and so on..."),       // ellipsis
        ("Pi\u{0301}\u{0301}", "pi"),                  // stray combining marks
        ("caf\u{e9}", "cafe"),                       // precomposed e-acute
        ("Sa\u{1E41}yutta\u{1E41}", "samyuttam"),      // niggahita, precomposed
        ("Sa\u{1E43}yutta\u{1E43}", "samyuttam"),      // the other niggahita
        ("Sa\u{14B}yutta\u{14B}", "samyuttam"),        // eng
        ("MAH\u{100}VAGGA", "mahavagga"),            // uppercase with a diacritic
        ("Di\u{304}gha", "digha"),                   // DECOMPOSED long vowel
    ] {
        let folded = normalize_for_search(stored);
        assert!(
            folded.is_ascii(),
            "{stored:?} folded to {folded:?}, which is not ASCII -- the filter \
             compares against a folded column, so anything left here is a title \
             the user cannot reach"
        );
        assert!(
            folded.contains(&normalize_for_search(typed)),
            "typing {typed:?} should reach {stored:?} (folded: {folded:?})"
        );
    }
}

#[test]
fn every_seeded_row_has_a_non_empty_search_text() {
    // The filter matches `search_text` and nothing else, so a row with an empty
    // one is invisible with no error anywhere -- the worst shape a defect can
    // take here. Any future writer that forgets the column fails this.
    let mut db_conn = seeded_db();
    let rows: Vec<(String, String)> = navigation_nodes::table
        .select((navigation_nodes::node_uid, navigation_nodes::search_text))
        .load(&mut db_conn)
        .unwrap();

    assert!(!rows.is_empty());
    for (node_uid, search_text) in rows {
        assert!(
            !search_text.trim().is_empty(),
            "{node_uid} has an empty search_text and can never be filtered to"
        );
        assert!(
            search_text.contains(&normalize_for_search(&node_uid)),
            "{node_uid}: its own uid must be in its search_text, or a filter on \
             the uid misses it"
        );
    }
}

#[test]
fn sort_path_orders_the_tree_in_document_order() {
    // Fixed width is what makes a string compare equal a numeric one.
    assert_eq!(join_sort_path(&[], 0), "|000000|");
    assert_eq!(join_sort_path(&[0, 2], 5), "|000000|000002|000005|");

    // A parent's key is a prefix of its children's, so it always precedes its
    // own subtree...
    let parent = join_sort_path(&[0], 1);
    let child = join_sort_path(&[0, 1], 0);
    assert!(parent < child);

    // ...and a whole subtree precedes its parent's next sibling.
    let uncle = join_sort_path(&[0], 2);
    assert!(child < uncle);

    // Two digits vs one must not order as text: 9 before 10.
    assert!(join_sort_path(&[], 9) < join_sort_path(&[], 10));

    // The CST root offset survives the padding, which is the whole point of
    // ordering the filter by this rather than by `path`.
    assert!(join_sort_path(&[], 2) < join_sort_path(&[], 100));
}

// ---------------------------------------------------------------------------
// The JSON views the window renders rows from.
// ---------------------------------------------------------------------------

/// A branch, a leaf that resolves, a leaf whose text is absent, and a CST
/// fragment row — the four cases the row rendering distinguishes.
fn view_of(
    node_uid: &str,
    sutta_uid: Option<&str>,
    is_available: bool,
    cst_file: Option<&str>,
) -> NavigationNodeView {
    let mut row = node(node_uid, Some("sn56"), 0, "Title", None, &["sutta"]);
    row.sutta_uid = sutta_uid.map(|s| s.to_string());
    row.is_available = is_available;
    row.cst_file = cst_file.map(|s| s.to_string());

    let mut db_conn = migrated_db();
    diesel::insert_into(navigation_nodes::table)
        .values(&row)
        .execute(&mut db_conn)
        .expect("insert");
    let stored = node_by_uid_conn(&mut db_conn, node_uid)
        .expect("query")
        .expect("row");
    NavigationNodeView::from(&stored)
}

#[test]
fn a_branch_is_not_openable() {
    let view = view_of("sn56.branch", None, true, None);
    assert!(!view.is_openable);
    assert!(!view.is_fragment);
    assert_eq!(view.in_file, None);
}

#[test]
fn a_leaf_with_a_present_text_is_openable() {
    let view = view_of("sn56.11", Some("sn56.11/pli/ms"), true, None);
    assert!(view.is_openable);
}

#[test]
fn a_leaf_whose_text_is_absent_is_not_openable() {
    // FR-42: dimmed, never an empty tab. The uid is still carried so the row
    // can show what it would have opened.
    let view = view_of("sn56.12", Some("sn56.12/pli/ms"), false, None);
    assert!(!view.is_openable);
    assert_eq!(view.sutta_uid.as_deref(), Some("sn56.12/pli/ms"));
}

#[test]
fn only_a_fragment_row_carries_the_in_file_label() {
    // A division and a fragment are siblings carrying the same `cst_file`
    // (FR-16), so the uid prefix is the only thing that tells them apart.
    let division = view_of("cst:s0201a.att1.xml", None, true, Some("s0201a.att.xml"));
    assert!(!division.is_fragment);
    assert_eq!(division.in_file, None);
    assert_eq!(division.cst_file.as_deref(), Some("s0201a.att.xml"));

    let fragment = view_of(
        &format!("{}mn11.att/pli/cst", FRAGMENT_UID_PREFIX),
        Some("mn11.att/pli/cst"),
        true,
        Some("s0201a.att.xml"),
    );
    assert!(fragment.is_fragment);
    assert_eq!(fragment.in_file.as_deref(), Some("s0201a.att.xml"));
}

#[test]
fn children_json_is_an_array_of_rows_in_order() {
    let mut db_conn = seeded_db();
    let rows = children_of_conn(&mut db_conn, Some("sn")).expect("query");
    let views: Vec<NavigationNodeView> = rows.iter().map(NavigationNodeView::from).collect();
    let json = serde_json::to_string(&views).expect("serialize");

    let parsed: Vec<NavigationNodeView> = serde_json::from_str(&json).expect("round trip");
    assert_eq!(parsed, views);
    assert!(json.contains("\"is_openable\""));
    assert!(json.contains("\"has_children\""));
}

#[test]
fn a_filter_match_serializes_with_its_breadcrumbs() {
    let mut db_conn = seeded_db();
    let matches = filter_nodes_conn(&mut db_conn, "dhammacakka", 20).expect("query");
    assert!(!matches.is_empty(), "the seed contains sn56.11");

    let views: Vec<NavigationMatchView> = matches.iter().map(NavigationMatchView::from).collect();
    assert_eq!(views[0].breadcrumbs, matches[0].breadcrumbs);
    assert_eq!(views[0].node.node_uid, matches[0].node.node_uid);

    let json = serde_json::to_string(&views).expect("serialize");
    let parsed: Vec<NavigationMatchView> = serde_json::from_str(&json).expect("round trip");
    assert_eq!(parsed, views);
}

#[test]
fn the_public_json_api_is_empty_without_app_data() {
    // Same tolerance as the typed readers: no `AppData` (and so no table) must
    // render as "nothing here", not as a parse error in QML.
    assert_eq!(children_json(None), "[]");
    assert_eq!(filter_json("dhamma", 10), "[]");
    assert_eq!(node_json("sn56.11"), "null");
}

// ---------------------------------------------------------------------------
// Translation counts (FR-47, FR-48)
// ---------------------------------------------------------------------------

/// Only the three columns the count query touches. The real `suttas` table has
/// 30-odd more, including the content blobs the prefix-range shape exists to
/// avoid reading; a query that ever needed one of them would fail here.
fn suttas_db(rows: &[(&str, &str)]) -> SqliteConnection {
    let mut db_conn = empty_db();
    db_conn
        .batch_execute(
            "CREATE TABLE suttas (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 uid TEXT NOT NULL,
                 language TEXT NOT NULL
             );
             CREATE INDEX idx_suttas_language_uid ON suttas(language, uid);",
        )
        .expect("suttas table");
    for (row_uid, lang) in rows {
        diesel::sql_query(format!(
            "INSERT INTO suttas (uid, language) VALUES ('{}', '{}')",
            row_uid, lang
        ))
        .execute(&mut db_conn)
        .expect("insert sutta");
    }
    db_conn
}

const SAMPLE_SUTTAS: &[(&str, &str)] = &[
    ("sn56.11/pli/ms", "pli"),
    ("sn56.11/en/sujato", "en"),
    ("sn56.11/en/bodhi", "en"),
    ("sn56.11/de/sabbamitta", "de"),
    ("sn56.11/hu/koszi", "hu"),
    ("sn56.1/en/sujato", "en"),
    ("sn56.110/en/sujato", "en"),
    ("mn1/en/sujato", "en"),
    ("s0305m.mul11.xml/pli/cst", "pli"),
];

#[test]
fn a_level_count_is_per_uid_and_excludes_the_root_language() {
    let mut db_conn = suttas_db(SAMPLE_SUTTAS);
    let uids = vec!["sn56.11/pli/ms".to_string(), "mn1/pli/ms".to_string()];
    let langs = vec!["pli".to_string(), "en".to_string(), "de".to_string()];

    let counts = translation_counts_conn(&mut db_conn, &uids, &langs).unwrap();

    // Two English plus one German; the Pāli root text is not a translation of
    // itself, and Hungarian is not installed.
    assert_eq!(counts.get("sn56.11/pli/ms"), Some(&3));
    assert_eq!(counts.get("mn1/pli/ms"), Some(&1));
}

#[test]
fn a_prefix_range_does_not_leak_into_a_neighbouring_uid() {
    // `sn56.1` and `sn56.110` both start with the characters of `sn56.11`, and
    // a LIKE 'sn56.11%' would count all three as one text.
    let mut db_conn = suttas_db(SAMPLE_SUTTAS);
    let uids = vec![
        "sn56.1/pli/ms".to_string(),
        "sn56.11/pli/ms".to_string(),
        "sn56.110/pli/ms".to_string(),
    ];
    let langs = vec!["en".to_string(), "de".to_string()];

    let counts = translation_counts_conn(&mut db_conn, &uids, &langs).unwrap();

    assert_eq!(counts.get("sn56.1/pli/ms"), Some(&1));
    assert_eq!(counts.get("sn56.11/pli/ms"), Some(&3));
    assert_eq!(counts.get("sn56.110/pli/ms"), Some(&1));
}

#[test]
fn a_text_with_no_translation_is_absent_rather_than_zero() {
    // The row renders no indicator at all for such a text, so "missing" and
    // "zero" must not both be representable.
    let mut db_conn = suttas_db(SAMPLE_SUTTAS);
    let uids = vec!["s0305m.mul11.xml/pli/cst".to_string()];
    let langs = vec!["en".to_string()];

    let counts = translation_counts_conn(&mut db_conn, &uids, &langs).unwrap();

    assert!(counts.is_empty());
}

#[test]
fn counts_are_empty_with_no_uids_and_with_no_installed_language_but_pali() {
    let mut db_conn = suttas_db(SAMPLE_SUTTAS);
    let langs = vec!["en".to_string()];
    assert!(translation_counts_conn(&mut db_conn, &[], &langs).unwrap().is_empty());

    let uids = vec!["sn56.11/pli/ms".to_string()];
    let only_pali = vec!["pli".to_string()];
    assert!(translation_counts_conn(&mut db_conn, &uids, &only_pali).unwrap().is_empty());
}

#[test]
fn a_level_larger_than_one_chunk_is_counted_in_full() {
    // The OR chain is chunked because SQLite limits expression depth, and the
    // largest level in the built tree is 728 rows.
    let mut rows: Vec<(String, String)> = Vec::new();
    for i in 0..(COUNT_CHUNK * 3 + 7) {
        rows.push((format!("x{}/en/tr", i), "en".to_string()));
    }
    let borrowed: Vec<(&str, &str)> =
        rows.iter().map(|(u, l)| (u.as_str(), l.as_str())).collect();
    let mut db_conn = suttas_db(&borrowed);

    let uids: Vec<String> = (0..(COUNT_CHUNK * 3 + 7))
        .map(|i| format!("x{}/pli/ms", i))
        .collect();
    let langs = vec!["en".to_string()];

    let counts = translation_counts_conn(&mut db_conn, &uids, &langs).unwrap();

    assert_eq!(counts.len(), COUNT_CHUNK * 3 + 7);
    assert_eq!(counts.get("x0/pli/ms"), Some(&1));
}

#[test]
fn a_node_pointing_at_a_translation_does_not_count_itself() {
    // Rare but real: the built tree has one node whose `sutta_uid` is a
    // translation (`an11.22-29/en/sujato`), which the language filter does not
    // exclude the way it excludes every `…/pli/…` node.
    let mut db_conn = suttas_db(SAMPLE_SUTTAS);
    let uids = vec!["sn56.11/pli/ms".to_string(), "sn56.11/en/sujato".to_string()];
    let langs = vec!["en".to_string()];

    let counts = translation_counts_conn(&mut db_conn, &uids, &langs).unwrap();

    // Two English texts: the Pāli node sees both, the English one sees the other.
    assert_eq!(counts.get("sn56.11/pli/ms"), Some(&2));
    assert_eq!(counts.get("sn56.11/en/sujato"), Some(&1));
}

#[test]
fn a_node_that_is_the_only_text_in_an_installed_language_is_absent() {
    // Subtracting itself leaves nothing, which must read as "no indicator"
    // rather than as a count of zero.
    let mut db_conn = suttas_db(&[("mn1/en/sujato", "en"), ("mn1/pli/ms", "pli")]);
    let uids = vec!["mn1/en/sujato".to_string()];
    let langs = vec!["en".to_string()];

    let counts = translation_counts_conn(&mut db_conn, &uids, &langs).unwrap();

    assert!(counts.is_empty());
}

#[test]
fn a_commentary_is_not_counted_as_a_translation_of_the_text() {
    // `mn1.att/pli/cst` sorts before `mn1/`, since '.' < '/', so the prefix
    // range excludes it. The reader's translations panel may still offer it;
    // this indicator counts translations only.
    let mut db_conn = suttas_db(&[
        ("mn1/pli/ms", "pli"),
        ("mn1/en/sujato", "en"),
        ("mn1.att/en/someone", "en"),
        ("mn1.tik/en/someone", "en"),
    ]);
    let uids = vec!["mn1/pli/ms".to_string()];
    let langs = vec!["en".to_string()];

    let counts = translation_counts_conn(&mut db_conn, &uids, &langs).unwrap();

    assert_eq!(counts.get("mn1/pli/ms"), Some(&1));
}

#[test]
fn the_bare_key_is_the_uid_up_to_the_first_slash() {
    assert_eq!(bare_sutta_key("sn56.11/pli/ms"), "sn56.11");
    assert_eq!(bare_sutta_key("s0305m.mul11.xml/pli/cst"), "s0305m.mul11.xml");
    assert_eq!(bare_sutta_key("sn56.11"), "sn56.11");
    assert_eq!(bare_sutta_key(""), "");
}

#[test]
fn the_public_count_api_is_empty_without_app_data() {
    assert!(translation_counts(&["sn56.11/pli/ms".to_string()]).is_empty());
    assert_eq!(translation_counts_json(&["sn56.11/pli/ms".to_string()]), "{}");
}
