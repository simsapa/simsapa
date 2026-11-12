//! Segment files of the Munindo and Nyanadipa translations.
//!
//! Each source folder holds a `bilara/` folder with one `<sutta_id>.json` per
//! text, in suttas-core's segmentation form (every Pāli `ms` segment id, in
//! Pāli order), and, for Nyanadipa, `bilara/footnotes.json`. This module reads
//! them, computes suttas-core-compatible hashes, and implements the checks of
//! the `check-segment-files` command. See docs/munindo-nyanadipa-segmentation.md.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use simsapa_backend::footnotes::word_count;

use crate::bootstrap::helpers::{dhp_chapter_number, dhp_verse_group_ranges};
use crate::bootstrap::source_paths;

// === File types ===

/// `bilara/<sutta_id>.json`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SegmentFile {
    pub sutta_id: String,
    /// `munindo` or `nyanadipa`.
    pub translator: String,
    /// The source file the segments were cut from, relative to the source
    /// folder (`markdown/dhp-14.md`).
    pub input: String,
    /// `text_hash()` of the source file.
    pub input_hash: String,
    /// `id_list_hash()` of the Pāli `ms` segment ids.
    pub pali_hash: String,
    /// A translator's introduction to this text, as HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub introduction: Option<String>,
    /// Every Pāli segment id, in Pāli order.
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Segment {
    pub id: String,
    pub text: String,
    /// `copy` (empty or structural, taken over unchanged) or `manual` (text
    /// placed by hand: never overwrite without asking).
    pub source: String,
}

/// The `source` values a segment file may use.
pub const SEGMENT_SOURCES: [&str; 2] = ["copy", "manual"];

/// `bilara/footnotes.json`: the notes of every text of one source, with the
/// fields of a suttas-core `segment_footnotes` row.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FootnoteFile {
    pub footnotes: Vec<FootnoteEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FootnoteEntry {
    pub sutta_id: String,
    pub segment_id: String,
    /// The marker of the source (`1` for `[^1]`).
    pub marker: String,
    /// 1-based reading order within the text.
    pub number: u32,
    /// 0-based index of the word the note follows in the segment text,
    /// counted as `simsapa_backend::footnotes::word_count()` counts; `None`
    /// places the note at the segment's end.
    pub word_index: Option<u32>,
    /// The note as HTML.
    pub content: String,
    pub editor: bool,
}

/// Which source a folder or a segment file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Munindo,
    Nyanadipa,
}

impl SourceKind {
    pub fn translator(self) -> &'static str {
        match self {
            SourceKind::Munindo => "munindo",
            SourceKind::Nyanadipa => "nyanadipa",
        }
    }

    /// The kind of a source folder, from its folder name.
    pub fn from_source_folder(folder: &Path) -> Result<Self> {
        match folder.file_name().and_then(|n| n.to_str()) {
            Some(source_paths::DHAMMAPADA_MUNINDO_DIR) => Ok(SourceKind::Munindo),
            Some(source_paths::NYANADIPA_DIR) => Ok(SourceKind::Nyanadipa),
            _ => bail!(
                "{}: not a segment-file source folder (expected {} or {})",
                folder.display(), source_paths::DHAMMAPADA_MUNINDO_DIR, source_paths::NYANADIPA_DIR,
            ),
        }
    }

    /// The `bilara/` folder of a source folder of this kind.
    pub fn segment_dir(self, source_folder: &Path) -> PathBuf {
        match self {
            SourceKind::Munindo => source_folder.join(source_paths::MUNINDO_BILARA_SUBDIR),
            SourceKind::Nyanadipa => source_folder.join(source_paths::NYANADIPA_BILARA_SUBDIR),
        }
    }

    /// The source texts that must each be the `input` of one segment file,
    /// relative to the source folder and sorted: Munindo's chapters
    /// (`markdown/dhp-<n>.md`, not the book-wide front matter) and Nyanadipa's
    /// suttas (`texts-sc-numbering/*.md`).
    pub fn source_texts(self, source_folder: &Path) -> Result<Vec<String>> {
        let (subdir, prefix) = match self {
            SourceKind::Munindo => (source_paths::MUNINDO_TEXTS_SUBDIR, "dhp-"),
            SourceKind::Nyanadipa => (source_paths::NYANADIPA_TEXTS_SUBDIR, ""),
        };
        let dir = source_folder.join(subdir);
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(anyhow!("Reading {}: {}", dir.display(), e)),
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
            .filter(|n| n.starts_with(prefix) && n.ends_with(".md"))
            .map(|n| format!("{}/{}", subdir, n))
            .collect();
        names.sort();
        Ok(names)
    }
}

// === Readers ===

/// Reads one segment file. Fails on invalid JSON and on a `source` value other
/// than `copy` / `manual`.
pub fn read_segment_file(path: &Path) -> Result<SegmentFile> {
    let text = fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    let file: SegmentFile =
        serde_json::from_str(&text).with_context(|| format!("Parsing {}", path.display()))?;
    if let Some(seg) = file.segments.iter().find(|s| !SEGMENT_SOURCES.contains(&s.source.as_str())) {
        bail!("{}: segment {} has the unknown source {:?}", path.display(), seg.id, seg.source);
    }
    Ok(file)
}

/// The segment file paths of a `bilara/` folder (`*.json` except
/// `footnotes.json`), sorted by file name.
pub fn segment_file_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let footnotes_name = Path::new(source_paths::SEGMENT_FOOTNOTES_SUBFILE).file_name();
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("Reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter(|p| p.file_name() != footnotes_name)
        .collect();
    paths.sort();
    Ok(paths)
}

/// Reads every segment file of a `bilara/` folder, sorted by file name.
pub fn read_segment_dir(dir: &Path) -> Result<Vec<(PathBuf, SegmentFile)>> {
    segment_file_paths(dir)?
        .into_iter()
        .map(|p| read_segment_file(&p).map(|f| (p, f)))
        .collect()
}

