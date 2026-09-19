//! Read layer for the sutta navigation tree (`navigation_nodes`).
//!
//! The table is written by the CLI bootstrap from two independent spines — the
//! SuttaCentral structure in the `sc-data` checkout and the tipitaka.org CST
//! toc files — and is read here one level at a time. Nothing in this module
//! ever loads the whole tree: the level query is
//! `WHERE parent_uid = ? ORDER BY sort_index`, and the whole-tree filter is two
//! statements over the materialized `path` column.
//!
//! **Every read tolerates the table being absent or empty.** A migration
//! failure at startup is non-fatal by design (see docs/database-migrations.md),
//! so `no such table: navigation_nodes` is a reachable state, not a bug. The
//! reads answer it with an empty result and `tree_is_available() == false`, and
//! the window says "navigation index not available" rather than drawing an
//! empty tree that looks like a corpus with nothing in it.
//!
//! See docs/sutta-navigation-tree.md.

use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::db::appdata_schema::navigation_nodes;
use crate::logger::error;

/// Separates ancestor `node_uid`s in the materialized `path` column. A path is
/// delimited at both ends (`|sutta|sutta/linked|sn|`) so that a prefix match is
/// unambiguous, and is the empty string for a root. No `node_uid` contains it.
pub const PATH_DELIMITER: char = '|';

/// Build a `path` value from the ancestor `node_uid`s, root first, excluding
/// the node itself.
pub fn join_path(ancestors: &[String]) -> String {
    if ancestors.is_empty() {
        return String::new();
    }
    format!(
        "{d}{}{d}",
        ancestors.join(&PATH_DELIMITER.to_string()),
        d = PATH_DELIMITER
    )
}

/// The ancestor `node_uid`s of a `path` value, root first.
pub fn split_path(path: &str) -> Vec<&str> {
    path.split(PATH_DELIMITER).filter(|s| !s.is_empty()).collect()
}

#[derive(Debug, Clone, PartialEq, Queryable, Selectable, Serialize, Deserialize)]
#[diesel(table_name = navigation_nodes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct NavigationNode {
    pub id: i32,
    pub node_uid: String,
    pub parent_uid: Option<String>,
    pub sort_index: i32,
    pub node_type: String,
    pub tree_source: String,
    pub title_pali: Option<String>,
    pub title_en: Option<String>,
    pub blurb: Option<String>,
    pub sutta_uid: Option<String>,
    pub cst_file: Option<String>,
    pub has_children: bool,
    pub child_count: i32,
    pub is_available: bool,
    pub path: String,
}

impl NavigationNode {
    /// The title a breadcrumb segment or a compact row shows when it has room
    /// for one line only. Falls back through the English title to the uid
    /// rather than rendering an empty segment.
    pub fn display_title(&self) -> &str {
        self.title_pali
            .as_deref()
            .filter(|s| !s.is_empty())
            .or_else(|| self.title_en.as_deref().filter(|s| !s.is_empty()))
            .unwrap_or(&self.node_uid)
    }
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = navigation_nodes)]
pub struct NewNavigationNode {
    pub node_uid: String,
    pub parent_uid: Option<String>,
    pub sort_index: i32,
    pub node_type: String,
    pub tree_source: String,
    pub title_pali: Option<String>,
    pub title_en: Option<String>,
    pub blurb: Option<String>,
    pub sutta_uid: Option<String>,
    pub cst_file: Option<String>,
    pub has_children: bool,
    pub child_count: i32,
    pub is_available: bool,
    pub path: String,
}

/// A whole-tree filter hit: the node, plus the titles of its ancestors so the
/// flat result list can show where the node sits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationMatch {
    pub node: NavigationNode,
    /// Root first, one entry per ancestor. An ancestor that is missing from
    /// the table (a defect) is carried as its `node_uid` rather than dropped,
    /// so a broken path is visible instead of silently shortened.
    pub breadcrumbs: Vec<String>,
}

// ============================================================================
// Connection-level queries
//
// These take a connection so they can be unit-tested against a temp database.
// The public wrappers below add the `AppData` lookup and the tolerance rules.
// ============================================================================

pub fn children_of_conn(
    db_conn: &mut SqliteConnection,
    parent: Option<&str>,
) -> QueryResult<Vec<NavigationNode>> {
    let query = navigation_nodes::table
        .select(NavigationNode::as_select())
        .order(navigation_nodes::sort_index.asc());

    match parent {
        Some(uid) => query
            .filter(navigation_nodes::parent_uid.eq(uid))
            .load::<NavigationNode>(db_conn),
        None => query
            .filter(navigation_nodes::parent_uid.is_null())
            .load::<NavigationNode>(db_conn),
    }
}

pub fn node_by_uid_conn(
    db_conn: &mut SqliteConnection,
    node_uid: &str,
) -> QueryResult<Option<NavigationNode>> {
    navigation_nodes::table
        .filter(navigation_nodes::node_uid.eq(node_uid))
        .select(NavigationNode::as_select())
        .first::<NavigationNode>(db_conn)
        .optional()
}

