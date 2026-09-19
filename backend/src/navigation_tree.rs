//! Read layer for the sutta navigation tree (`navigation_nodes`).
//!
//! The table is written by the CLI bootstrap from two independent spines — the
//! SuttaCentral structure in the `sc-data` checkout and the tipitaka.org CST
//! toc files — and is read here one level at a time. Nothing in this module
//! ever loads the whole tree: the level query is
//! `WHERE parent_uid = ? ORDER BY sort_index`, and the whole-tree filter is two
//! statements over the materialized `search_text`, `sort_path` and `path`
//! columns.
//!
//! **Three columns are materialized at build time, and changing how any of them
//! is derived means rebuilding the tree.** `search_text` (what the filter
//! matches), `sort_path` (what orders the results) and `path` (what
//! reconstructs the breadcrumbs) are all written by the CLI, and a stale one
//! fails *silently* -- an edit to `normalize_for_search` alone would leave the
//! query folded one way and the column folded another, so matches just stop
//! appearing with nothing logged. Re-run `simsapa_cli build-navigation-tree`
//! after touching any of them, and see docs/sutta-navigation-tree.md.
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
use crate::helpers::{consistent_niggahita, pali_to_ascii};
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

/// Width each `sort_index` is zero-padded to in a `sort_path`.
///
/// Fixed width is the whole trick: it makes a lexicographic compare equal a
/// numeric one, so no collation or splitting is needed to order the tree. Six
/// digits is far past the largest level in the corpus (728 children under
/// `patthana`) and past the root offsets.
const SORT_PATH_WIDTH: usize = 6;

/// Build a `sort_path`: every ancestor's `sort_index`, root first, then the
/// node's own. Delimited at both ends like [`join_path`].
///
/// Ordering by this yields the tree's own document order, which is what the
/// filter's flat result list is sorted by — so a match appears where the user
/// would have found it by browsing. A parent's key is a prefix of its
/// children's, so a parent always precedes its subtree.
pub fn join_sort_path(ancestor_indexes: &[i32], own_index: i32) -> String {
    let mut out = String::with_capacity((ancestor_indexes.len() + 1) * (SORT_PATH_WIDTH + 1) + 1);
    for index in ancestor_indexes.iter().chain(std::iter::once(&own_index)) {
        out.push(PATH_DELIMITER);
        // A negative index would break the width and so the ordering. Nothing
        // produces one -- `flatten` assigns from `enumerate()` -- and clamping
        // keeps a future mistake from silently reordering the tree.
        out.push_str(&format!("{:0width$}", (*index).max(0), width = SORT_PATH_WIDTH));
    }
    out.push(PATH_DELIMITER);
    out
}

/// The ancestor `node_uid`s of a `path` value, root first.
pub fn split_path(path: &str) -> Vec<&str> {
    path.split(PATH_DELIMITER).filter(|s| !s.is_empty()).collect()
}

/// Fold a title, a uid or a filter query to the one form the whole-tree filter
/// compares in.
///
/// Four steps, in this order:
///
/// 1. [`consistent_niggahita`] — ṃ and ŋ become ṁ, so the three niggahita
///    conventions in the corpus collapse to one. The spines apply this to the
///    titles they *store* as well, so what the row displays and what the filter
///    matches cannot drift apart.
/// 2. [`pali_to_ascii`] — every diacritic folds to its ASCII letter. This is
///    what lets a reader on a phone keyboard type `samyutta` and reach
///    `Saṁyuttaṁ`, or `saccasamyutta` and reach `Saccasaṁyuttaṁ`.
/// 3. [`fold_typography`] then [`strip_remaining_marks`] — the curly quotes,
///    dashes and ellipses the corpus writes, and anything accented the Pāli
///    table does not list. Without these, `lion's roar` typed on a keyboard
///    matches none of the 203 titles that spell it `lion’s`.
/// 4. lowercase — SQLite's `LIKE` folds case for ASCII only, so `Ā` would never
///    match `ā`. By this point there is nothing but ASCII left, but lowercasing
///    here rather than relying on `LIKE` keeps the column's contract explicit.
///
/// **The same function must run over the stored column and over the query**, or
/// the fold is one-sided and the filter silently misses.
pub fn normalize_for_search(text: &str) -> String {
    let folded = pali_to_ascii(Some(&consistent_niggahita(Some(text.to_string()))));
    strip_remaining_marks(&fold_typography(&folded)).to_lowercase()
}

