use super::*;

fn leaf(uid: &str) -> TreeNode {
    TreeNode::new(uid, "sutta")
}

fn branch(uid: &str, children: Vec<TreeNode>) -> TreeNode {
    let mut n = TreeNode::new(uid, "branch");
    n.children = children;
    n
}

#[test]
fn flatten_assigns_source_order_parents_and_paths() {
    let roots = vec![branch(
        "sutta",
        vec![branch("sn", vec![leaf("sn56.11"), leaf("sn56.12")])],
    )];

    let mut defects = DefectReport::default();
    let rows = flatten(&roots, "sc", &mut defects);

    assert!(defects.is_empty());
    let by_uid = |uid: &str| rows.iter().find(|r| r.node_uid == uid).unwrap();

    assert_eq!(by_uid("sutta").parent_uid, None);
    assert_eq!(by_uid("sutta").path, "");
    assert_eq!(by_uid("sn").path, "|sutta|");
    assert_eq!(by_uid("sn56.11").path, "|sutta|sn|");
    assert_eq!(by_uid("sn56.11").parent_uid.as_deref(), Some("sn"));

    // sort_index is the position among siblings, i.e. the source's own order.
    assert_eq!(by_uid("sn56.11").sort_index, 0);
    assert_eq!(by_uid("sn56.12").sort_index, 1);

    assert!(by_uid("sn").has_children);
    assert_eq!(by_uid("sn").child_count, 2);
    assert!(!by_uid("sn56.11").has_children);
    assert_eq!(by_uid("sn56.11").child_count, 0);
}

#[test]
fn a_repeated_node_uid_is_reported_and_its_subtree_skipped() {
    // The column is unique, so inserting the second occurrence would abort the
    // whole transaction. The first wins and the second is named in the report.
    let roots = vec![branch(
        "root",
        vec![
            branch("dup", vec![leaf("kept")]),
            branch("dup", vec![leaf("dropped")]),
        ],
    )];

    let mut defects = DefectReport::default();
    let rows = flatten(&roots, "sc", &mut defects);

    let uids: Vec<&str> = rows.iter().map(|r| r.node_uid.as_str()).collect();
    assert_eq!(uids, vec!["root", "dup", "kept"]);
    assert_eq!(defects.count_of("duplicate node uid (subtree skipped)"), 1);
}

#[test]
fn several_roots_are_ordered_and_carry_no_parent() {
    let roots = vec![leaf("sutta"), leaf("vinaya"), leaf("abhidhamma")];
    let mut defects = DefectReport::default();
    let rows = flatten(&roots, "sc", &mut defects);

    assert_eq!(rows.len(), 3);
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.sort_index, i as i32);
        assert_eq!(row.parent_uid, None);
        assert_eq!(row.path, "");
        assert_eq!(row.tree_source, "sc");
    }
}

#[test]
fn the_defect_report_counts_by_kind() {
    let mut defects = DefectReport::default();
    defects.record("missing text", "a");
    defects.record("missing text", "b");
    defects.record("missing file", "c");

    assert_eq!(defects.len(), 3);
    assert_eq!(defects.count_of("missing text"), 2);
    assert_eq!(defects.count_of("missing file"), 1);
    assert_eq!(defects.count_of("nothing of the kind"), 0);
}

