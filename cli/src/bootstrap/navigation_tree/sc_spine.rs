//! The SuttaCentral-derived navigation spine (`tree_source = 'sc'`).
//!
//! The structure comes from the local `sc-data` checkout, which already holds
//! the whole hierarchy as files — no ArangoDB query is needed for the shape:
//!
//! | Data | Path under `sc-data/` |
//! |---|---|
//! | Roots and their grouping levels | `structure/tree/super-tree.json` |
//! | Per-collection trees | `structure/tree/{sutta,vinaya,abhidhamma}/<uid>-tree.json` |
//! | Blurbs | `additional-info/blurbs.json` |
//!
//! Titles do come from ArangoDB's `names` collection, through the bootstrap's
//! existing `get_titles()`. `misc/uid_expansion.json` cannot supply them: it
//! carries one name per uid, so it can give a collection's display name and its
//! acronym but never the Pāli-and-English pair a row renders.
//!
//! ## Scope
//!
//! SuttaCentral's structure spans the Chinese, Sanskrit, Tibetan and Gāndhārī
//! collections; Simsapa has no texts for them, so building them would fill the
//! tree with dead branches. [`SC_SCOPE`] is the allowlist that keeps them out,
//! and it is applied **while walking the super-tree**, so an excluded
//! collection is never built and then pruned. Widening the scope is an edit to
//! that table and nothing else.
//!
//! ## Node uids
//!
//! Stable across bootstraps, because the UI persists expansion state and the
//! breadcrumb path by them (never a row id, never an enumeration order):
//!
//! - A super-tree level is qualified by its ancestors — `sutta`,
//!   `sutta/linked`, `sutta/minor`, `sutta/minor/kn`. Those keys are generic
//!   words that name a *grouping*, and qualifying them keeps one root's
//!   grouping from colliding with another's.
//! - Everything from a `*-tree.json` uses its bare SuttaCentral uid — `sn`,
//!   `sn56`, `sn56.11`. Those are globally unique already.
//!
//! Titles and blurbs are always looked up by the **source** uid (the last
//! segment), never by the qualified node uid.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use arangors::Database;
use arangors::client::reqwest::ReqwestClient;
use diesel::prelude::*;
use serde_json::Value;

use simsapa_backend::logger;

use super::{DefectReport, TreeNode};
use crate::bootstrap::suttacentral::get_titles;

pub const TREE_SOURCE: &str = "sc";

/// A grouping level of `super-tree.json` that is in scope, and what is allowed
/// inside it. An entry that is absent is excluded; an empty `collections` means
/// "no collections directly here", never "all of them".
pub struct ScopeGroup {
    pub uid: &'static str,
    /// The super-tree nests one level deeper in one place: `minor` → `kn`.
    pub groups: &'static [ScopeGroup],
    pub collections: &'static [&'static str],
}

pub struct ScopeRoot {
    pub uid: &'static str,
    pub groups: &'static [ScopeGroup],
}

const KN: &[ScopeGroup] = &[ScopeGroup {
    uid: "kn",
    groups: &[],
    collections: &[
        "kp", "dhp", "ud", "iti", "snp", "vv", "pv", "thag", "thig", "tha-ap", "thi-ap", "bv",
        "cp", "ja", "mnd", "cnd", "ps", "ne", "pe", "mil",
    ],
}];

/// The Theravāda Pāli canon, and only it.
///
/// `structure/tree/sutta/` alone holds 67 collection trees, most of them
/// Chinese, Sanskrit or Tibetan; this reaches 24 of them. `dharmapadas` is
/// excluded even though `dhp` is Pāli, because `dhp` is already reachable under
/// `minor/kn` and a second path to it would duplicate the node.
pub const SC_SCOPE: &[ScopeRoot] = &[
    ScopeRoot {
        uid: "sutta",
        groups: &[
            ScopeGroup { uid: "long", groups: &[], collections: &["dn"] },
            ScopeGroup { uid: "middle", groups: &[], collections: &["mn"] },
            ScopeGroup { uid: "linked", groups: &[], collections: &["sn"] },
            ScopeGroup { uid: "numbered", groups: &[], collections: &["an"] },
            ScopeGroup { uid: "minor", groups: KN, collections: &[] },
        ],
    },
    ScopeRoot {
        uid: "vinaya",
        groups: &[ScopeGroup {
            uid: "pli-tv-vi",
            groups: &[],
            collections: &[
                "pli-tv-bu-vb",
                "pli-tv-bi-vb",
                "pli-tv-kd",
                "pli-tv-pvr",
                "pli-tv-bu-pm",
                "pli-tv-bi-pm",
            ],
        }],
    },
    ScopeRoot {
        uid: "abhidhamma",
        groups: &[ScopeGroup {
            uid: "pli-tv-ab",
            groups: &[],
            collections: &["ds", "vb", "dt", "pp", "kv", "ya", "patthana"],
        }],
    },
];

