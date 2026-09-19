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
