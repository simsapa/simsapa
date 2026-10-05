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
use crate::helpers::{
    consistent_niggahita, pali_to_ascii, query_text_to_uid_field_query, whole_sutta_ref,
};
use crate::logger::error;
use crate::navigation_texts::{navigation_texts, NavigationTexts};
use crate::types::SearchArea;

/// Separates ancestor `node_uid`s in the materialized `path` column. A path is
/// delimited at both ends (`|sutta|sutta/linked|sn|`) so that a prefix match is
/// unambiguous, and is the empty string for a root. No `node_uid` contains it.
pub const PATH_DELIMITER: char = '|';

/// Prefix of a mapped-fragment node's `node_uid` (`cst:frag:mn11.att/pli/cst`).
///
/// A fragment row and the division row above it are siblings under the same
/// parent and both carry the same `cst_file`, so the file name alone cannot
/// tell them apart. The prefix is what FR-17's subdued "in s0201a.att.xml"
/// affordance keys on: it is written by the CST spine builder and read back by
/// [`NavigationNode::is_fragment`], so the two must not drift.
pub const FRAGMENT_UID_PREFIX: &str = "cst:frag:";

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
    pub sutta_uid: Option<String>,
    /// The uid of the text this leaf opens when it has none of its own.
    ///
    /// `None` on every ordinary node. A SuttaCentral parallels-page leaf —
    /// each Pātimokkha rule is one — carries the uid of the Vibhaṅga text it
    /// was resolved to at bootstrap, so the row can say where its text comes
    /// from. Without that the reader presses Open on a bhikkhunī rule and
    /// lands on a bhikkhu rule with nothing to explain why.
    ///
    /// It is the target's **own** uid (`pli-tv-bu-vb-pj1`), not the
    /// `suttas.uid` (`pli-tv-bu-vb-pj1/pli/ms`) that `sutta_uid` holds: the
    /// row shows it to a reader, and the edition suffix is noise there.
    pub redirect_uid: Option<String>,
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

    /// Whether tapping this node's Open button can reach a text (FR-42).
    ///
    /// Two conditions, and both are load-bearing: the node must name a sutta,
    /// and that sutta must have been found in the `suttas` table when the
    /// spine was built. A branch has no `sutta_uid` at all; a leaf whose
    /// target text is absent has one but was marked unavailable, and the
    /// window dims it rather than opening an empty tab.
    pub fn is_openable(&self) -> bool {
        self.is_available
            && self
                .sutta_uid
                .as_deref()
                .is_some_and(|uid| !uid.is_empty())
    }

    /// Whether this row opens a text that is not its own (FR-42's sibling
    /// case): a SuttaCentral parallels page resolved to the Pāli text it lists.
    pub fn is_redirected(&self) -> bool {
        self.redirect_uid
            .as_deref()
            .is_some_and(|uid| !uid.is_empty())
    }

    /// Whether this row is a mapped fragment of a partially mapped CST file
    /// rather than a division of it. See [`FRAGMENT_UID_PREFIX`].
    pub fn is_fragment(&self) -> bool {
        self.node_uid.starts_with(FRAGMENT_UID_PREFIX)
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
    pub sutta_uid: Option<String>,
    pub redirect_uid: Option<String>,
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

/// The uids a filter query names, when it is typed as a sutta reference or a
/// uid: `dn 1` → `dn1`, `SN 56.11` → `sn56.11`, `Dhp 182` → the chapter uid.
///
/// Two readings, in this order, both kept:
///
/// 1. **the reference as typed**, its spaces removed (`sn 56` → `sn56`);
/// 2. **the Sutta Search window's reading** ([`query_text_to_uid_field_query`]),
///    so a reference the search bar resolves is one the filter resolves too.
///
/// They differ exactly where the search bar converts a verse number: it reads
/// `sn 56` as Suttanipāta verse 56 (`snp1.3`, `Sn` being the PTS abbreviation),
/// while a reader browsing the tree most likely means the Saccasaṁyutta. Both
/// are listed, the literal one first.
///
/// Empty when the query is not a single reference — a reading with a space
/// left in it is prose that merely *contains* a reference.
pub fn ref_query_uids(query: &str) -> Vec<String> {
    // Folded first, so `Paṭṭhāna 1` and `Thī 1` read as the ASCII references
    // the pattern knows. The fold is the filter's own, so it cannot drift from
    // what `search_text` was folded with.
    let folded = normalize_for_search(query);
    let trimmed = folded.trim();
    let mut uids: Vec<String> = Vec::with_capacity(2);

    if let Some((collection, number)) = whole_sutta_ref(trimmed) {
        uids.push(format!("{}{}", collection, number));
    }

    // The tree holds suttas only, so the query is read as the search bar reads
    // it in the Suttas area.
    let converted = query_text_to_uid_field_query(trimmed, &SearchArea::Suttas);
    if let Some(uid) = converted.strip_prefix("uid:") {
        let uid = uid.trim().trim_end_matches('/');
        if !uid.is_empty() && !uid.contains(char::is_whitespace) {
            uids.push(uid.to_string());
        }
    }

    // `%` and `_` would be wildcards in the `LIKE` of the exact match.
    uids.retain(|u| !u.contains(['%', '_']));
    uids.dedup();
    uids
}

/// The nodes a uid names exactly: the node whose own uid it is, and the leaves
/// that open that text (`dn1` → `dn1/pli/ms`), in tree order.
///
/// A redirected leaf is not matched through its `sutta_uid`: that is its
/// parallel's text, so `pli-tv-bu-vb-pj1` would otherwise list every bhikkhunī
/// rule served by it.
fn exact_uid_nodes_conn(
    db_conn: &mut SqliteConnection,
    uid: &str,
    limit: i64,
) -> QueryResult<Vec<NavigationNode>> {
    navigation_nodes::table
        .filter(
            navigation_nodes::node_uid.eq(uid).or(navigation_nodes::redirect_uid.is_null().and(
                navigation_nodes::sutta_uid
                    .eq(uid)
                    .or(navigation_nodes::sutta_uid.like(format!("{}/%", uid))),
            )),
        )
        .select(NavigationNode::as_select())
        .order(navigation_nodes::sort_path.asc())
        .limit(limit)
        .load::<NavigationNode>(db_conn)
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
/// **A query typed as a sutta reference is matched as one** (see
/// [`ref_query_uids`]): `dn 1` lists DN 1 first, then every node whose
/// folded fields contain `dn1` — the substring matches a typed `dn1` gives.
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
    let ref_uids = ref_query_uids(query);

    let mut matches: Vec<NavigationNode> = Vec::new();
    // The first reading that named a node. The substring pass runs on it,
    // because `dn 1` as typed contains a space that no folded field has.
    let mut hit_uid: Option<&str> = None;
    for uid in &ref_uids {
        if (matches.len() as i64) >= limit {
            break;
        }
        let found = exact_uid_nodes_conn(db_conn, uid, limit)?;
        if !found.is_empty() && hit_uid.is_none() {
            hit_uid = Some(uid);
        }
        for n in found {
            if !matches.iter().any(|m| m.node_uid == n.node_uid) {
                matches.push(n);
            }
        }
    }
    matches.truncate(limit.max(0) as usize);

    // Which text the substring pass looks for. A reading that named a node
    // exactly is the one the reader meant. Failing that, each reading is tried
    // as a substring in turn -- `kv 1` names no node, but `kv1` is the prefix
    // of `kv1.1`, `kv1.2`, ... -- and last of all the query as typed, since a
    // reading that matches nothing at all (`raja 1` → `raja1`) was never a
    // reference.
    let mut candidates: Vec<&str> = Vec::with_capacity(ref_uids.len() + 1);
    match hit_uid {
        Some(uid) => candidates.push(uid),
        None => {
            candidates.extend(ref_uids.iter().map(String::as_str));
            candidates.push(query);
        }
    }

    for text_query in candidates {
        if (matches.len() as i64) >= limit {
            break;
        }
        let Some(pattern) = like_pattern(text_query) else { continue };

        let substring: Vec<NavigationNode> = navigation_nodes::table
            .filter(navigation_nodes::search_text.like(&pattern))
            .select(NavigationNode::as_select())
            // Tree order, so a match sits where browsing would have found it --
            // and so the LIMIT keeps the first N in that order rather than the
            // first N alphabetically. See `join_sort_path`.
            .order(navigation_nodes::sort_path.asc())
            .limit(limit)
            .load::<NavigationNode>(db_conn)?;

        let found_any = !substring.is_empty();
        let seen: std::collections::HashSet<String> =
            matches.iter().map(|n| n.node_uid.clone()).collect();
        let room = (limit as usize).saturating_sub(matches.len());
        matches.extend(
            substring
                .into_iter()
                .filter(|n| !seen.contains(&n.node_uid))
                .take(room),
        );
        if found_any {
            break;
        }
    }

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
    // A whole row, not `COUNT(*)`: the question this answers is "will the
    // level queries work", and those name every column of the table.
    //
    // The schema spans two migrations (the table, then the `redirect_uid`
    // column), and a migration failure at startup is non-fatal by design. So
    // "table present, later column missing" is reachable -- and a `COUNT(*)`
    // succeeds on exactly that table. The window would then be told the tree
    // is available, and every level query after it would fail: an empty
    // window with no explanation, which is the outcome FR-13 exists to
    // prevent. Selecting a row fails the same way the level queries will.
    let row = navigation_nodes::table
        .select(NavigationNode::as_select())
        .first::<NavigationNode>(db_conn)
        .optional()?;
    Ok(row.is_some())
}

// ============================================================================
// Public API
//
// A missing table, a missing `AppData` (before `init_app_data()`, and in unit
// tests) and a failed query all answer the same way: empty, and logged.
// ============================================================================

pub(crate) fn with_appdata<T, F>(what: &str, empty: T, op: F) -> T
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
            // Includes "no such table: navigation_nodes" and "no such column:
            // redirect_uid" -- both reachable, because the schema spans two
            // migrations and a migration failure at startup is non-fatal by
            // design.
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

// ============================================================================
// JSON views
//
// The window renders from JSON strings, as the topic-index functions do, so
// the shape a row is drawn from is defined here rather than in the bridge:
// every field is derived from a `NavigationNode` in one place, and the derived
// flags (`is_openable`, `is_fragment`, `in_file`) are unit-testable without a
// Qt object.
// ============================================================================

/// One row as the window draws it. Carries the stored columns the rendering
/// needs plus the three derived flags; the ordering and path columns are
/// deliberately left out, since the level query already returns rows in order
/// and the breadcrumbs come from [`NavigationMatchView`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationNodeView {
    pub node_uid: String,
    pub parent_uid: Option<String>,
    pub node_type: String,
    pub tree_source: String,
    pub title_pali: Option<String>,
    pub title_en: Option<String>,
    /// The summary, from [`NavigationTexts::blurb`].
    pub blurb: Option<String>,
    /// The project's own note, from [`NavigationTexts::context_note`]. Drawn
    /// below the blurb, and governed by the same Summaries toggle.
    pub context_note: Option<String>,
    pub sutta_uid: Option<String>,
    pub cst_file: Option<String>,
    pub has_children: bool,
    pub child_count: i32,
    /// [`NavigationNode::is_openable`] — false means the row is dimmed and its
    /// Open button is disabled, never that a tab opens empty.
    pub is_openable: bool,
    pub is_fragment: bool,
    /// The file a fragment row is a portion of (FR-17), and `None` on every
    /// other row — including a division row, which carries the same
    /// `cst_file` but is not a portion of anything the list also shows.
    pub in_file: Option<String>,
    /// The uid of the text this row opens when it is not the row's own: a
    /// Pātimokkha rule served by its Vibhaṅga text. `None` on every ordinary
    /// row.
    ///
    /// The row **must** show it. A bhikkhunī rule whose own division has no
    /// text opens the bhikkhu rule SuttaCentral names, and without the label
    /// the reader is left wondering why pressing Open on a nuns' rule produced
    /// a monks' one.
    pub redirect_uid: Option<String>,
    /// The reference the sidebar's search results show for this text
    /// (`DN 1`, `SN 56.11`), from the opened row's `suttas.sutta_ref`, so the
    /// row can read `SN 56.11 Dhammacakkappavattanasutta`. Filled per level by
    /// [`attach_sutta_refs`]; `None` where [`wants_sutta_ref`] says the row
    /// should not show one.
    pub sutta_ref: Option<String>,
}