#[test]
fn the_two_spines_do_not_interleave_at_the_root_level() {
    // Each spine is built and written independently and numbers its own roots
    // from zero, so without the offset the root level is
    // `sutta(0) atthakatha(0) vinaya(1) tika(1) ...` -- the two groups shuffled,
    // with the tie broken only by whichever spine was inserted last.
    let sc = flatten(
        &[leaf("sutta"), leaf("vinaya"), leaf("abhidhamma")],
        "sc",
        &mut DefectReport::default(),
    );
    let cst = flatten(
        &[leaf("cst:atthakatha"), leaf("cst:tika"), leaf("cst:anna")],
        "cst",
        &mut DefectReport::default(),
    );

    let sc_indexes: Vec<i32> = sc.iter().map(|r| r.sort_index).collect();
    let cst_indexes: Vec<i32> = cst.iter().map(|r| r.sort_index).collect();
    assert_eq!(sc_indexes, vec![0, 1, 2]);

    // Every CST root sorts after every SC root, whichever was written first.
    assert!(
        cst_indexes.iter().min().unwrap() > sc_indexes.iter().max().unwrap(),
        "CST roots {cst_indexes:?} must all follow the SC roots {sc_indexes:?}"
    );
    assert!(cst_indexes.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn titles_are_folded_and_carried_into_search_text() {
    // The CST toc writes `Saccasaṃyuttaṃ` and SuttaCentral writes
    // `Saccasaṁyutta`: one word, two spellings, two spines. Folding happens in
    // the single pass every row of both spines goes through, so a displayed
    // title and its `search_text` cannot drift apart.
    let mut node = TreeNode::new("sn56", "samyutta");
    node.title_pali = Some("  Saccasaṃyuttaṃ  ".to_string());
    node.title_en = Some("On the Truths".to_string());
    node.sutta_uid = Some("sn56/pli/ms".to_string());

    let rows = flatten(&[node], "sc", &mut DefectReport::default());
    let row = &rows[0];

    assert_eq!(row.title_pali.as_deref(), Some("Saccasaṁyuttaṁ"));
    assert!(row.search_text.is_ascii());
    for expected in ["saccasamyuttam", "on the truths", "sn56", "sn56/pli/ms"] {
        assert!(
            row.search_text.contains(expected),
            "search_text {:?} is missing {expected:?}",
            row.search_text
        );
    }
}

#[test]
fn every_flattened_row_carries_a_search_text() {
    // A row with an empty `search_text` is invisible to the filter and reports
    // nothing -- the worst shape a defect can take here. A node with no titles
    // at all still has its uid to be found by.
    let rows = flatten(
        &[branch("sutta", vec![leaf("sn56.11")])],
        "sc",
        &mut DefectReport::default(),
    );
    assert!(!rows.is_empty());
    for row in &rows {
        assert!(
            !row.search_text.trim().is_empty(),
            "{} has an empty search_text",
            row.node_uid
        );
    }
}

#[test]
fn sort_path_puts_filter_results_in_document_order() {
    // `path` is built from node_uids, so ordering by it is alphabetical: the
    // CST roots came back before the SC ones (`|cst:...` < `|sutta|`) and Añña
    // before Aṭṭhakathā, contradicting the level order the user had just been
    // browsing. `sort_path` is the same walk in sort_indexes.
    let sc = flatten(
        &[branch("sutta", vec![leaf("dn"), leaf("mn")]), leaf("vinaya")],
        "sc",
        &mut DefectReport::default(),
    );
    let cst = flatten(
        &[leaf("cst:atthakatha"), leaf("cst:anna")],
        "cst",
        &mut DefectReport::default(),
    );

    let mut all: Vec<(String, String)> = sc
        .iter()
        .chain(cst.iter())
        .map(|r| (r.sort_path.clone(), r.node_uid.clone()))
        .collect();
    all.sort();

    let order: Vec<&str> = all.iter().map(|(_, uid)| uid.as_str()).collect();
    assert_eq!(
        order,
        vec!["sutta", "dn", "mn", "vinaya", "cst:atthakatha", "cst:anna"],
        "a parent must precede its subtree, a subtree must precede its parent's \
         next sibling, and every SC root must precede every CST root"
    );

    // Ordering by `path` instead gives the wrong answer -- the thing this
    // column exists to prevent.
    let mut by_path: Vec<(String, String)> = sc
        .iter()
        .chain(cst.iter())
        .map(|r| (r.path.clone(), r.node_uid.clone()))
        .collect();
    by_path.sort();
    let path_order: Vec<&str> = by_path.iter().map(|(_, uid)| uid.as_str()).collect();
    assert_ne!(path_order, order);
}

fn texts(toml: &str) -> BTreeMap<String, String> {
    parse_node_texts(toml).unwrap()
}

fn sc_blurbs(uids: &[&str]) -> HashSet<String> {
    uids.iter().map(|s| s.to_string()).collect()
}

#[test]
fn keys_that_name_a_node_are_checked_and_report_nothing() {
    let roots = vec![branch("sutta", vec![leaf("sn56.11"), leaf("sn56.12")])];
    let mut defects = DefectReport::default();
    let rows = flatten(&roots, "sc", &mut defects);

    let notes = texts(
        r#"
"sn56.11" = "The first discourse."
"cst:tika" = "Sub-commentaries."
"#,
    );
    // A replacement, an addition on a node with no SuttaCentral blurb, and a
    // suppression of a blurb that exists.
    let overrides = texts(
        r#"
"sn56.11" = "A corrected summary."
"sutta" = "A summary SuttaCentral has none for."
"sn56.12" = ""
"cst:tika" = "A summary for a CST node."
"#,
    );

    let checked = validate_node_text_keys(
        &rows,
        "sc",
        &notes,
        &overrides,
        &sc_blurbs(&["sn56.11", "sn56.12"]),
        &mut defects,
    );

    // The `cst:` keys belong to the other spine, which is built separately:
    // they are neither counted nor reported here.
    assert_eq!(checked, CheckedKeys { context_notes: 1, overrides: 3 });
    assert!(defects.is_empty());
}

#[test]
fn validation_writes_nothing_to_the_rows() {
    let mut defects = DefectReport::default();
    let rows = flatten(&[leaf("sn56.11")], "sc", &mut defects);
    let before = format!("{:?}", rows);

    validate_node_text_keys(
        &rows,
        "sc",
        &texts(r#""sn56.11" = "A note.""#),
        &texts(r#""sn56.11" = "An override.""#),
        &HashSet::new(),
        &mut defects,
    );

    assert_eq!(format!("{:?}", rows), before);
}

#[test]
fn a_context_note_for_an_unknown_node_is_reported_per_spine() {
    let notes = texts(
        r#"
"sn99.99" = "No such sutta."
"cst:nope" = "No such book."
"#,
    );
    let none = BTreeMap::new();

    let mut sc_defects = DefectReport::default();
    let sc_rows = flatten(&[leaf("sutta")], "sc", &mut sc_defects);
    validate_node_text_keys(&sc_rows, "sc", &notes, &none, &HashSet::new(), &mut sc_defects);
    assert_eq!(sc_defects.len(), 1);
    assert_eq!(sc_defects.count_of("context note for a node uid not in the tree"), 1);

    let mut cst_defects = DefectReport::default();
    let cst_rows = flatten(&[leaf("cst:tika")], "cst", &mut cst_defects);
    validate_node_text_keys(&cst_rows, "cst", &notes, &none, &HashSet::new(), &mut cst_defects);
    assert_eq!(cst_defects.len(), 1);
    assert_eq!(cst_defects.count_of("context note for a node uid not in the tree"), 1);
}

#[test]
fn a_blurb_override_for_an_unknown_node_is_reported_per_spine() {
    let overrides = texts(
        r#"
"sn99.99" = "No such sutta."
"cst:nope" = "No such book."
"#,
    );
    let none = BTreeMap::new();
    let label = "blurb override for a node uid not in the tree";

    let mut sc_defects = DefectReport::default();
    let sc_rows = flatten(&[leaf("sutta")], "sc", &mut sc_defects);
    let checked =
        validate_node_text_keys(&sc_rows, "sc", &none, &overrides, &HashSet::new(), &mut sc_defects);
    assert_eq!(checked.overrides, 1);
    assert_eq!(sc_defects.len(), 1);
    assert_eq!(sc_defects.count_of(label), 1);

    let mut cst_defects = DefectReport::default();
    let cst_rows = flatten(&[leaf("cst:tika")], "cst", &mut cst_defects);
    let checked = validate_node_text_keys(
        &cst_rows,
        "cst",
        &none,
        &overrides,
        &HashSet::new(),
        &mut cst_defects,
    );
    assert_eq!(checked.overrides, 1);
    assert_eq!(cst_defects.len(), 1);
    assert_eq!(cst_defects.count_of(label), 1);
}

#[test]
fn an_empty_override_that_suppresses_nothing_is_reported() {
    let label = "empty blurb override for a node with no SuttaCentral blurb";
    let none = BTreeMap::new();
    let overrides = texts(
        r#"
"sn56.11" = ""
"sn56.12" = ""
"sn99.99" = ""
"cst:tika" = ""
"#,
    );

    // `sn56.11` has a blurb to suppress; `sn56.12` has none. `sn99.99` names
    // no node, which is the one thing reported about it.
    let roots = vec![branch("sutta", vec![leaf("sn56.11"), leaf("sn56.12")])];
    let mut sc_defects = DefectReport::default();
    let sc_rows = flatten(&roots, "sc", &mut sc_defects);
    validate_node_text_keys(
        &sc_rows,
        "sc",
        &none,
        &overrides,
        &sc_blurbs(&["sn56.11"]),
        &mut sc_defects,
    );
    assert_eq!(sc_defects.len(), 2);
    assert_eq!(sc_defects.count_of(label), 1);
    assert_eq!(sc_defects.count_of("blurb override for a node uid not in the tree"), 1);

    // No CST node has a SuttaCentral blurb, so an empty override there never
    // suppresses anything.
    let mut cst_defects = DefectReport::default();
    let cst_rows = flatten(&[leaf("cst:tika")], "cst", &mut cst_defects);
    validate_node_text_keys(
        &cst_rows,
        "cst",
        &none,
        &overrides,
        &HashSet::new(),
        &mut cst_defects,
    );
    assert_eq!(cst_defects.len(), 1);
    assert_eq!(cst_defects.count_of(label), 1);
}
