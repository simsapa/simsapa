//! Writes `assets/navigation-blurb-suttacentral.toml`: SuttaCentral's English
//! blurbs for the nodes of the SC spine, keyed by `node_uid`.
//!
//! The file is generated wholesale on every SC spine build and embedded in the
//! app, which is where the Sutta Navigation window reads its summaries from.
//! See docs/sutta-navigation-tree.md.
//!
//! Every entry of `blurbs.json` is accounted for as exactly one of:
//!
//! - **used by a node** of the final tree — written under that node's uid;
//! - **re-keyed** by [`BLURB_KEY_REMAPS`] — written under the node(s) the table
//!   names;
//! - **skipped** by one of the three skip lists — not written.
//!
//! Anything else is recorded as a defect and not written. Nothing is inferred:
//! the skip lists and the re-keying table are fixed data, and a row of either
//! that no longer matches the source is reported rather than forced.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use simsapa_backend::navigation_texts::unwrap_note;

use super::{fold_title, DefectReport, TreeNode};

pub const BLURB_FILE_NAME: &str = "navigation-blurb-suttacentral.toml";

/// A file that is always in the repository's `assets/` folder. Its presence is
/// what tells [`write`] that the folder it was handed is that folder.
const ASSETS_DIR_MARKER: &str = "navigation-context-notes.toml";

/// Column at which a blurb's text is wrapped in the file.
const WRAP_WIDTH: usize = 80;

/// Super-tree grouping levels. In the Pāli-only scope each holds one member and
/// is elided from the tree by `collapse_redundant_groups`, or is never built
/// (`medium`, `other`: the super-tree has no level by that name in scope). Their
/// text is the cross-tradition summary ("Collections of “long” discourses and
/// other related texts"); the node that survives carries the specific one.
const SKIPPED_GROUPING_KEYS: &[&str] = &["long", "medium", "linked", "numbered", "minor", "other"];

/// Collections outside the tree's Theravāda Pāli scope (`SC_SCOPE`): the
/// Chinese, Sanskrit, Tibetan and Gāndhārī collections and their groupings.
/// Simsapa has no texts for them, so the tree has no node for them.
const SKIPPED_NON_THERAVADA_KEYS: &[&str] = &[
    "da", "da-ot", "ma", "ma-ot", "sa", "sa-2", "sa-3", "sa-ot", "ea", "ea-2", "ea-ot",
    "lzh-ja", "lzh-dhp", "lzh-nbs", "lzh-ssnp", "lzh-dharani", "lzh-svk", "arv", "avs", "divy",
    "lal", "uv", "mkv", "sht", "sf", "d", "up", "pdhp", "pf", "gdhp", "gf", "kf", "uf", "sg",
    "lzh-dk", "pr", "dy", "vk", "pk", "jp", "jp-2", "mvs", "sab", "san-dk",
];

/// Per-rule Vibhaṅga blurbs, each with its Pātimokkha counterpart.
///
/// The tree holds each of these passages as one ranged leaf
/// (`pli-tv-bu-vb-as1-7`, `pli-tv-bi-vb-pj1-4`, `pli-tv-bi-vb-pc91-93`), so
/// there is no per-rule node to show the blurb on; and each text is identical
/// to the blurb of the Pātimokkha rule, which the tree does show. A pair whose
/// two texts stop being identical is reported, because the second half of that
/// reason no longer holds for it.
const SKIPPED_VIBHANGA_RULE_KEYS: &[(&str, &str)] = &[
    ("pli-tv-bu-vb-as1", "pli-tv-bu-pm-as1"),
    ("pli-tv-bu-vb-as2", "pli-tv-bu-pm-as2"),
    ("pli-tv-bu-vb-as3", "pli-tv-bu-pm-as3"),
    ("pli-tv-bu-vb-as4", "pli-tv-bu-pm-as4"),
    ("pli-tv-bu-vb-as5", "pli-tv-bu-pm-as5"),
    ("pli-tv-bu-vb-as6", "pli-tv-bu-pm-as6"),
    ("pli-tv-bu-vb-as7", "pli-tv-bu-pm-as7"),
    ("pli-tv-bi-vb-as1", "pli-tv-bi-pm-as1"),
    ("pli-tv-bi-vb-as2", "pli-tv-bi-pm-as2"),
    ("pli-tv-bi-vb-as3", "pli-tv-bi-pm-as3"),
    ("pli-tv-bi-vb-as4", "pli-tv-bi-pm-as4"),
    ("pli-tv-bi-vb-as5", "pli-tv-bi-pm-as5"),
    ("pli-tv-bi-vb-as6", "pli-tv-bi-pm-as6"),
    ("pli-tv-bi-vb-as7", "pli-tv-bi-pm-as7"),
    ("pli-tv-bi-vb-pj1", "pli-tv-bi-pm-pj1"),
    ("pli-tv-bi-vb-pj2", "pli-tv-bi-pm-pj2"),
    ("pli-tv-bi-vb-pj3", "pli-tv-bi-pm-pj3"),
    ("pli-tv-bi-vb-pj4", "pli-tv-bi-pm-pj4"),
    ("pli-tv-bi-vb-pc91", "pli-tv-bi-pm-pc91"),
    ("pli-tv-bi-vb-pc92", "pli-tv-bi-pm-pc92"),
    ("pli-tv-bi-vb-pc93", "pli-tv-bi-pm-pc93"),
];