/// Reads a footnote file; `None` when it does not exist.
pub fn read_footnote_file(path: &Path) -> Result<Option<FootnoteFile>> {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Ok(None),
        Err(e) => return Err(anyhow!("Checking {}: {}", path.display(), e)),
    }
    let text = fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    let file = serde_json::from_str(&text).with_context(|| format!("Parsing {}", path.display()))?;
    Ok(Some(file))
}

// === Hashes (suttas-core's src/lib/hash.ts) ===

static RE_HASH_MARKERS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\^[^\]]+\]|\[\]").unwrap());
static RE_WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// The first 16 hex digits of the SHA-256 of the NFC form of `text`.
pub fn hash(text: &str) -> String {
    let nfc: String = text.nfc().collect();
    let digest = Sha256::digest(nfc.as_bytes());
    digest.iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

/// `hash()` of `text` with footnote markers (`[^n]`, `[]`) removed and
/// whitespace collapsed: re-indenting or moving a marker leaves it unchanged.
pub fn text_hash(text: &str) -> String {
    let without_markers = RE_HASH_MARKERS.replace_all(text, "");
    hash(RE_WHITESPACE.replace_all(&without_markers, " ").trim())
}

/// `hash()` of an ordered segment-id list joined by `\n`.
pub fn id_list_hash<S: AsRef<str>>(ids: &[S]) -> String {
    let joined: Vec<&str> = ids.iter().map(|s| s.as_ref()).collect();
    hash(&joined.join("\n"))
}

// === The Pāli root and template ===

/// The SuttaCentral folders that hold these sources' texts, inside
/// `root/pli/ms/sutta/` and `html/pli/ms/sutta/`.
const PALI_COLLECTION_SUBDIRS: [&str; 2] = ["kn/dhp", "kn/snp"];

fn find_file(dir: &Path, name: &str) -> Result<Option<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(anyhow!("Reading {}: {}", dir.display(), e)),
    };
    let mut subdirs = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Ok(Some(path));
        }
    }
    subdirs.sort();
    for sub in subdirs {
        if let Some(found) = find_file(&sub, name)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn pali_file(sc_data_dir: &Path, layer: &str, file_name: &str) -> Result<PathBuf> {
    let base = sc_data_dir.join(source_paths::SC_BILARA_DATA_SUBDIR).join(layer).join("pli/ms/sutta");
    for sub in PALI_COLLECTION_SUBDIRS {
        if let Some(path) = find_file(&base.join(sub), file_name)? {
            return Ok(path);
        }
    }
    bail!("{} not found under {}", file_name, base.display())
}

fn read_ordered_map(path: &Path) -> Result<IndexMap<String, String>> {
    let text = fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("Parsing {}", path.display()))
}

/// The Pāli `ms` root text of a sutta, in file order.
pub fn pali_root(sc_data_dir: &Path, sutta_id: &str) -> Result<IndexMap<String, String>> {
    read_ordered_map(&pali_file(sc_data_dir, "root", &format!("{}_root-pli-ms.json", sutta_id))?)
}

/// The Pāli `ms` segment ids of a sutta, in order.
pub fn pali_root_ids(sc_data_dir: &Path, sutta_id: &str) -> Result<Vec<String>> {
    Ok(pali_root(sc_data_dir, sutta_id)?.into_keys().collect())
}

/// The Pāli `html/` template of a sutta, in order.
pub fn pali_template(sc_data_dir: &Path, sutta_id: &str) -> Result<IndexMap<String, String>> {
    read_ordered_map(&pali_file(sc_data_dir, "html", &format!("{}_html.json", sutta_id))?)
}

/// The template key rendered as the page title (`<h1 …>`): `dhpN:0.3` in the
/// Dhammapada (`range-title`), `snpX.Y:0.2` in the Sutta Nipāta
/// (`sutta-title`).
pub fn page_title_key(template: &IndexMap<String, String>) -> Option<&str> {
    template.iter().find(|(_, markup)| markup.contains("<h1")).map(|(k, _)| k.as_str())
}

// === Source text normalisation ===

/// A source Markdown file reduced to what its segment file must reproduce.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NormalizedSource {
    /// The `#` (Nyanadipa) or `===`-underlined (Munindo) title, plain text.
    pub title: String,
    /// The body as plain words joined by single spaces.
    pub text: String,
    /// The footnote markers of the body, in reading order (`1` for `[^1]`).
    pub footnote_markers: Vec<String>,
    /// Marker → the definition's Markdown text.
    pub footnote_definitions: IndexMap<String, String>,
}

static RE_MD_ATX_TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#\s+(.*?)\s*#*\s*$").unwrap());
static RE_MD_SETEXT_RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^=+\s*$").unwrap());
static RE_MD_FOOTNOTE_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[\^([^\]]+)\]:\s*(.*)$").unwrap());
static RE_MD_FOOTNOTE_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\^([^\]]+)\]").unwrap());
/// Munindo's verse-number line: `183.\\`, `186-187.\\`, `262 -263.\\`.
static RE_MUNINDO_VERSE_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+(?:\s*-\s*\d+)?\.\\{0,2}\s*$").unwrap());
/// Nyanadipa's verse-number prefix: `1\. `.
static RE_NYANADIPA_VERSE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\\\.\s+").unwrap());
/// A Markdown hard line break at the end of a line: `\\`, `\` (trailing spaces
/// are trimmed before this applies).
static RE_MD_LINE_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\{1,2}$").unwrap());
static RE_MD_EMPHASIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*{1,2}([^*]+?)\*{1,2}").unwrap());
static RE_MD_ESCAPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\([[:punct:]])").unwrap());
static RE_HTML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());