impl NavigationNodeView {
    /// The view of a stored node, with its summary and context note looked up
    /// in `texts` by `node_uid`.
    ///
    /// The texts are an argument, not a reach for
    /// [`navigation_texts()`](crate::navigation_texts::navigation_texts), so a
    /// view can be built against texts of the caller's choosing.
    pub fn from_node(node: &NavigationNode, texts: &NavigationTexts) -> Self {
        let is_fragment = node.is_fragment();
        Self {
            node_uid: node.node_uid.clone(),
            parent_uid: node.parent_uid.clone(),
            node_type: node.node_type.clone(),
            tree_source: node.tree_source.clone(),
            title_pali: node.title_pali.clone(),
            title_en: node.title_en.clone(),
            blurb: texts.blurb(&node.node_uid).map(|s| s.to_string()),
            context_note: texts.context_note(&node.node_uid).map(|s| s.to_string()),
            sutta_uid: node.sutta_uid.clone(),
            cst_file: node.cst_file.clone(),
            has_children: node.has_children,
            child_count: node.child_count,
            is_openable: node.is_openable(),
            is_fragment,
            in_file: is_fragment.then(|| node.cst_file.clone()).flatten(),
            redirect_uid: node.redirect_uid.clone(),
            sutta_ref: None,
        }
    }
}

