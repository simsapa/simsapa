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
//!
//! ## Collapsed grouping levels
//!
//! The super-tree's grouping levels separate the *traditions* — `long` holds
//! the Pāli `dn` beside the Chinese `da`, `vinaya` holds `pli-tv-vi` beside
//! `lzh-mg-vi`. In the Pāli-only scope above they each come out holding a
//! single member, so [`collapse_redundant_groups`] elides them and the tree
//! reads `Suttapiṭaka › Dīghanikāya` rather than
//! `Suttapiṭaka › Dīgha › Dīghanikāya`. A node's uid keeps the source
//! ancestry it was built from; only its place in the tree changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use arangors::Database;
use arangors::client::reqwest::ReqwestClient;
use diesel::prelude::*;
use serde_json::Value;

use simsapa_backend::helpers::sutta_range_from_ref;
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
    /// `blurbs.json`'s English entries by their source uid. Empty when the file
    /// has no `en` section, which is what tells the blurb export not to write.
    pub blurbs: HashMap<String, String>,
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
    /// Base sutta uid → the translated title stored on the very row the leaf
    /// opens (`suttas.title_trans`). It outranks every other source of the
    /// English line: it is the title the corpus itself gives that text.
    own_trans: HashMap<String, String>,
    /// `relationship/parallels.json`, indexed uid → the other uids in its
    /// groups. See [`ScSpineBuilder::parallel_redirect`].
    parallels: HashMap<String, Vec<String>>,
    /// Corpus rows that cover a *span* of rules, indexed by their
    /// [`sutta_range_from_ref`] group: `pli-tv-bu-vb-as` → `(1, 7,
    /// "pli-tv-bu-vb-as1-7")`. See [`ScSpineBuilder::covering_rows`].
    ranged_rows: HashMap<String, Vec<(u32, u32, String)>>,
    /// How many leaves were opened through a parallel rather than their own
    /// uid. Logged as a count, because it is the number that moves when the
    /// corpus or the sc-data checkout changes.
    redirects: usize,
    /// How many ranged leaves became branches over their individual rules.
    range_expansions: usize,
    pub defects: DefectReport,
}