fn collapse_whitespace(text: &str) -> String {
    RE_WHITESPACE.replace_all(text, " ").trim().to_string()
}

/// A straight apostrophe after a letter or digit becomes `’`
/// (`Ajita's` → `Ajita’s`), as the Markdown-to-HTML conversion of these
/// sources sets it (the book and its HTML print it curly). The only straight
/// apostrophes in the sources are in sixteen of Nyanadipa's headings.
pub fn curly_apostrophes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev: Option<char> = None;
    for c in text.chars() {
        if c == '\'' && prev.is_some_and(char::is_alphanumeric) {
            out.push('\u{2019}');
        } else {
            out.push(c);
        }
        prev = Some(c);
    }
    out
}

/// Markdown inline text to plain words: emphasis and escapes removed,
/// apostrophes curly, whitespace collapsed.
fn markdown_inline_plain(text: &str) -> String {
    let text = RE_MD_EMPHASIS.replace_all(text, "$1");
    let text = RE_MD_ESCAPE.replace_all(&text, "$1");
    collapse_whitespace(&curly_apostrophes(&text))
}

/// Segment HTML to plain words: tags removed, entities decoded, whitespace
/// collapsed.
pub fn html_plain(text: &str) -> String {
    let text = RE_HTML_TAG.replace_all(text, "");
    collapse_whitespace(&html_escape::decode_html_entities(&text))
}

/// Reduces a source Markdown file to its title, its body as plain words, and
/// its footnotes. Removed from the body: the title, the translator's verse
/// numbers, footnote markers and definitions, Markdown line-break syntax,
/// emphasis and escapes.
pub fn normalized_source_text(source_md: &str, kind: SourceKind) -> NormalizedSource {
    let mut out = NormalizedSource::default();
    let lines: Vec<&str> = source_md.lines().collect();
    let mut body: Vec<String> = Vec::with_capacity(lines.len());
    let mut title_seen = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        i += 1;

        if !title_seen && !line.trim().is_empty() {
            title_seen = true;
            match kind {
                SourceKind::Nyanadipa => {
                    if let Some(caps) = RE_MD_ATX_TITLE.captures(line) {
                        out.title = markdown_inline_plain(&caps[1]);
                        continue;
                    }
                }
                SourceKind::Munindo => {
                    if lines.get(i).is_some_and(|next| RE_MD_SETEXT_RULE.is_match(next)) {
                        out.title = markdown_inline_plain(line);
                        i += 1;
                        continue;
                    }
                }
            }
        }

        if let Some(caps) = RE_MD_FOOTNOTE_DEF.captures(line) {
            out.footnote_definitions.insert(caps[1].to_string(), caps[2].trim().to_string());
            continue;
        }

        let mut line = line.trim_end().to_string();
        match kind {
            SourceKind::Munindo => {
                if RE_MUNINDO_VERSE_NUMBER.is_match(line.trim()) {
                    continue;
                }
            }
            SourceKind::Nyanadipa => {
                line = RE_NYANADIPA_VERSE_NUMBER.replace(&line, "").to_string();
            }
        }
        line = RE_MD_LINE_BREAK.replace(&line, "").to_string();
        body.push(line);
    }

    let body = body.join("\n");
    out.footnote_markers = RE_MD_FOOTNOTE_REF.captures_iter(&body).map(|c| c[1].to_string()).collect();
    let body = RE_MD_FOOTNOTE_REF.replace_all(&body, "");
    out.text = markdown_inline_plain(&body);
    out
}

/// The segment texts of a file joined in order as plain words, leaving out
/// `skip_id` (the title segment, compared separately).
pub fn joined_segment_text(file: &SegmentFile, skip_id: Option<&str>) -> String {
    let joined: String = file
        .segments
        .iter()
        .filter(|s| Some(s.id.as_str()) != skip_id)
        .map(|s| format!("{} ", s.text))
        .collect();
    html_plain(&joined)
}

// === Checks ===

/// One problem found by a check.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub file: PathBuf,
    pub segment_id: Option<String>,
    pub message: String,
}

impl Problem {
    fn new(file: &Path, segment_id: Option<&str>, message: impl Into<String>) -> Self {
        Problem { file: file.to_path_buf(), segment_id: segment_id.map(String::from), message: message.into() }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.segment_id {
            Some(id) => write!(f, "{} [{}]: {}", self.file.display(), id, self.message),
            None => write!(f, "{}: {}", self.file.display(), self.message),
        }
    }
}

/// Check 1: the segment ids are exactly the Pāli ids, in Pāli order.
pub fn check_segment_ids(path: &Path, file: &SegmentFile, pali_ids: &[String]) -> Vec<Problem> {
    let ids: Vec<&str> = file.segments.iter().map(|s| s.id.as_str()).collect();
    if ids.iter().copied().eq(pali_ids.iter().map(String::as_str)) {
        return vec![];
    }
    let at = ids.iter().zip(pali_ids).position(|(a, b)| *a != b.as_str()).unwrap_or(ids.len().min(pali_ids.len()));
    vec![Problem::new(
        path,
        ids.get(at).copied(),
        format!(
            "segment ids differ from the Pāli at position {}: file has {:?}, Pāli has {:?} ({} ids in the file, {} in the Pāli)",
            at, ids.get(at), pali_ids.get(at), ids.len(), pali_ids.len(),
        ),
    )]
}

fn excerpt(text: &str, char_at: usize) -> String {
    const CONTEXT: usize = 40;
    let chars: Vec<char> = text.chars().collect();
    let from = char_at.saturating_sub(CONTEXT);
    let to = (char_at + CONTEXT).min(chars.len());
    chars[from.min(chars.len())..to].iter().collect()
}