/// Blurbs that `blurbs.json` files under a uid its own tree does not use:
/// the `blurbs.json` key, the node uids the blurb is written under, and a
/// prefix to remove from the text.
///
/// A list of named cases, never a heuristic — no range arithmetic, no nearest
/// uid. These are defects in SuttaCentral's data, worked around here until they
/// are fixed there:
///
/// - `sn17.37-43`, `sn33.51-55` — the blurb spans a range the tree splits in
///   two, so both nodes get the text.
/// - `sn39.1`, `an3.156` — a single uid where the tree has a range.
/// - `sn23.159`, `sn28.159` — a wrong key; the text itself begins with the uid
///   it belongs to, which is removed.
/// - `pli-tv-bu-pm-an` — a different abbreviation from the tree's `-ay`.
///
/// Every row is checked on every run ([`collect`]). A row that no longer
/// applies is reported and skipped, which is the signal to delete it.
const BLURB_KEY_REMAPS: &[(&str, &[&str], Option<&str>)] = &[
    ("sn17.37-43", &["sn17.37", "sn17.38-43"], None),
    ("sn33.51-55", &["sn33.51-54", "sn33.55"], None),
    ("sn39.1", &["sn39.1-15"], None),
    ("an3.156", &["an3.156-162"], None),
    ("sn23.159", &["sn24.1"], Some("sn24.1: ")),
    ("sn28.159", &["sn29.1"], Some("sn29.1: ")),
    ("pli-tv-bu-pm-an", &["pli-tv-bu-pm-ay"], None),
];

/// The fixed data [`collect`] works from. A parameter rather than a reference
/// to the constants, so the tests can exercise each rule on a table of their
/// own.
pub struct BlurbTables<'a> {
    pub skipped_grouping: &'a [&'a str],
    pub skipped_non_theravada: &'a [&'a str],
    pub skipped_vibhanga_rules: &'a [(&'a str, &'a str)],
    pub key_remaps: &'a [(&'a str, &'a [&'a str], Option<&'a str>)],
}

pub const BLURB_TABLES: BlurbTables<'static> = BlurbTables {
    skipped_grouping: SKIPPED_GROUPING_KEYS,
    skipped_non_theravada: SKIPPED_NON_THERAVADA_KEYS,
    skipped_vibhanga_rules: SKIPPED_VIBHANGA_RULE_KEYS,
    key_remaps: BLURB_KEY_REMAPS,
};

/// One entry of the generated file.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeBlurb {
    pub node_uid: String,
    /// The node's displayed title, for the entry's comment line.
    pub title: String,
    pub text: String,
    /// The `blurbs.json` key, where the entry was re-keyed from another uid.
    pub source_key: Option<String>,
}

#[derive(Debug, Default)]
pub struct BlurbExport {
    /// In tree order — the order the window shows the nodes in.
    pub tree_entries: Vec<TreeBlurb>,
    /// `blurbs.json` entries left out by a skip list.
    pub skipped: usize,
    /// `blurbs.json` entries written under another uid by the re-keying table.
    pub rekeyed: usize,
}

/// `names` entries and blurbs carry stray leading and trailing whitespace. Only
/// that is touched: the text is written as the source wrote it.
fn clean(s: &str) -> String {
    s.trim().to_string()
}

