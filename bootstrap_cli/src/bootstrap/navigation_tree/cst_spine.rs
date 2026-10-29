//! The CST-derived navigation spine (`tree_source = 'cst'`).
//!
//! The structure comes from the tipitaka.org table-of-contents files in
//! `tipitaka-org-vri-cst/tipitaka-xml/tipitaka.org/romn/`:
//!
//! | File | Root |
//! |---|---|
//! | `toc1.xml` | Mūla — **cross-check only**, never a fourth root: the Mūla texts are browsed on the SuttaCentral spine |
//! | `toc2.xml` | Aṭṭhakathā |
//! | `toc3.xml` | Ṭīkā |
//! | `toc4.xml` | Añña |
//! | `cscd/<book>.toc.xml` | the inner level of one book |
//!
//! A `<tree text="…" src="…">` is a branch continued in the named file; a
//! `<tree text="…" action="cscd/s0305m.mul0.xml">` is a leaf naming one
//! division of a source file. The `cscd` tocs are flat: one root `<tree>` with
//! leaves under it, measured across all 217 of them.
//!
//! ## Two different `romn/` directories
//!
//! - `tipitaka-xml/romn/` holds the 217 **source** XML files, and is what
//!   `TipitakaXmlImporter` already reads.
//! - `tipitaka-xml/tipitaka.org/romn/` holds only `toc1`–`toc4` plus
//!   `tipitaka_toc.xml`, with the 217 `*.toc.xml` under its `cscd/`.
//!
//! The pairing between the two is 1:1, which is what
//! [`CstSpineBuilder::cross_check_sources`] verifies.
//!
//! ## Encoding
//!
//! Every file is read through `tipitaka_xml_parser::encoding::read_xml_file`,
//! which detects by BOM. This matters: `toc1`–`toc4` are UTF-8 with a BOM
//! **although their XML declaration says `encoding="UTF-16"`**, while the
//! `cscd` tocs really are UTF-16. Decoding either by its declaration produces
//! mojibake, which is why there is one decoder and no second one.
//!
//! ## Node uids
//!
//! Stable across bootstraps — the UI persists expansion state and breadcrumbs
//! by them — and derived from source identifiers only, never from a row id or
//! an enumeration counter:
//!
//! - roots: fixed (`cst:atthakatha`, `cst:tika`, `cst:anna`);
//! - a grouping node, which has a title and nothing else to identify it: its
//!   parent's uid plus a slug of that title (`cst:atthakatha/suttapitaka-atthakatha`).
//!   Sibling titles were measured unique across all four toc files; a
//!   collision is reported as a defect rather than disambiguated by position;
//! - a `src=` branch: its toc file's stem (`cst:s0101a.att`);
//! - a leaf: its division file name (`cst:s0305m.mul0.xml`), which is unique
//!   across all 2698 leaves and is also what the division record's uid is
//!   built from (`s0305m.mul0.xml/pli/cst`).
//!
//! ## Level labels
//!
//! Where a grouping node's title names a nikāya,
//! `NikayaStructure::from_nikaya_name` supplies that book's level list and each
//! deeper node takes the next entry, clamped at the last. Measured against the
//! real tocs this lands correctly: under Dīgha a leaf is a `Sutta`, under
//! Majjhima a `Vagga`, under Saṃyutta a `Samyutta` — which is what those
//! leaves are. Outside a nikāya (Vinaya, Abhidhamma, Añña) the generic labels
//! are used. The CST grouping is **never** normalized toward the SC spine's.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use diesel::prelude::*;
use quick_xml::events::Event;
use quick_xml::Reader;

use simsapa_backend::helpers::consistent_niggahita;
use simsapa_backend::logger;
use simsapa_backend::navigation_tree;
use tipitaka_xml_parser::encoding::read_xml_file;
use tipitaka_xml_parser::nikaya_structure::NikayaStructure;
use tipitaka_xml_parser::types::GroupType;

use crate::bootstrap::cst_coverage::{coverage_of, CoverageClass, FileCoverage};
use crate::bootstrap::tipitaka_xml::{fragment_title, fragment_uid};

use super::{DefectReport, TreeNode};

pub const TREE_SOURCE: &str = "cst";

/// A root of the CST spine: which toc file defines it, and how it is shown.
pub struct CstRoot {
    pub toc_file: &'static str,
    pub node_uid: &'static str,
    pub title: &'static str,
}

/// The three CST roots that are built.
///
/// `toc1.xml` (Mūla) is deliberately absent: those texts are browsed on the SC
/// spine, and a fourth root would be a second path to the same material. It is
/// still read, for [`CstSpineBuilder::cross_check_sources`].
pub const CST_ROOTS: &[CstRoot] = &[
    CstRoot { toc_file: "toc2.xml", node_uid: "cst:atthakatha", title: "Aṭṭhakathā" },
    CstRoot { toc_file: "toc3.xml", node_uid: "cst:tika", title: "Ṭīkā" },
    CstRoot { toc_file: "toc4.xml", node_uid: "cst:anna", title: "Añña" },
];

/// The Mūla toc, read for the cross-check only.
pub const MULA_TOC: &str = "toc1.xml";

/// One `<tree>` element of a toc file.
#[derive(Debug, Default)]
struct TocElement {
    text: Option<String>,
    src: Option<String>,
    action: Option<String>,
    children: Vec<TocElement>,
}

impl TocElement {
    fn is_leaf(&self) -> bool {
        self.action.is_some()
    }
}