/// Check 2: the joined segment text (without the title segment) equals the
/// normalised source body. Reports the first differing word, with context and
/// the segment it falls in.
pub fn check_joined_text(
    path: &Path,
    file: &SegmentFile,
    source: &NormalizedSource,
    title_id: Option<&str>,
) -> Vec<Problem> {
    // Each word of the segments, with the segment it comes from.
    let mut seg_words: Vec<(String, &str)> = Vec::new();
    for seg in file.segments.iter().filter(|s| Some(s.id.as_str()) != title_id) {
        for w in html_plain(&seg.text).split(' ').filter(|w| !w.is_empty()) {
            seg_words.push((w.to_string(), seg.id.as_str()));
        }
    }
    let src_words: Vec<&str> = source.text.split(' ').filter(|w| !w.is_empty()).collect();

    let first = seg_words
        .iter()
        .zip(&src_words)
        .position(|((a, _), b)| a != b)
        .or_else(|| (seg_words.len() != src_words.len()).then(|| seg_words.len().min(src_words.len())));
    let Some(at) = first else { return vec![] };

    let char_offset = |words: &mut dyn Iterator<Item = &str>| -> usize {
        words.take(at).map(|w| w.chars().count() + 1).sum()
    };
    let seg_text = seg_words.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>().join(" ");
    let seg_at = char_offset(&mut seg_words.iter().map(|(w, _)| w.as_str()));
    let src_at = char_offset(&mut src_words.iter().copied());
    let segment_id = seg_words.get(at).or(seg_words.last()).map(|(_, id)| *id);

    vec![Problem::new(
        path,
        segment_id,
        format!(
            "joined text differs from the source at word {}:\n    segments: …{}…\n    source:   …{}…",
            at, excerpt(&seg_text, seg_at), excerpt(&source.text, src_at),
        ),
    )]
}

/// Check 3: `input_hash` and `pali_hash` match the current source file and
/// Pāli ids.
pub fn check_hashes(path: &Path, file: &SegmentFile, source_md: &str, pali_ids: &[String]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let input_hash = text_hash(source_md);
    if file.input_hash != input_hash {
        problems.push(Problem::new(
            path,
            None,
            format!("stale: input_hash {} but {} hashes to {}", file.input_hash, file.input, input_hash),
        ));
    }
    let pali_hash = id_list_hash(pali_ids);
    if file.pali_hash != pali_hash {
        problems.push(Problem::new(
            path,
            None,
            format!("stale: pali_hash {} but the Pāli ids hash to {}", file.pali_hash, pali_hash),
        ));
    }
    problems
}

/// The value the title segment must hold: `<chapter>. <title>` for Munindo,
/// the Markdown heading for Nyanadipa.
pub fn expected_title(kind: SourceKind, sutta_id: &str, source_title: &str) -> Option<String> {
    match kind {
        SourceKind::Munindo => dhp_chapter_number(sutta_id).map(|n| format!("{}. {}", n, source_title)),
        SourceKind::Nyanadipa => Some(source_title.to_string()),
    }
}

/// Check 4: the segments above the title and the Dhammapada verse-group
/// heading segments are empty (the importer fills the headings), and the title
/// segment holds the source title.
pub fn check_headings_and_title(
    path: &Path,
    file: &SegmentFile,
    kind: SourceKind,
    template: &IndexMap<String, String>,
    source: &NormalizedSource,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    let texts: HashMap<&str, &str> = file.segments.iter().map(|s| (s.id.as_str(), s.text.as_str())).collect();

    let Some(title_id) = page_title_key(template) else {
        return vec![Problem::new(path, None, "the Pāli template has no <h1> title segment")];
    };

    for key in template.keys().take_while(|k| k.as_str() != title_id) {
        if let Some(text) = texts.get(key.as_str()).filter(|t| !t.is_empty()) {
            problems.push(Problem::new(path, Some(key), format!("segment above the title must be empty, holds {:?}", text)));
        }
    }

    let ids: Vec<&str> = file.segments.iter().map(|s| s.id.as_str()).collect();
    for heading in dhp_verse_group_ranges(&ids).keys() {
        if let Some(text) = texts.get(heading.as_str()).filter(|t| !t.is_empty()) {
            problems.push(Problem::new(path, Some(heading), format!("verse-group heading must be empty (the importer writes the range), holds {:?}", text)));
        }
    }

    if source.title.is_empty() {
        problems.push(Problem::new(path, None, format!("no title found in {}", file.input)));
    }
    match expected_title(kind, &file.sutta_id, &source.title) {
        None => problems.push(Problem::new(path, Some(title_id), format!("{} is not a Dhammapada chapter range", file.sutta_id))),
        Some(expected) => {
            let actual = html_plain(texts.get(title_id).copied().unwrap_or(""));
            if actual != expected {
                problems.push(Problem::new(path, Some(title_id), format!("title segment holds {:?}, expected {:?}", actual, expected)));
            }
        }
    }
    problems
}

