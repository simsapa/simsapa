//! Builds the sutta navigation tree (`navigation_nodes`) at bootstrap time.
//!
//! Two independent spines land in one table, distinguished by `tree_source`:
//!
//! - `sc` — the SuttaCentral structure, from the local `sc-data` checkout
//!   ([`sc_spine`]).
//! - `cst` — the Chaṭṭha Saṅgāyana structure, from the tipitaka.org toc files.
//!
//! **The two are not reconciled.** Each follows its own source's grouping, and
//! the same text sitting at a different depth in the Mūla root than in the
//! Aṭṭhakathā root is the correct outcome. See docs/sutta-navigation-tree.md.
//!
//! This module holds what both spines share: the in-memory [`TreeNode`], the
//! flattening pass that assigns `sort_index`, `path`, `has_children` and
//! `child_count`, the [`DefectReport`], and the database writer.
//!
//! ## Defects are reported, never repaired
//!
//! A `src=` pointing at a missing file, a duplicate uid, a leaf whose target
//! text is not in the database: each is recorded with its offending identifier
//! and logged. The builder never invents a title and never guesses a mapping.
//! An unresolvable leaf is kept and marked unavailable rather than dropped, so
//! the gap is visible in the window instead of silently absent.

pub mod coverage_report;
pub mod cst_spine;
pub mod sc_spine;

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use diesel::prelude::*;
use simsapa_backend::db::appdata_schema::navigation_nodes;
use simsapa_backend::logger;
use simsapa_backend::navigation_tree::{join_path, NewNavigationNode};

/// Rows per `INSERT`. SQLite's default bind-parameter ceiling is 32766 and the
/// table has 15 insertable columns, so 500 rows (7500 parameters) stays well
/// inside it on every SQLite the project builds against.
const INSERT_CHUNK: usize = 500;

/// A node as the spine builders construct it, before it is flattened into rows.
///
/// `sort_index` is not a field: it is the child's position in its parent's
/// `children`, which is the source file's own order. A builder that needed to
/// set it explicitly would be re-ordering, and the UI never re-sorts.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub node_uid: String,
    pub node_type: String,
    pub title_pali: Option<String>,
    pub title_en: Option<String>,
    pub blurb: Option<String>,
    pub sutta_uid: Option<String>,
    pub cst_file: Option<String>,
    pub is_available: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(node_uid: impl Into<String>, node_type: impl Into<String>) -> Self {
        Self {
            node_uid: node_uid.into(),
            node_type: node_type.into(),
            title_pali: None,
            title_en: None,
            blurb: None,
            sutta_uid: None,
            cst_file: None,
            is_available: true,
            children: Vec::new(),
        }
    }
}

/// Source-data defects found while building, grouped by kind so a regression
/// shows up as a number rather than as a line to be spotted in a long log.
#[derive(Debug, Default)]
pub struct DefectReport {
    entries: Vec<(String, String)>,
}