pub struct CstSpineBuilder {
    /// `tipitaka.org/romn/`, holding `toc1`–`toc4` and `cscd/`.
    site_dir: PathBuf,
    /// `romn/`, holding the 217 source XML files.
    source_dir: PathBuf,
    /// Base uid of an imported CST row (`s0305m.mul0.xml`, `s0305m.mul.xml`)
    /// → the full `suttas.uid` a leaf opens.
    cst_uids: HashMap<String, String>,
    /// `src=` targets seen, for the 1:1 cross-check against `romn/`.
    reachable_sources: HashSet<String>,
    /// Which records each source file got, so a fully mapped file's toc leaves
    /// become branches over its fragments rather than leaves over divisions
    /// that were deliberately not written.
    coverage: HashMap<String, FileCoverage>,
    /// Source file → its mapped Sutta fragments, in source order.
    fragments: HashMap<String, Vec<FragmentInfo>>,
    /// Toc leaves seen, and the ones that reached neither a record nor a
    /// populated branch -- the report's headline number.
    pub toc_leaves_total: usize,
    pub unassigned_leaves: Vec<String>,
    /// Toc leaves opening the adjacent record because their section is merged
    /// into it. Expected, counted rather than reported as a defect.
    pub merged_leaves: Vec<String>,
    pub defects: DefectReport,
}

/// One mapped fragment, as the tree needs it: enough to make a row, and the
/// titles that decide which toc leaf it falls under.
#[derive(Debug, Clone)]
struct FragmentInfo {
    /// The `suttas.uid` the import wrote, e.g. `mn11.att/pli/cst`.
    uid: String,
    title: String,
    /// The fragment's own title plus its group levels' titles, normalized --
    /// the set a toc leaf's title is matched against.
    match_titles: HashSet<String>,
}

impl CstSpineBuilder {
    pub fn new(
        site_dir: &Path,
        source_dir: &Path,
        fragments_db_path: &Path,
        db_conn: &mut SqliteConnection,
    ) -> Result<Self> {
        let cst_uids = load_cst_rows(db_conn)?;
        let (coverage, fragments) = load_fragments(fragments_db_path)?;
        Ok(Self {
            site_dir: site_dir.to_path_buf(),
            source_dir: source_dir.to_path_buf(),
            cst_uids,
            reachable_sources: HashSet::new(),
            coverage,
            fragments,
            toc_leaves_total: 0,
            unassigned_leaves: Vec::new(),
            merged_leaves: Vec::new(),
            defects: DefectReport::default(),
        })
    }

    /// Build the three CST roots, in each toc file's own order.
    pub fn build(&mut self) -> Result<Vec<TreeNode>> {
        let mut roots = Vec::new();

        for root in CST_ROOTS {
            let path = self.site_dir.join(root.toc_file);
            let Some(element) = self.read_toc(&path) else {
                self.defects.record("root toc file missing or unparsable", root.toc_file);
                continue;
            };

            let mut node = TreeNode::new(root.node_uid, "root");
            node.title_pali = Some(root.title.to_string());
            node.children = self.build_children(&element, root.node_uid, None, 0);
            roots.push(node);
        }

        // Read the Mūla toc for its `src=` targets only, so the cross-check
        // covers all 217 files rather than the 156 the three built roots reach.
        let mula_path = self.site_dir.join(MULA_TOC);
        match self.read_toc(&mula_path) {
            Some(element) => self.collect_sources(&element),
            None => self.defects.record("root toc file missing or unparsable", MULA_TOC),
        }

        self.cross_check_sources();

        Ok(roots)
    }

    /// The children of one toc element.
    ///
    /// `nikaya` is the level list inherited from the nearest ancestor whose
    /// title names a nikāya, and `depth` is the level index these children sit
    /// at — reset to 0 by a node that starts a nikāya, and otherwise one more
    /// than their parent's, whether or not a nikāya is in scope.
    fn build_children(
        &mut self,
        element: &TocElement,
        parent_uid: &str,
        nikaya: Option<&NikayaStructure>,
        depth: usize,
    ) -> Vec<TreeNode> {
        let mut out = Vec::new();
        let mut used_uids: HashSet<String> = HashSet::new();

        for child in &element.children {
            let title = child.text.clone().unwrap_or_default();

            // A title of its own may start a nikāya's level list; otherwise the
            // ancestor's list continues one level deeper.
            let own = NikayaStructure::normalize_name(&title)
                .and_then(|n| NikayaStructure::from_nikaya_name(&n));
            let (structure, child_depth) = match (&own, nikaya) {
                (Some(s), _) => (Some(s.clone()), 0usize),
                (None, Some(s)) => (Some(s.clone()), depth),
                (None, None) => (None, depth),
            };

            // The four root tocs carry no `action=` leaves -- measured, all
            // 254 of their `<tree>` elements are either a grouping node or a
            // `src=` branch. One appearing here would be a structure this
            // walker does not model, so it is reported rather than guessed at.
            let node = if child.is_leaf() {
                self.defects
                    .record("root toc has an action= leaf, which is not modelled", title.clone());
                None
            } else if let Some(src) = &child.src {
                self.make_src_branch(child, src, structure.as_ref(), child_depth)
            } else {
                self.make_group(child, parent_uid, structure.as_ref(), child_depth)
            };

            let Some(mut node) = node else { continue };

            if !used_uids.insert(node.node_uid.clone()) {
                self.defects.record("duplicate node uid among toc siblings", &node.node_uid);
                continue;
            }

            // A grouping node's children come from the same file; a `src=`
            // branch's were already filled in by `make_src_branch`.
            if !child.is_leaf() && child.src.is_none() {
                node.children =
                    self.build_children(child, &node.node_uid, structure.as_ref(), child_depth + 1);
            }

            out.push(node);
        }

        out
    }