pub struct ScSpineBuilder {
    sc_data_dir: PathBuf,
    titles_pali: HashMap<String, String>,
    titles_en: HashMap<String, String>,
    blurbs: HashMap<String, String>,
    /// Base sutta uid (`sn56.11`) → the full `suttas.uid` a leaf opens
    /// (`sn56.11/pli/ms`).
    sutta_uids: HashMap<String, String>,
    /// Base sutta uid → the Pāli title of that text's Pāli row, and → the
    /// title of an English row. Kept apart **by language**: the fallback for
    /// the English line may only come from an English row, or a Pāli row's
    /// `title` column (which holds the Pāli name) would be copied into it and
    /// every row would print its Pāli title twice.
    fallback_pali: HashMap<String, String>,
    fallback_en: HashMap<String, String>,
    pub defects: DefectReport,
}

impl ScSpineBuilder {
    pub fn new(
        sc_data_dir: &Path,
        arango_db: &Database<ReqwestClient>,
        db_conn: &mut SqliteConnection,
    ) -> Result<Self> {
        let titles_pali = get_titles(arango_db, "pli")
            .context("Failed to read Pāli titles from the ArangoDB 'names' collection")?;
        let titles_en = get_titles(arango_db, "en")
            .context("Failed to read English titles from the ArangoDB 'names' collection")?;
        logger::info(&format!(
            "sc_spine: names: {} Pāli, {} English",
            titles_pali.len(),
            titles_en.len()
        ));

        let blurbs = load_blurbs(sc_data_dir)?;
        let (sutta_uids, fallback_pali, fallback_en) = load_sutta_rows(db_conn)?;

        Ok(Self {
            sc_data_dir: sc_data_dir.to_path_buf(),
            titles_pali,
            titles_en,
            blurbs,
            sutta_uids,
            fallback_pali,
            fallback_en,
            defects: DefectReport::default(),
        })
    }

    /// Build the three SC roots, in `super-tree.json`'s own order.
    pub fn build(&mut self) -> Result<Vec<TreeNode>> {
        let path = self.sc_data_dir.join("structure/tree/super-tree.json");
        let super_tree: Value = read_json(&path)?;

        let entries = super_tree.as_array().with_context(|| {
            format!("super-tree.json is not an array: {}", path.display())
        })?;

        let mut roots = Vec::new();

        // Walk the source, not the scope table, so the roots and groups keep
        // the source's order; the table only decides what is let through.
        for entry in entries {
            let Some((root_uid, value)) = single_key(entry) else {
                self.defects.record("super-tree entry is not a single-key object", entry.to_string());
                continue;
            };
            let Some(scope) = SC_SCOPE.iter().find(|r| r.uid == root_uid) else {
                continue; // Out of scope by design (FR-2a), not a defect.
            };

            let mut root = self.make_node(root_uid, root_uid, "root");
            root.children = self.build_groups(value, root_uid, scope.groups, root_uid);
            roots.push(root);
        }

        for scope in SC_SCOPE {
            if !roots.iter().any(|r| r.node_uid == scope.uid) {
                self.defects
                    .record("root in scope but absent from super-tree.json", scope.uid);
            }
        }

        Ok(roots)
    }

    /// One grouping level of the super-tree. `qualified_parent` is the parent's
    /// node uid, which is what the children's node uids are qualified with.
    fn build_groups(
        &mut self,
        value: &Value,
        root_uid: &str,
        scope: &[ScopeGroup],
        qualified_parent: &str,
    ) -> Vec<TreeNode> {
        let Some(entries) = value.as_array() else {
            self.defects
                .record("super-tree grouping is not an array", qualified_parent.to_string());
            return Vec::new();
        };

        let mut out = Vec::new();
        for entry in entries {
            let Some((group_uid, group_value)) = single_key(entry) else {
                // A bare string at a grouping level would be a collection
                // directly under a root, which the super-tree does not do.
                self.defects.record(
                    "super-tree grouping child is not a single-key object",
                    format!("{}: {}", qualified_parent, entry),
                );
                continue;
            };
            let Some(group_scope) = scope.iter().find(|g| g.uid == group_uid) else {
                continue; // Out of scope by design.
            };

            let node_uid = format!("{}/{}", qualified_parent, group_uid);
            let mut node = self.make_node(&node_uid, group_uid, "group");
            node.children =
                self.build_group_children(group_value, root_uid, group_scope, &node_uid);
            out.push(node);
        }
        out
    }