/// Check 5: every footnote marker of each source appears exactly once in the
/// footnote file, numbered in reading order, on a segment of that text, at a
/// word the segment has, with the definition's wording; and the footnote file
/// has no note without a source marker.
pub fn check_footnotes(
    footnote_path: &Path,
    footnotes: Option<&FootnoteFile>,
    texts: &[(PathBuf, SegmentFile, NormalizedSource)],
) -> Vec<Problem> {
    let mut problems = Vec::new();
    let empty = Vec::new();
    let entries = footnotes.map(|f| &f.footnotes).unwrap_or(&empty);

    let known: HashSet<&str> = texts.iter().map(|(_, f, _)| f.sutta_id.as_str()).collect();
    for e in entries.iter().filter(|e| !known.contains(e.sutta_id.as_str())) {
        problems.push(Problem::new(footnote_path, Some(&e.segment_id), format!("note {} is for {}, which has no segment file", e.marker, e.sutta_id)));
    }

    for (path, file, source) in texts {
        let notes: Vec<&FootnoteEntry> = entries.iter().filter(|e| e.sutta_id == file.sutta_id).collect();
        if footnotes.is_none() && !source.footnote_markers.is_empty() {
            problems.push(Problem::new(path, None, format!("{} has {} footnote markers but there is no {}", file.input, source.footnote_markers.len(), footnote_path.display())));
            continue;
        }
        let segments: HashMap<&str, &str> = file.segments.iter().map(|s| (s.id.as_str(), s.text.as_str())).collect();

        for (i, marker) in source.footnote_markers.iter().enumerate() {
            if source.footnote_markers[..i].contains(marker) {
                problems.push(Problem::new(path, None, format!("{} uses the marker [^{}] twice", file.input, marker)));
                continue;
            }
            let matching: Vec<&&FootnoteEntry> = notes.iter().filter(|e| &e.marker == marker).collect();
            match matching.as_slice() {
                [] => problems.push(Problem::new(footnote_path, None, format!("{}: note [^{}] is missing", file.sutta_id, marker))),
                [note] => {
                    if note.number as usize != i + 1 {
                        problems.push(Problem::new(footnote_path, Some(&note.segment_id), format!("{}: note [^{}] has number {}, its reading order is {}", file.sutta_id, marker, note.number, i + 1)));
                    }
                    match segments.get(note.segment_id.as_str()) {
                        None => problems.push(Problem::new(footnote_path, Some(&note.segment_id), format!("{}: note [^{}] is on a segment the text does not have", file.sutta_id, marker))),
                        Some(text) => {
                            let words = word_count(text);
                            if let Some(w) = note.word_index.filter(|w| *w as usize >= words) {
                                problems.push(Problem::new(footnote_path, Some(&note.segment_id), format!("{}: note [^{}] word_index {} but the segment has {} words", file.sutta_id, marker, w, words)));
                            }
                        }
                    }
                    match source.footnote_definitions.get(marker) {
                        None => problems.push(Problem::new(path, None, format!("{} has no definition for [^{}]", file.input, marker))),
                        Some(def) => {
                            let (expected, actual) = (markdown_inline_plain(def), html_plain(&note.content));
                            if expected != actual {
                                problems.push(Problem::new(footnote_path, Some(&note.segment_id), format!("{}: note [^{}] content {:?} differs from the source's {:?}", file.sutta_id, marker, actual, expected)));
                            }
                        }
                    }
                }
                many => problems.push(Problem::new(footnote_path, None, format!("{}: note [^{}] appears {} times", file.sutta_id, marker, many.len()))),
            }
        }

        for note in notes.iter().filter(|e| !source.footnote_markers.contains(&e.marker)) {
            problems.push(Problem::new(footnote_path, Some(&note.segment_id), format!("{}: note [^{}] has no marker in {}", file.sutta_id, note.marker, file.input)));
        }
    }
    problems
}

/// Check 6: the segment files together cover the whole source. Every source
/// text (`SourceKind::source_texts()`) is the `input` of exactly one segment
/// file, and every segment file's `input` is one of them. With check 2 on each
/// file, this means the segment files join up to the complete original text.
pub fn check_coverage(
    segment_dir: &Path,
    source_texts: &[String],
    files: &[(PathBuf, String)],
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for text in source_texts {
        let users: Vec<&(PathBuf, String)> = files.iter().filter(|(_, input)| input == text).collect();
        match users.as_slice() {
            [] => problems.push(Problem::new(segment_dir, None, format!("{} is the input of no segment file", text))),
            [_] => {}
            many => {
                let names: Vec<String> = many.iter().map(|(p, _)| p.display().to_string()).collect();
                problems.push(Problem::new(segment_dir, None, format!("{} is the input of {} segment files: {}", text, many.len(), names.join(", "))));
            }
        }
    }
    for (path, input) in files.iter().filter(|(_, input)| !source_texts.contains(input)) {
        problems.push(Problem::new(path, None, format!("input {:?} is not one of the source texts", input)));
    }
    problems
}

/// The result of checking one source folder.
#[derive(Debug, Default)]
pub struct CheckReport {
    pub files_checked: usize,
    pub problems: Vec<Problem>,
}

/// Runs every check on the segment files of `source_folder`
/// (`dhammapada-munindo` or `nyanadipa-translations`) against the Pāli of
/// `sc_data_dir` (the `sc-data` folder). Read-only.
pub fn check_source_folder(source_folder: &Path, sc_data_dir: &Path) -> Result<CheckReport> {
    let kind = SourceKind::from_source_folder(source_folder)?;
    let dir = kind.segment_dir(source_folder);
    let mut report = CheckReport::default();
    let source_texts = kind.source_texts(source_folder)?;

    let paths = match dir.try_exists() {
        Ok(true) => segment_file_paths(&dir)?,
        Ok(false) => vec![],
        Err(e) => bail!("Checking {}: {}", dir.display(), e),
    };
    if paths.is_empty() && source_texts.is_empty() {
        return Ok(report);
    }

    let mut texts: Vec<(PathBuf, SegmentFile, NormalizedSource)> = Vec::new();
    // (path, input) of every file that could be read, for check 6.
    let mut inputs: Vec<(PathBuf, String)> = Vec::new();
    for path in paths {
        report.files_checked += 1;
        if let Ok(file) = read_segment_file(&path) {
            inputs.push((path.clone(), file.input));
        }
        match check_one_file(&path, kind, source_folder, sc_data_dir) {
            Ok((problems, file, source)) => {
                report.problems.extend(problems);
                texts.push((path, file, source));
            }
            Err(e) => report.problems.push(Problem::new(&path, None, format!("{:#}", e))),
        }
    }
    report.problems.extend(check_coverage(&dir, &source_texts, &inputs));

    let footnote_path = source_folder.join(source_paths::SEGMENT_FOOTNOTES_SUBFILE);
    match read_footnote_file(&footnote_path) {
        Ok(footnotes) => report.problems.extend(check_footnotes(&footnote_path, footnotes.as_ref(), &texts)),
        Err(e) => report.problems.push(Problem::new(&footnote_path, None, format!("{:#}", e))),
    }
    Ok(report)
}