    /// A `<tree text="…">` with element children and no `src=`.
    fn make_group(
        &mut self,
        element: &TocElement,
        parent_uid: &str,
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> Option<TreeNode> {
        let Some(title) = element.text.as_ref().filter(|t| !t.trim().is_empty()) else {
            self.defects.record("toc grouping node has no text", parent_uid.to_string());
            return None;
        };

        let node_uid = format!("{}/{}", parent_uid, slug(title));
        let mut node = TreeNode::new(node_uid, level_label(structure, title, depth, false));
        node.title_pali = Some(title.trim().to_string());
        Some(node)
    }

    /// A `<tree text="…" src="./cscd/s0101a.att.toc.xml">` — a branch whose
    /// children are the leaves of that file.
    fn make_src_branch(
        &mut self,
        element: &TocElement,
        src: &str,
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> Option<TreeNode> {
        let toc_name = file_name_of(src);
        let stem = toc_name.strip_suffix(".toc.xml").unwrap_or(&toc_name).to_string();
        // A toc's own name is the source file's name: `s0101a.att.toc.xml`
        // continues `romn/s0101a.att.xml`.
        let source_file = format!("{}.xml", stem);
        self.reachable_sources.insert(source_file.clone());

        let path = self.site_dir.join("cscd").join(&toc_name);
        let Some(inner) = self.read_toc(&path) else {
            self.defects.record("toc src= names a missing or unparsable file", toc_name);
            return None;
        };

        let title = element.text.as_deref().unwrap_or_default().trim().to_string();
        let mut node =
            TreeNode::new(format!("cst:{}", stem), level_label(structure, &title, depth, false));
        node.title_pali = (!title.is_empty()).then_some(title);
        node.cst_file = Some(source_file.clone());

        let mut leaves = Vec::new();
        collect_leaves(&inner, &mut leaves);
        node.children = self.build_file_children(&source_file, &leaves, structure, depth + 1);

        Some(node)
    }

    /// The rows under one source file's branch, decided by its coverage class.
    ///
    /// | Class | Rows |
    /// |---|---|
    /// | fully mapped | one **branch** per toc leaf, over the fragments that fall under it |
    /// | partially mapped | the division rows, each followed by the fragment rows that fall under it -- **siblings**, never nested |
    /// | unmapped | the division rows |
    /// | unsplittable | one row for the whole file |
    ///
    /// The class is read from the fragments, but the *unsplittable* case is
    /// detected from the database instead: the import assigns it only when a
    /// split fails, and a file whose divisions are all absent while its
    /// whole-file record is present is exactly that -- which also keeps this
    /// right when the spine is rebuilt on its own.
    fn build_file_children(
        &mut self,
        source_file: &str,
        leaves: &[TocLeaf],
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> Vec<TreeNode> {
        self.toc_leaves_total += leaves.len();

        let class = coverage_of(&self.coverage, source_file).class;

        if class == CoverageClass::FullyMapped {
            return self.build_fragment_branches(source_file, leaves, structure, depth);
        }

        // The whole-file fallback: no division of this file was imported, but
        // the file itself was.
        let any_division = leaves
            .iter()
            .any(|leaf| self.cst_uids.contains_key(&leaf.division_file));
        if !any_division && self.cst_uids.contains_key(source_file) {
            self.defects
                .record("file imported whole because the split failed", source_file.to_string());
            let mut node = TreeNode::new(
                format!("cst:{}", source_file),
                level_label(structure, "", depth, true),
            );
            node.title_pali = Some(source_file.trim_end_matches(".xml").to_string());
            node.cst_file = Some(source_file.to_string());
            node.sutta_uid = self.cst_uids.get(source_file).cloned();
            return vec![node];
        }

        let fragments = if class == CoverageClass::PartiallyMapped {
            self.assign_fragments(source_file, leaves)
        } else {
            vec![Vec::new(); leaves.len()]
        };

        let mut out = Vec::new();
        for (index, leaf) in leaves.iter().enumerate() {
            out.push(self.make_division(leaf, source_file, structure, depth));
            // A mapped fragment of a partially mapped file is a sibling of the
            // division it falls in, in source order -- not a second line on the
            // division's row, and not nested inside it. Both are openable.
            for fragment in &fragments[index] {
                out.push(self.make_fragment_row(fragment, source_file, structure, depth));
            }
        }

        out
    }

    /// A fully mapped file's toc leaves become branches over its fragments.
    ///
    /// This is what keeps the finest granularity available without storing the
    /// text twice: the file has no division records, because its fragments
    /// already cover it.
    fn build_fragment_branches(
        &mut self,
        source_file: &str,
        leaves: &[TocLeaf],
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> Vec<TreeNode> {
        let assigned = self.assign_fragments(source_file, leaves);

        let mut out = Vec::new();
        for (index, (leaf, fragments)) in leaves.iter().zip(&assigned).enumerate() {
            let mut node = TreeNode::new(
                format!("cst:{}", leaf.division_file),
                level_label(structure, &leaf.title, depth, fragments.is_empty()),
            );
            node.title_pali = (!leaf.title.is_empty()).then(|| leaf.title.clone());
            node.cst_file = Some(source_file.to_string());

            if fragments.is_empty() {
                // A preamble or postamble section. SuttaCentral has no uid for
                // one, so the parser merges it into the adjacent sutta record --
                // correctly, and by design. The toc still lists it, so the leaf
                // opens the record that carries its text: the next one for a
                // preamble, the previous for a postamble. It is a real entry
                // point, not a defect, and not an empty branch.
                match adjacent_record(&assigned, index) {
                    Some(uid) => {
                        node.sutta_uid = Some(uid);
                        self.merged_leaves
                            .push(format!("{} ({})", leaf.division_file, source_file));
                    }
                    None => {
                        // No fragment anywhere in the file: nothing to merge
                        // into, which is a real gap.
                        node.is_available = false;
                        self.defects.record(
                            "toc leaf matches no fragment and the file has none",
                            format!("{}: {}", source_file, leaf.title),
                        );
                        self.unassigned_leaves
                            .push(format!("{} ({})", leaf.division_file, source_file));
                    }
                }
            } else if let [only] = fragments.as_slice()
                && titles_are_equivalent(&leaf.title, &only.title)
            {
                // One fragment, and it is *the same text under the same name*
                // as the toc leaf. The child would carry an identical title and
                // an identical level label, adding nothing but a tap: the row
                // would say "1. Mahāpadānasuttavaṇṇanā", and inside it would be
                // one row also saying "1. Mahāpadānasuttavaṇṇanā".
                //
                // So the leaf opens the fragment directly. **No source level is
                // skipped by this** -- the level being dropped is not in the CST
                // toc at all, it is one the fragment join above invents, and a
                // one-element group is not a level. A leaf whose single
                // fragment has a *different* name is a real containment
                // (`4. Okkantasaṁyuttaṁ` over `1-10. Cakkhusuttādivaṇṇanā`) and
                // keeps its branch.
                node.node_type = level_label(structure, &leaf.title, depth, true);
                node.sutta_uid = Some(only.uid.clone());
            } else {
                node.children = fragments
                    .iter()
                    .map(|fragment| self.make_fragment_row(fragment, source_file, structure, depth + 1))
                    .collect();
            }

            out.push(node);
        }

        out
    }

    /// Group a file's mapped fragments by the toc leaf they fall under.
    ///
    /// The join is on the **titles**: a fragment belongs to the leaf whose
    /// title is one of its own or of its group levels'. Verified exact for
    /// `s0305m.mul.xml`, whose twelve toc leaf titles are byte-identical and in
    /// the same order to the distinct group-level titles of its fragments.
    /// A fragment matching no leaf falls to the previous matched leaf, which
    /// preserves source order, and is reported.
    fn assign_fragments(&mut self, source_file: &str, leaves: &[TocLeaf]) -> Vec<Vec<FragmentInfo>> {
        let mut out: Vec<Vec<FragmentInfo>> = vec![Vec::new(); leaves.len()];
        let Some(fragments) = self.fragments.get(source_file).cloned() else {
            return out;
        };

        let titles: HashMap<String, usize> = leaves
            .iter()
            .enumerate()
            .map(|(index, leaf)| (normalize_title(&leaf.title), index))
            .collect();

        let mut current: Option<usize> = None;
        for fragment in fragments {
            let matched = fragment.match_titles.iter().find_map(|t| titles.get(t).copied());
            match matched.or(current) {
                Some(index) => {
                    current = Some(index);
                    out[index].push(fragment);
                }
                None => self.defects.record(
                    "fragment falls under no toc leaf",
                    format!("{}: {}", source_file, fragment.uid),
                ),
            }
        }

        out
    }

    /// One division row: a toc leaf of a file that was split.
    fn make_division(
        &mut self,
        leaf: &TocLeaf,
        source_file: &str,
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> TreeNode {
        let mut node = TreeNode::new(
            format!("cst:{}", leaf.division_file),
            level_label(structure, &leaf.title, depth, true),
        );
        node.title_pali = (!leaf.title.is_empty()).then(|| leaf.title.clone());
        node.cst_file = Some(source_file.to_string());

        match self.cst_uids.get(&leaf.division_file) {
            Some(full_uid) => node.sutta_uid = Some(full_uid.clone()),
            None => {
                node.is_available = false;
                self.defects
                    .record("toc leaf has no division record in the database", leaf.division_file.clone());
                self.unassigned_leaves
                    .push(format!("{} ({})", leaf.division_file, source_file));
            }
        }

        node
    }

    /// One mapped-fragment row. `cst_file` carries the file it is a portion of,
    /// which is what lets the window render the subdued "in s0201a.att.xml"
    /// affordance so it is not mistaken for a duplicate of the division above.
    fn make_fragment_row(
        &mut self,
        fragment: &FragmentInfo,
        source_file: &str,
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> TreeNode {
        let mut node = TreeNode::new(
            format!("{}{}", navigation_tree::FRAGMENT_UID_PREFIX, fragment.uid),
            level_label(structure, &fragment.title, depth, true),
        );
        node.title_pali = (!fragment.title.is_empty()).then(|| fragment.title.clone());
        node.cst_file = Some(source_file.to_string());
        node.sutta_uid = Some(fragment.uid.clone());
        node
    }

    /// Record every `src=` target of a toc file without building nodes from it.
    fn collect_sources(&mut self, element: &TocElement) {
        for child in &element.children {
            if let Some(src) = &child.src {
                let toc_name = file_name_of(src);
                let stem = toc_name.strip_suffix(".toc.xml").unwrap_or(&toc_name);
                self.reachable_sources.insert(format!("{}.xml", stem));
            }
            self.collect_sources(child);
        }
    }

    /// The `romn/` source files and the toc `src=` targets must pair 1:1.
    ///
    /// Both directions are reported: a toc naming a file that is not there,
    /// and a source file no toc reaches. Neither is repaired.
    fn cross_check_sources(&mut self) {
        let mut on_disk: HashSet<String> = HashSet::new();
        match std::fs::read_dir(&self.source_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".xml") {
                        on_disk.insert(name);
                    }
                }
            }
            Err(e) => {
                self.defects.record(
                    "CST source directory unreadable",
                    format!("{}: {}", self.source_dir.display(), e),
                );
                return;
            }
        }

        for name in &self.reachable_sources {
            if !on_disk.contains(name) {
                self.defects.record("toc names a source file that is not in romn/", name);
            }
        }
        for name in &on_disk {
            if !self.reachable_sources.contains(name) {
                self.defects.record("romn/ file is not reachable from any toc", name);
            }
        }

        logger::info(&format!(
            "cst_spine: {} source files on disk, {} reachable from the toc files",
            on_disk.len(),
            self.reachable_sources.len()
        ));
    }

    /// Read and parse one toc file, or `None` if it is absent or malformed.
    fn read_toc(&mut self, path: &Path) -> Option<TocElement> {
        match path.try_exists() {
            Ok(true) => {}
            _ => return None,
        }

        let text = match read_xml_file(path) {
            Ok(text) => text,
            Err(e) => {
                self.defects
                    .record("toc file could not be decoded", format!("{}: {}", path.display(), e));
                return None;
            }
        };

        match parse_toc(&text) {
            Ok(element) => Some(element),
            Err(e) => {
                self.defects
                    .record("toc file could not be parsed", format!("{}: {}", path.display(), e));
                None
            }
        }
    }
}

/// One leaf of a `cscd` toc: the division it names and the title it shows.
#[derive(Debug, Clone)]
pub struct TocLeaf {
    /// The `text=` attribute, which is the division's displayed Pāli title.
    pub title: String,
    /// The `action=` target's file name, e.g. `s0305m.mul0.xml`. This is what
    /// the division record's uid is built from (`s0305m.mul0.xml/pli/cst`).
    pub division_file: String,
}

/// The leaves of one `cscd` toc file, in document order.
///
/// This is the CST import's view of a toc — it needs the leaves and nothing
/// else — and it shares this module's single decoder and parser rather than
/// opening the file a second way.
pub fn read_toc_leaves(path: &Path) -> Result<Vec<TocLeaf>> {
    let text = read_xml_file(path)
        .with_context(|| format!("Failed to decode {}", path.display()))?;
    let root = parse_toc(&text).with_context(|| format!("Failed to parse {}", path.display()))?;

    let mut out = Vec::new();
    collect_leaves(&root, &mut out);
    Ok(out)
}

fn collect_leaves(element: &TocElement, out: &mut Vec<TocLeaf>) {
    for child in &element.children {
        if let Some(action) = &child.action {
            let division_file = file_name_of(action);
            if !division_file.is_empty() {
                out.push(TocLeaf {
                    title: child.text.as_deref().unwrap_or_default().trim().to_string(),
                    division_file,
                });
            }
        }
        collect_leaves(child, out);
    }
}

/// Parse a toc document into its `<tree>` nesting.
///
/// Everything that is not a `<tree>` element — the XML declaration, comments,
/// whitespace — is ignored; the toc files carry no text content of their own.
fn parse_toc(text: &str) -> Result<TocElement> {
    let mut reader = Reader::from_str(text);
    reader.trim_text(true);
    reader.check_end_names(false);

    let mut root = TocElement::default();
    // Indices into a flat arena would be simpler, but the tree is three deep
    // and the files are small, so an owned stack is clearer.
    let mut stack: Vec<TocElement> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"tree" => {
                stack.push(element_from_attrs(&e)?);
            }
            Ok(Event::Empty(e)) if e.local_name().as_ref() == b"tree" => {
                let node = element_from_attrs(&e)?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root.children.push(node),
                }
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"tree" => {
                if let Some(node) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => root = node,
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(anyhow::anyhow!("XML error at {}: {}", reader.buffer_position(), e)),
        }
        buf.clear();
    }

    Ok(root)
}

fn element_from_attrs(e: &quick_xml::events::BytesStart) -> Result<TocElement> {
    let mut out = TocElement::default();
    for attr in e.attributes() {
        let attr = attr.context("Malformed attribute in a toc <tree> element")?;
        let value = attr.unescape_value().context("Unescapable toc attribute value")?.to_string();
        match attr.key.local_name().as_ref() {
            b"text" => out.text = Some(value),
            b"src" => out.src = Some(value),
            b"action" => out.action = Some(value),
            _ => {}
        }
    }
    Ok(out)
}

/// The level label for a node, in `GroupType`'s vocabulary (FR-5) — never a
/// parallel one of this module's own invention.
///
/// Three sources, in order:
///
/// 1. **The node's own title**, where it names its level in Pāli. This is
///    first because the commentary tocs are shallower than the mūla structure
///    they comment on, so a depth index alone mislabels them: the Aṅguttara
///    aṭṭhakathā's leaves are vaggas (`1. Rūpādivaggavaṇṇanā`) sitting at the
///    depth `NikayaStructure` reserves for a paṇṇāsaka, and the Dhammapada
///    aṭṭhakathā's are vaggas (`1. Yamakavaggo`) at a sutta's depth. The title
///    is the source's own word for what the node is, so trusting it is
///    reporting the data rather than reconciling it.
/// 2. **`NikayaStructure`'s level list** at this depth, where an ancestor named
///    a nikāya. Verified against the real tocs for the titles that name no
///    level (`Ganthārambhakathā` and its like).
/// 3. **Generic names**, outside a nikāya — Vinaya, Abhidhamma and the whole
///    Añña root, which `NikayaStructure` does not cover.
fn level_label(structure: Option<&NikayaStructure>, title: &str, depth: usize, is_leaf: bool) -> String {
    if let Some(group_type) = level_from_title(title) {
        return group_type_name(&group_type).to_string();
    }

    if let Some(structure) = structure {
        if !structure.levels.is_empty() {
            let index = depth.min(structure.levels.len() - 1);
            return group_type_name(&structure.levels[index]).to_string();
        }
    }

    if is_leaf {
        "division".to_string()
    } else if depth == 0 {
        "group".to_string()
    } else {
        "book".to_string()
    }
}

/// The level a Pāli title names, if it names one.
///
/// Matched on the stem so the declensional and commentarial endings all fall
/// in together — `-vaggo`, `-vaggaṃ`, `-vaggavaṇṇanā`. Most specific first:
/// `saṃyutta` and `paṇṇāsa` would otherwise be missed, and a title that names
/// two levels is named for the finer of them.
fn level_from_title(title: &str) -> Option<GroupType> {
    let t = title.to_lowercase();
    // `ṃ` and `ṁ` are both in use across the CST files and the toc text.
    let t = t.replace('ṁ', "ṃ");

    if t.contains("saṃyutt") {
        Some(GroupType::Samyutta)
    } else if t.contains("paṇṇāsa") {
        Some(GroupType::Pannasaka)
    } else if t.contains("vagg") {
        Some(GroupType::Vagga)
    } else if t.contains("nikāy") {
        Some(GroupType::Nikaya)
    } else if t.contains("sutt") {
        Some(GroupType::Sutta)
    } else {
        None
    }
}

fn group_type_name(group_type: &GroupType) -> &'static str {
    match group_type {
        GroupType::Nikaya => "nikaya",
        GroupType::Book => "book",
        GroupType::Pannasaka => "pannasaka",
        GroupType::Vagga => "vagga",
        GroupType::Samyutta => "samyutta",
        GroupType::Sutta => "sutta",
    }
}
/// Whether a toc leaf and a fragment name the same text.
///
/// Compared with the niggahita folded, because the two strings reach here from
/// different sources -- the toc's `text=` attribute and the fragment's own
/// title -- and the corpus does not spell it consistently. The rest is compared
/// verbatim: this decides whether a level is real, and a looser test (ignoring
/// the leading number, say) would collapse a genuine containment.
fn titles_are_equivalent(leaf_title: &str, fragment_title: &str) -> bool {
    let fold = |s: &str| consistent_niggahita(Some(s.trim().to_string()));
    !leaf_title.trim().is_empty() && fold(leaf_title) == fold(fragment_title)
}


/// The last path segment of a toc `src=` or `action=` value, which are written
/// as `./cscd/x.toc.xml` and `cscd/x.xml` respectively.
fn file_name_of(value: &str) -> String {
    value.rsplit('/').next().unwrap_or(value).trim().to_string()
}

/// A title reduced to a node-uid segment: lowercased, whitespace joined with
/// `-`, and anything that is neither alphanumeric nor `-` dropped. Pāli
/// diacritics are alphanumeric, so they survive and the segment stays
/// recognisable. The niggahita is folded to `ṁ`, as in the titles: the toc
/// text writes `ṃ`, and a uid spelt differently from its title is a uid the
/// context-note and override keys have to copy the wrong spelling into.
fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;

