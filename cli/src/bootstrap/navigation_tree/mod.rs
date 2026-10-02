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

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use diesel::prelude::*;
use simsapa_backend::db::appdata_schema::navigation_nodes;
use simsapa_backend::logger;
use simsapa_backend::helpers::consistent_niggahita;
use simsapa_backend::navigation_tree::{
    build_search_text, join_path, join_sort_path, NewNavigationNode,
};

/// Rows per `INSERT`. SQLite's default bind-parameter ceiling is 32766 and the
/// table has 18 insertable columns, so 500 rows (9000 parameters) stays well
/// inside it on every SQLite the project builds against.
const INSERT_CHUNK: usize = 500;

/// The project's own per-node notes: `"<node_uid>" = """<note>"""`.
///
/// Embedded, so the build does not depend on the directory it is run from. A
/// rebuild of the CLI is what picks up an edit.
const CONTEXT_NOTES_TOML: &str =
    include_str!("../../../../assets/navigation-context-notes.toml");

/// Every CST node uid starts with this, and no SC one does — which is what
/// lets a note be assigned to the spine that owns it.
const CST_NODE_UID_PREFIX: &str = "cst:";

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
    /// Set only where `sutta_uid` is a text the node does not own: the uid of
    /// that text, for the row to show. See
    /// [`sc_spine::ScSpineBuilder::parallel_redirect`].
    pub redirect_uid: Option<String>,
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
            redirect_uid: None,
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

/// Where a spine's roots start counting at the top level.
///
/// The two spines are built and written **independently** — each one can be
/// rebuilt on its own, and each numbers its own roots from zero — so without an
/// offset the root level interleaves them: `sutta(0) atthakatha(0) vinaya(1)
/// tika(1) abhidhamma(2) anna(2)`, with the tie broken only by whichever spine
/// happened to be inserted last. FR-6 wants the two groups whole and in order:
///
/// ```text
/// Sutta / Vinaya / Abhidhamma          (sc)
/// Aṭṭhakathā / Ṭīkā / Añña             (cst)
/// ```
///
/// The gap is wide enough that a spine can grow a root without colliding.
/// Nothing below the root level needs this: every other node's siblings all
/// come from one spine.
fn root_sort_base(tree_source: &str) -> i32 {
    match tree_source {
        sc_spine::TREE_SOURCE => 0,
        cst_spine::TREE_SOURCE => 100,
        _ => 1000,
    }
}

/// Flatten the built subtrees into insertable rows.
///
/// This is the one pass that assigns the derived columns, so no builder has to
/// keep them consistent by hand:
///
/// - `sort_index` — the child's position among its siblings, i.e. source order,
///   offset at the root level by [`root_sort_base`] so the two spines do not
///   interleave.
/// - `path` — the ancestors' `node_uid`s, which is what makes the whole-tree
///   filter's breadcrumbs one indexed query.
/// - `sort_path` — the same walk in `sort_index`es, which is what orders the
///   filter's flat result list in the tree's own document order.
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
    let mut ancestor_indexes: Vec<i32> = Vec::new();

    let base = root_sort_base(tree_source);
    for (index, root) in roots.iter().enumerate() {
        flatten_node(
            root,
            None,
            base + index as i32,
            tree_source,
            &mut ancestors,
            &mut ancestor_indexes,
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
    ancestor_indexes: &mut Vec<i32>,
    seen: &mut HashSet<String>,
    rows: &mut Vec<NewNavigationNode>,
    defects: &mut DefectReport,
) {
    if !seen.insert(node.node_uid.clone()) {
        defects.record("duplicate node uid (subtree skipped)", &node.node_uid);
        return;
    }

    // The niggahita is folded here, once, for both spines. The sources write it
    // three ways -- `ṁ` (CST, SuttaCentral MS), `ṃ` (PTS, DPD) and `ŋ` -- and a
    // title reaches this point from ArangoDB's `names`, from an imported
    // `suttas` row, or from a CST toc's `text=` attribute, none of which agree.
    // Folding at the one place every row passes through is what keeps the
    // displayed title and its `search_text` from drifting apart, and stops two
    // spellings of one nikāya name reading as two different places.
    //
    // Blurbs are left alone: they are prose, shown as the source wrote them.
    let title_pali = node.title_pali.as_deref().map(fold_title);
    let title_en = node.title_en.as_deref().map(fold_title);

    rows.push(NewNavigationNode {
        node_uid: node.node_uid.clone(),
        parent_uid: parent_uid.map(|s| s.to_string()),
        sort_index,
        node_type: node.node_type.clone(),
        tree_source: tree_source.to_string(),
        title_pali: title_pali.clone(),
        title_en: title_en.clone(),
        blurb: node.blurb.clone(),
        // Filled by `apply_context_notes`, once the spine is flat.
        context_note: None,
        sutta_uid: node.sutta_uid.clone(),
        redirect_uid: node.redirect_uid.clone(),
        cst_file: node.cst_file.clone(),
        has_children: !node.children.is_empty(),
        child_count: node.children.len() as i32,
        is_available: node.is_available,
        search_text: build_search_text(
            title_pali.as_deref(),
            title_en.as_deref(),
            &node.node_uid,
            node.sutta_uid.as_deref(),
        ),
        path: join_path(ancestors),
        sort_path: join_sort_path(ancestor_indexes, sort_index),
    });

    ancestors.push(node.node_uid.clone());
    ancestor_indexes.push(sort_index);
    for (index, child) in node.children.iter().enumerate() {
        flatten_node(
            child,
            Some(&node.node_uid),
            index as i32,
            tree_source,
            ancestors,
            ancestor_indexes,
            seen,
            rows,
            defects,
        );
    }
    ancestor_indexes.pop();
    ancestors.pop();
}

/// Parse the notes file. A key with an empty note is dropped: it names a node
/// and says nothing about it.
///
/// A file that does not parse fails the build. It is the project's own file,
/// so there is no source author to report to and nothing to salvage — and a
/// silently ignored file would ship a tree with every note missing. That
/// includes an **unquoted key**: TOML reads `sn56.11 = "…"` as a nested table,
/// which is not a string and is rejected here.
pub fn parse_context_notes(toml_text: &str) -> Result<BTreeMap<String, String>> {
    let notes: BTreeMap<String, String> = toml::from_str(toml_text).context(
        "assets/navigation-context-notes.toml must be a flat table of \"node_uid\" = \"note text\" (quote every key)",
    )?;
    Ok(notes
        .into_iter()
        .map(|(uid, note)| (uid, unwrap_note(&note)))
        .filter(|(_, note)| !note.is_empty())
        .collect())
}

/// Undo the hand-wrapping of a note: the file is written to be edited, so a
/// note is wrapped across lines there, and those line breaks are not part of
/// it. Lines are joined with a space; a blank line is a paragraph break.
fn unwrap_note(note: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in note.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join(" "));
                current.clear();
            }
        } else {
            current.extend(words);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    paragraphs.join("\n\n")
}