impl DefectReport {
    /// `kind` groups the defect for the summary; `identifier` names the
    /// offending uid, file or leaf, and is what makes the entry actionable.
    pub fn record(&mut self, kind: &str, identifier: impl Into<String>) {
        self.entries.push((kind.to_string(), identifier.into()));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn count_of(&self, kind: &str) -> usize {
        self.entries.iter().filter(|(k, _)| k == kind).count()
    }

    /// Log a count per kind and a bounded sample of the identifiers. The
    /// counts are the regression signal; the sample is what makes one
    /// investigable without re-running the build.
    pub fn log_summary(&self, context: &str, sample_per_kind: usize) {
        if self.entries.is_empty() {
            logger::info(&format!("{}: no source-data defects", context));
            return;
        }

        let mut kinds: Vec<&str> = self.entries.iter().map(|(k, _)| k.as_str()).collect();
        kinds.sort();
        kinds.dedup();

        logger::warn(&format!(
            "{}: {} source-data defect(s) in {} kind(s)",
            context,
            self.entries.len(),
            kinds.len()
        ));

        for kind in kinds {
            let ids: Vec<&str> = self
                .entries
                .iter()
                .filter(|(k, _)| k == kind)
                .map(|(_, id)| id.as_str())
                .collect();
            let shown: Vec<&str> = ids.iter().take(sample_per_kind).copied().collect();
            let more = ids.len().saturating_sub(shown.len());
            logger::warn(&format!(
                "{}: {}: {}{}",
                context,
                kind,
                ids.len(),
                if more > 0 {
                    format!(" -- e.g. {} (+{} more)", shown.join(", "), more)
                } else {
                    format!(" -- {}", shown.join(", "))
                }
            ));
        }
    }
}

/// Flatten the built subtrees into insertable rows.
///
/// This is the one pass that assigns the derived columns, so no builder has to
/// keep them consistent by hand:
///
/// - `sort_index` — the child's position among its siblings, i.e. source order.
/// - `path` — the ancestors' `node_uid`s, which is what makes the whole-tree
///   filter and its breadcrumbs one indexed query.
/// - `has_children` / `child_count` — from `children.len()`.
///
/// A `node_uid` that repeats is a source-data defect: the first occurrence
/// wins, the second is recorded and **its whole subtree is skipped**, because
/// the column is unique and inserting it would abort the transaction.
pub fn flatten(
    roots: &[TreeNode],
    tree_source: &str,
    defects: &mut DefectReport,
) -> Vec<NewNavigationNode> {
    let mut rows = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut ancestors: Vec<String> = Vec::new();

    for (index, root) in roots.iter().enumerate() {
        flatten_node(
            root,
            None,
            index as i32,
            tree_source,
            &mut ancestors,
            &mut seen,
            &mut rows,
            defects,
        );
    }

    rows
}

#[allow(clippy::too_many_arguments)]
fn flatten_node(
    node: &TreeNode,
    parent_uid: Option<&str>,
    sort_index: i32,
    tree_source: &str,
    ancestors: &mut Vec<String>,
    seen: &mut HashSet<String>,
    rows: &mut Vec<NewNavigationNode>,
    defects: &mut DefectReport,
) {
    if !seen.insert(node.node_uid.clone()) {
        defects.record("duplicate node uid (subtree skipped)", &node.node_uid);
        return;
    }

    rows.push(NewNavigationNode {
        node_uid: node.node_uid.clone(),
        parent_uid: parent_uid.map(|s| s.to_string()),
        sort_index,
        node_type: node.node_type.clone(),
        tree_source: tree_source.to_string(),
        title_pali: node.title_pali.clone(),
        title_en: node.title_en.clone(),
        blurb: node.blurb.clone(),
        sutta_uid: node.sutta_uid.clone(),
        cst_file: node.cst_file.clone(),
        has_children: !node.children.is_empty(),
        child_count: node.children.len() as i32,
        is_available: node.is_available,
        path: join_path(ancestors),
    });

    ancestors.push(node.node_uid.clone());
    for (index, child) in node.children.iter().enumerate() {
        flatten_node(
            child,
            Some(&node.node_uid),
            index as i32,
            tree_source,
            ancestors,
            seen,
            rows,
            defects,
        );
    }
    ancestors.pop();
}

/// Replace one spine's rows, in a single transaction.
///
/// Scoped by `tree_source` so the two spines can be rebuilt independently, and
/// so re-running a build is idempotent rather than a uid-collision failure.
pub fn replace_spine(
    db_conn: &mut SqliteConnection,
    tree_source: &str,
    rows: &[NewNavigationNode],
) -> Result<usize> {
    db_conn
        .transaction::<usize, diesel::result::Error, _>(|conn| {
            diesel::delete(
                navigation_nodes::table.filter(navigation_nodes::tree_source.eq(tree_source)),
            )
            .execute(conn)?;

            let mut written = 0;
            for chunk in rows.chunks(INSERT_CHUNK) {
                written += diesel::insert_into(navigation_nodes::table)
                    .values(chunk)
                    .execute(conn)?;
            }
            Ok(written)
        })
        .with_context(|| format!("Failed to write the '{}' navigation spine", tree_source))
}

/// Build the SuttaCentral spine and write it, replacing whatever `sc` rows are
/// already there.
///
/// Must run **after** the sutta import, so that every leaf's `sutta_uid` is
/// resolved against rows that exist and an unresolvable one can be reported
/// (it is kept and marked unavailable, never dropped).
pub fn build_sc_spine(db_conn: &mut SqliteConnection, sc_data_dir: &Path) -> Result<()> {
    if !sc_data_dir.exists() {
        logger::warn(&format!(
            "build_sc_spine: sc-data not found, skipping: {}",
            sc_data_dir.display()
        ));
        return Ok(());
    }

    let arango_db = crate::bootstrap::suttacentral::connect_to_arangodb()
        .context("The SC navigation spine needs ArangoDB for the 'names' titles")?;

    let mut builder = sc_spine::ScSpineBuilder::new(sc_data_dir, &arango_db, db_conn)?;
    let roots = builder.build()?;
    log_node_counts("sc_spine", &roots);

    let rows = flatten(&roots, sc_spine::TREE_SOURCE, &mut builder.defects);
    let written = replace_spine(db_conn, sc_spine::TREE_SOURCE, &rows)?;
    logger::info(&format!("sc_spine: wrote {} rows", written));

    builder.defects.log_summary("sc_spine", 8);
    analyze_appdata(db_conn);
    Ok(())
}

/// Build the CST spine and write it, replacing whatever `cst` rows are there.
///
/// `site_dir` is `tipitaka-xml/tipitaka.org/romn/` (the toc files);
/// `source_dir` is `tipitaka-xml/romn/` (the 217 source XML files). They are
/// two different directories with the same name, and passing one for the other
/// yields an empty spine.
///
/// Like the SC spine, this must run **after** the CST import: a toc leaf whose
/// division record does not exist is kept and marked unavailable, and that is
/// only an honest report if the import has already run.
pub fn build_cst_spine(
    db_conn: &mut SqliteConnection,
    site_dir: &Path,
    source_dir: &Path,
    fragments_db_path: &Path,
    report: Option<&mut coverage_report::CstCoverageReport>,
) -> Result<()> {
    if !site_dir.exists() {
        logger::warn(&format!(
            "build_cst_spine: tipitaka.org toc files not found, skipping: {}",
            site_dir.display()
        ));
        return Ok(());
    }

    let mut builder =
        cst_spine::CstSpineBuilder::new(site_dir, source_dir, fragments_db_path, db_conn)?;
    let roots = builder.build()?;
    log_node_counts("cst_spine", &roots);

    let rows = flatten(&roots, cst_spine::TREE_SOURCE, &mut builder.defects);
    let written = replace_spine(db_conn, cst_spine::TREE_SOURCE, &rows)?;
    logger::info(&format!("cst_spine: wrote {} rows", written));

    builder.defects.log_summary("cst_spine", 8);

    // Only the spine knows which toc leaves ended up opening something, so it
    // fills that half of the coverage report.
    if let Some(report) = report {
        report.toc_leaves_total = builder.toc_leaves_total;
        report.toc_leaves_unassigned = builder.unassigned_leaves;
        report.toc_leaves_merged = builder.merged_leaves;
    }

    analyze_appdata(db_conn);
    Ok(())
}

/// Refresh `sqlite_stat1` after a spine has been written.
///
/// Every path that grows a shipped database at runtime owes it an `ANALYZE` --
/// see docs/user-data-and-sqlite-analyze.md. Without one the planner has no
/// statistics for the new table and can pick a scan over `idx_navigation_nodes_parent`,
/// which is the whole point of querying a level at a time.
///
/// A failure is logged and not propagated: stale statistics make a query slow,
/// not wrong, and are no reason to fail a bootstrap that has already written
/// the rows.
fn analyze_appdata(db_conn: &mut SqliteConnection) {
    match diesel::sql_query("ANALYZE").execute(db_conn) {
        Ok(_) => logger::info("navigation_tree: ANALYZE done"),
        Err(e) => logger::warn(&format!("navigation_tree: ANALYZE failed: {}", e)),
    }
}

/// Log the node count per root, which is the figure a build is checked against.
pub fn log_node_counts(context: &str, roots: &[TreeNode]) {
    fn count(node: &TreeNode) -> usize {
        1 + node.children.iter().map(count).sum::<usize>()
    }
    let total: usize = roots.iter().map(count).sum();
    logger::info(&format!("{}: {} nodes in {} root(s)", context, total, roots.len()));
    for root in roots {
        logger::info(&format!(
            "{}:   {} ({}): {} nodes, {} direct children",
            context,
            root.node_uid,
            root.title_pali.as_deref().unwrap_or("-"),
            count(root),
            root.children.len()
        ));
    }
}

#[cfg(test)]
mod tests;