    for ch in consistent_niggahita(Some(title.trim().to_lowercase())).chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch);
        } else {
            pending_dash = true;
        }
    }

    out
}

/// Index the imported CST rows by their base uid, so a toc leaf can be
/// resolved to the record it opens.
///
/// A CST uid is `<file>/pli/cst`, and the base is the file name — which is the
/// division file name (`s0305m.mul0.xml`) for a division record and the source
/// file name (`s0305m.mul.xml`) for a whole-file one.
fn load_cst_rows(db_conn: &mut SqliteConnection) -> Result<HashMap<String, String>> {
    use simsapa_backend::db::appdata_schema::suttas;

    let rows: Vec<String> = suttas::table
        .filter(suttas::uid.like("%/cst"))
        .select(suttas::uid)
        .order(suttas::uid.asc())
        .load(db_conn)
        .context("Failed to read the CST suttas rows for navigation-tree leaf resolution")?;

    let mut out = HashMap::new();
    for uid in rows {
        let base = uid.split('/').next().unwrap_or(&uid).to_string();
        out.entry(base).or_insert(uid);
    }

    logger::info(&format!("cst_spine: {} CST base uids indexed", out.len()));
    Ok(out)
}

/// The record a leaf with no fragments of its own belongs to: the first
/// fragment of the next leaf that has any, or -- for a trailing leaf -- the
/// last fragment of the previous one.
///
/// This mirrors how the corpus is built. SuttaCentral has no uid for a
/// preamble or postamble section, so its text is merged into the first or last
/// sutta record of the file; the toc still names the section, and this is what
/// points that name at the record actually carrying it.
fn adjacent_record(assigned: &[Vec<FragmentInfo>], index: usize) -> Option<String> {
    assigned[index + 1..]
        .iter()
        .find_map(|f| f.first())
        .or_else(|| assigned[..index].iter().rev().find_map(|f| f.last()))
        .map(|fragment| fragment.uid.clone())
}