/// `%` and `_` are stripped rather than escaped. They are not search characters
/// a reader would type into a title filter, and diesel's `.like()` emits no
/// `ESCAPE` clause, so escaping them would only make a backslash match
/// literally.
///
/// `None` when nothing is left to match on. The emptiness test runs **after**
/// the strip, not before it: a query of `"%"` is non-empty as typed but empty
/// once cleaned, and letting it through would turn the pattern into `%%` and
/// return the whole tree as a flat list.
fn like_pattern(query: &str) -> Option<String> {
    let cleaned: String = query.chars().filter(|c| *c != '%' && *c != '_').collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    Some(format!("%{}%", cleaned))
}

/// Filter the **whole** tree on the Pāli title, the English title, the node uid
/// and the sutta uid, returning each match with its breadcrumb titles.
///
/// Two statements by design: the matches with their `path` strings, then one
/// `WHERE node_uid IN (…)` over the union of every match's ancestors. Resolving
/// ancestor *titles* from a delimiter-joined path inside a single statement
/// would need a recursive CTE to split the string, which buys nothing here.
pub fn filter_nodes_conn(
    db_conn: &mut SqliteConnection,
    query: &str,
    limit: i64,
) -> QueryResult<Vec<NavigationMatch>> {
    let pattern = match like_pattern(query) {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };

    let matches: Vec<NavigationNode> = navigation_nodes::table
        .filter(
            navigation_nodes::title_pali
                .like(&pattern)
                .or(navigation_nodes::title_en.like(&pattern))
                .or(navigation_nodes::node_uid.like(&pattern))
                .or(navigation_nodes::sutta_uid.like(&pattern)),
        )
        .select(NavigationNode::as_select())
        .order((
            navigation_nodes::path.asc(),
            navigation_nodes::sort_index.asc(),
        ))
        .limit(limit)
        .load::<NavigationNode>(db_conn)?;

    if matches.is_empty() {
        return Ok(Vec::new());
    }

    let mut ancestor_uids: Vec<String> = matches
        .iter()
        .flat_map(|n| split_path(&n.path))
        .map(|s| s.to_string())
        .collect();
    ancestor_uids.sort();
    ancestor_uids.dedup();

    let titles: Vec<(String, Option<String>, Option<String>)> = if ancestor_uids.is_empty() {
        Vec::new()
    } else {
        navigation_nodes::table
            .filter(navigation_nodes::node_uid.eq_any(&ancestor_uids))
            .select((
                navigation_nodes::node_uid,
                navigation_nodes::title_pali,
                navigation_nodes::title_en,
            ))
            .load::<(String, Option<String>, Option<String>)>(db_conn)?
    };

    let title_of = |uid: &str| -> String {
        titles
            .iter()
            .find(|(n, _, _)| n == uid)
            .and_then(|(_, pali, en)| {
                pali.as_deref()
                    .filter(|s| !s.is_empty())
                    .or_else(|| en.as_deref().filter(|s| !s.is_empty()))
            })
            .unwrap_or(uid)
            .to_string()
    };

    Ok(matches
        .into_iter()
        .map(|node| {
            let breadcrumbs = split_path(&node.path).iter().map(|u| title_of(u)).collect();
            NavigationMatch { node, breadcrumbs }
        })
        .collect())
}

pub fn tree_is_available_conn(db_conn: &mut SqliteConnection) -> QueryResult<bool> {
    let count: i64 = navigation_nodes::table.count().get_result(db_conn)?;
    Ok(count > 0)
}

// ============================================================================
// Public API
//
// A missing table, a missing `AppData` (before `init_app_data()`, and in unit
// tests) and a failed query all answer the same way: empty, and logged.
// ============================================================================

fn with_appdata<T, F>(what: &str, empty: T, op: F) -> T
where
    F: FnOnce(&mut SqliteConnection) -> QueryResult<T>,
{
    let app_data = match crate::try_get_app_data() {
        Some(a) => a,
        None => return empty,
    };

    match app_data.dbm.appdata.do_read(op) {
        Ok(v) => v,
        Err(e) => {
            // Includes "no such table: navigation_nodes", which is reachable:
            // a migration failure at startup is non-fatal by design.
            error(&format!("navigation_tree::{}: {}", what, e));
            empty
        }
    }
}

/// One level of the tree. `None` is the root level.
pub fn children_of(parent: Option<&str>) -> Vec<NavigationNode> {
    with_appdata("children_of", Vec::new(), |db_conn| {
        children_of_conn(db_conn, parent)
    })
}

/// Whole-tree filter. See `filter_nodes_conn`.
pub fn filter_nodes(query: &str, limit: i64) -> Vec<NavigationMatch> {
    with_appdata("filter_nodes", Vec::new(), |db_conn| {
        filter_nodes_conn(db_conn, query, limit)
    })
}

pub fn node_by_uid(node_uid: &str) -> Option<NavigationNode> {
    with_appdata("node_by_uid", None, |db_conn| {
        node_by_uid_conn(db_conn, node_uid)
    })
}

/// False when the table is missing, empty, or unreadable — the three cases the
/// window must report as "navigation index not available".
pub fn tree_is_available() -> bool {
    with_appdata("tree_is_available", false, tree_is_available_conn)
}

#[cfg(test)]
mod tests;