/// Checks 1–4 on one file. An error means the file or its inputs could not be
/// read at all.
fn check_one_file(
    path: &Path,
    kind: SourceKind,
    source_folder: &Path,
    sc_data_dir: &Path,
) -> Result<(Vec<Problem>, SegmentFile, NormalizedSource)> {
    let file = read_segment_file(path)?;
    let mut problems = Vec::new();

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if file.sutta_id != stem {
        problems.push(Problem::new(path, None, format!("sutta_id {:?} does not match the file name", file.sutta_id)));
    }
    if file.translator != kind.translator() {
        problems.push(Problem::new(path, None, format!("translator {:?}, expected {:?}", file.translator, kind.translator())));
    }

    let source_path = source_folder.join(&file.input);
    let source_md = fs::read_to_string(&source_path)
        .with_context(|| format!("Reading the input {}", source_path.display()))?;
    let source = normalized_source_text(&source_md, kind);
    let pali_ids = pali_root_ids(sc_data_dir, &file.sutta_id)?;
    let template = pali_template(sc_data_dir, &file.sutta_id)?;
    let title_id = page_title_key(&template);

    problems.extend(check_segment_ids(path, &file, &pali_ids));
    problems.extend(check_joined_text(path, &file, &source, title_id));
    problems.extend(check_hashes(path, &file, &source_md, &pali_ids));
    problems.extend(check_headings_and_title(path, &file, kind, &template, &source));
    Ok((problems, file, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/segment_files")
    }

    #[test]
    fn test_hash_matches_suttas_core() {
        // Values computed with suttas-core's src/lib/hash.ts (bun).
        assert_eq!(
            text_hash("  Refrain from doing evil,[^1]\n\tcultivate  the good;[] \n\n purify the heart.  "),
            "4b8a4e2b335ec7bc",
        );
        assert_eq!(hash("Saṃsāra"), "76f4379ee6795fee");
        // NFD input hashes as its NFC form.
        assert_eq!(hash("Sa\u{1e43}sa\u{0304}ra"), "76f4379ee6795fee");
        assert_eq!(id_list_hash(&["dhp1:0.1", "dhp1:1"]), "2bb0a44c9c2703e7");
    }

    /// `pali_hash` of a real suttas-core segmentation file, recomputed from
    /// its own ids.
    #[test]
    fn test_pali_hash_of_a_suttas_core_segmentation() {
        let path = source_paths::bootstrap_assets_dir_from_crate()
            .join("suttas-core/data/segmentations/than/dhp179-196.json");
        if !matches!(path.try_exists(), Ok(true)) {
            eprintln!("suttas-core segmentation not found, skipping: {:?}", path);
            return;
        }
        let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let ids: Vec<&str> = value["segments"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(id_list_hash(&ids), value["pali_hash"].as_str().unwrap());
    }

    #[test]
    fn test_normalized_source_munindo() {
        let md = "The Buddha\n==========\n\n179.\\\\\nThe Buddha’s perfection is complete;\\\\\nthere is no more work.\n\n262 -263.\\\\\nAs *kusa* grass\\\\\nwrongly grasped.\n";
        let n = normalized_source_text(md, SourceKind::Munindo);
        assert_eq!(n.title, "The Buddha");
        assert_eq!(n.text, "The Buddha’s perfection is complete; there is no more work. As kusa grass wrongly grasped.");
        assert!(n.footnote_markers.is_empty());
    }

    #[test]
    fn test_normalized_source_nyanadipa() {
        let md = "# The Cave\n\n1\\. Imprisoned in a cave,  \nthey are incorrigible,[^1] entered  \n*upon* an uneven path [said he].\n\n2\\. Tied down.\n\n[^1]: “Incorrigible” for *avadāniyā*.\n";
        let n = normalized_source_text(md, SourceKind::Nyanadipa);
        assert_eq!(n.title, "The Cave");
        assert_eq!(n.text, "Imprisoned in a cave, they are incorrigible, entered upon an uneven path [said he]. Tied down.");
        assert_eq!(n.footnote_markers, vec!["1"]);
        assert_eq!(n.footnote_definitions["1"], "“Incorrigible” for *avadāniyā*.");
    }

    #[test]
    fn test_joined_segment_text_strips_markup() {
        let file = SegmentFile {
            sutta_id: "snp4.2".into(),
            translator: "nyanadipa".into(),
            input: "x.md".into(),
            input_hash: String::new(),
            pali_hash: String::new(),
            introduction: None,
            segments: vec![
                Segment { id: "snp4.2:0.2".into(), text: "The Cave".into(), source: "manual".into() },
                Segment { id: "snp4.2:1.1".into(), text: "Imprisoned in\na <i>cave</i>, ".into(), source: "manual".into() },
                Segment { id: "snp4.2:1.2".into(), text: "Tom &amp; Jerry".into(), source: "manual".into() },
            ],
        };
        assert_eq!(joined_segment_text(&file, Some("snp4.2:0.2")), "Imprisoned in a cave, Tom & Jerry");
    }

    #[test]
    fn test_read_segment_file_rejects_unknown_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dhp1-20.json");
        fs::write(&path, r#"{"sutta_id":"dhp1-20","translator":"munindo","input":"x","input_hash":"","pali_hash":"","segments":[{"id":"dhp1:1","text":"a","source":"llm"}]}"#).unwrap();
        let err = read_segment_file(&path).unwrap_err();
        assert!(format!("{:#}", err).contains("unknown source"), "{:#}", err);
    }

    #[test]
    fn test_read_footnote_file_absent_is_none() {
        assert!(read_footnote_file(Path::new("/nonexistent/footnotes.json")).unwrap().is_none());
    }

    // --- The fixture folders: one good text per source, checked against a
    // --- two-verse fake sc-data.

    fn sc_data() -> PathBuf {
        fixtures_dir().join("sc-data")
    }

    fn munindo_folder() -> PathBuf {
        fixtures_dir().join(source_paths::DHAMMAPADA_MUNINDO_DIR)
    }

    fn nyanadipa_folder() -> PathBuf {
        fixtures_dir().join(source_paths::NYANADIPA_DIR)
    }

    #[test]
    fn test_good_fixtures_have_no_problems() {
        for folder in [munindo_folder(), nyanadipa_folder()] {
            let report = check_source_folder(&folder, &sc_data()).unwrap();
            assert_eq!(report.files_checked, 1, "{:?}", folder);
            assert!(report.problems.is_empty(), "{:?}: {:#?}", folder, report.problems);
        }
    }

    #[test]
    fn test_read_segment_dir_skips_footnotes_file() {
        let files = read_segment_dir(&nyanadipa_folder().join(source_paths::NYANADIPA_BILARA_SUBDIR)).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.sutta_id, "snp4.2");
    }

    #[test]
    fn test_missing_bilara_folder_checks_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(source_paths::DHAMMAPADA_MUNINDO_DIR);
        fs::create_dir(&folder).unwrap();
        let report = check_source_folder(&folder, &sc_data()).unwrap();
        assert_eq!(report.files_checked, 0);
        assert!(report.problems.is_empty());
    }

    #[test]
    fn test_unknown_source_folder_is_an_error() {
        assert!(check_source_folder(Path::new("/tmp/some-folder"), &sc_data()).is_err());
    }

    /// The good Munindo fixture, its source, Pāli ids and template.
    fn munindo_fixture() -> (PathBuf, SegmentFile, String, NormalizedSource, Vec<String>, IndexMap<String, String>) {
        let path = munindo_folder().join("bilara/dhp1-20.json");
        let file = read_segment_file(&path).unwrap();
        let md = fs::read_to_string(munindo_folder().join(&file.input)).unwrap();
        let source = normalized_source_text(&md, SourceKind::Munindo);
        let ids = pali_root_ids(&sc_data(), "dhp1-20").unwrap();
        let template = pali_template(&sc_data(), "dhp1-20").unwrap();
        (path, file, md, source, ids, template)
    }

    fn set_text(file: &mut SegmentFile, id: &str, text: &str) {
        file.segments.iter_mut().find(|s| s.id == id).unwrap().text = text.to_string();
    }

    #[test]
    fn test_check_ids_reports_missing_and_reordered() {
        let (path, mut file, _, _, ids, _) = munindo_fixture();
        assert!(check_segment_ids(&path, &file, &ids).is_empty());

        file.segments.swap(4, 5);
        let p = check_segment_ids(&path, &file, &ids);
        assert_eq!(p.len(), 1);
        assert!(p[0].message.contains("position 4"), "{}", p[0]);

        file.segments.swap(4, 5);
        file.segments.pop();
        assert_eq!(check_segment_ids(&path, &file, &ids).len(), 1);
    }

    #[test]
    fn test_check_text_reports_changed_word_and_dropped_line() {
        let (path, mut file, _, source, _, template) = munindo_fixture();
        let title = page_title_key(&template);
        assert!(check_joined_text(&path, &file, &source, title).is_empty());

        set_text(&mut file, "dhp2:2", "the mind is the forerunner\n");
        let p = check_joined_text(&path, &file, &source, title);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].segment_id.as_deref(), Some("dhp2:2"));
        assert!(p[0].message.contains("source:"), "{}", p[0]);

        let (_, mut file, ..) = munindo_fixture();
        set_text(&mut file, "dhp2:4", "");
        assert_eq!(check_joined_text(&path, &file, &source, title).len(), 1);
    }

    #[test]
    fn test_check_hashes_reports_stale() {
        let (path, mut file, md, _, ids, _) = munindo_fixture();
        assert!(check_hashes(&path, &file, &md, &ids).is_empty());
        // Whitespace-only edits do not make the file stale.
        assert!(check_hashes(&path, &file, &md.replace('\n', "\n\n"), &ids).is_empty());

        file.input_hash = "0000000000000000".into();
        file.pali_hash = "0000000000000000".into();
        let p = check_hashes(&path, &file, &md, &ids);
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|p| p.message.starts_with("stale")));
    }

    #[test]
    fn test_check_headings_and_title() {
        let (path, mut file, _, source, _, template) = munindo_fixture();
        assert!(check_headings_and_title(&path, &file, SourceKind::Munindo, &template, &source).is_empty());

        set_text(&mut file, "dhp1:0.4", "1–2.");
        set_text(&mut file, "dhp1:0.1", "Khuddakanikāya");
        set_text(&mut file, "dhp1:0.3", "Pairs");
        let p = check_headings_and_title(&path, &file, SourceKind::Munindo, &template, &source);
        let ids: Vec<_> = p.iter().map(|p| p.segment_id.as_deref().unwrap()).collect();
        assert_eq!(ids, vec!["dhp1:0.1", "dhp1:0.4", "dhp1:0.3"], "{:#?}", p);
    }

    fn nyanadipa_texts() -> (PathBuf, FootnoteFile, Vec<(PathBuf, SegmentFile, NormalizedSource)>) {
        let path = nyanadipa_folder().join("bilara/snp4.2.json");
        let file = read_segment_file(&path).unwrap();
        let md = fs::read_to_string(nyanadipa_folder().join(&file.input)).unwrap();
        let source = normalized_source_text(&md, SourceKind::Nyanadipa);
        let fn_path = nyanadipa_folder().join(source_paths::SEGMENT_FOOTNOTES_SUBFILE);
        let footnotes = read_footnote_file(&fn_path).unwrap().unwrap();
        (fn_path, footnotes, vec![(path, file, source)])
    }

    #[test]
    fn test_check_footnotes() {
        let (fn_path, footnotes, texts) = nyanadipa_texts();
        assert!(check_footnotes(&fn_path, Some(&footnotes), &texts).is_empty());

        // Missing note.
        let none = FootnoteFile { footnotes: vec![] };
        let p = check_footnotes(&fn_path, Some(&none), &texts);
        assert!(p.len() == 1 && p[0].message.contains("missing"), "{:#?}", p);

        // No footnote file at all.
        assert_eq!(check_footnotes(&fn_path, None, &texts).len(), 1);

        // Duplicated note, word index past the segment, wrong number, changed
        // wording, a note for an unknown text.
        let mut bad = footnotes.clone();
        bad.footnotes.push(bad.footnotes[0].clone());
        assert!(check_footnotes(&fn_path, Some(&bad), &texts)[0].message.contains("appears 2 times"));

        let mut bad = footnotes.clone();
        bad.footnotes[0].word_index = Some(99);
        bad.footnotes[0].number = 2;
        bad.footnotes[0].content = "Something else.".into();
        let p = check_footnotes(&fn_path, Some(&bad), &texts);
        assert_eq!(p.len(), 3, "{:#?}", p);

        let mut bad = footnotes.clone();
        bad.footnotes[0].sutta_id = "snp9.9".into();
        let p = check_footnotes(&fn_path, Some(&bad), &texts);
        assert!(p.iter().any(|p| p.message.contains("no segment file")), "{:#?}", p);
    }

    #[test]
    fn test_curly_apostrophes() {
        assert_eq!(curly_apostrophes("Ajita's Question"), "Ajita\u{2019}s Question");
        assert_eq!(curly_apostrophes("Jatukaṇṇī's Question"), "Jatukaṇṇī\u{2019}s Question");
        // A source defect keeps its capital; only the apostrophe changes.
        assert_eq!(curly_apostrophes("Upasīva'S Question"), "Upasīva\u{2019}S Question");
        // Not after a letter or digit: left as it is.
        assert_eq!(curly_apostrophes("'quoted' text"), "'quoted\u{2019} text");
        assert_eq!(curly_apostrophes("Ajita\u{2019}s"), "Ajita\u{2019}s");
    }

    #[test]
    fn test_nyanadipa_title_with_straight_apostrophe() {
        let n = normalized_source_text("# Ajita's Question\n\n1\\. Line.\n", SourceKind::Nyanadipa);
        assert_eq!(n.title, "Ajita\u{2019}s Question");
        assert_eq!(expected_title(SourceKind::Nyanadipa, "snp5.2", &n.title).unwrap(), "Ajita\u{2019}s Question");
    }

    #[test]
    fn test_check_coverage() {
        let dir = Path::new("bilara");
        let texts = vec!["markdown/dhp-1.md".to_string(), "markdown/dhp-2.md".to_string()];
        let file = |name: &str, input: &str| (PathBuf::from(name), input.to_string());

        let good = vec![file("dhp1-20.json", "markdown/dhp-1.md"), file("dhp21-32.json", "markdown/dhp-2.md")];
        assert!(check_coverage(dir, &texts, &good).is_empty());

        // A source text with no segment file.
        let p = check_coverage(dir, &texts, &good[..1]);
        assert!(p.len() == 1 && p[0].message.contains("dhp-2.md is the input of no segment file"), "{:#?}", p);

        // Two files cut from the same source, and an input that is not a source text.
        let bad = vec![
            file("dhp1-20.json", "markdown/dhp-1.md"),
            file("dhp21-32.json", "markdown/dhp-1.md"),
            file("dhp33-43.json", "html/dhp-2.html"),
        ];
        let p = check_coverage(dir, &texts, &bad);
        let messages: Vec<&str> = p.iter().map(|p| p.message.as_str()).collect();
        assert_eq!(p.len(), 3, "{:#?}", p);
        assert!(messages[0].contains("dhp-1.md is the input of 2 segment files"));
        assert!(messages[1].contains("dhp-2.md is the input of no segment file"));
        assert!(messages[2].contains("is not one of the source texts"));
    }

    #[test]
    fn test_source_texts_leave_out_front_matter() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(source_paths::DHAMMAPADA_MUNINDO_DIR);
        fs::create_dir_all(folder.join("markdown")).unwrap();
        for name in ["dhp-1.md", "dhp-2.md", "preface-1st.md", "introduction.md"] {
            fs::write(folder.join("markdown").join(name), "").unwrap();
        }
        assert_eq!(
            SourceKind::Munindo.source_texts(&folder).unwrap(),
            vec!["markdown/dhp-1.md", "markdown/dhp-2.md"],
        );

        // A source text without a segment file is reported, even with no bilara/ folder.
        let report = check_source_folder(&folder, &sc_data()).unwrap();
        assert_eq!(report.files_checked, 0);
        assert_eq!(report.problems.len(), 2, "{:#?}", report.problems);
    }

    #[test]
    fn test_nyanadipa_title_check() {
        let (_, _, texts) = nyanadipa_texts();
        let (path, mut file, source) = texts.into_iter().next().unwrap();
        let template = pali_template(&sc_data(), "snp4.2").unwrap();
        assert!(check_headings_and_title(&path, &file, SourceKind::Nyanadipa, &template, &source).is_empty());
        set_text(&mut file, "snp4.2:0.2", "The Octad on the Cave");
        assert_eq!(check_headings_and_title(&path, &file, SourceKind::Nyanadipa, &template, &source).len(), 1);
    }
}