/// Map the typographic characters the corpus actually contains onto what a
/// keyboard produces.
///
/// Measured over the built tree, these six are what survive `pali_to_ascii`:
/// `’` (203 titles — "the lion’s roar"), `…`, `–`, `“`, `”` and `é`. A reader
/// types a straight apostrophe, a hyphen and three dots, so without this fold
/// searching `lion's roar` finds nothing at all. The project already treats
/// curly-vs-straight quotes as a real defect elsewhere — see the iti-sandhi
/// quote-variant handling in the search normalization path.
fn fold_typography(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' => out.push('\''),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => out.push('"'),
            '\u{2010}'..='\u{2015}' | '\u{2212}' => out.push('-'),
            '\u{00A0}' | '\u{2009}' | '\u{202F}' => out.push(' '),
            '\u{2026}' => out.push_str("..."),
            _ => out.push(c),
        }
    }
    out
}

/// Drop any combining mark left after the explicit folds.
///
/// This is the catch-all, not the main event: `pali_to_ascii` handles the Pāli
/// letters by table, and a table can only fold what someone listed. It misses
/// `é` (which is in two English titles today), it misses **uppercase `Ŋ`**
/// (`consistent_niggahita` replaces only the lowercase one and the table has no
/// `Ŋ` either), and it misses any Pāli letter written *decomposed* — `a` plus a
/// combining macron — which is a different byte sequence that looks identical.
/// Decomposing and dropping the marks catches all three, and costs nothing on
/// text that is already ASCII.
fn strip_remaining_marks(text: &str) -> String {
    use unicode_normalization::char::is_combining_mark;
    use unicode_normalization::UnicodeNormalization;

    if text.is_ascii() {
        return text.to_string();
    }
    text.nfd().filter(|c| !is_combining_mark(*c)).nfc().collect()
}

/// The `search_text` value for a node: everything the filter matches on
/// (FR-27), folded and joined with a space. Empty fields contribute nothing.
pub fn build_search_text(
    title_pali: Option<&str>,
    title_en: Option<&str>,
    node_uid: &str,
    sutta_uid: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(4);
    for field in [title_pali, title_en, Some(node_uid), sutta_uid] {
        if let Some(value) = field {
            let folded = normalize_for_search(value);
            if !folded.trim().is_empty() {
                parts.push(folded);
            }
        }
    }
    parts.join(" ")
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
    pub search_text: String,
    pub path: String,
    pub sort_path: String,
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
    pub search_text: String,
    pub path: String,
    pub sort_path: String,
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
    // Folded exactly as `search_text` was at build time. A one-sided fold is a
    // silent miss, not an error, which is why both go through the one function.
    let folded = normalize_for_search(query);
    let cleaned: String = folded.chars().filter(|c| *c != '%' && *c != '_').collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    Some(format!("%{}%", cleaned))
}

/// Filter the **whole** tree on the Pāli title, the English title, the node uid
/// and the sutta uid, returning each match with its breadcrumb titles.
///
/// Matching is against the folded `search_text` column, which carries all four
/// fields, so `samyutta` reaches `Saṁyuttaṁ` and `MAHAVAGGA` reaches
/// `Mahāvaggo`. Branch nodes are matched as well as leaves: the filter is how a
/// reader who already knows what they want skips the drill-down, and a level
/// name is as legitimate a thing to search for as a sutta title.
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
        .filter(navigation_nodes::search_text.like(&pattern))
        .select(NavigationNode::as_select())
        // Tree order, so a match sits where browsing would have found it --
        // and so the LIMIT keeps the first N in that order rather than the
        // first N alphabetically. See `join_sort_path`.
        .order(navigation_nodes::sort_path.asc())
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