    /// The members of one grouping level: collection uids as strings, and — in
    /// the one place the super-tree nests further, `minor` → `kn` — more
    /// groups.
    fn build_group_children(
        &mut self,
        value: &Value,
        root_uid: &str,
        scope: &ScopeGroup,
        qualified_parent: &str,
    ) -> Vec<TreeNode> {
        let Some(entries) = value.as_array() else {
            self.defects
                .record("super-tree group is not an array", qualified_parent.to_string());
            return Vec::new();
        };

        let mut out = Vec::new();
        for entry in entries {
            match entry {
                Value::String(collection_uid) => {
                    if !scope.collections.contains(&collection_uid.as_str()) {
                        continue; // Out of scope by design.
                    }
                    if let Some(node) = self.build_collection(root_uid, collection_uid) {
                        out.push(node);
                    }
                }
                Value::Object(_) => {
                    let Some((nested_uid, nested_value)) = single_key(entry) else {
                        self.defects.record(
                            "nested super-tree group is not a single-key object",
                            format!("{}: {}", qualified_parent, entry),
                        );
                        continue;
                    };
                    let Some(nested_scope) = scope.groups.iter().find(|g| g.uid == nested_uid)
                    else {
                        continue; // Out of scope by design.
                    };
                    let node_uid = format!("{}/{}", qualified_parent, nested_uid);
                    let mut node = self.make_node(&node_uid, nested_uid, "group");
                    node.children = self.build_group_children(
                        nested_value,
                        root_uid,
                        nested_scope,
                        &node_uid,
                    );
                    out.push(node);
                }
                _ => self.defects.record(
                    "unexpected super-tree group member",
                    format!("{}: {}", qualified_parent, entry),
                ),
            }
        }
        out
    }

    /// One `<uid>-tree.json`. Its top level is a single-key object whose key is
    /// the collection uid.
    fn build_collection(&mut self, root_uid: &str, collection_uid: &str) -> Option<TreeNode> {
        let path = self
            .sc_data_dir
            .join("structure/tree")
            .join(root_uid)
            .join(format!("{}-tree.json", collection_uid));

        let value: Value = match read_json(&path) {
            Ok(v) => v,
            Err(e) => {
                self.defects.record(
                    "collection tree file unreadable",
                    format!("{}: {}", path.display(), e),
                );
                return None;
            }
        };

        let Some((top_uid, children)) = single_key(&value) else {
            self.defects
                .record("collection tree is not a single-key object", path.display().to_string());
            return None;
        };
        if top_uid != collection_uid {
            self.defects.record(
                "collection tree's top key does not match its file name",
                format!("{}: {}", path.display(), top_uid),
            );
        }

        let mut node = self.make_node(collection_uid, collection_uid, "collection");
        node.children = self.build_subtree(children, collection_uid);
        Some(node)
    }

    /// Everything below a collection, depth-agnostically: the shape is driven
    /// entirely by the nesting in the file, never by a level count. DN is
    /// Nikāya → Book → Sutta, SN adds Saṁyutta and Vagga, AN uses Paṇṇāsaka,
    /// and this code does not know or care.
    fn build_subtree(&mut self, value: &Value, context: &str) -> Vec<TreeNode> {
        let Some(entries) = value.as_array() else {
            self.defects
                .record("tree level is not an array", context.to_string());
            return Vec::new();
        };

        let mut out = Vec::new();
        for entry in entries {
            match entry {
                Value::String(uid) => out.push(self.make_leaf(uid)),
                Value::Object(_) => {
                    let Some((branch_uid, branch_value)) = single_key(entry) else {
                        self.defects.record(
                            "tree branch is not a single-key object",
                            format!("{}: {}", context, entry),
                        );
                        continue;
                    };
                    let mut node = self.make_node(branch_uid, branch_uid, "branch");
                    node.children = self.build_subtree(branch_value, branch_uid);
                    out.push(node);
                }
                _ => self
                    .defects
                    .record("unexpected tree member", format!("{}: {}", context, entry)),
            }
        }
        out
    }

    /// A text. Its `sutta_uid` is resolved against the rows the sutta import
    /// actually wrote, so a leaf the corpus does not carry is marked
    /// unavailable — dimmed and non-openable in the window — rather than
    /// dropped, which would hide the gap.
    fn make_leaf(&mut self, source_uid: &str) -> TreeNode {
        let mut node = self.make_node(source_uid, source_uid, "sutta");

        match self.sutta_uids.get(source_uid) {
            Some(full_uid) => node.sutta_uid = Some(full_uid.clone()),
            None => {
                node.is_available = false;
                self.defects.record("leaf has no text in the database", source_uid);
            }
        }

        // The `names` collection first, the imported row second, and nothing
        // at all third -- never an invented title, and never a title taken
        // from a row in the wrong language.
        if node.title_pali.is_none() {
            node.title_pali = self.fallback_pali.get(source_uid).cloned();
        }
        if node.title_en.is_none() {
            node.title_en = self.fallback_en.get(source_uid).cloned();
        }

        node
    }

