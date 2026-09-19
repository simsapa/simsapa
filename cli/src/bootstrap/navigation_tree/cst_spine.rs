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

use simsapa_backend::logger;
use tipitaka_xml_parser::encoding::read_xml_file;
use tipitaka_xml_parser::nikaya_structure::NikayaStructure;
use tipitaka_xml_parser::types::GroupType;

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
    pub defects: DefectReport,
}

impl CstSpineBuilder {
    pub fn new(site_dir: &Path, source_dir: &Path, db_conn: &mut SqliteConnection) -> Result<Self> {
        let cst_uids = load_cst_rows(db_conn)?;
        Ok(Self {
            site_dir: site_dir.to_path_buf(),
            source_dir: source_dir.to_path_buf(),
            cst_uids,
            reachable_sources: HashSet::new(),
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

            let node = if child.is_leaf() {
                self.make_leaf(child, structure.as_ref(), child_depth)
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
        node.cst_file = Some(source_file);
        node.children = self.build_leaves(&inner, &node, structure, depth + 1);
        Some(node)
    }

    /// The leaves of one `cscd` toc. They are flat in every one of the 217
    /// files, but the walk is recursive anyway so a nested one would build
    /// rather than be silently truncated.
    fn build_leaves(
        &mut self,
        element: &TocElement,
        parent: &TreeNode,
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> Vec<TreeNode> {
        let mut out = Vec::new();

        for child in &element.children {
            if child.is_leaf() {
                if let Some(mut node) = self.make_leaf(child, structure, depth) {
                    if node.cst_file.is_none() {
                        node.cst_file = parent.cst_file.clone();
                    }
                    out.push(node);
                }
            } else {
                let Some(mut node) = self.make_group(child, &parent.node_uid, structure, depth)
                else {
                    continue;
                };
                node.cst_file = parent.cst_file.clone();
                node.children = self.build_leaves(child, &node, structure, depth + 1);
                out.push(node);
            }
        }

        out
    }

    /// A `<tree text="…" action="cscd/s0305m.mul0.xml">` — one division.
    ///
    /// The division records themselves are produced by the CST import, so a
    /// leaf whose record is not there yet is kept and marked unavailable (the
    /// window dims it) and reported, never dropped and never pointed at the
    /// whole-file record — which would make every leaf of a file open the same
    /// text.
    fn make_leaf(
        &mut self,
        element: &TocElement,
        structure: Option<&NikayaStructure>,
        depth: usize,
    ) -> Option<TreeNode> {
        let action = element.action.as_ref()?;
        let division = file_name_of(action);
        if division.is_empty() {
            self.defects.record("toc leaf has an empty action=", action.clone());
            return None;
        }

        let title = element.text.as_deref().unwrap_or_default().trim().to_string();
        let mut node = TreeNode::new(
            format!("cst:{}", division),
            level_label(structure, &title, depth, true),
        );
        node.title_pali = (!title.is_empty()).then_some(title);
        node.cst_file = Some(division.clone());

        match self.cst_uids.get(&division) {
            Some(full_uid) => node.sutta_uid = Some(full_uid.clone()),
            None => {
                node.is_available = false;
                self.defects.record("toc leaf has no division record in the database", division);
            }
        }

        Some(node)
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

/// The last path segment of a toc `src=` or `action=` value, which are written
/// as `./cscd/x.toc.xml` and `cscd/x.xml` respectively.
fn file_name_of(value: &str) -> String {
    value.rsplit('/').next().unwrap_or(value).trim().to_string()
}

/// A title reduced to a node-uid segment: lowercased, whitespace joined with
/// `-`, and anything that is neither alphanumeric nor `-` dropped. Pāli
/// diacritics are alphanumeric, so they survive and the segment stays
/// recognisable.
fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;

    for ch in title.trim().to_lowercase().chars() {
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

#[cfg(test)]
mod tests {
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