/// Whether a row shows its text's `sutta_ref` before the title.
///
/// Two kinds of leaf do not:
///
/// - a **redirected** leaf: the ref belongs to the parallel it opens, so a
///   bhikkhunī rule would be labelled with the bhikkhu rule's reference;
/// - a CST **division or whole-file** record (`s0305m.mul11.xml/pli/cst`): its
///   stored ref is derived from the file name (`Vin 01a.att0`), which says
///   nothing a reader can use and duplicates the uid line.
pub fn wants_sutta_ref(view: &NavigationNodeView) -> bool {
    if view.redirect_uid.as_deref().is_some_and(|u| !u.is_empty()) {
        return false;
    }
    match view.sutta_uid.as_deref() {
        Some(uid) if !uid.is_empty() => !bare_sutta_key(uid).ends_with(".xml"),
        _ => false,
    }
}

/// `suttas.uid` → `suttas.sutta_ref` for the given uids, in one indexed
/// statement per chunk.
pub fn sutta_refs_conn(
    db_conn: &mut SqliteConnection,
    sutta_uids: &[String],
) -> QueryResult<std::collections::HashMap<String, String>> {
    use crate::db::appdata_schema::suttas;

    let mut out = std::collections::HashMap::new();
    for chunk in sutta_uids.chunks(COUNT_CHUNK * 5) {
        let rows: Vec<(String, String)> = suttas::table
            .filter(suttas::uid.eq_any(chunk))
            .select((suttas::uid, suttas::sutta_ref))
            .load(db_conn)?;
        for (uid, sutta_ref) in rows {
            let trimmed = sutta_ref.trim();
            if !trimmed.is_empty() {
                out.insert(uid, trimmed.to_string());
            }
        }
    }
    Ok(out)
}