impl ScSpineBuilder {
    /// An empty builder, for unit-testing the resolvers that take no I/O.
    /// The real [`Self::new`] needs ArangoDB and the sc-data checkout.
    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            sc_data_dir: PathBuf::new(),
            titles_pali: HashMap::new(),
            titles_en: HashMap::new(),
            blurbs: HashMap::new(),
            sutta_uids: HashMap::new(),
            fallback_pali: HashMap::new(),
            fallback_en: HashMap::new(),
            own_trans: HashMap::new(),
            parallels: HashMap::new(),
            ranged_rows: HashMap::new(),
            redirects: 0,
            range_expansions: 0,
            defects: DefectReport::default(),
        }
    }

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
        let parallels = load_parallels(sc_data_dir)?;
        let SuttaIndex { sutta_uids, fallback_pali, fallback_en, own_trans } =
            load_sutta_rows(db_conn)?;
        let ranged_rows = index_ranged_rows(&sutta_uids);

        Ok(Self {
            sc_data_dir: sc_data_dir.to_path_buf(),
            titles_pali,
            titles_en,
            blurbs,
            sutta_uids,
            fallback_pali,
            fallback_en,
            own_trans,
            ranged_rows,
            parallels,
            redirects: 0,
            range_expansions: 0,
            defects: DefectReport::default(),
        })
    }

    /// How many leaves were opened through [`Self::parallel_redirect`].
    pub fn redirect_count(&self) -> usize {
        self.redirects
    }

    /// How many ranged leaves became branches through
    /// [`Self::expand_range_leaf`].
    pub fn range_expansion_count(&self) -> usize {
        self.range_expansions
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
            collapse_redundant_groups(&mut root);
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
            // No text of its own. SuttaCentral renders such a leaf as a page
            // of parallels, so before dimming it, ask what text it points at.
            None => match self.parallel_redirect(source_uid) {
                ParallelRedirect::To { base_uid, sutta_uid } => {
                    node.sutta_uid = Some(sutta_uid);
                    // What the row shows, so the reader can see that a
                    // bhikkhunī rule is being served by a bhikkhu one.
                    node.redirect_uid = Some(base_uid);
                    self.redirects += 1;
                }
                ParallelRedirect::Ambiguous(candidates) => {
                    node.is_available = false;
                    self.defects.record(
                        "leaf has no text and several Pāli parallels in its division",
                        format!("{} -- {}", source_uid, candidates.join(", ")),
                    );
                }
                // A leaf that names a *span* of rules and has no text of its
                // own is not a dead end: each rule in it may have one.
                ParallelRedirect::None => match self.expand_range_leaf(source_uid) {
                    Some(children) => {
                        node.node_type = "branch".to_string();
                        node.children = children;
                        self.range_expansions += 1;
                    }
                    None => {
                        node.is_available = false;
                        self.defects
                            .record("leaf has no text and no Pāli parallel", source_uid);
                    }
                },
            },
        }

        // The `names` collection first, the imported row second, and nothing
        // at all third -- never an invented title, and never a title taken
        // from a row in the wrong language.
        //
        // The English line has one source above `names`: a translated title
        // saved on the row this leaf opens. It is keyed by the leaf's own uid,
        // so a redirected leaf never borrows its parallel's title.
        if let Some(t) = self.own_trans.get(source_uid) {
            node.title_en = Some(clean(t));
        }
        if node.title_pali.is_none() {
            node.title_pali = self.fallback_pali.get(source_uid).map(|s| clean(s));
        }
        if node.title_en.is_none() {
            node.title_en = self.fallback_en.get(source_uid).map(|s| clean(s));
        }

        node
    }

    /// The Pāli text a textless leaf should open, from SuttaCentral's own
    /// `relationship/parallels.json`.
    ///
    /// ## Why the leaf has no text
    ///
    /// The Pātimokkha is the rules as they are **recited**, and SuttaCentral
    /// gives each rule of it a leaf — `pli-tv-bu-pm-pj1`. There is no document
    /// behind that uid: SuttaCentral renders the page as a list of the same
    /// rule in every tradition it carries (`lzh-dg-bu-vb-pj1/lzh/taisho`,
    /// `xct-mu-bu-pm-pj1`, …), among which the Pāli one is
    /// `pli-tv-bu-vb-pj1/pli/ms` — the rule inside the Vibhaṅga, with its
    /// analysis. Simsapa carries only the Pāli, so the parallels page has
    /// nothing to show and the leaf must simply open that text.
    ///
    /// Left unresolved these are 538 of the 543 dimmed leaves, which makes
    /// **both Pātimokkhas unbrowsable**: their collection nodes are branches
    /// with no text of their own, so every route into them ends on a dimmed
    /// row.
    ///
    /// ## Two filters, and the second one is the whole point
    ///
    /// A parallels group is the *corresponding rule across traditions and
    /// across the two divisions*, so it always holds four Pāli members — the
    /// bhikkhu pair and the bhikkhunī pair:
    ///
    /// ```text
    /// pli-tv-bi-pm-ss7 → { pli-tv-bu-vb-ss5, pli-tv-bu-pm-ss5,
    ///                      pli-tv-bi-vb-ss7, pli-tv-bi-pm-ss7 }
    /// ```
    ///
    /// Only `pli-tv-bi-vb-ss7` is *this* rule's text. `pli-tv-bu-vb-ss5` is
    /// the monks' corresponding rule — a different rule with different text,
    /// and following it would silently open the wrong one. So a candidate must
    /// be **in the same division**: the first three hyphen segments equal
    /// (`pli-tv-bi`). Measured, dropping that filter "resolves" 425 of 538
    /// instead of 345, and the extra 80 are all wrong in exactly this way.
    ///
    /// The candidate must also have a text in the corpus, which is what makes
    /// this a redirect rather than a second dead end.
    ///
    /// ## Why not rewrite the uid
    ///
    /// Replacing `-pm-` with `-vb-` reproduces 342 of these 345 and never
    /// disagrees with the data — but it is a guess about uid spelling, which
    /// FR-8 forbids, and it is not complete: `pli-tv-bi-pm-pc91`, `pc92` and
    /// `pc93` are all one text, `pli-tv-bi-vb-pc90`, which no rewrite finds.
    /// The group states the relationship; the code only reads it.
    ///
    /// **Only the `parallels` relation is followed**, never `mentions` or
    /// `retells`: those say one text refers to another, which is no reason to
    /// open it in place of this one.
    fn parallel_redirect(&self, source_uid: &str) -> ParallelRedirect {
        let Some(members) = self.parallels.get(source_uid) else {
            return ParallelRedirect::None;
        };
        let own_division = division(source_uid);

        // `pli-tv-` is SuttaCentral's prefix for the **Theravāda Vinaya**, and
        // it is deliberately the whole reach of this mechanism. A Pāli *sutta*
        // uid is `mn10`, with no prefix, so a textless leaf anywhere but the
        // vinaya can never be redirected -- it stays an honest dimmed row.
        // Every one of the 543 textless leaves in this corpus is a vinaya rule,
        // so nothing is given up by refusing to generalise past what was
        // measured.
        let candidates_pool: Vec<&String> = members
            .iter()
            .filter(|m| m.as_str() != source_uid && m.starts_with("pli-tv-"))
            .collect();

        // Same division first, and a rule of the other division only as a
        // fallback -- so a leaf whose own division does carry the text never
        // reaches across for it.
        let same: Vec<&String> = candidates_pool
            .iter()
            .copied()
            .filter(|m| division(m) == own_division)
            .collect();
        let other: Vec<&String> = candidates_pool
            .iter()
            .copied()
            // The candidate must itself be a rule, i.e. have a division. The
            // groups also name whole Khandhaka chapters (`pli-tv-kd22`), which
            // are where a rule's story is told, not the rule's text -- five
            // leaves were ambiguous until this filter was added.
            .filter(|m| division(m) != own_division && division(m).is_some())
            .collect();

        for (pool, through_range) in
            [(&same, false), (&same, true), (&other, false), (&other, true)]
        {
            let candidates: Vec<String> = if through_range {
                pool.iter()
                    .flat_map(|m| self.covering_rows(m))
                    .collect()
            } else {
                pool.iter()
                    .filter(|m| self.sutta_uids.contains_key(m.as_str()))
                    .map(|m| (*m).clone())
                    .collect()
            };

            match self.settle(source_uid, candidates) {
                ParallelRedirect::None => continue,
                settled => return settled,
            }
        }

        ParallelRedirect::None
    }

    /// Reduce one tier's candidates to a single target, or report the tie.
    fn settle(&self, source_uid: &str, candidates: Vec<String>) -> ParallelRedirect {
        let mut candidates = candidates;
        candidates.sort();
        candidates.dedup();

        // Several survive only where one rule's group also names a nearby rule
        // of the same division (`pm-pj4` → `vb-pj4` and `vb-pc8`). The rule
        // designator is what tells them apart, and it is a tie-break *among
        // candidates the source already vouched for* -- not a way of finding
        // one. If it does not single one out, the leaf is reported and stays
        // dimmed rather than opening a guess.
        if candidates.len() > 1 {
            let wanted = rule_segment(source_uid);
            let exact: Vec<String> = candidates
                .iter()
                .filter(|m| rule_segment(m) == wanted)
                .cloned()
                .collect();
            if exact.len() == 1 {
                candidates = exact;
            }
        }

        match candidates.len() {
            0 => ParallelRedirect::None,
            1 => {
                let base = candidates.remove(0);
                match self.sutta_uids.get(base.as_str()) {
                    Some(full) => ParallelRedirect::To {
                        base_uid: base,
                        sutta_uid: full.clone(),
                    },
                    None => ParallelRedirect::None,
                }
            }
            _ => ParallelRedirect::Ambiguous(candidates),
        }
    }

    /// The corpus rows whose numeric range **contains** `target`.
    ///
    /// SuttaCentral names a rule individually (`pli-tv-bi-vb-pj1`) where the
    /// corpus stores several rules as one document (`pli-tv-bi-vb-pj1-4`, the
    /// four pārājikas the nuns share with the monks). Without this the
    /// redirect resolves to a uid that has no row and the leaf stays dimmed
    /// although its text is present.
    ///
    /// The range comes from [`sutta_range_from_ref`], the same parser the
    /// sutta import uses for `sn30.7-16` — vinaya rule uids parse under it
    /// unchanged (`pli-tv-bu-vb-as1-7` → group `pli-tv-bu-vb-as`, 1..=7), so
    /// there is no second range parser here.
    fn covering_rows(&self, target: &str) -> Vec<String> {
        let Some(range) = sutta_range_from_ref(target) else {
            return Vec::new();
        };
        let (Some(start), Some(end)) = (range.start, range.end) else {
            return Vec::new();
        };
        let Some(rows) = self.ranged_rows.get(&range.group) else {
            return Vec::new();
        };
        rows.iter()
            .filter(|(from, to, _)| *from <= start && end <= *to)
            .map(|(_, _, uid)| uid.clone())
            .collect()
    }

    /// Turn a leaf that names a **span of rules** into a branch over them.
    ///
    /// SuttaCentral's bhikkhunī Vibhaṅga has five leaves that stand for
    /// several rules at once — `pli-tv-bi-vb-ss7-9`, `sk2-74` — because the
    /// nuns' text says only that those rules are the monks'. There is no
    /// document behind the ranged uid, and none behind it in this corpus
    /// either, so the leaf was dimmed and 168 rules were unreachable.
    ///
    /// Each rule *individually* does resolve: `pli-tv-bi-vb-ss7` is in
    /// `parallels.json` and its group names `pli-tv-bu-vb-ss5`. So the range
    /// becomes a branch and each rule becomes a leaf, resolved by the ordinary
    /// path — [`Self::parallel_redirect`] needs no special case for them.
    ///
    /// ## Why this cannot run away
    ///
    /// **Only unresolved leaves reach here.** It is the last arm after "has
    /// its own text" and "has a parallel", and the SC spine holds **247**
    /// ranged leaves — every SN, AN and Dhp range (`sn30.7-16`, `an1.394-574`,
    /// `dhp1-20`) — which the corpus stores under that same ranged uid and
    /// which therefore never get here. Exactly five do.
    ///
    /// **Every rule uid must be one SuttaCentral names.** The numbers are
    /// expanded from the range the source itself wrote, but a uid built that
    /// way is still a construction, so each must appear in `parallels.json`
    /// before it is allowed to become a node. One unknown number and the whole
    /// range is left alone and reported — expanding a span into rules
    /// SuttaCentral does not have is precisely the invention FR-8 forbids.
    /// Measured: all 168 are named, and all 168 resolve.
    ///
    /// **At least one rule must have a text.** Checked *before* the children
    /// are built, so a range that cannot be expanded records one defect
    /// against itself rather than 73 against children that are then thrown
    /// away. Without this a future corpus could turn one honest dimmed leaf
    /// into a branch of dimmed ones.
    ///
    /// A rule that is named and has no text is still kept, and dimmed, like
    /// any other leaf: the gap is reported where it is, not hidden by dropping
    /// the row.
    ///
    /// Recursion terminates because a single-number uid has `start == end` and
    /// returns on the first check.
    fn expand_range_leaf(&mut self, source_uid: &str) -> Option<Vec<TreeNode>> {
        let range = sutta_range_from_ref(source_uid)?;
        let (Some(start), Some(end)) = (range.start, range.end) else {
            return None;
        };
        if end <= start {
            return None;
        }

        let rules: Vec<String> = (start..=end)
            .map(|n| format!("{}{}", range.group, n))
            .collect();

        if let Some(unknown) = rules.iter().find(|u| !self.parallels.contains_key(u.as_str())) {
            self.defects.record(
                "ranged leaf not expanded: SuttaCentral does not name every rule in it",
                format!("{} -- e.g. {}", source_uid, unknown),
            );
            return None;
        }

        let any_has_text = rules.iter().any(|u| {
            self.sutta_uids.contains_key(u.as_str())
                || matches!(self.parallel_redirect(u), ParallelRedirect::To { .. })
        });
        if !any_has_text {
            self.defects.record(
                "ranged leaf not expanded: no rule in it has a text",
                source_uid,
            );
            return None;
        }

        Some(rules.iter().map(|u| self.make_leaf(u)).collect())
    }

    fn make_node(&self, node_uid: &str, source_uid: &str, node_type: &str) -> TreeNode {
        let mut node = TreeNode::new(node_uid, node_type);
        node.source_uid = Some(source_uid.to_string());
        // An empty `names` entry is no title at all, and must leave the field
        // `None` so `make_leaf`'s fallbacks still run.
        let non_empty = |s: &String| Some(clean(s)).filter(|c| !c.trim().is_empty());
        node.title_pali = self.titles_pali.get(source_uid).and_then(non_empty);
        node.title_en = self.titles_en.get(source_uid).and_then(non_empty);
        node
    }
}