    fn make_node(&self, node_uid: &str, source_uid: &str, node_type: &str) -> TreeNode {
        let mut node = TreeNode::new(node_uid, node_type);
        node.title_pali = self.titles_pali.get(source_uid).map(|s| clean(s));
        node.title_en = self.titles_en.get(source_uid).map(|s| clean(s));
        node.blurb = self.blurbs.get(source_uid).map(|s| clean(s));
        node
    }
}

/// `names` entries carry trailing spaces, and a blurb may be indented. Only
/// whitespace is touched: the text itself is shown as the source wrote it.
fn clean(s: &str) -> String {
    s.trim().to_string()
}

fn single_key(value: &Value) -> Option<(&str, &Value)> {
    let map = value.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.iter().next().map(|(k, v)| (k.as_str(), v))
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("Failed to parse {}", path.display()))
}

/// `blurbs.json` is `{ "<lang>": { "<grouping>": { "<uid>": "<blurb>" } } }`.
/// Only `en` is read — selecting among the other languages is out of scope, and
/// the blurbs are shown as SuttaCentral wrote them.
///
/// The grouping level is flattened away: it is an index into SuttaCentral's own
/// pages, not part of the key. Measured over the shipped checkout, the 14
/// groupings contribute 4189 uids with **no** conflicting duplicate, so
/// flattening loses nothing.
fn load_blurbs(sc_data_dir: &Path) -> Result<HashMap<String, String>> {
    let path = sc_data_dir.join("additional-info/blurbs.json");
    let value: Value = read_json(&path)?;

    let mut out = HashMap::new();
    let Some(en) = value.get("en").and_then(|v| v.as_object()) else {
        logger::warn(&format!("sc_spine: no 'en' blurbs in {}", path.display()));
        return Ok(out);
    };

    for grouping in en.values() {
        let Some(map) = grouping.as_object() else { continue };
        for (uid, blurb) in map {
            if let Some(text) = blurb.as_str() {
                out.insert(uid.clone(), text.to_string());
            }
        }
    }

    logger::info(&format!("sc_spine: {} blurbs", out.len()));
    Ok(out)
}

/// Index the imported `suttas` rows by their base uid (the part before the
/// first `/`).
///
/// Three maps, because they answer three different questions:
///
/// - which row a leaf **opens** — a Pāli row where there is one, since the
///   navigation tree is a route into the Pāli canon; otherwise the first row by
///   uid, which is what keeps a text that exists only in translation openable;
/// - the Pāli **title** fallback, from a Pāli row;
/// - the English **title** fallback, from an English row.
///
/// The last two must stay separate: a Pāli row's `title` column holds the Pāli
/// name, so filling the English line from the row that was chosen for opening
/// would print the same title twice on 1705 of the tree's rows.
type SuttaIndex = (
    HashMap<String, String>,
    HashMap<String, String>,
    HashMap<String, String>,
);

fn load_sutta_rows(db_conn: &mut SqliteConnection) -> Result<SuttaIndex> {
    use simsapa_backend::db::appdata_schema::suttas;

    let rows: Vec<(String, String, Option<String>, Option<String>)> = suttas::table
        .select((suttas::uid, suttas::language, suttas::title_pali, suttas::title))
        .order(suttas::uid.asc())
        .load(db_conn)
        .context("Failed to read the suttas table for navigation-tree leaf resolution")?;

    let mut uids: HashMap<String, String> = HashMap::new();
    let mut chosen_is_pali: HashMap<String, bool> = HashMap::new();
    let mut fallback_pali: HashMap<String, String> = HashMap::new();
    let mut fallback_en: HashMap<String, String> = HashMap::new();

    for (uid, language, title_pali, title) in rows {
        let base = uid.split('/').next().unwrap_or(&uid).to_string();
        let is_pali = language == "pli";

        let better = match chosen_is_pali.get(&base) {
            None => true,
            Some(false) => is_pali,
            Some(true) => false,
        };
        if better {
            chosen_is_pali.insert(base.clone(), is_pali);
            uids.insert(base.clone(), uid);
        }

        let first_non_empty = |a: Option<String>, b: Option<String>| {
            a.filter(|s| !s.trim().is_empty())
                .or_else(|| b.filter(|s| !s.trim().is_empty()))
                .map(|s| s.trim().to_string())
        };

        if is_pali {
            if let Some(t) = first_non_empty(title_pali, title) {
                fallback_pali.entry(base).or_insert(t);
            }
        } else if language == "en" {
            if let Some(t) = first_non_empty(title, title_pali) {
                fallback_en.entry(base).or_insert(t);
            }
        }
    }

    logger::info(&format!(
        "sc_spine: {} sutta base uids indexed ({} Pāli, {} English title fallbacks)",
        uids.len(),
        fallback_pali.len(),
        fallback_en.len()
    ));
    Ok((uids, fallback_pali, fallback_en))
}