/// Fill `sutta_ref` on every row of a level that should show one. A failed
/// lookup leaves the refs empty -- the rows still render, titled as before.
pub fn attach_sutta_refs(views: &mut [&mut NavigationNodeView]) {
    let mut uids: Vec<String> = views
        .iter()
        .filter(|v| wants_sutta_ref(v))
        .filter_map(|v| v.sutta_uid.clone())
        .collect();
    if uids.is_empty() {
        return;
    }
    uids.sort();
    uids.dedup();

    let refs = with_appdata("attach_sutta_refs", std::collections::HashMap::new(), |db_conn| {
        sutta_refs_conn(db_conn, &uids)
    });
    for view in views.iter_mut() {
        if wants_sutta_ref(view) {
            view.sutta_ref = view.sutta_uid.as_ref().and_then(|u| refs.get(u).cloned());
        }
    }
}

/// One whole-tree filter hit: the row, and the titles of its ancestors so the
/// flat result list shows where it sits (FR-27).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationMatchView {
    pub node: NavigationNodeView,
    pub breadcrumbs: Vec<String>,
}

impl NavigationMatchView {
    pub fn from_match(m: &NavigationMatch, texts: &NavigationTexts) -> Self {
        Self {
            node: NavigationNodeView::from_node(&m.node, texts),
            breadcrumbs: m.breadcrumbs.clone(),
        }
    }
}