/// `node_type` of a super-tree grouping level — the only kind of node
/// [`collapse_redundant_groups`] is allowed to elide.
const GROUP: &str = "group";

/// Elide the super-tree grouping levels that the Pāli-only scope has left with
/// nothing to distinguish.
///
/// SuttaCentral's `long` level exists to hold the *several* traditions' long
/// collections side by side — the Pāli `dn`, the Chinese `da`, and so on.
/// FR-2a restricts this spine to the Theravāda Pāli canon, so every one of
/// those levels comes out of the walk holding exactly one member, and
/// `Suttapiṭaka › Dīgha › Dīghanikāya` asks the reader to walk two levels that
/// say the same thing. Skipping the grouping loses no information, so the tree
/// shows `Suttapiṭaka › Dīghanikāya`.
///
/// **Two shapes, one rule.** A parent with exactly one child has a redundant
/// *edge*, and which end survives is decided by what the end is:
///
/// | Shape | Example | Elided | Result |
/// |---|---|---|---|
/// | a grouping level with a single child | `sutta › long › dn` | the grouping | `sutta › dn` |
/// | a **root** whose single child is a grouping level | `vinaya › pli-tv-vi › …` | the grouping | `vinaya › …` |
///
/// The second shape is the same redundancy one level up: SuttaCentral splits
/// `vinaya` by school (`pli-tv-vi`, `lzh-mg-vi`, `san-mg-vi`, …) and
/// `abhidhamma` likewise, and in scope there is only ever the Theravāda one.
/// A root is never elided, so there the grouping gives way instead. It is
/// evaluated **before** the first shape and on the tree as built, because it
/// asks a question about the source: whether the root had one grouping to
/// begin with.
///
/// The tie-break is what makes `sutta › minor › kn` come out as `sutta › kn`
/// and not as `sutta › minor`: `minor` is the grouping with the single child,
/// so `minor` goes and `kn` — the Khuddakanikāya, a real collection with 20
/// members — stays. A rule phrased as "elide an only child" would have taken
/// the wrong one.
///
/// **Only `group` nodes are elided, and this is the load-bearing part.** A
/// collection, a branch or a leaf keeps its level however few children it has.
/// `sn8` Vaṅgīsasaṁyutta holds exactly one vagga, `sn8-vangisavagga`
/// Vaṅgīsavagga, and looks just like `sutta > long > dn` — but `long` holds
/// one member because the scope deleted its siblings, while `sn8` holds one
/// vagga because the Saṁyutta Nikāya has one there. A saṁyutta and a vagga are
/// **distinct groupings of the source with distinct names**, and a reader may
/// go looking for either of them, so collapsing one into the other would lose
/// a name the corpus uses and a uid it refers to. 180 branch nodes in the
/// built tree have exactly one child; not one of them may go.
///
/// The type test is what enforces it, and it is safe because `"group"` is
/// assigned at exactly two places — [`ScSpineBuilder::build_groups`] and
/// [`ScSpineBuilder::build_group_children`], the two super-tree walkers.
/// Nothing from a `*-tree.json` can ever carry it: `build_collection` writes
/// `"collection"`, `build_subtree` writes `"branch"` and `make_leaf` writes
/// `"sutta"`. If a future change gives a `*-tree.json` node the `"group"` type,
/// this rule starts eating real levels.
///
/// **Nothing of substance is dropped with them.** The elided levels' blurbs
/// are the cross-tradition summaries ("Collections of “long” discourses and
/// other related texts"), while the surviving node carries the specific one
/// ("The Long Discourses (Dīgha Nikāya, abbreviated DN) is a collection of 34
/// discourses…"); `pli-tv-vi` and `pli-tv-ab` have no blurb at all, and their
/// titles ("Theravāda Vinayapiṭaka") only restate the root's.
///
/// **Node uids are not rewritten.** `kn` keeps the uid `sutta/minor/kn` even
/// though its parent is now `sutta`: the uid is qualified by the node's
/// ancestry **in the source**, which is what makes it stable across
/// bootstraps (T-3). Were the scope later widened so that `minor` regains a
/// sibling and stops being elided, `kn`'s uid would not move. `parent_uid`,
/// `path` and `sort_path` are derived by the flattening pass from the tree
/// this function returns, so the collapse reaches the breadcrumbs and the
/// filter's result paths with no further code.
fn collapse_redundant_groups(root: &mut TreeNode) {
    // Shape 2 runs **first, and on the tree as it was built**, because it asks
    // whether the root's own grouping is the only one the scope let through.
    // Run it after shape 1 and the question changes: `sutta > minor > kn`
    // collapses to `sutta > kn`, `sutta` momentarily has one grouping child,
    // and the Khuddakanikāya's 20 collections get spliced into the root.
    if root.children.len() == 1 && root.children[0].node_type == GROUP {
        let only = root.children.remove(0);
        root.children = only.children;
    }

    collapse_pass_through_groups(root);
}