/// The nodes of the final tree, depth-first. Mirrors the flattening pass: a
/// repeated `node_uid` is not in the tree, and neither is its subtree.
fn walk<'a>(nodes: &'a [TreeNode], seen: &mut HashSet<&'a str>, out: &mut Vec<&'a TreeNode>) {
    for node in nodes {
        if !seen.insert(node.node_uid.as_str()) {
            continue;
        }
        out.push(node);
        walk(&node.children, seen, out);
    }
}

/// The title the window shows for a node, on one line: Pāli, else English,
/// else the uid.
fn display_title(node: &TreeNode) -> String {
    let one_line = |s: &str| {
        fold_title(s)
            .split_whitespace()
            .map(|word| word.chars().filter(|c| !c.is_control()).collect::<String>())
            .filter(|word| !word.is_empty())
            .collect::<Vec<String>>()
            .join(" ")
    };
    [node.title_pali.as_deref(), node.title_en.as_deref()]
        .into_iter()
        .flatten()
        .map(one_line)
        .find(|title| !title.is_empty())
        .unwrap_or_else(|| node.node_uid.clone())
}

/// Pair the final tree with `blurbs.json`.
///
/// `roots` must be the tree **after** the grouping levels were collapsed: a
/// level that was built and then elided is not a node, and its blurb is used by
/// no node.
pub fn collect(
    roots: &[TreeNode],
    blurbs: &HashMap<String, String>,
    tables: &BlurbTables,
    defects: &mut DefectReport,
) -> BlurbExport {
    let mut nodes: Vec<&TreeNode> = Vec::new();
    walk(roots, &mut HashSet::new(), &mut nodes);

    let node_uids: HashSet<&str> = nodes.iter().map(|n| n.node_uid.as_str()).collect();

    // A `blurbs.json` key is used when a node carries it as its source uid,
    // whether or not the text under it is empty.
    let used_keys: HashSet<&str> = nodes
        .iter()
        .filter_map(|n| n.source_uid.as_deref())
        .filter(|uid| blurbs.contains_key(*uid))
        .collect();

    let own_blurb = |node: &TreeNode| -> Option<String> {
        node.source_uid
            .as_deref()
            .and_then(|uid| blurbs.get(uid))
            .map(|text| clean(text))
            .filter(|text| !text.is_empty())
    };
    let nodes_with_own_blurb: HashSet<&str> = nodes
        .iter()
        .filter(|n| own_blurb(n).is_some())
        .map(|n| n.node_uid.as_str())
        .collect();

    // Target node uid -> (source key, text).
    let mut rekeyed_texts: HashMap<&str, (&str, String)> = HashMap::new();
    let mut applied_remaps: HashSet<&str> = HashSet::new();

    for &(source_key, targets, prefix) in tables.key_remaps {
        let Some(source_text) = blurbs.get(source_key) else {
            defects.record("blurb key remap: source key not in blurbs.json", source_key);
            continue;
        };

        let mut applies = true;
        for &target in targets {
            let pair = || format!("{} -> {}", source_key, target);
            if !node_uids.contains(target) {
                defects.record("blurb key remap: target node not in the tree", pair());
                applies = false;
            } else if nodes_with_own_blurb.contains(target) {
                defects.record("blurb key remap: target node has its own blurb", pair());
                applies = false;
            }
        }

        let mut text = clean(source_text);
        if let Some(prefix) = prefix {
            match text.strip_prefix(prefix) {
                Some(rest) => text = clean(rest),
                None => {
                    defects.record(
                        "blurb key remap: prefix not found",
                        format!("{} -- '{}'", source_key, prefix),
                    );
                    applies = false;
                }
            }
        }

        if !applies {
            continue;
        }
        applied_remaps.insert(source_key);
        if !text.is_empty() {
            for &target in targets {
                rekeyed_texts.insert(target, (source_key, text.clone()));
            }
        }
    }

    let mut export = BlurbExport::default();

    for node in &nodes {
        let (text, source_key) = match own_blurb(node) {
            Some(text) => (text, None),
            None => match rekeyed_texts.get(node.node_uid.as_str()) {
                Some((source_key, text)) => (text.clone(), Some(source_key.to_string())),
                None => continue,
            },
        };
        export.tree_entries.push(TreeBlurb {
            node_uid: node.node_uid.clone(),
            title: display_title(node),
            text,
            source_key,
        });
    }

    let skip_list: HashSet<&str> = tables
        .skipped_grouping
        .iter()
        .chain(tables.skipped_non_theravada.iter())
        .copied()
        .chain(tables.skipped_vibhanga_rules.iter().map(|(key, _)| *key))
        .collect();

    // Sorted, so the defect report reads the same on every run.
    let mut keys: Vec<&str> = blurbs.keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    for key in keys {
        if used_keys.contains(key) {
            continue;
        }
        if applied_remaps.contains(key) {
            export.rekeyed += 1;
        } else if skip_list.contains(key) {
            export.skipped += 1;
        } else {
            defects.record("blurb for a uid the tree has no node for", key);
        }
    }

    // A skip-list row that the source no longer matches is stale.
    let listed_keys = tables
        .skipped_grouping
        .iter()
        .chain(tables.skipped_non_theravada.iter())
        .copied()
        .chain(tables.skipped_vibhanga_rules.iter().map(|(key, _)| *key));
    for key in listed_keys {
        if !blurbs.contains_key(key) {
            defects.record("blurb skip list: key not in blurbs.json", key);
        } else if used_keys.contains(key) {
            defects.record("blurb skip list: key is now used by a node", key);
        }
    }

    for &(key, counterpart) in tables.skipped_vibhanga_rules {
        let Some(text) = blurbs.get(key) else { continue };
        if used_keys.contains(key) {
            continue;
        }
        let same = blurbs.get(counterpart).is_some_and(|other| clean(other) == clean(text));
        if !same {
            defects.record(
                "blurb skip list: Vibhaṅga text differs from its Pātimokkha counterpart",
                format!("{} / {}", key, counterpart),
            );
        }
    }

    export
}