/// Serialize, logging and returning an empty JSON array on failure.
///
/// A serialization error here is not recoverable by the caller and must not
/// leave the window parsing an error string as if it were data, so the empty
/// array is the one answer that renders as "nothing here" rather than as a
/// broken level.
fn to_json_array<T: Serialize>(what: &str, values: &[T]) -> String {
    match serde_json::to_string(values) {
        Ok(json) => json,
        Err(e) => {
            error(&format!("navigation_tree::{}: {}", what, e));
            "[]".to_string()
        }
    }
}

/// One level of the tree as a JSON array of [`NavigationNodeView`].
/// `None` is the root level. Empty array when the table is absent or empty.
pub fn children_json(parent: Option<&str>) -> String {
    let texts = navigation_texts();
    let mut views: Vec<NavigationNodeView> = children_of(parent)
        .iter()
        .map(|node| NavigationNodeView::from_node(node, texts))
        .collect();
    attach_sutta_refs(&mut views.iter_mut().collect::<Vec<_>>());
    to_json_array("children_json", &views)
}

/// Whole-tree filter as a JSON array of [`NavigationMatchView`].
pub fn filter_json(query: &str, limit: i64) -> String {
    let texts = navigation_texts();
    let mut views: Vec<NavigationMatchView> = filter_nodes(query, limit)
        .iter()
        .map(|m| NavigationMatchView::from_match(m, texts))
        .collect();
    attach_sutta_refs(&mut views.iter_mut().map(|m| &mut m.node).collect::<Vec<_>>());
    to_json_array("filter_json", &views)
}

/// One node as a JSON object, or `null` when there is no such node. `null` is
/// distinguishable from a node with every field empty, which an empty object
/// would not be.
pub fn node_json(node_uid: &str) -> String {
    let mut view = node_by_uid(node_uid)
        .as_ref()
        .map(|node| NavigationNodeView::from_node(node, navigation_texts()));
    if let Some(v) = view.as_mut() {
        attach_sutta_refs(&mut [v]);
    }
    match serde_json::to_string(&view) {
        Ok(json) => json,
        Err(e) => {
            error(&format!("navigation_tree::node_json: {}", e));
            "null".to_string()
        }
    }
}


// ============================================================================
// Translation counts (FR-47, FR-48)
//
// "Translations installed right now" — the count reflects the languages the
// user currently has, so it changes with every language download or removal
// and cannot be computed at bootstrap and stored. It is therefore computed for
// a whole displayed level in one statement, never once per row: the largest
// level in the built tree is 728 rows.
// ============================================================================

/// The language of the root text. It is not a translation, so it is excluded
/// from every count -- which is also what keeps a Pāli-only text from
/// reporting a translation it does not have.
///
/// Named for the *text*, not for the tree: "root" elsewhere in this module
/// means the top of the hierarchy (`node_type = "root"`, the root level).
pub const ROOT_TEXT_LANGUAGE: &str = "pli";

/// How many prefix ranges go into one statement. SQLite limits expression
/// depth (`SQLITE_MAX_EXPR_DEPTH`, 1000 by default) and a level of 728 rows
/// would otherwise build one OR chain that deep.
const COUNT_CHUNK: usize = 100;

/// The `suttas.uid` prefix a text's translations share: `sn56.11/pli/ms` →
/// `sn56.11`. FR-9's column list has no bare-key column, so the key is derived
/// at the first `/`.
pub fn bare_sutta_key(sutta_uid: &str) -> &str {
    sutta_uid.split('/').next().unwrap_or(sutta_uid)
}