/// Shape 1 of [`collapse_redundant_groups`], applied at every depth: a
/// grouping level standing over a single child is a pass-through, so the child
/// takes its place.
fn collapse_pass_through_groups(node: &mut TreeNode) {
    for child in node.children.iter_mut() {
        collapse_pass_through_groups(child);
    }

    for child in node.children.iter_mut() {
        // A loop, not an `if`: the child that moves up may itself be such a
        // grouping.
        while child.node_type == GROUP && child.children.len() == 1 {
            let only = child.children.remove(0);
            *child = only;
        }
    }
}

/// What [`ScSpineBuilder::parallel_redirect`] found for a leaf with no text.
enum ParallelRedirect {
    /// No parallels group, or nothing in it that this corpus can open.
    None,
    /// Exactly one same-division Pāli parallel that has a text.
    To {
        /// The parallel's own uid. Not written to the row today — the node
        /// keeps its own uid, as SuttaCentral's page does — but it is what a
        /// Written to `redirect_uid`, which is what the row renders.
        base_uid: String,
        /// The `suttas.uid` the leaf's Open button hands to the reader.
        sutta_uid: String,
    },
    /// Several survived both filters. Reported and left dimmed, never guessed
    /// between (FR-8). Zero cases on today's data.
    Ambiguous(Vec<String>),
}

/// The division a vinaya rule uid belongs to: its first three hyphen segments,
/// `pli-tv-bu` or `pli-tv-bi`.
///
/// `None` for anything with fewer than four segments, which is every sutta uid
/// (`sn56.11`) and every collection uid — so a non-vinaya leaf can never be
/// redirected by division equality, and the mechanism stays where it was
/// measured.
fn division(uid: &str) -> Option<(&str, &str, &str)> {
    let mut segments = uid.split('-');
    let a = segments.next()?;
    let b = segments.next()?;
    let c = segments.next()?;
    segments.next()?;
    Some((a, b, c))
}

/// The rule designator at the end of a vinaya uid: `pj1`, `sk27`.
fn rule_segment(uid: &str) -> &str {
    uid.rsplit('-').next().unwrap_or(uid)
}

/// `relationship/parallels.json`, indexed uid → the other uids of every group
/// it appears in.
///
/// The file is a list of single-key objects, and **only the `parallels` key is
/// read**. The other two relations it carries, `mentions` and `retells`, say
/// that one text refers to another — true, and no reason at all to open it in
/// place of the text the user asked for.
///
/// Two member spellings are normalized away, and one kind is skipped:
///
/// - a leading `~` marks a partial parallel (`~mn10`);
/// - a `#` suffix narrows it to a segment range (`sn1.20#28.1`);
/// - a member with a space in it is a **prose label**, not a uid
///   ("Bhikkhu Parivāra Pārājika 1"), and is dropped rather than looked up.
fn load_parallels(sc_data_dir: &Path) -> Result<HashMap<String, Vec<String>>> {
    let path = sc_data_dir.join("relationship/parallels.json");
    let value: Value = match read_json(&path) {
        Ok(v) => v,
        Err(e) => {
            // Not fatal: without it every textless leaf simply stays dimmed,
            // which is the behaviour this file improves on, not one it is
            // required for.
            logger::warn(&format!("sc_spine: no parallels: {:#}", e));
            return Ok(HashMap::new());
        }
    };

    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut groups = 0usize;

    for entry in value.as_array().map(|a| a.as_slice()).unwrap_or_default() {
        let Some(list) = entry.get("parallels").and_then(|v| v.as_array()) else {
            continue;
        };
        let members: Vec<String> = list
            .iter()
            .filter_map(|v| v.as_str())
            .filter_map(parallel_uid)
            .map(|s| s.to_string())
            .collect();
        if members.len() < 2 {
            continue;
        }
        groups += 1;
        for member in &members {
            let others = out.entry(member.clone()).or_default();
            others.extend(members.iter().filter(|m| *m != member).cloned());
        }
    }

    logger::info(&format!(
        "sc_spine: {} parallel group(s) over {} uid(s)",
        groups,
        out.len()
    ));
    Ok(out)
}

/// Index the corpus's base uids by [`sutta_range_from_ref`] group, keeping
/// only the rows that span more than one number.
///
/// A single-number row is reachable by the exact tiers already, so indexing it
/// here would only let a range tier re-find it; keeping the index to real
/// spans is what makes "resolved through a range" mean what it says.
fn index_ranged_rows(sutta_uids: &HashMap<String, String>) -> HashMap<String, Vec<(u32, u32, String)>> {
    let mut out: HashMap<String, Vec<(u32, u32, String)>> = HashMap::new();
    for base in sutta_uids.keys() {
        let Some(range) = sutta_range_from_ref(base) else {
            continue;
        };
        let (Some(start), Some(end)) = (range.start, range.end) else {
            continue;
        };
        if end > start {
            out.entry(range.group)
                .or_default()
                .push((start, end, base.clone()));
        }
    }
    out
}

/// One `parallels` member as a uid, or `None` if it is not one.
fn parallel_uid(entry: &str) -> Option<&str> {
    let uid = entry
        .strip_prefix('~')
        .unwrap_or(entry)
        .split('#')
        .next()
        .unwrap_or("")
        .trim();
    if uid.is_empty() || uid.contains(' ') {
        None
    } else {
        Some(uid)
    }
}