/// Normalized for the toc-leaf join: trimmed, and with the two niggahita
/// spellings folded together, because the toc and the fragments do not always
/// agree on which one they use.
fn normalize_title(title: &str) -> String {
    title.trim().replace('ṁ', "ṃ")
}

/// The coverage classification and every file's mapped fragments.
///
/// Read once here rather than per file: the fragments database is 161 MB and
/// the tree touches 217 files.
fn load_fragments(
    fragments_db_path: &Path,
) -> Result<(HashMap<String, FileCoverage>, HashMap<String, Vec<FragmentInfo>>)> {
    match fragments_db_path.try_exists() {
        Ok(true) => {}
        _ => {
            logger::warn(&format!(
                "cst_spine: fragments.sqlite3 not found, no fragment rows will be built: {}",
                fragments_db_path.display()
            ));
            return Ok((HashMap::new(), HashMap::new()));
        }
    }

    let db_url = format!("file:{}?mode=ro", fragments_db_path.display());
    let mut frag_conn = SqliteConnection::establish(&db_url)
        .with_context(|| format!("Failed to open {}", fragments_db_path.display()))?;

    let coverage = crate::bootstrap::cst_coverage::compute_coverage(&mut frag_conn)?;

    use tipitaka_xml_parser::fragments_schema::xml_fragments;
    let rows: Vec<(String, Option<String>, Option<String>, String)> = xml_fragments::table
        .filter(xml_fragments::frag_type.eq("Sutta"))
        .filter(xml_fragments::sc_code.is_not_null())
        .select((
            xml_fragments::cst_file,
            xml_fragments::sc_code,
            xml_fragments::cst_sutta,
            xml_fragments::group_levels,
        ))
        .order((xml_fragments::cst_file.asc(), xml_fragments::start_line.asc()))
        .load(&mut frag_conn)
        .context("Failed to read fragments for the navigation tree")?;

    let mut fragments: HashMap<String, Vec<FragmentInfo>> = HashMap::new();
    // One row per *imported record*, not per fragment. Several fragments can
    // carry the same `sc_code` -- 110 of them do, e.g. three in `an4.1.att` --
    // and the import keeps only the first, so a tree built per fragment would
    // name a record that was never written and collide on `node_uid`. Ordered
    // by (file, start_line), which is the import's own order, so the survivor
    // is the same one.
    let mut seen_uids: HashSet<String> = HashSet::new();

    for (cst_file, sc_code, cst_sutta, group_levels) in rows {
        let Some(sc_code) = sc_code else { continue };
        let uid = fragment_uid(&cst_file, &sc_code);
        if !seen_uids.insert(uid.clone()) {
            continue;
        }
        let title = fragment_title(cst_sutta.as_deref(), &group_levels);

        let mut match_titles: HashSet<String> = HashSet::new();
        if !title.is_empty() {
            match_titles.insert(normalize_title(&title));
        }
        // The group levels carry the vagga / saṃyutta the fragment sits in,
        // which is what a toc leaf usually names. The fragment's own title is
        // matched too: two Dīgha suttas have an empty group-level title and
        // would otherwise match nothing.
        if let Ok(levels) = serde_json::from_str::<Vec<serde_json::Value>>(&group_levels) {
            for level in levels {
                if let Some(t) = level.get("title").and_then(|v| v.as_str()) {
                    if !t.trim().is_empty() {
                        match_titles.insert(normalize_title(t));
                    }
                }
            }
        }

        fragments.entry(cst_file.clone()).or_default().push(FragmentInfo {
            uid,
            title,
            match_titles,
        });
    }

    logger::info(&format!(
        "cst_spine: {} files classified, {} with mapped fragments",
        coverage.len(),
        fragments.len()
    ));
    Ok((coverage, fragments))
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_single_fragment_collapses_only_when_it_is_the_same_text() {
        // The toc leaf and its one fragment carry the same name: the child
        // would repeat its parent verbatim, so the leaf opens the fragment.
        assert!(titles_are_equivalent(
            "1. Mahāpadānasuttavaṇṇanā",
            "1. Mahāpadānasuttavaṇṇanā"
        ));
        // Spelled with the other niggahita by the other source -- still the
        // same text, because flatten() folds both to `ṁ` anyway.
        assert!(titles_are_equivalent("4. Okkantasaṃyuttaṃ", "4. Okkantasaṁyuttaṁ"));

        // A real containment, and the case that makes the equivalence test
        // necessary rather than a count test: the saṁyutta's one child is a
        // ranged record standing for ten suttas. Collapsing it would lose the
        // fragment's own name.
        assert!(!titles_are_equivalent(
            "4. Okkantasaṁyuttaṁ",
            "1-10. Cakkhusuttādivaṇṇanā"
        ));

        // Do not loosen this to ignore the leading number: two sections of one
        // book can share a name and differ only by it.
        assert!(!titles_are_equivalent("1. Yamakavaggo", "2. Yamakavaggo"));

        // An untitled leaf is never "the same text as" anything.
        assert!(!titles_are_equivalent("", ""));
        assert!(!titles_are_equivalent("   ", "anything"));
    }
    use super::*;

    #[test]
    fn parses_nested_branches_and_leaves() {
        let text = r#"<?xml version="1.0"?>
            <tree>
              <!-- comment -->
              <tree text="Suttapiṭaka">
                <tree text="Dīgha nikāya" src="./cscd/s0101a.att.toc.xml" />
              </tree>
            </tree>"#;

        let root = parse_toc(text).unwrap();
        assert_eq!(root.children.len(), 1);
        let group = &root.children[0];
        assert_eq!(group.text.as_deref(), Some("Suttapiṭaka"));
        assert_eq!(group.children.len(), 1);
        assert_eq!(group.children[0].src.as_deref(), Some("./cscd/s0101a.att.toc.xml"));
        assert!(!group.children[0].is_leaf());
    }

    #[test]
    fn parses_flat_cscd_leaves() {
        let text = r#"<?xml version="1.0" encoding="UTF-16"?><tree>
            <tree text="1. Maggasaṃyuttaṃ" action="cscd/s0305m.mul0.xml" target="text" />
            <tree text="2. Bojjhaṅgasaṃyuttaṃ" action="cscd/s0305m.mul1.xml" target="text" />
            </tree>"#;

        let root = parse_toc(text).unwrap();
        assert_eq!(root.children.len(), 2);
        assert!(root.children[0].is_leaf());
        assert_eq!(file_name_of(root.children[1].action.as_ref().unwrap()), "s0305m.mul1.xml");
    }

    #[test]
    fn slugs_keep_pali_diacritics_and_drop_punctuation() {
        assert_eq!(slug("Vinayapiṭaka (aṭṭhakathā)"), "vinayapiṭaka-aṭṭhakathā");
        assert_eq!(slug("Dīgha nikāya (aṭṭhakathā)"), "dīgha-nikāya-aṭṭhakathā");
        assert_eq!(slug("Saṃyuttanikāya (ṭīkā)"), "saṁyuttanikāya-ṭīkā");
        assert_eq!(slug("Vaṃsa-gantha-saṅgaho"), "vaṁsa-gantha-saṅgaho");
    }

    /// The level list is what FR-5 asks to be reused, and the depths it is
    /// indexed at are what make a Saṃyutta leaf a saṃyutta rather than a sutta.
    #[test]
    fn level_labels_follow_the_nikaya_structure() {
        // A title that names no level falls through to the level list.
        let digha = NikayaStructure::from_nikaya_name("digha").unwrap();
        assert_eq!(level_label(Some(&digha), "Ganthārambhakathā", 1, false), "book");
        assert_eq!(level_label(Some(&digha), "Ganthārambhakathā", 2, true), "sutta");

        // A title that does name one wins, which is what keeps the commentary
        // tocs from inheriting the mūla structure's depths.
        assert_eq!(level_label(Some(&digha), "1. Brahmajālasuttavaṇṇanā", 2, true), "sutta");

        let samyutta = NikayaStructure::from_nikaya_name("samyutta").unwrap();
        assert_eq!(level_label(Some(&samyutta), "1. Maggasaṃyuttaṃ", 2, true), "samyutta");

        let anguttara = NikayaStructure::from_nikaya_name("anguttara").unwrap();
        // Depth 2 is Pannasaka in the level list, but the title says vagga.
        assert_eq!(level_label(Some(&anguttara), "1. Rūpādivaggavaṇṇanā", 2, true), "vagga");

        let khuddaka = NikayaStructure::from_nikaya_name("khuddaka").unwrap();
        assert_eq!(level_label(Some(&khuddaka), "1. Yamakavaggo", 2, true), "vagga");

        // Outside a nikāya the generic labels are used.
        assert_eq!(level_label(None, "Visuddhimagga", 0, false), "group");
        assert_eq!(level_label(None, "Nidānādikathā", 1, false), "book");
        assert_eq!(level_label(None, "Nidānādikathā", 2, true), "division");
    }

    /// The four root tocs declare `encoding="UTF-16"` but are UTF-8 with a BOM,
    /// while the `cscd` tocs really are UTF-16. One decoder handles both; this
    /// is the check that it does, against the real files.
    #[test]
    fn reads_both_toc_encodings() {
        let site_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../bootstrap-assets-resources/tipitaka-org-vri-cst/tipitaka-xml/tipitaka.org/romn");
        if !site_dir.join("toc2.xml").exists() {
            eprintln!("skipping: CST checkout not present at {}", site_dir.display());
            return;
        }

        let mislabelled = read_xml_file(&site_dir.join("toc2.xml")).unwrap();
        assert!(mislabelled.contains("aṭṭhakathā"), "toc2.xml decoded as mojibake");

        let utf16 = read_xml_file(&site_dir.join("cscd/s0305m.mul.toc.xml")).unwrap();
        assert!(utf16.contains("Maggasaṃyuttaṃ"), "cscd toc decoded as mojibake");

        let root = parse_toc(&utf16).unwrap();
        assert_eq!(root.children.len(), 12, "s0305m.mul.toc.xml has 12 leaves");
    }
}