/// Translations of each of `sutta_uids`, in `languages`, keyed by the uid as
/// passed in.
///
/// Matching is an indexed prefix range per uid — `uid >= 'sn56.11/' AND uid <
/// 'sn56.110'`, where `'0'` is the byte after `'/'` — so `idx_suttas(language,
/// uid)` covers the whole statement and none of the 842 MB of content columns
/// is touched. The range also excludes the commentaries (`mn1.att/pli/cst`
/// sorts before `mn1/`, since `'.' < '/'`), which are related texts rather
/// than translations of the text.
///
/// **A node whose own text is a translation does not count itself.** Almost
/// every node points at a `…/pli/…` text, which the language filter already
/// excludes, but not all of them do, so the node's own uid is subtracted where
/// it was matched. A uid left with nothing is absent from the map rather than
/// present with a zero, which is what the caller renders as "no indicator".
pub fn translation_counts_conn(
    db_conn: &mut SqliteConnection,
    sutta_uids: &[String],
    languages: &[String],
) -> QueryResult<std::collections::HashMap<String, i32>> {
    use crate::db::appdata_schema::suttas;

    let mut counts: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    if sutta_uids.is_empty() {
        return Ok(counts);
    }

    let langs: Vec<String> = languages
        .iter()
        .filter(|l| l.as_str() != ROOT_TEXT_LANGUAGE)
        .cloned()
        .collect();
    if langs.is_empty() {
        return Ok(counts);
    }

    // Distinct bare keys, so two nodes pointing into the same text cost one
    // range rather than two. The map is keyed back to every input uid below.
    let mut keys: Vec<String> = sutta_uids
        .iter()
        .map(|u| bare_sutta_key(u).to_string())
        .filter(|k| !k.is_empty())
        .collect();
    keys.sort();
    keys.dedup();

    let mut by_key: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    // The matched uids themselves, so a node that points at one of them can be
    // subtracted from its own count below. Bounded by the level's texts times
    // their translations -- a few thousand short strings at the largest level.
    let mut matched: std::collections::HashSet<String> = std::collections::HashSet::new();

    for chunk in keys.chunks(COUNT_CHUNK) {
        let mut query = suttas::table
            .into_boxed()
            .filter(suttas::language.eq_any(&langs))
            .select(suttas::uid);

        type RangePredicate = Box<
            dyn diesel::BoxableExpression<
                suttas::table,
                diesel::sqlite::Sqlite,
                SqlType = diesel::sql_types::Bool,
            >,
        >;

        let mut ranges = chunk.iter().map(|k| (format!("{}/", k), format!("{}0", k)));
        let (first_lo, first_hi) = match ranges.next() {
            Some(r) => r,
            None => continue,
        };
        let mut predicate: RangePredicate =
            Box::new(suttas::uid.ge(first_lo).and(suttas::uid.lt(first_hi)));
        for (lo, hi) in ranges {
            predicate = Box::new(predicate.or(suttas::uid.ge(lo).and(suttas::uid.lt(hi))));
        }
        query = query.filter(predicate);

        for found_uid in query.load::<String>(db_conn)? {
            let key = bare_sutta_key(&found_uid).to_string();
            *by_key.entry(key).or_insert(0) += 1;
            matched.insert(found_uid);
        }
    }

    for sutta_uid in sutta_uids {
        if let Some(n) = by_key.get(bare_sutta_key(sutta_uid)) {
            let own = if matched.contains(sutta_uid) { 1 } else { 0 };
            if *n > own {
                counts.insert(sutta_uid.clone(), *n - own);
            }
        }
    }

    Ok(counts)
}

/// [`translation_counts_conn`] against the installed sutta languages, which is
/// what "installed right now" means. Empty when `AppData` is not up yet, when
/// the query fails, or when nothing has a translation.
pub fn translation_counts(sutta_uids: &[String]) -> std::collections::HashMap<String, i32> {
    let languages = match crate::try_get_app_data() {
        Some(a) => a.get_cached_sutta_languages(),
        None => return std::collections::HashMap::new(),
    };
    with_appdata("translation_counts", std::collections::HashMap::new(), |db_conn| {
        translation_counts_conn(db_conn, sutta_uids, &languages)
    })
}

/// The level's counts as a JSON object, `{"sn56.11/pli/ms": 12, …}`, so one
/// bridge call serves a whole level (FR-48). Uids with no translation are
/// absent rather than zero.
pub fn translation_counts_json(sutta_uids: &[String]) -> String {
    let counts = translation_counts(sutta_uids);
    match serde_json::to_string(&counts) {
        Ok(json) => json,
        Err(e) => {
            error(&format!("navigation_tree::translation_counts_json: {}", e));
            "{}".to_string()
        }
    }
}

#[cfg(test)]
mod tests;