/// Set `context_note` on the rows the notes file names.
///
/// The two spines are built and written independently, so each call sees only
/// its own rows. A note is therefore checked against the spine that owns its
/// key ([`CST_NODE_UID_PREFIX`]): one naming a node that spine does not have is
/// recorded as a defect, and one belonging to the other spine is not this
/// call's to judge.
pub fn apply_context_notes(
    rows: &mut [NewNavigationNode],
    tree_source: &str,
    notes: &BTreeMap<String, String>,
    defects: &mut DefectReport,
) -> usize {
    let is_cst_spine = tree_source == cst_spine::TREE_SOURCE;
    let mut applied: HashSet<&str> = HashSet::new();

    for row in rows.iter_mut() {
        if let Some((uid, note)) = notes.get_key_value(&row.node_uid) {
            row.context_note = Some(note.clone());
            applied.insert(uid.as_str());
        }
    }

    for uid in notes.keys() {
        let owned = uid.starts_with(CST_NODE_UID_PREFIX) == is_cst_spine;
        if owned && !applied.contains(uid.as_str()) {
            defects.record("context note for a node uid not in the tree", uid);
        }
    }

    applied.len()
}

/// Trim a title and fold its niggahita to `ṁ`. See the call site in
/// [`flatten_node`] for why this is done there and only there.
fn fold_title(s: &str) -> String {
    consistent_niggahita(Some(s.trim().to_string()))
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
    logger::info(&format!(
        "sc_spine: {} leaf/leaves opened through a parallel Pāli text, {} ranged leaf/leaves expanded into their rules",
        builder.redirect_count(),
        builder.range_expansion_count()
    ));

    let mut rows = flatten(&roots, sc_spine::TREE_SOURCE, &mut builder.defects);
    let notes = parse_context_notes(CONTEXT_NOTES_TOML)?;
    let noted = apply_context_notes(&mut rows, sc_spine::TREE_SOURCE, &notes, &mut builder.defects);
    logger::info(&format!("sc_spine: {} context note(s) applied", noted));
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

    let mut rows = flatten(&roots, cst_spine::TREE_SOURCE, &mut builder.defects);
    let notes = parse_context_notes(CONTEXT_NOTES_TOML)?;
    let noted = apply_context_notes(&mut rows, cst_spine::TREE_SOURCE, &notes, &mut builder.defects);
    logger::info(&format!("cst_spine: {} context note(s) applied", noted));
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