const FILE_HEADER: &str = r#"# GENERATED FILE -- do not edit by hand.
#
# SuttaCentral's English summaries ("blurbs") for the Sutta Navigation window,
# from sc-data/additional-info/blurbs.json. One entry per tree node that has a
# blurb: the key is the node's `node_uid`. See docs/sutta-navigation-tree.md.
#
# This file is rewritten whenever the SuttaCentral spine of the navigation tree
# is built: by the CLI's full `bootstrap`, and by
#
#     cargo run -- build-navigation-tree --db-path <appdata.sqlite3> --sc-data-dir <sc-data>
#
# An edit made here is lost at the next build. To correct a summary, add one to
# a node that has none, or hide one, add an entry to
# `navigation-blurb-overrides.toml` instead.
"#;

/// A TOML basic-string key.
fn quote_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 2);
    out.push('"');
    for c in key.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Wrap one paragraph at [`WRAP_WIDTH`], breaking only between words. A word
/// longer than the width gets a line to itself.
fn wrap_paragraph(paragraph: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in paragraph.split(' ') {
        let word_width = word.chars().count();
        if !current.is_empty() && current_width + 1 + word_width > WRAP_WIDTH {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            current_width += 1;
        }
        current.push_str(word);
        current_width += word_width;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Escape the body of a multi-line basic string: every `\`, every control
/// character other than the line breaks this module writes itself, and every
/// third `"` of a run — two in a row are literal, three would close the string.
fn escape_body(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut quotes = 0;
    for c in body.chars() {
        if c == '"' {
            quotes += 1;
            if quotes == 3 {
                out.push_str("\\\"");
                quotes = 0;
            } else {
                out.push('"');
            }
            continue;
        }
        quotes = 0;
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push('\n'),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The whole file. The same export always renders to the same bytes: nothing
/// here depends on the run.
pub fn render(export: &BlurbExport) -> String {
    let mut out = String::from(FILE_HEADER);

    for entry in &export.tree_entries {
        out.push('\n');
        match &entry.source_key {
            Some(source_key) => {
                out.push_str(&format!("# {} (blurbs.json key: {})\n", entry.title, source_key))
            }
            None => out.push_str(&format!("# {}\n", entry.title)),
        }

        // The parser's own reading of the text decides the layout, so that what
        // is written is what it gives back: a blank line appears in the value
        // only where the text has a paragraph break.
        let unwrapped = unwrap_note(&entry.text);
        let body = unwrapped
            .split("\n\n")
            .map(|paragraph| wrap_paragraph(paragraph).join("\n"))
            .collect::<Vec<String>>()
            .join("\n\n");

        out.push_str(&format!(
            "{} = \"\"\"\n{}\n\"\"\"\n",
            quote_key(&entry.node_uid),
            escape_body(&body)
        ));
    }

    out
}

#[derive(Debug, PartialEq)]
pub enum WriteOutcome {
    Written(PathBuf),
    /// The file on disk already has this content.
    Unchanged(PathBuf),
    /// The folder is not the repository's `assets/` folder; nothing was written.
    NotAnAssetsDir,
}

/// Write the generated file into `assets_dir`, all or nothing: to a temporary
/// file beside it, then renamed into place.
///
/// Refuses a folder that does not hold the context-notes file, so a wrong
/// working directory never leaves a stray file somewhere unexpected.
pub fn write(assets_dir: &Path, content: &str) -> Result<WriteOutcome> {
    if !matches!(assets_dir.join(ASSETS_DIR_MARKER).try_exists(), Ok(true)) {
        return Ok(WriteOutcome::NotAnAssetsDir);
    }

    let path = assets_dir.join(BLURB_FILE_NAME);
    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
        return Ok(WriteOutcome::Unchanged(path));
    }

    let temp_path = assets_dir.join(format!(".{}.tmp", BLURB_FILE_NAME));
    std::fs::write(&temp_path, content)
        .with_context(|| format!("Failed to write {}", temp_path.display()))?;
    if let Err(e) = std::fs::rename(&temp_path, &path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e).with_context(|| format!("Failed to replace {}", path.display()));
    }

    Ok(WriteOutcome::Written(path))
}

/// The repository's `assets/` folder, for a CLI run from `bootstrap_cli/` — resolved from
/// the working directory, the way the bootstrap resolves its input folder.
pub fn default_assets_dir() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("Failed to get current working directory")?;
    Ok(simsapa_backend::normalize_lexically(&cwd.join("../assets")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use simsapa_backend::navigation_texts::parse_node_texts;

    fn node(node_uid: &str, source_uid: &str, children: Vec<TreeNode>) -> TreeNode {
        let mut n = TreeNode::new(node_uid, "branch");
        n.source_uid = Some(source_uid.to_string());
        n.children = children;
        n
    }

    fn leaf(uid: &str) -> TreeNode {
        node(uid, uid, Vec::new())
    }

    fn blurbs(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    const NO_TABLES: BlurbTables<'static> = BlurbTables {
        skipped_grouping: &[],
        skipped_non_theravada: &[],
        skipped_vibhanga_rules: &[],
        key_remaps: &[],
    };

    fn entry(node_uid: &str, title: &str, text: &str) -> TreeBlurb {
        TreeBlurb {
            node_uid: node_uid.to_string(),
            title: title.to_string(),
            text: text.to_string(),
            source_key: None,
        }
    }

    fn export_of(entries: Vec<TreeBlurb>) -> BlurbExport {
        BlurbExport { tree_entries: entries, skipped: 0, rekeyed: 0 }
    }

    // --- render ---

    #[test]
    fn every_rendered_entry_parses_back_to_its_text() {
        let long = "The Long Discourses (Dīgha Nikāya, abbreviated DN) is a collection of 34 \
                    discourses in the Pali canon (Tipiṭaka) of the Theravāda school. "
            .repeat(6);
        let entries = vec![
            entry("dn", "Dīghanikāya", &long),
            entry("backslash", "A", r"A path C:\dir and a trailing backslash \"),
            entry("quotes", "B", r#"He said "this", then ""that"", then """three""" and """"four""""."#),
            entry("ends-in-quotes", "C", r#"The last word is "quoted"""#),
            entry("curly", "D", "Collections of “long” discourses — and ‘other’ related texts."),
            entry("markup", "E", "The <i>Brahmajāla</i> is the <b>first</b> & longest."),
            entry("paragraphs", "F", "First paragraph.\n\nSecond   paragraph,\nwrapped by hand."),
            entry("sutta/minor/kn", "Khuddakanikāya", "A node uid with slashes."),
            entry(r#"odd"key\"#, "G", "A key that needs escaping."),
            entry("control", "H", "A bell\u{7} inside."),
            entry("long-word", "I", &format!("before {} after", "x".repeat(200))),
        ];

        let rendered = render(&export_of(entries.clone()));
        let parsed = parse_node_texts(&rendered).expect("the rendered file parses");

        assert_eq!(parsed.len(), entries.len());
        for e in &entries {
            assert_eq!(parsed[&e.node_uid], unwrap_note(&e.text), "entry {}", e.node_uid);
        }
    }

    #[test]
    fn long_text_is_wrapped_at_whitespace_only() {
        let text = "word ".repeat(100);
        let rendered = render(&export_of(vec![entry("dn", "Dīghanikāya", &text)]));

        let body: Vec<&str> = rendered
            .lines()
            .skip_while(|l| !l.starts_with("\"dn\""))
            .skip(1)
            .take_while(|l| *l != "\"\"\"")
            .collect();
        assert!(body.len() > 1);
        assert!(body.iter().all(|l| !l.is_empty() && l.chars().count() <= WRAP_WIDTH));
        assert!(body.iter().all(|l| !l.starts_with(' ') && !l.ends_with(' ')));
    }

    #[test]
    fn rendering_is_deterministic_and_keeps_the_given_order() {
        let entries = vec![
            entry("sn", "Saṁyuttanikāya", "Linked."),
            entry("dn", "Dīghanikāya", "Long."),
            entry("an", "Aṅguttaranikāya", "Numbered."),
        ];
        let export = export_of(entries);

        let first = render(&export);
        assert_eq!(first, render(&export));

        let position = |key: &str| first.find(key).unwrap();
        assert!(position("\"sn\"") < position("\"dn\""));
        assert!(position("\"dn\"") < position("\"an\""));
    }

    #[test]
    fn an_entry_is_preceded_by_its_title_and_a_rekeyed_one_names_its_source_key() {
        let mut rekeyed = entry("sn24.1", "Vātasutta", "Text.");
        rekeyed.source_key = Some("sn23.159".to_string());
        let rendered = render(&export_of(vec![entry("dn", "Dīghanikāya", "Long."), rekeyed]));

        assert!(rendered.starts_with("# GENERATED FILE"));
        assert!(rendered.contains("\n# Dīghanikāya\n\"dn\" = \"\"\"\nLong.\n\"\"\"\n"));
        assert!(rendered.contains("\n# Vātasutta (blurbs.json key: sn23.159)\n\"sn24.1\" = "));
    }

    // --- collect ---

    #[test]
    fn a_node_is_keyed_by_its_node_uid_and_entries_are_in_tree_order() {
        let roots = vec![node(
            "sutta",
            "sutta",
            vec![leaf("dn"), node("sutta/minor/kn", "kn", vec![leaf("dhp")])],
        )];
        let blurbs = blurbs(&[("dhp", "  Dhammapada. "), ("kn", "Minor."), ("dn", "Long.")]);
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &NO_TABLES, &mut defects);

        assert!(defects.is_empty());
        let keys: Vec<&str> = export.tree_entries.iter().map(|e| e.node_uid.as_str()).collect();
        assert_eq!(keys, ["dn", "sutta/minor/kn", "dhp"]);
        assert_eq!(export.tree_entries[2].text, "Dhammapada.");
        assert_eq!((export.skipped, export.rekeyed), (0, 0));
    }

    #[test]
    fn the_comment_title_is_pali_then_english_then_the_uid() {
        let mut pali = leaf("dn");
        pali.title_pali = Some("Dīghanikāyaṃ\n".to_string());
        pali.title_en = Some("Long Discourses".to_string());
        let mut english = leaf("mn");
        english.title_en = Some("Middle Discourses".to_string());
        let roots = vec![pali, english, leaf("sn")];
        let blurbs = blurbs(&[("dn", "a"), ("mn", "b"), ("sn", "c")]);

        let export = collect(&roots, &blurbs, &NO_TABLES, &mut DefectReport::default());

        let titles: Vec<&str> = export.tree_entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Dīghanikāyaṁ", "Middle Discourses", "sn"]);
    }

    #[test]
    fn a_skipped_key_is_not_written_and_is_not_a_defect() {
        // `long` was elided from the tree: only `dn` is a node.
        let roots = vec![node("sutta", "sutta", vec![leaf("dn")])];
        let blurbs = blurbs(&[("dn", "Long."), ("long", "Cross-tradition."), ("da", "Chinese.")]);
        let tables = BlurbTables {
            skipped_grouping: &["long"],
            skipped_non_theravada: &["da"],
            ..NO_TABLES
        };
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &tables, &mut defects);

        assert!(defects.is_empty());
        assert_eq!(export.tree_entries.len(), 1);
        assert_eq!(export.skipped, 2);
    }

    #[test]
    fn an_unused_entry_on_no_list_is_reported_and_not_written() {
        let roots = vec![leaf("dn")];
        let blurbs = blurbs(&[("dn", "Long."), ("zz9", "New upstream.")]);
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &NO_TABLES, &mut defects);

        assert_eq!(export.tree_entries.len(), 1);
        assert_eq!(defects.len(), 1);
        assert_eq!(defects.count_of("blurb for a uid the tree has no node for"), 1);
    }

    #[test]
    fn a_stale_skip_list_row_is_reported() {
        let roots = vec![leaf("dn"), leaf("pm-as1")];
        let blurbs = blurbs(&[
            ("dn", "Long."),
            ("pm-as1", "The rule."),
            ("vb-as1", "The rule, reworded."),
        ]);
        let tables = BlurbTables {
            // `gone` is no longer in blurbs.json; `dn` is used by a node.
            skipped_grouping: &["gone", "dn"],
            skipped_vibhanga_rules: &[("vb-as1", "pm-as1")],
            ..NO_TABLES
        };
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &tables, &mut defects);

        assert_eq!(defects.len(), 3);
        assert_eq!(defects.count_of("blurb skip list: key not in blurbs.json"), 1);
        assert_eq!(defects.count_of("blurb skip list: key is now used by a node"), 1);
        assert_eq!(
            defects.count_of("blurb skip list: Vibhaṅga text differs from its Pātimokkha counterpart"),
            1
        );
        // The node still gets its blurb as usual.
        assert!(export.tree_entries.iter().any(|e| e.node_uid == "dn"));
    }

    #[test]
    fn a_vibhanga_rule_identical_to_its_patimokkha_counterpart_is_skipped_quietly() {
        let roots = vec![leaf("pm-as1")];
        let blurbs = blurbs(&[("pm-as1", "The rule."), ("vb-as1", " The rule. ")]);
        let tables = BlurbTables { skipped_vibhanga_rules: &[("vb-as1", "pm-as1")], ..NO_TABLES };
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &tables, &mut defects);

        assert!(defects.is_empty());
        assert_eq!(export.skipped, 1);
    }

    #[test]
    fn a_remap_row_writes_the_text_under_each_target_in_tree_order() {
        let roots = vec![leaf("sn17.37"), leaf("dn"), leaf("sn17.38-43"), leaf("sn24.1")];
        let blurbs = blurbs(&[
            ("dn", "Long."),
            ("sn17.37-43", "Gain and honour."),
            ("sn23.159", "sn24.1: Winds do not blow."),
        ]);
        let tables = BlurbTables {
            key_remaps: &[
                ("sn17.37-43", &["sn17.37", "sn17.38-43"], None),
                ("sn23.159", &["sn24.1"], Some("sn24.1: ")),
            ],
            ..NO_TABLES
        };
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &tables, &mut defects);

        assert!(defects.is_empty());
        assert_eq!(export.rekeyed, 2);
        let got: Vec<(&str, &str, Option<&str>)> = export
            .tree_entries
            .iter()
            .map(|e| (e.node_uid.as_str(), e.text.as_str(), e.source_key.as_deref()))
            .collect();
        assert_eq!(
            got,
            [
                ("sn17.37", "Gain and honour.", Some("sn17.37-43")),
                ("dn", "Long.", None),
                ("sn17.38-43", "Gain and honour.", Some("sn17.37-43")),
                ("sn24.1", "Winds do not blow.", Some("sn23.159")),
            ]
        );
    }

    /// Runs one remap row that must not apply, and returns the defect report.
    fn rejected_remap(
        roots: Vec<TreeNode>,
        blurbs: HashMap<String, String>,
        row: (&str, &[&str], Option<&str>),
        unused_source: bool,
    ) -> DefectReport {
        let rows = [row];
        let tables = BlurbTables { key_remaps: &rows, ..NO_TABLES };
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &tables, &mut defects);

        assert_eq!(export.rekeyed, 0);
        assert!(export.tree_entries.iter().all(|e| e.source_key.is_none()));
        // The blurb of a rejected row is used by nothing, and says so.
        assert_eq!(
            defects.count_of("blurb for a uid the tree has no node for"),
            usize::from(unused_source)
        );
        defects
    }

    #[test]
    fn a_remap_row_whose_source_key_is_gone_is_reported_and_skipped() {
        let defects = rejected_remap(
            vec![leaf("sn39.1-15")],
            blurbs(&[]),
            ("sn39.1", &["sn39.1-15"], None),
            false,
        );
        assert_eq!(defects.count_of("blurb key remap: source key not in blurbs.json"), 1);
    }

    #[test]
    fn a_remap_row_whose_target_is_not_in_the_tree_is_reported_and_skipped() {
        // One of the two targets is missing: the other is left untouched too.
        let defects = rejected_remap(
            vec![leaf("sn17.37")],
            blurbs(&[("sn17.37-43", "Gain and honour.")]),
            ("sn17.37-43", &["sn17.37", "sn17.38-43"], None),
            true,
        );
        assert_eq!(defects.count_of("blurb key remap: target node not in the tree"), 1);
    }

    #[test]
    fn a_remap_row_whose_target_has_its_own_blurb_is_reported_and_the_source_wins() {
        let roots = vec![leaf("sn39.1-15")];
        let blurbs = blurbs(&[("sn39.1", "Old."), ("sn39.1-15", "Fixed upstream.")]);
        let rows: [(&str, &[&str], Option<&str>); 1] = [("sn39.1", &["sn39.1-15"], None)];
        let tables = BlurbTables { key_remaps: &rows, ..NO_TABLES };
        let mut defects = DefectReport::default();

        let export = collect(&roots, &blurbs, &tables, &mut defects);

        assert_eq!(defects.count_of("blurb key remap: target node has its own blurb"), 1);
        assert_eq!(export.tree_entries.len(), 1);
        assert_eq!(export.tree_entries[0].text, "Fixed upstream.");
        assert_eq!(export.tree_entries[0].source_key, None);
    }

    #[test]
    fn a_remap_row_whose_prefix_is_not_there_is_reported_and_skipped() {
        let defects = rejected_remap(
            vec![leaf("sn24.1")],
            blurbs(&[("sn23.159", "Winds do not blow.")]),
            ("sn23.159", &["sn24.1"], Some("sn24.1: ")),
            true,
        );
        assert_eq!(defects.count_of("blurb key remap: prefix not found"), 1);
    }

    #[test]
    fn the_shipped_tables_are_disjoint_and_complete() {
        assert_eq!(SKIPPED_GROUPING_KEYS.len(), 6);
        assert_eq!(SKIPPED_NON_THERAVADA_KEYS.len(), 44);
        assert_eq!(SKIPPED_VIBHANGA_RULE_KEYS.len(), 21);
        assert_eq!(BLURB_KEY_REMAPS.len(), 7);

        let keys: Vec<&str> = SKIPPED_GROUPING_KEYS
            .iter()
            .chain(SKIPPED_NON_THERAVADA_KEYS.iter())
            .copied()
            .chain(SKIPPED_VIBHANGA_RULE_KEYS.iter().map(|(key, _)| *key))
            .chain(BLURB_KEY_REMAPS.iter().map(|(key, _, _)| *key))
            .collect();
        let distinct: HashSet<&str> = keys.iter().copied().collect();
        assert_eq!(distinct.len(), keys.len(), "a key is listed twice");

        let targets: Vec<&str> =
            BLURB_KEY_REMAPS.iter().flat_map(|(_, targets, _)| targets.iter().copied()).collect();
        let distinct_targets: HashSet<&str> = targets.iter().copied().collect();
        assert_eq!(targets.len(), 9);
        assert_eq!(distinct_targets.len(), 9, "two rows write to one node");
    }

    // --- write ---

    #[test]
    fn write_refuses_a_folder_that_is_not_the_assets_folder() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(write(dir.path(), "content").unwrap(), WriteOutcome::NotAnAssetsDir);
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn write_replaces_the_file_and_leaves_an_identical_one_alone() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ASSETS_DIR_MARKER), "").unwrap();
        let path = dir.path().join(BLURB_FILE_NAME);

        assert_eq!(write(dir.path(), "one").unwrap(), WriteOutcome::Written(path.clone()));
        assert_eq!(write(dir.path(), "one").unwrap(), WriteOutcome::Unchanged(path.clone()));
        assert_eq!(write(dir.path(), "two").unwrap(), WriteOutcome::Written(path.clone()));

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        // No temporary file is left behind.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