/// `names` entries carry trailing spaces, and a blurb may be indented. Only
/// whitespace is touched: the text itself is shown as the source wrote it.
fn clean(s: &str) -> String {
    s.trim().to_string()
}

/// Which `suttas` row an SC leaf opens when several carry the same text.
///
/// **Lower wins.** The SC spine's own edition is the Mahāsaṅgīti (`pli/ms`),
/// which is what the SuttaCentral structure was built around; the CST text of
/// the same sutta is browsed on the CST spine, under its own toc. Picking
/// between them by uid order — which is what an unranked "first Pāli row" does,
/// because `dn1/pli/cst` sorts before `dn1/pli/ms` — silently handed 1,379 SC
/// leaves the CST edition while the other 5,906 got `ms`, so browsing DN and
/// browsing SN returned different editions for no stated reason.
///
/// A non-Pāli row is the last resort and not a real choice: it is taken only
/// where the corpus has no Pāli text at all, so that the node opens *something*
/// rather than being dimmed.
fn edition_rank(uid: &str, is_pali: bool) -> u8 {
    if !is_pali {
        return 30;
    }
    match uid.rsplit('/').next() {
        Some("ms") => 0,
        Some("cst") => 10,
        _ => 20,
    }
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
/// Four maps, because they answer four different questions:
///
/// - which row a leaf **opens** — a Pāli row where there is one, since the
///   navigation tree is a route into the Pāli canon; otherwise the first row by
///   uid, which is what keeps a text that exists only in translation openable;
/// - the Pāli **title** fallback, from a Pāli row;
/// - the English **title** fallback, from an English row, preferring the
///   translators in [`english_title_rank`] order;
/// - the translated title saved on the opened row itself (`title_trans`).
///
/// The Pāli and English fallbacks must stay separate: a Pāli row's `title`
/// column holds the Pāli name, so filling the English line from the row that
/// was chosen for opening would print the same title twice on 1705 of the
/// tree's rows. `title_trans` is the column that exists to hold the other one.
struct SuttaIndex {
    sutta_uids: HashMap<String, String>,
    fallback_pali: HashMap<String, String>,
    fallback_en: HashMap<String, String>,
    own_trans: HashMap<String, String>,
}

/// Which English translation's title fills the English line when the `names`
/// collection has none: Sujato, then Brahmali, then any other. Their titles
/// follow SuttaCentral's own naming, which is what the rest of the tree uses.
fn english_title_rank(uid: &str) -> u8 {
    match uid.rsplit('/').next() {
        Some("sujato") => 0,
        Some("brahmali") => 1,
        _ => 2,
    }
}

fn load_sutta_rows(db_conn: &mut SqliteConnection) -> Result<SuttaIndex> {
    use simsapa_backend::db::appdata_schema::suttas;

    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>)> = suttas::table
        .select((
            suttas::uid,
            suttas::language,
            suttas::title_pali,
            suttas::title,
            suttas::title_trans,
        ))
        .order(suttas::uid.asc())
        .load(db_conn)
        .context("Failed to read the suttas table for navigation-tree leaf resolution")?;

    let mut uids: HashMap<String, String> = HashMap::new();
    let mut chosen_rank: HashMap<String, u8> = HashMap::new();
    let mut fallback_pali: HashMap<String, String> = HashMap::new();
    let mut fallback_en: HashMap<String, String> = HashMap::new();
    let mut en_rank: HashMap<String, u8> = HashMap::new();
    let mut trans_by_uid: HashMap<String, String> = HashMap::new();

    for (uid, language, title_pali, title, title_trans) in rows {
        if let Some(t) = title_trans.filter(|s| !s.trim().is_empty()) {
            trans_by_uid.insert(uid.clone(), t.trim().to_string());
        }
        let title_rank_en = english_title_rank(&uid);

        let base = uid.split('/').next().unwrap_or(&uid).to_string();
        let is_pali = language == "pli";

        let rank = edition_rank(&uid, is_pali);
        let better = match chosen_rank.get(&base) {
            None => true,
            Some(current) => rank < *current,
        };
        if better {
            chosen_rank.insert(base.clone(), rank);
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
                let better = en_rank
                    .get(&base)
                    .is_none_or(|current| title_rank_en < *current);
                if better {
                    en_rank.insert(base.clone(), title_rank_en);
                    fallback_en.insert(base, t);
                }
            }
        }
    }

    // Only the row a leaf actually opens contributes: a translated title on
    // some other edition of the text belongs to that edition.
    let own_trans: HashMap<String, String> = uids
        .iter()
        .filter_map(|(base, full)| trans_by_uid.get(full).map(|t| (base.clone(), t.clone())))
        .collect();

    logger::info(&format!(
        "sc_spine: {} sutta base uids indexed ({} Pāli, {} English title fallbacks, {} own translated titles)",
        uids.len(),
        fallback_pali.len(),
        fallback_en.len(),
        own_trans.len()
    ));
    Ok(SuttaIndex { sutta_uids: uids, fallback_pali, fallback_en, own_trans })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_titles_prefer_sujato_then_brahmali_then_any_other() {
        assert!(english_title_rank("dn1/en/sujato") < english_title_rank("pli-tv-bu-vb-pj1/en/brahmali"));
        assert!(english_title_rank("pli-tv-bu-vb-pj1/en/brahmali") < english_title_rank("dn1/en/bodhi"));
        assert_eq!(english_title_rank("dn1/en/bodhi"), english_title_rank("dn1/en/thanissaro"));
    }

    fn group(uid: &str, children: Vec<TreeNode>) -> TreeNode {
        let mut n = TreeNode::new(uid, GROUP);
        n.children = children;
        n
    }

    fn collection(uid: &str, children: Vec<TreeNode>) -> TreeNode {
        let mut n = TreeNode::new(uid, "collection");
        n.children = children;
        n
    }

    fn child_uids(node: &TreeNode) -> Vec<&str> {
        node.children.iter().map(|c| c.node_uid.as_str()).collect()
    }

    #[test]
    fn a_grouping_level_over_one_collection_is_elided() {
        // `sutta > long > dn`: in a Pāli-only corpus `long` has nothing to
        // distinguish, so the reader should see `sutta > dn`.
        let mut sutta = TreeNode::new("sutta", "root");
        sutta.children = vec![
            group("sutta/long", vec![collection("dn", vec![])]),
            group("sutta/middle", vec![collection("mn", vec![])]),
        ];

        collapse_redundant_groups(&mut sutta);

        assert_eq!(child_uids(&sutta), vec!["dn", "mn"]);
    }

    #[test]
    fn the_collection_survives_a_chain_of_groupings_not_the_grouping() {
        // `sutta > minor > kn`: `minor` is the redundant one and `kn` -- the
        // Khuddakanikāya, with 20 members -- is the level worth keeping. A
        // rule phrased as "elide an only child" would have taken `kn`.
        let mut sutta = TreeNode::new("sutta", "root");
        sutta.children = vec![group(
            "sutta/minor",
            vec![group(
                "sutta/minor/kn",
                vec![collection("kp", vec![]), collection("dhp", vec![])],
            )],
        )];

        collapse_redundant_groups(&mut sutta);

        assert_eq!(child_uids(&sutta), vec!["sutta/minor/kn"]);
        assert_eq!(child_uids(&sutta.children[0]), vec!["kp", "dhp"]);
    }

    #[test]
    fn a_roots_only_school_grouping_is_elided_and_the_root_kept() {
        // `vinaya > pli-tv-vi > {6 collections}`. The grouping has six
        // children so the first shape does not fire; it is redundant because
        // it is the root's only child, and the root is never the end that
        // gives way.
        let mut vinaya = TreeNode::new("vinaya", "root");
        vinaya.children = vec![group(
            "vinaya/pli-tv-vi",
            vec![collection("pli-tv-bu-vb", vec![]), collection("pli-tv-kd", vec![])],
        )];

        collapse_redundant_groups(&mut vinaya);

        assert_eq!(vinaya.node_uid, "vinaya");
        assert_eq!(child_uids(&vinaya), vec!["pli-tv-bu-vb", "pli-tv-kd"]);
    }

    fn branch(uid: &str, title: &str, children: Vec<TreeNode>) -> TreeNode {
        let mut n = TreeNode::new(uid, "branch");
        n.title_pali = Some(title.to_string());
        n.children = children;
        n
    }

    #[test]
    fn a_samyutta_over_a_single_vagga_keeps_both_levels() {
        // The real shape of `sn8`: a saṁyutta holding exactly one vagga. It
        // looks like `sutta > long > dn` and is not the same thing. `long`
        // holds one member because the Pāli-only scope deleted its siblings;
        // `sn8` holds one vagga because the Saṁyutta Nikāya has one there.
        //
        // A saṁyutta and a vagga are distinct groupings of the source with
        // distinct names, and a reader may go looking for either
        // Vaṅgīsasaṁyutta or Vaṅgīsavagga, so collapsing them into each other
        // would lose a name the corpus uses and a uid it refers to. Measured
        // over the built tree, 180 branch nodes have exactly one child; none
        // of them may be elided.
        let mut sutta = TreeNode::new("sutta", "root");
        sutta.children = vec![group(
            "sutta/linked",
            vec![collection(
                "sn",
                vec![branch(
                    "sn-sagathavaggasamyutta",
                    "Sagāthāvaggasaṁyutta",
                    vec![branch(
                        "sn8",
                        "Vaṅgīsasaṁyutta",
                        vec![branch(
                            "sn8-vangisavagga",
                            "Vaṅgīsavagga",
                            vec![TreeNode::new("sn8.1", "sutta")],
                        )],
                    )],
                )],
            )],
        )];

        collapse_redundant_groups(&mut sutta);

        // Only the grouping level goes.
        assert_eq!(child_uids(&sutta), vec!["sn"]);

        let sagatha = &sutta.children[0].children[0];
        assert_eq!(sagatha.node_uid, "sn-sagathavaggasamyutta");
        let sn8 = &sagatha.children[0];
        assert_eq!(sn8.title_pali.as_deref(), Some("Vaṅgīsasaṁyutta"));
        assert_eq!(child_uids(sn8), vec!["sn8-vangisavagga"]);
        assert_eq!(
            sn8.children[0].title_pali.as_deref(),
            Some("Vaṅgīsavagga")
        );
    }

    #[test]
    fn a_roots_single_non_grouping_child_is_kept() {
        // Shape 2 turns on the child being a grouping level. A root left with
        // one collection is a corpus that has one collection, not a level to
        // skip -- splicing it away would put its vaggas straight under the
        // piṭaka and lose the nikāya's name.
        let mut abhidhamma = TreeNode::new("abhidhamma", "root");
        abhidhamma.children = vec![collection("ds", vec![branch("ds1", "Cittuppādakaṇḍa", vec![])])];

        collapse_redundant_groups(&mut abhidhamma);

        assert_eq!(child_uids(&abhidhamma), vec!["ds"]);
        assert_eq!(child_uids(&abhidhamma.children[0]), vec!["ds1"]);
    }

    fn builder_with(parallels: &[(&str, &[&str])], corpus: &[&str]) -> ScSpineBuilder {
        let mut b = ScSpineBuilder::for_test();
        for (uid, members) in parallels {
            b.parallels.insert(
                uid.to_string(),
                members.iter().map(|m| m.to_string()).collect(),
            );
        }
        for uid in corpus {
            b.sutta_uids
                .insert(uid.to_string(), format!("{}/pli/ms", uid));
        }
        b.ranged_rows = index_ranged_rows(&b.sutta_uids);
        b
    }

    /// The four Pāli members every vinaya parallels group carries: the
    /// bhikkhu pair and the bhikkhunī pair of the corresponding rule.
    const SS_GROUP: &[&str] = &[
        "pli-tv-bu-vb-ss5",
        "pli-tv-bu-pm-ss5",
        "pli-tv-bi-vb-ss7",
        "pli-tv-bi-pm-ss7",
    ];

    #[test]
    fn a_patimokkha_rule_opens_its_own_divisions_vibhanga_text() {
        let b = builder_with(
            &[("pli-tv-bi-pm-ss7", SS_GROUP)],
            &["pli-tv-bi-vb-ss7", "pli-tv-bu-vb-ss5"],
        );

        match b.parallel_redirect("pli-tv-bi-pm-ss7") {
            ParallelRedirect::To { base_uid, sutta_uid } => {
                assert_eq!(base_uid, "pli-tv-bi-vb-ss7");
                assert_eq!(sutta_uid, "pli-tv-bi-vb-ss7/pli/ms");
            }
            _ => panic!("expected a redirect"),
        }
    }

    #[test]
    fn the_own_division_text_wins_over_the_other_divisions() {
        // Both are in the group and both are in the corpus. The nun's own
        // rule is the one to open; the monk's is the fallback for when hers
        // is absent, never a competitor when she has one.
        let b = builder_with(
            &[("pli-tv-bi-pm-ss7", SS_GROUP)],
            &["pli-tv-bi-vb-ss7", "pli-tv-bu-vb-ss5"],
        );
        match b.parallel_redirect("pli-tv-bi-pm-ss7") {
            ParallelRedirect::To { base_uid, .. } => assert_eq!(base_uid, "pli-tv-bi-vb-ss7"),
            _ => panic!("expected the same-division text"),
        }
    }

    #[test]
    fn the_other_divisions_rule_is_the_fallback_when_the_own_one_is_absent() {
        // SuttaCentral's own page for `pli-tv-bi-pm-ss7` lists
        // `pli-tv-bu-vb-ss5` as the only openable Pāli text, because the nuns'
        // Vibhaṅga says the rule is the monks'. All 167 such redirects in the
        // corpus have a Pāli title **identical** to their target's
        // (Sañcaritta → Sañcaritta), which is the independent check that the
        // group names the same rule rather than a related one.
        let b = builder_with(&[("pli-tv-bi-pm-ss7", SS_GROUP)], &["pli-tv-bu-vb-ss5"]);
        match b.parallel_redirect("pli-tv-bi-pm-ss7") {
            ParallelRedirect::To { base_uid, .. } => assert_eq!(base_uid, "pli-tv-bu-vb-ss5"),
            _ => panic!("expected the cross-division fallback"),
        }
    }

    #[test]
    fn a_whole_chapter_is_not_a_cross_division_candidate() {
        // The groups also name whole Khandhaka chapters, where a rule's story
        // is told. `pli-tv-kd22` has no division, and admitting it left five
        // leaves ambiguous between it and the rule that is actually wanted.
        let b = builder_with(
            &[(
                "pli-tv-bi-pm-np21",
                &["pli-tv-bu-vb-np18", "pli-tv-kd22"],
            )],
            &["pli-tv-bu-vb-np18", "pli-tv-kd22"],
        );
        match b.parallel_redirect("pli-tv-bi-pm-np21") {
            ParallelRedirect::To { base_uid, .. } => assert_eq!(base_uid, "pli-tv-bu-vb-np18"),
            _ => panic!("expected the Vibhaṅga rule, not the Khandhaka chapter"),
        }
    }

    #[test]
    fn a_target_stored_as_part_of_a_ranged_row_is_found() {
        // The group names `pli-tv-bi-vb-pj1`; the corpus stores the four
        // pārājikas the nuns share with the monks as one document. Reusing
        // `sutta_range_from_ref` is what finds it -- and it must win over the
        // cross-division monk's rule, which is also in the group.
        let b = builder_with(
            &[(
                "pli-tv-bi-pm-pj1",
                &["pli-tv-bi-vb-pj1", "pli-tv-bu-vb-pj1"],
            )],
            &["pli-tv-bi-vb-pj1-4", "pli-tv-bu-vb-pj1"],
        );
        match b.parallel_redirect("pli-tv-bi-pm-pj1") {
            ParallelRedirect::To { base_uid, .. } => assert_eq!(base_uid, "pli-tv-bi-vb-pj1-4"),
            _ => panic!("expected the ranged same-division row"),
        }
    }

    #[test]
    fn a_range_that_does_not_contain_the_rule_is_not_a_cover() {
        let b = builder_with(
            &[("pli-tv-bi-pm-pj9", &["pli-tv-bi-vb-pj9"])],
            &["pli-tv-bi-vb-pj1-4"],
        );
        assert!(matches!(
            b.parallel_redirect("pli-tv-bi-pm-pj9"),
            ParallelRedirect::None
        ));
    }

    #[test]
    fn a_parallel_with_no_text_in_the_corpus_is_not_a_redirect() {
        let b = builder_with(&[("pli-tv-bi-pm-ss7", SS_GROUP)], &[]);
        assert!(matches!(
            b.parallel_redirect("pli-tv-bi-pm-ss7"),
            ParallelRedirect::None
        ));
    }

    #[test]
    fn the_rule_designator_breaks_a_tie_between_two_candidates() {
        // The real `pli-tv-bu-pm-pj4` case: its group also names `vb-pc8`,
        // and both are in the corpus.
        let b = builder_with(
            &[(
                "pli-tv-bu-pm-pj4",
                &["pli-tv-bu-vb-pj4", "pli-tv-bu-vb-pc8"],
            )],
            &["pli-tv-bu-vb-pj4", "pli-tv-bu-vb-pc8"],
        );

        match b.parallel_redirect("pli-tv-bu-pm-pj4") {
            ParallelRedirect::To { base_uid, .. } => assert_eq!(base_uid, "pli-tv-bu-vb-pj4"),
            _ => panic!("expected the tie-break to pick pj4"),
        }
    }

    #[test]
    fn a_tie_the_designator_cannot_break_is_reported_not_guessed() {
        let b = builder_with(
            &[(
                "pli-tv-bu-pm-pj4",
                &["pli-tv-bu-vb-pc8", "pli-tv-bu-vb-pc9"],
            )],
            &["pli-tv-bu-vb-pc8", "pli-tv-bu-vb-pc9"],
        );

        match b.parallel_redirect("pli-tv-bu-pm-pj4") {
            ParallelRedirect::Ambiguous(c) => assert_eq!(c.len(), 2),
            _ => panic!("expected the ambiguity to be reported"),
        }
    }

    #[test]
    fn the_mechanism_cannot_reach_outside_the_theravada_vinaya() {
        // Two independent guards, and the first is the decisive one: a Pāli
        // sutta uid is `mn10`, with no `pli-tv-` prefix, so it never enters
        // the candidate pool at all. A missing sutta stays an honest dimmed
        // row instead of opening a parallel that happens to exist.
        let b = builder_with(&[("sn56.11", &["mn10", "dn22"])], &["mn10", "dn22"]);
        assert!(matches!(
            b.parallel_redirect("sn56.11"),
            ParallelRedirect::None
        ));

        // The second: `division()` is None below four hyphen segments, which
        // is what keeps the cross-division fallback to rule-shaped uids.
        assert_eq!(division("sn56.11"), None);
        assert_eq!(division("pli-tv-kd22"), None);
        assert_eq!(division("pli-tv-bi-pm-ss7"), Some(("pli", "tv", "bi")));
    }

    #[test]
    fn parallel_members_are_normalized_and_prose_labels_dropped() {
        assert_eq!(parallel_uid("~mn10"), Some("mn10"));
        assert_eq!(parallel_uid("sn1.20#28.1"), Some("sn1.20"));
        assert_eq!(parallel_uid("~pli-tv-bu-vb-pj1#3.2"), Some("pli-tv-bu-vb-pj1"));
        // A prose label, not a uid -- looking it up would find nothing, but
        // dropping it here keeps it out of the candidate set entirely.
        assert_eq!(parallel_uid("Bhikkhu Parivāra Pārājika 1"), None);
    }

    /// A rule that SuttaCentral names, in a group with its bhikkhu counterpart.
    fn nun_rule(n: u32, monk: &str) -> (String, Vec<String>) {
        let uid = format!("pli-tv-bi-vb-ss{}", n);
        let members = vec![uid.clone(), monk.to_string()];
        (uid, members)
    }

    fn builder_with_rules(rules: &[(u32, &str)], corpus: &[&str]) -> ScSpineBuilder {
        let mut b = ScSpineBuilder::for_test();
        for (n, monk) in rules {
            let (uid, members) = nun_rule(*n, monk);
            b.parallels.insert(uid, members);
        }
        for uid in corpus {
            b.sutta_uids
                .insert(uid.to_string(), format!("{}/pli/ms", uid));
        }
        b.ranged_rows = index_ranged_rows(&b.sutta_uids);
        b
    }

    #[test]
    fn a_ranged_leaf_with_no_text_becomes_a_branch_over_its_rules() {
        // `pli-tv-bi-vb-ss7-9` stands for three rules because the nuns' text
        // says only that they are the monks'. Each rule resolves on its own,
        // so the range becomes a branch and nothing needs a special case.
        let mut b = builder_with_rules(
            &[(7, "pli-tv-bu-vb-ss5"), (8, "pli-tv-bu-vb-ss8"), (9, "pli-tv-bu-vb-ss9")],
            &["pli-tv-bu-vb-ss5", "pli-tv-bu-vb-ss8", "pli-tv-bu-vb-ss9"],
        );

        let node = b.make_leaf("pli-tv-bi-vb-ss7-9");

        assert_eq!(node.node_type, "branch");
        assert!(node.is_available, "a branch over real rules is not dimmed");
        assert_eq!(child_uids(&node), vec![
            "pli-tv-bi-vb-ss7",
            "pli-tv-bi-vb-ss8",
            "pli-tv-bi-vb-ss9",
        ]);
        assert_eq!(
            node.children[0].sutta_uid.as_deref(),
            Some("pli-tv-bu-vb-ss5/pli/ms")
        );
        assert_eq!(
            node.children[0].redirect_uid.as_deref(),
            Some("pli-tv-bu-vb-ss5")
        );
        assert_eq!(b.range_expansion_count(), 1);
        assert!(b.defects.is_empty());
    }

    #[test]
    fn a_ranged_leaf_with_its_own_text_is_never_expanded() {
        // The SC spine holds 261 ranged leaves the corpus stores under that
        // very uid -- `sn30.7-16`, `an1.394-574`, `dhp1-20`. Expansion is the
        // last arm after "has its own text", so none of them reaches it.
        let mut b = builder_with_rules(&[], &["sn30.7-16"]);
        let node = b.make_leaf("sn30.7-16");

        assert!(node.children.is_empty());
        assert_eq!(node.node_type, "sutta");
        assert_eq!(node.sutta_uid.as_deref(), Some("sn30.7-16/pli/ms"));
        assert_eq!(b.range_expansion_count(), 0);
    }

    #[test]
    fn a_range_whose_rules_suttacentral_does_not_all_name_is_left_alone() {
        // The numbers come from the range the source wrote, but a uid built
        // that way is still a construction. Rule 8 is not in parallels.json,
        // so the whole range is reported and stays a dimmed leaf rather than
        // sprouting a node SuttaCentral has never heard of.
        let mut b = builder_with_rules(
            &[(7, "pli-tv-bu-vb-ss5"), (9, "pli-tv-bu-vb-ss9")],
            &["pli-tv-bu-vb-ss5", "pli-tv-bu-vb-ss9"],
        );

        let node = b.make_leaf("pli-tv-bi-vb-ss7-9");

        assert!(node.children.is_empty());
        assert!(!node.is_available);
        assert_eq!(
            b.defects.count_of(
                "ranged leaf not expanded: SuttaCentral does not name every rule in it"
            ),
            1
        );
    }

    #[test]
    fn a_range_none_of_whose_rules_has_a_text_is_left_alone() {
        // One honest dimmed leaf beats a branch of three dimmed ones. The
        // check runs before the children are built, so the range records one
        // defect against itself rather than three against rows thrown away.
        let mut b = builder_with_rules(
            &[(7, "pli-tv-bu-vb-ss5"), (8, "pli-tv-bu-vb-ss8"), (9, "pli-tv-bu-vb-ss9")],
            &[],
        );

        let node = b.make_leaf("pli-tv-bi-vb-ss7-9");

        assert!(node.children.is_empty());
        assert!(!node.is_available);
        assert_eq!(
            b.defects.count_of("ranged leaf not expanded: no rule in it has a text"),
            1
        );
        assert_eq!(
            b.defects.count_of("leaf has no text and no Pāli parallel"),
            1,
            "the range itself is still reported as textless"
        );
    }

    #[test]
    fn expansion_terminates_on_a_single_rule() {
        // `make_leaf` recurses into `expand_range_leaf` for each rule, so a
        // single-number uid must not expand again. `start == end` returns on
        // the first check.
        let mut b = builder_with_rules(&[(7, "pli-tv-bu-vb-ss5")], &[]);
        let node = b.make_leaf("pli-tv-bi-vb-ss7");
        assert!(node.children.is_empty());
        assert_eq!(b.range_expansion_count(), 0);
    }

    #[test]
    fn only_grouping_levels_are_elided() {
        // The same rule one level down, exercised on the pass directly: a
        // vagga with a single sutta in it is a real level of the source, not
        // an artefact of the scope.
        let mut sn = TreeNode::new("sn", "collection");
        sn.children = vec![collection("sn56", vec![TreeNode::new("sn56.11", "sutta")])];

        collapse_pass_through_groups(&mut sn);

        assert_eq!(child_uids(&sn), vec!["sn56"]);
        assert_eq!(child_uids(&sn.children[0]), vec!["sn56.11"]);
    }

    #[test]
    fn a_grouping_with_several_members_stays() {
        // The guarantee that widening SC_SCOPE undoes the collapse by itself:
        // as soon as `long` holds a second collection it carries information
        // again and must reappear as a level.
        let mut sutta = TreeNode::new("sutta", "root");
        sutta.children = vec![
            group("sutta/long", vec![collection("dn", vec![]), collection("da", vec![])]),
            group("sutta/middle", vec![collection("mn", vec![])]),
        ];

        collapse_redundant_groups(&mut sutta);

        assert_eq!(child_uids(&sutta), vec!["sutta/long", "mn"]);
    }

    #[test]
    fn the_mahasangiti_edition_wins_where_several_exist() {
        // Lower wins. Without a rank the winner is whichever uid sorts first,
        // and `dn1/pli/cst` sorts before `dn1/pli/ms` -- which silently handed
        // 1,379 leaves the CST edition while the rest got `ms`, so browsing DN
        // and browsing SN returned different editions for no stated reason.
        assert!(edition_rank("dn1/pli/ms", true) < edition_rank("dn1/pli/cst", true));
        assert!(edition_rank("dn1/pli/cst", true) < edition_rank("dn1/pli/other", true));
        // A non-Pāli row is the last resort, not a choice: it is taken only
        // where the corpus has no Pāli text at all, so the node opens something
        // rather than being dimmed.
        assert!(edition_rank("dn1/pli/other", true) < edition_rank("dn1/en/sujato", false));
        // A uid with no source segment must not outrank a real edition.
        assert!(edition_rank("dn1/pli/ms", true) < edition_rank("dn1", true));
    }
}
