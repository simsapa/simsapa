//! Segment files of the Munindo and Nyanadipa translations, and of the
//! Dhammapada translations SuttaCentral has only as HTML (Ānandajoti,
//! Buddharakkhita, Feldmeier).
//!
//! Each source folder holds a `bilara/` folder with one `<sutta_id>.json` per
//! text, in suttas-core's segmentation form (every Pāli `ms` segment id, in
//! Pāli order), and, for Nyanadipa, `bilara/footnotes.json`;
//! `dhammapada-suttacentral` holds one `bilara/<translator>/` folder per
//! translator. This module reads them, computes suttas-core-compatible hashes,
//! and implements the checks of the `check-segment-files` command, and builds
//! and imports the records of `DhammapadaMunindoImporter`,
//! `NyanadipaImporter` and `DhammapadaSuttaCentralImporter`. See
//! docs/munindo-nyanadipa-segmentation.md.

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

use diesel::prelude::*;
use simsapa_backend::db::appdata_models::NewSutta;
use simsapa_backend::db::appdata_schema::suttas;
use simsapa_backend::footnotes::word_count;
use simsapa_backend::helpers::{consistent_niggahita, pali_to_ascii, sutta_html_to_plain_text};
use simsapa_backend::logger;

use crate::bootstrap::footnote_sutta_links::{link_sutta_refs, SuttaRefResolver};
use crate::bootstrap::helpers::{
    apply_dhp_verse_ranges, dhp_chapter_number, dhp_verse_group_ranges, uid_to_nikaya, uid_to_ref, SuttaData,
};
use crate::bootstrap::source_paths;
use crate::bootstrap::suttacentral::bilara_html_and_plain_text;
use crate::bootstrap::suttas_core::{footnotes_value_from, pli_ms_uids, write_comments, SegmentNote};

// === File types ===

/// `bilara/<sutta_id>.json`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SegmentFile {
    pub sutta_id: String,
    /// `munindo`, `nyanadipa`, `anandajoti`, `buddharakkhita` or `feldmeier`.
    pub translator: String,
    /// The source file the segments were cut from, relative to the source
    /// folder (`markdown/dhp-14.md`), or for a SuttaCentral HTML text to the
    /// `sc-data` folder (`html_text/en/pli/sutta/kn/dhp/feldmeier/dhp/dhp1-20.html`).
    pub input: String,
    /// `text_hash()` of the source file.
    pub input_hash: String,
    /// `id_list_hash()` of the Pāli `ms` segment ids.
    pub pali_hash: String,
    /// A translator's introduction to this text, as HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub introduction: Option<String>,
    /// The copyright holder, for `suttas.copyright`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,
    /// The licence, for `suttas.license`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Every Pāli segment id, in Pāli order.
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Segment {
    pub id: String,
    pub text: String,
    /// `copy` (empty or structural, taken over unchanged), `auto` (text placed
    /// by the drafting script of `dhammapada-suttacentral`, which may redo it)
    /// or `manual` (text placed by hand: never overwrite without asking).
    pub source: String,
}

/// The `source` values a segment file may use.
pub const SEGMENT_SOURCES: [&str; 3] = ["copy", "auto", "manual"];

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
    /// The SuttaCentral HTML Dhammapada translations, in
    /// `dhammapada-suttacentral/bilara/<translator>/`.
    Anandajoti,
    Buddharakkhita,
    Feldmeier,
}

/// The translators of `dhammapada-suttacentral`, in import order.
pub const SUTTACENTRAL_DHP_KINDS: [SourceKind; 3] = [SourceKind::Anandajoti, SourceKind::Buddharakkhita, SourceKind::Feldmeier];

impl SourceKind {
    pub fn translator(self) -> &'static str {
        match self {
            SourceKind::Munindo => "munindo",
            SourceKind::Nyanadipa => "nyanadipa",
            SourceKind::Anandajoti => "anandajoti",
            SourceKind::Buddharakkhita => "buddharakkhita",
            SourceKind::Feldmeier => "feldmeier",
        }
    }

    /// A Dhammapada translation SuttaCentral has as HTML, whose record the
    /// SuttaCentral import writes and the segment file replaces.
    pub fn is_suttacentral_html(self) -> bool {
        SUTTACENTRAL_DHP_KINDS.contains(&self)
    }

    /// A Dhammapada translation: its records get the verse-group ranges and a
    /// `<chapter>. <title>` title segment.
    pub fn is_dhammapada(self) -> bool {
        self == SourceKind::Munindo || self.is_suttacentral_html()
    }

    /// The kinds of a source folder, from its folder name: one, or the three
    /// translators of `dhammapada-suttacentral`.
    pub fn kinds_of_source_folder(folder: &Path) -> Result<Vec<Self>> {
        match folder.file_name().and_then(|n| n.to_str()) {
            Some(source_paths::DHAMMAPADA_MUNINDO_DIR) => Ok(vec![SourceKind::Munindo]),
            Some(source_paths::NYANADIPA_DIR) => Ok(vec![SourceKind::Nyanadipa]),
            Some(source_paths::DHAMMAPADA_SUTTACENTRAL_DIR) => Ok(SUTTACENTRAL_DHP_KINDS.to_vec()),
            _ => bail!(
                "{}: not a segment-file source folder (expected {}, {} or {})",
                folder.display(), source_paths::DHAMMAPADA_MUNINDO_DIR, source_paths::NYANADIPA_DIR,
                source_paths::DHAMMAPADA_SUTTACENTRAL_DIR,
            ),
        }
    }

    /// The folder of the segment files of this kind in its source folder.
    pub fn segment_dir(self, source_folder: &Path) -> PathBuf {
        match self {
            SourceKind::Munindo => source_folder.join(source_paths::MUNINDO_BILARA_SUBDIR),
            SourceKind::Nyanadipa => source_folder.join(source_paths::NYANADIPA_BILARA_SUBDIR),
            _ => source_folder.join(source_paths::SUTTACENTRAL_DHP_BILARA_SUBDIR).join(self.translator()),
        }
    }

    /// The folder a segment file's `input` is relative to: the source folder,
    /// or `sc-data` for the SuttaCentral HTML texts.
    pub fn input_base(self, source_folder: &Path, sc_data_dir: &Path) -> PathBuf {
        if self.is_suttacentral_html() {
            sc_data_dir.to_path_buf()
        } else {
            source_folder.to_path_buf()
        }
    }

    /// The source texts that must each be the `input` of one segment file,
    /// relative to `input_base()` and sorted: Munindo's chapters
    /// (`markdown/dhp-<n>.md`, not the book-wide front matter), Nyanadipa's
    /// suttas (`texts-sc-numbering/*.md`), and the translator's SuttaCentral
    /// chapter pages (`html_text/…/<translator>/dhp/dhp*.html`).
    pub fn source_texts(self, source_folder: &Path, sc_data_dir: &Path) -> Result<Vec<String>> {
        let sc_subdir = source_paths::suttacentral_dhp_html_subdir(self.translator());
        let (subdir, prefix, extension) = match self {
            SourceKind::Munindo => (source_paths::MUNINDO_TEXTS_SUBDIR, "dhp-", ".md"),
            SourceKind::Nyanadipa => (source_paths::NYANADIPA_TEXTS_SUBDIR, "", ".md"),
            _ => (sc_subdir.as_str(), "dhp", ".html"),
        };
        let dir = self.input_base(source_folder, sc_data_dir).join(subdir);
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(anyhow!("Reading {}: {}", dir.display(), e)),
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
            .filter(|n| n.starts_with(prefix) && n.ends_with(extension))
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

static RE_HTML_H1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<h1[^>]*>(.*?)</h1>").unwrap());
/// A SuttaCentral verse anchor: `<a class='ref sc' id='sc1' …>SC 1</a>`.
static RE_SC_VERSE_ANCHOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<a class='ref sc'[^>]*>[^<]*</a>").unwrap());

/// Reduces a SuttaCentral `html_text` page to its `<h1>` title and the plain
/// words of its body: the text between `</header>` and `<footer>`, without the
/// verse anchors (`SC 1`). The header (division names) and the footer
/// (translator, edition, licence) are not part of the text.
pub fn normalized_suttacentral_html(source_html: &str) -> NormalizedSource {
    let title = RE_HTML_H1.captures(source_html).map(|c| html_plain(&c[1])).unwrap_or_default();
    let body = source_html.split_once("</header>").map(|(_, b)| b).unwrap_or("");
    let body = body.split("<footer").next().unwrap_or("");
    let body = RE_SC_VERSE_ANCHOR.replace_all(body, "");
    NormalizedSource { title, text: html_plain(&body), ..Default::default() }
}

/// Reduces a source file to its title, its body as plain words, and its
/// footnotes. From a Markdown source this removes the title, the translator's
/// verse numbers, footnote markers and definitions, Markdown line-break
/// syntax, emphasis and escapes; a SuttaCentral HTML source goes through
/// `normalized_suttacentral_html()`.
pub fn normalized_source_text(source_md: &str, kind: SourceKind) -> NormalizedSource {
    if kind.is_suttacentral_html() {
        return normalized_suttacentral_html(source_md);
    }
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
                _ => {}
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
            _ => {}
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

// === Building and importing records ===

/// A record built from a segment file, ready to insert.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentedRecord {
    pub uid: String,
    /// The record title: the page-title segment's text, without Munindo's
    /// `<chapter>. ` prefix.
    pub title: String,
    pub content_json: String,
    pub content_json_tmpl: String,
    /// The introduction's plain text (when there is one), then the segments'.
    pub content_plain: String,
    /// The introduction, as HTML.
    pub description: Option<String>,
    /// `sutta_comments.content_json` for the text's notes; `None` without notes.
    pub footnotes_value: Option<String>,
    pub footnote_count: usize,
    pub copyright: Option<String>,
    pub license: Option<String>,
}

impl SourceKind {
    /// The record uid of a text: `dhp179-196/en/munindo`, `snp4.2/en/nyanadipa`.
    pub fn record_uid(self, sutta_id: &str) -> String {
        format!("{}/en/{}", sutta_id, self.translator())
    }

    /// The `source_info` of the records: the folder their text is read from.
    pub fn source_info(self) -> String {
        match self {
            SourceKind::Munindo => format!("{}/{}", source_paths::DHAMMAPADA_MUNINDO_DIR, source_paths::MUNINDO_BILARA_SUBDIR),
            SourceKind::Nyanadipa => format!("{}/{}", source_paths::NYANADIPA_DIR, source_paths::NYANADIPA_BILARA_SUBDIR),
            _ => format!(
                "{}/{}/{}",
                source_paths::DHAMMAPADA_SUTTACENTRAL_DIR, source_paths::SUTTACENTRAL_DHP_BILARA_SUBDIR, self.translator(),
            ),
        }
    }
}

static RE_CHAPTER_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\s+").unwrap());

/// The record title from the page-title segment's text: Nyanadipa's heading
/// as it is, a Dhammapada chapter's `14. The Buddha` without its chapter
/// number.
pub fn record_title(kind: SourceKind, title_segment: &str) -> String {
    let title = html_plain(title_segment);
    if kind.is_dhammapada() {
        RE_CHAPTER_PREFIX.replace(&title, "").into_owned()
    } else {
        title
    }
}

impl From<&FootnoteEntry> for SegmentNote {
    fn from(e: &FootnoteEntry) -> Self {
        SegmentNote {
            segment_id: e.segment_id.clone(),
            number: e.number,
            word_index: e.word_index,
            content: e.content.clone(),
            editor: e.editor,
        }
    }
}

/// Builds the record of one segment file on `template` (the `<sutta_id>/pli/ms`
/// record's `content_json_tmpl`): one `content_json` key per template key, in
/// template order, `""` where the file has no text; the Dhammapada verse-group
/// ranges for Munindo; the plain text with the introduction first; and the
/// notes encoded as `sutta_comments` stores them. `link_fn` turns the sutta
/// references in a note into links. A segment id or a note that the template
/// does not have is an error.
pub fn build_segmented_record(
    kind: SourceKind,
    file: &SegmentFile,
    template: &IndexMap<String, String>,
    notes: &[FootnoteEntry],
    link_fn: &dyn Fn(&str) -> String,
) -> Result<SegmentedRecord> {
    let uid = kind.record_uid(&file.sutta_id);
    if file.translator != kind.translator() {
        bail!("{}: translator {:?}, expected {:?}", uid, file.translator, kind.translator());
    }
    if let Some(seg) = file.segments.iter().find(|s| !template.contains_key(&s.id)) {
        bail!("{}: segment {} is not in the Pāli template", uid, seg.id);
    }

    let mut texts: HashMap<&str, &str> = file.segments.iter().map(|s| (s.id.as_str(), s.text.as_str())).collect();
    let mut content: IndexMap<String, String> = template
        .keys()
        .map(|k| {
            let text = texts.remove(k.as_str()).unwrap_or("");
            (k.clone(), consistent_niggahita(Some(text.to_string())))
        })
        .collect();

    if kind.is_dhammapada() {
        apply_dhp_verse_ranges(&mut content, false, &uid);
    }

    let title_key = page_title_key(template).ok_or_else(|| anyhow!("{}: the Pāli template has no <h1> title segment", uid))?;
    let title = record_title(kind, content.get(title_key).map(String::as_str).unwrap_or(""));
    if title.is_empty() {
        bail!("{}: the title segment {} is empty", uid, title_key);
    }

    let content_json = serde_json::to_string(&content)?;
    let content_json_tmpl = serde_json::to_string(template)?;
    let (_html, plain) = bilara_html_and_plain_text(&content_json, &content_json_tmpl)
        .with_context(|| format!("Rendering {}", uid))?;

    let description = file
        .introduction
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| consistent_niggahita(Some(s.to_string())));
    let content_plain = match &description {
        Some(intro) => format!("{}\n\n{}", sutta_html_to_plain_text(intro), plain),
        None => plain,
    };

    let segment_notes: Vec<SegmentNote> = notes.iter().map(SegmentNote::from).collect();
    let (footnotes_value, footnote_count) = footnotes_value_from(segment_notes, template, &uid, link_fn)?;
    if footnote_count != notes.len() {
        bail!("{}: {} of its {} notes could not be placed", uid, notes.len() - footnote_count, notes.len());
    }

    Ok(SegmentedRecord {
        uid,
        title,
        content_json,
        content_json_tmpl,
        content_plain,
        description,
        footnotes_value,
        footnote_count,
        copyright: file.copyright.clone().filter(|s| !s.trim().is_empty()),
        license: file.license.clone().filter(|s| !s.trim().is_empty()),
    })
}

/// Whether `suttas` has a row with this uid.
fn sutta_record_exists(conn: &mut SqliteConnection, uid: &str) -> Result<bool> {
    let id: Option<i32> = suttas::table
        .filter(suttas::uid.eq(uid))
        .select(suttas::id)
        .first(conn)
        .optional()?;
    Ok(id.is_some())
}

/// The `content_json_tmpl` of Simsapa's own `<sutta_id>/pli/ms` record, in
/// document order. Its absence is an error naming the uid.
pub fn pali_template_from_db(conn: &mut SqliteConnection, sutta_id: &str) -> Result<IndexMap<String, String>> {
    let pli_uid = format!("{}/pli/ms", sutta_id);
    let tmpl: Option<Option<String>> = suttas::table
        .filter(suttas::uid.eq(&pli_uid))
        .select(suttas::content_json_tmpl)
        .first(conn)
        .optional()?;
    let tmpl = tmpl
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow!("No Pāli template for {}: {} is missing or has no content_json_tmpl", sutta_id, pli_uid))?;
    serde_json::from_str(&tmpl).with_context(|| format!("Invalid content_json_tmpl in {}", pli_uid))
}

/// Inserts one built record and its notes.
fn insert_segmented_record(conn: &mut SqliteConnection, kind: SourceKind, sutta_id: &str, rec: &SegmentedRecord) -> Result<()> {
    let sutta_ref = uid_to_ref(sutta_id);
    let nikaya = uid_to_nikaya(sutta_id);
    let (range_group, range_start, range_end) = SuttaData::parse_range_from_uid(&rec.uid);
    let title_ascii = pali_to_ascii(Some(&rec.title));
    let source_info = kind.source_info();

    diesel::insert_into(suttas::table)
        .values(NewSutta {
            uid: &rec.uid,
            sutta_ref: &sutta_ref,
            nikaya: &nikaya,
            language: "en",
            group_path: None,
            group_index: None,
            order_index: None,
            sutta_range_group: range_group.as_deref(),
            sutta_range_start: range_start,
            sutta_range_end: range_end,
            title: Some(&rec.title),
            title_ascii: Some(&title_ascii),
            title_pali: None,
            title_trans: None,
            description: rec.description.as_deref(),
            content_plain: Some(&rec.content_plain),
            content_html: None,
            content_json: Some(&rec.content_json),
            content_json_tmpl: Some(&rec.content_json_tmpl),
            source_uid: Some(kind.translator()),
            source_info: Some(&source_info),
            source_language: None,
            message: None,
            copyright: rec.copyright.as_deref(),
            license: rec.license.as_deref(),
        })
        .execute(conn)
        .with_context(|| format!("Inserting {}", rec.uid))?;
    let id: i32 = suttas::table.filter(suttas::uid.eq(&rec.uid)).select(suttas::id).first(conn)?;
    write_comments(conn, id, &rec.uid, "en", kind.translator(), rec.footnotes_value.as_deref())
}

/// What the import did with one SuttaCentral HTML record.
enum Replaced {
    Done,
    /// The record already has `content_json`: SuttaCentral now publishes the
    /// text segmented, and its own text is kept.
    AlreadySegmented,
}

/// Writes `rec` over the SuttaCentral HTML record of the same uid, which keeps
/// its `id`, `uid`, reference, range fields and `source_uid`; its title
/// becomes the translator's. A missing record is an error.
fn replace_suttacentral_record(conn: &mut SqliteConnection, kind: SourceKind, rec: &SegmentedRecord) -> Result<Replaced> {
    let existing: Option<(i32, Option<String>)> = suttas::table
        .filter(suttas::uid.eq(&rec.uid))
        .select((suttas::id, suttas::content_json))
        .first(conn)
        .optional()?;
    let Some((id, content_json)) = existing else {
        bail!("{}: no such record; the SuttaCentral import writes it, and this step only replaces its text", rec.uid);
    };
    if content_json.is_some_and(|c| !c.trim().is_empty()) {
        return Ok(Replaced::AlreadySegmented);
    }
    let title_ascii = pali_to_ascii(Some(&rec.title));
    diesel::update(suttas::table.filter(suttas::id.eq(id)))
        .set((
            suttas::title.eq(Some(&rec.title)),
            suttas::title_ascii.eq(Some(&title_ascii)),
            suttas::description.eq(rec.description.as_ref()),
            suttas::content_plain.eq(Some(&rec.content_plain)),
            suttas::content_html.eq(None::<String>),
            suttas::content_json.eq(Some(&rec.content_json)),
            suttas::content_json_tmpl.eq(Some(&rec.content_json_tmpl)),
            suttas::copyright.eq(rec.copyright.as_ref()),
            suttas::license.eq(rec.license.as_ref()),
            suttas::source_info.eq(Some(kind.source_info())),
        ))
        .execute(conn)
        .with_context(|| format!("Updating {}", rec.uid))?;
    write_comments(conn, id, &rec.uid, "en", kind.translator(), rec.footnotes_value.as_deref())?;
    Ok(Replaced::Done)
}

/// Imports every segment file of `source_folder` (`dhammapada-munindo`,
/// `nyanadipa-translations` or `dhammapada-suttacentral`) into appdata, in one
/// transaction per kind, and returns the uids written. The SuttaCentral import
/// must have run: each text's template is its `…/pli/ms` record's, and a
/// SuttaCentral HTML text replaces the text of its existing record (one that
/// SuttaCentral meanwhile publishes segmented is kept, with a warning). A
/// missing or empty segment folder, a missing Nyanadipa `footnotes.json`, a
/// missing record to replace, and any file that fails to build are errors, so
/// a release cannot ship with a text missing. Under `--limit` a text whose
/// template or record to replace was not imported is skipped
/// (`skip_under_limit()`).
pub fn import_source_folder(conn: &mut SqliteConnection, source_folder: &Path) -> Result<Vec<String>> {
    let mut uids = Vec::new();
    for kind in SourceKind::kinds_of_source_folder(source_folder)? {
        uids.extend(import_kind(conn, kind, source_folder)?);
    }
    Ok(uids)
}

fn import_kind(conn: &mut SqliteConnection, kind: SourceKind, source_folder: &Path) -> Result<Vec<String>> {
    let dir = kind.segment_dir(source_folder);
    match dir.try_exists() {
        Ok(true) => {}
        Ok(false) => bail!("Segment file folder not found: {}", dir.display()),
        Err(e) => bail!("Checking {}: {}", dir.display(), e),
    }
    let files = read_segment_dir(&dir)?;
    if files.is_empty() {
        bail!("No segment files in {}", dir.display());
    }

    let footnote_path = source_folder.join(source_paths::SEGMENT_FOOTNOTES_SUBFILE);
    let footnotes = match (read_footnote_file(&footnote_path)?, kind) {
        (Some(f), _) => f.footnotes,
        (None, SourceKind::Nyanadipa) => bail!("Footnote file not found: {}", footnote_path.display()),
        (None, _) => vec![],
    };
    let sutta_ids: HashSet<&str> = files.iter().map(|(_, f)| f.sutta_id.as_str()).collect();
    if let Some(e) = footnotes.iter().find(|e| !sutta_ids.contains(e.sutta_id.as_str())) {
        bail!("{}: note {} is for {}, which has no segment file", footnote_path.display(), e.marker, e.sutta_id);
    }

    let resolver = SuttaRefResolver::new(&pli_ms_uids(conn)?, &[]);
    let link_fn = |html: &str| link_sutta_refs(html, &|r| resolver.resolve_pli(r));

    conn.transaction::<_, anyhow::Error, _>(|conn| {
        let mut uids = Vec::with_capacity(files.len());
        for (path, file) in &files {
            let notes: Vec<FootnoteEntry> =
                footnotes.iter().filter(|e| e.sutta_id == file.sutta_id).cloned().collect();
            let pli_uid = format!("{}/pli/ms", file.sutta_id);
            if !sutta_record_exists(conn, &pli_uid)?
                && crate::bootstrap::skip_under_limit(&format!("{}: no template, {} was not imported", path.display(), pli_uid)) {
                continue;
            }
            let template = pali_template_from_db(conn, &file.sutta_id)?;
            let rec = build_segmented_record(kind, file, &template, &notes, &link_fn)
                .with_context(|| format!("Building {}", path.display()))?;
            if kind.is_suttacentral_html()
                && !sutta_record_exists(conn, &rec.uid)?
                && crate::bootstrap::skip_under_limit(&format!("{}: {} to replace was not imported", path.display(), rec.uid)) {
                continue;
            }
            if kind.is_suttacentral_html() {
                match replace_suttacentral_record(conn, kind, &rec)? {
                    Replaced::Done => uids.push(rec.uid),
                    Replaced::AlreadySegmented => logger::warn(&format!(
                        "{}: SuttaCentral now has this text segmented; keeping its text, not {}",
                        rec.uid, path.display(),
                    )),
                }
            } else {
                insert_segmented_record(conn, kind, &file.sutta_id, &rec)?;
                uids.push(rec.uid);
            }
        }
        Ok(uids)
    })
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

/// The value the title segment must hold: `<chapter>. <title>` for a
/// Dhammapada chapter, the Markdown heading for Nyanadipa.
pub fn expected_title(kind: SourceKind, sutta_id: &str, source_title: &str) -> Option<String> {
    if kind.is_dhammapada() {
        dhp_chapter_number(sutta_id).map(|n| format!("{}. {}", n, source_title))
    } else {
        Some(source_title.to_string())
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
/// (`dhammapada-munindo`, `nyanadipa-translations` or
/// `dhammapada-suttacentral`) against the Pāli of `sc_data_dir` (the `sc-data`
/// folder). Read-only.
pub fn check_source_folder(source_folder: &Path, sc_data_dir: &Path) -> Result<CheckReport> {
    let mut report = CheckReport::default();
    for kind in SourceKind::kinds_of_source_folder(source_folder)? {
        let r = check_kind(kind, source_folder, sc_data_dir)?;
        report.files_checked += r.files_checked;
        report.problems.extend(r.problems);
    }
    Ok(report)
}

fn check_kind(kind: SourceKind, source_folder: &Path, sc_data_dir: &Path) -> Result<CheckReport> {
    let dir = kind.segment_dir(source_folder);
    let mut report = CheckReport::default();
    let source_texts = kind.source_texts(source_folder, sc_data_dir)?;

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

    let source_path = kind.input_base(source_folder, sc_data_dir).join(&file.input);
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
            copyright: None,
            license: None,
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

    /// Feldmeier's Dhp 1–2 only; the other two translators have no fixture.
    fn suttacentral_folder() -> PathBuf {
        fixtures_dir().join(source_paths::DHAMMAPADA_SUTTACENTRAL_DIR)
    }

    #[test]
    fn test_good_fixtures_have_no_problems() {
        for folder in [munindo_folder(), nyanadipa_folder(), suttacentral_folder()] {
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
            SourceKind::Munindo.source_texts(&folder, &sc_data()).unwrap(),
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

    // === Building and importing records ===

    fn identity(s: &str) -> String {
        s.to_string()
    }

    fn snp4_2_fixture() -> (SegmentFile, Vec<FootnoteEntry>, IndexMap<String, String>) {
        let (_, footnotes, texts) = nyanadipa_texts();
        let file = texts.into_iter().next().unwrap().1;
        (file, footnotes.footnotes, pali_template(&sc_data(), "snp4.2").unwrap())
    }

    #[test]
    fn test_build_record_follows_the_template() {
        let (_, mut file, _, _, _, template) = munindo_fixture();
        // A file listing fewer ids than the template still gets every key.
        file.segments.retain(|s| s.id != "dhp2:5");
        set_text(&mut file, "dhp2:6", "with a pure heart, saṃsāra.");
        let rec = build_segmented_record(SourceKind::Munindo, &file, &template, &[], &identity).unwrap();

        assert_eq!(rec.uid, "dhp1-20/en/munindo");
        assert_eq!(rec.title, "The Pairs");
        let content: IndexMap<String, String> = serde_json::from_str(&rec.content_json).unwrap();
        assert!(content.keys().eq(template.keys()));
        assert_eq!(content["dhp2:5"], "");
        assert_eq!(content["dhp2:6"], "with a pure heart, saṁsāra.", "niggahita made consistent");
        // The verse-group ranges are written; the title stays.
        assert_eq!(content["dhp1:0.4"], "1.");
        assert_eq!(content["dhp2:0"], "2.");
        assert_eq!(content["dhp1:0.3"], "1. The Pairs");
        assert_eq!(rec.content_json_tmpl, serde_json::to_string(&template).unwrap());
        assert!(rec.content_plain.contains("all states of being are determined by the heart"), "{}", rec.content_plain);
        assert!(rec.description.is_none());
        assert!(rec.footnotes_value.is_none());
    }

    #[test]
    fn test_build_record_introduction_comes_first_in_the_plain_text() {
        let (_, mut file, _, _, _, template) = munindo_fixture();
        file.introduction = Some("<p>A chapter on <i>pairs</i>.</p>".to_string());
        let rec = build_segmented_record(SourceKind::Munindo, &file, &template, &[], &identity).unwrap();
        assert_eq!(rec.description.as_deref(), Some("<p>A chapter on <i>pairs</i>.</p>"));
        let intro_at = rec.content_plain.find("a chapter on pairs").expect(&rec.content_plain);
        assert!(intro_at < rec.content_plain.find("all states of being").unwrap());
    }

    #[test]
    fn test_build_record_rejects_what_the_template_lacks() {
        let (_, mut file, _, _, _, template) = munindo_fixture();
        file.segments.push(Segment { id: "dhp3:1".into(), text: "x".into(), source: "manual".into() });
        let err = build_segmented_record(SourceKind::Munindo, &file, &template, &[], &identity).unwrap_err();
        assert!(err.to_string().contains("dhp3:1"), "{}", err);

        let (file, mut notes, template) = snp4_2_fixture();
        notes[0].segment_id = "snp4.2:99.1".into();
        assert!(build_segmented_record(SourceKind::Nyanadipa, &file, &template, &notes, &identity).is_err());

        let (_, mut file, _, _, _, template) = munindo_fixture();
        set_text(&mut file, "dhp1:0.3", "");
        assert!(build_segmented_record(SourceKind::Munindo, &file, &template, &[], &identity).is_err());
    }

    #[test]
    fn test_build_record_nyanadipa_notes() {
        let (file, notes, template) = snp4_2_fixture();
        let rec = build_segmented_record(SourceKind::Nyanadipa, &file, &template, &notes, &|s: &str| format!("[{}]", s)).unwrap();
        assert_eq!(rec.uid, "snp4.2/en/nyanadipa");
        assert_eq!(rec.title, "The Cave");
        assert_eq!(rec.footnote_count, 1);
        let value: IndexMap<String, String> = serde_json::from_str(rec.footnotes_value.as_deref().unwrap()).unwrap();
        let simsapa_backend::footnotes::CommentValue::Footnotes(stored) =
            simsapa_backend::footnotes::parse_comment_value(&value["snp4.2:3.2"])
        else {
            panic!("{}", value["snp4.2:3.2"]);
        };
        assert_eq!((stored[0].number, stored[0].word_index), (1, Some(2)));
        assert!(stored[0].html.starts_with('[') && stored[0].html.contains("avadāniyā"), "link_fn applied");
        assert!(!rec.content_plain.contains("the root meaning"));
        // No range in a Sutta Nipāta text.
        assert!(!rec.content_json.contains('–'));
    }

    #[test]
    fn test_record_title() {
        assert_eq!(record_title(SourceKind::Munindo, "14. The Buddha"), "The Buddha");
        assert_eq!(record_title(SourceKind::Nyanadipa, "Ajita’s Question"), "Ajita’s Question");
        assert_eq!(record_title(SourceKind::Nyanadipa, "4. Not a chapter"), "4. Not a chapter");
    }

    #[test]
    fn test_import_fixture_folders() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = test_support::scratch_appdata(&dir);
        for id in ["dhp1-20", "snp4.2"] {
            test_support::insert_pali(&mut conn, id, &serde_json::to_string(&pali_template(&sc_data(), id).unwrap()).unwrap());
        }
        assert_eq!(import_source_folder(&mut conn, &munindo_folder()).unwrap(), ["dhp1-20/en/munindo"]);
        assert_eq!(import_source_folder(&mut conn, &nyanadipa_folder()).unwrap(), ["snp4.2/en/nyanadipa"]);

        let row = test_support::record_row(&mut conn, "dhp1-20/en/munindo");
        assert_eq!(row.title, "The Pairs");
        assert_eq!(row.sutta_ref, "Dhp 1-20");
        assert_eq!(row.source_info.as_deref(), Some("dhammapada-munindo/bilara"));
        assert!(row.content_html.is_none());
        let row = test_support::record_row(&mut conn, "snp4.2/en/nyanadipa");
        assert_eq!(row.source_info.as_deref(), Some("nyanadipa-translations/bilara"));
    }

    #[test]
    fn test_normalized_suttacentral_html() {
        let html = fs::read_to_string(sc_data().join("html_text/en/pli/sutta/kn/dhp/feldmeier/dhp/dhp1-20.html")).unwrap();
        let n = normalized_source_text(&html, SourceKind::Feldmeier);
        assert_eq!(n.title, "Pairs");
        assert!(n.text.starts_with("All phenomena are preceded by the mind, Created"), "{}", n.text);
        assert!(n.text.ends_with("Happiness follows as an ever-present shadow."), "the footer is not text: {}", n.text);
        assert!(!n.text.contains("SC 1"), "verse anchors are removed");
        assert!(n.footnote_markers.is_empty());
        assert_eq!(expected_title(SourceKind::Feldmeier, "dhp1-20", &n.title).unwrap(), "1. Pairs");
        assert_eq!(record_title(SourceKind::Feldmeier, "1. Pairs"), "Pairs");
    }

    #[test]
    fn test_suttacentral_kinds_and_folders() {
        let folder = suttacentral_folder();
        assert_eq!(SourceKind::kinds_of_source_folder(&folder).unwrap(), SUTTACENTRAL_DHP_KINDS);
        assert_eq!(SourceKind::Feldmeier.segment_dir(&folder), folder.join("bilara/feldmeier"));
        assert_eq!(SourceKind::Feldmeier.input_base(&folder, &sc_data()), sc_data());
        assert_eq!(SourceKind::Munindo.input_base(&munindo_folder(), &sc_data()), munindo_folder());
        assert_eq!(SourceKind::Anandajoti.source_info(), "dhammapada-suttacentral/bilara/anandajoti");
        assert_eq!(
            SourceKind::Feldmeier.source_texts(&folder, &sc_data()).unwrap(),
            vec!["html_text/en/pli/sutta/kn/dhp/feldmeier/dhp/dhp1-20.html"],
        );
        assert!(SourceKind::Anandajoti.source_texts(&folder, &sc_data()).unwrap().is_empty());
    }

    /// A SuttaCentral HTML-only record, as the SuttaCentral import writes it.
    fn insert_html_record(conn: &mut SqliteConnection, uid: &str) -> i32 {
        diesel::sql_query(
            "INSERT INTO suttas (uid, sutta_ref, nikaya, language, title, content_html, source_uid) \
             VALUES (?, 'Dhp 1-20', 'kn', 'en', '1. Pairs ', '<div>old</div>', 'feldmeier')",
        )
        .bind::<diesel::sql_types::Text, _>(uid)
        .execute(conn)
        .unwrap();
        suttas::table.filter(suttas::uid.eq(uid)).select(suttas::id).first(conn).unwrap()
    }

    #[test]
    fn test_import_suttacentral_replaces_the_html_record() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = test_support::scratch_appdata(&dir);
        let template = pali_template(&sc_data(), "dhp1-20").unwrap();
        test_support::insert_pali(&mut conn, "dhp1-20", &serde_json::to_string(&template).unwrap());

        // No record to replace: an error naming it.
        let err = import_kind(&mut conn, SourceKind::Feldmeier, &suttacentral_folder()).unwrap_err();
        assert!(format!("{:#}", err).contains("dhp1-20/en/feldmeier"), "{:#}", err);

        let id = insert_html_record(&mut conn, "dhp1-20/en/feldmeier");
        let uids = import_kind(&mut conn, SourceKind::Feldmeier, &suttacentral_folder()).unwrap();
        assert_eq!(uids, ["dhp1-20/en/feldmeier"]);

        let row = test_support::record_row(&mut conn, "dhp1-20/en/feldmeier");
        assert!(row.content_html.is_none());
        assert_eq!(row.title, "Pairs", "the translator's title, without the chapter number");
        assert_eq!(row.sutta_ref, "Dhp 1-20");
        assert_eq!(row.source_info.as_deref(), Some("dhammapada-suttacentral/bilara/feldmeier"));
        let content = test_support::content_map(&row.content_json);
        assert!(content.keys().eq(template.keys()), "content_json keys follow the Pāli template");
        assert_eq!(content["dhp1:0.3"], "1. Pairs");
        assert_eq!(content["dhp1:0.4"], "1.", "the verse-group range");
        assert_eq!(content["dhp2:0"], "2.");
        assert_eq!(content["dhp1:2"], "Created by the mind,\nAnd have the mind as their master.\n");
        assert!(row.content_plain.contains("all phenomena are preceded by the mind"), "{}", row.content_plain);

        let (same_id, copyright, license): (i32, Option<String>, Option<String>) = suttas::table
            .filter(suttas::uid.eq("dhp1-20/en/feldmeier"))
            .select((suttas::id, suttas::copyright, suttas::license))
            .first(&mut conn)
            .unwrap();
        assert_eq!(same_id, id, "updated in place");
        assert_eq!(copyright.as_deref(), Some("Peter Feldmeier"));
        assert_eq!(license.as_deref(), Some("Creative Commons Attribution-NonCommercial 4.0 International License"));
    }

    #[test]
    fn test_import_suttacentral_keeps_a_record_already_segmented() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = test_support::scratch_appdata(&dir);
        test_support::insert_pali(&mut conn, "dhp1-20", &serde_json::to_string(&pali_template(&sc_data(), "dhp1-20").unwrap()).unwrap());
        insert_html_record(&mut conn, "dhp1-20/en/feldmeier");
        diesel::update(suttas::table.filter(suttas::uid.eq("dhp1-20/en/feldmeier")))
            .set(suttas::content_json.eq(Some(r#"{"dhp1:1":"SuttaCentral's own"}"#)))
            .execute(&mut conn)
            .unwrap();

        let uids = import_kind(&mut conn, SourceKind::Feldmeier, &suttacentral_folder()).unwrap();
        assert!(uids.is_empty());
        let row = test_support::record_row(&mut conn, "dhp1-20/en/feldmeier");
        assert_eq!(row.content_json, r#"{"dhp1:1":"SuttaCentral's own"}"#);
        assert_eq!(row.title, "1. Pairs ");
    }

    #[test]
    fn test_import_without_pali_template_names_the_uid() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = test_support::scratch_appdata(&dir);
        let err = import_source_folder(&mut conn, &munindo_folder()).unwrap_err();
        assert!(format!("{:#}", err).contains("dhp1-20/pli/ms"), "{:#}", err);
        // Nothing was written.
        let n: i64 = suttas::table.count().get_result(&mut conn).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_import_missing_or_empty_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = test_support::scratch_appdata(&dir);
        let folder = dir.path().join(source_paths::DHAMMAPADA_MUNINDO_DIR);
        fs::create_dir(&folder).unwrap();
        assert!(import_source_folder(&mut conn, &folder).is_err(), "no bilara/ folder");
        fs::create_dir(folder.join(source_paths::MUNINDO_BILARA_SUBDIR)).unwrap();
        assert!(import_source_folder(&mut conn, &folder).is_err(), "empty bilara/ folder");

        // Nyanadipa without its footnote file.
        let folder = dir.path().join(source_paths::NYANADIPA_DIR);
        fs::create_dir_all(folder.join(source_paths::NYANADIPA_BILARA_SUBDIR)).unwrap();
        fs::copy(nyanadipa_folder().join("bilara/snp4.2.json"), folder.join("bilara/snp4.2.json")).unwrap();
        test_support::insert_pali(&mut conn, "snp4.2", &serde_json::to_string(&pali_template(&sc_data(), "snp4.2").unwrap()).unwrap());
        let err = import_source_folder(&mut conn, &folder).unwrap_err();
        assert!(err.to_string().contains("footnotes.json"), "{}", err);
    }
}

/// A scratch appdata for the importer tests, so they never write to the dist
/// database.
#[cfg(test)]
pub mod test_support {
    use super::*;
    use diesel::sql_query;
    use diesel::sql_types::Text;

    /// A scratch appdata with the real schema, in `dir`.
    pub fn scratch_appdata(dir: &tempfile::TempDir) -> SqliteConnection {
        let path = dir.path().join("appdata.sqlite3");
        let mut conn = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
        crate::bootstrap::run_migrations(&mut conn).unwrap();
        conn
    }

    /// Inserts a `<sutta_id>/pli/ms` record with only what the importers read.
    pub fn insert_pali(conn: &mut SqliteConnection, sutta_id: &str, template: &str) {
        sql_query("INSERT INTO suttas (uid, sutta_ref, nikaya, language, content_json_tmpl, source_uid) VALUES (?, ?, 'kn', 'pli', ?, 'ms')")
            .bind::<Text, _>(format!("{}/pli/ms", sutta_id))
            .bind::<Text, _>(sutta_id)
            .bind::<Text, _>(template)
            .execute(conn)
            .unwrap();
    }

    /// Imports `source_folder` (real bootstrap assets) into a scratch appdata
    /// seeded with the Dhammapada and Sutta Nipāta Pāli templates of the dist
    /// database, which is opened read-only, and with an HTML-only record for
    /// each Dhammapada chapter of the SuttaCentral HTML translators (the uid
    /// and title of the dist database's record), as the SuttaCentral import
    /// writes them. `None` (the test skips) when the folder or the dist
    /// database is absent.
    pub fn import_into_scratch(source_folder: &Path) -> Option<(tempfile::TempDir, SqliteConnection, Vec<String>)> {
        let dist = source_paths::bootstrap_assets_dir_from_crate().join("dist/simsapa/app-assets/appdata.sqlite3");
        if !matches!(source_folder.join("bilara").try_exists(), Ok(true)) || !matches!(dist.try_exists(), Ok(true)) {
            simsapa_backend::logger::write_stdout(format_args!("Skipping: {} or {} not found\n", source_folder.display(), dist.display()));
            return None;
        }
        let mut dist_conn =
            SqliteConnection::establish(&crate::bootstrap::suttas_core::sqlite_readonly_uri(&dist).unwrap()).unwrap();
        let templates: Vec<(String, Option<String>)> = suttas::table
            .filter(suttas::uid.like("dhp%/pli/ms").or(suttas::uid.like("snp%/pli/ms")))
            .select((suttas::uid, suttas::content_json_tmpl))
            .load(&mut dist_conn)
            .unwrap();

        let translators: Vec<&str> = SUTTACENTRAL_DHP_KINDS.iter().map(|k| k.translator()).collect();
        let html_records: Vec<(String, String, Option<String>, Option<String>)> = suttas::table
            .filter(suttas::uid.like("dhp%/en/%"))
            .filter(suttas::source_uid.eq_any(&translators))
            .filter(suttas::uid.not_like("%.att/%"))
            .select((suttas::uid, suttas::sutta_ref, suttas::title, suttas::source_uid))
            .load(&mut dist_conn)
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let mut conn = scratch_appdata(&dir);
        for (uid, tmpl) in templates {
            if let (Some(sutta_id), Some(tmpl)) = (uid.strip_suffix("/pli/ms"), tmpl) {
                insert_pali(&mut conn, sutta_id, &tmpl);
            }
        }
        for (uid, sutta_ref, title, source_uid) in html_records {
            sql_query("INSERT INTO suttas (uid, sutta_ref, nikaya, language, title, content_html, source_uid) VALUES (?, ?, 'kn', 'en', ?, '<div>html</div>', ?)")
                .bind::<Text, _>(uid)
                .bind::<Text, _>(sutta_ref)
                .bind::<diesel::sql_types::Nullable<Text>, _>(title)
                .bind::<diesel::sql_types::Nullable<Text>, _>(source_uid)
                .execute(&mut conn)
                .unwrap();
        }
        let uids = import_source_folder(&mut conn, source_folder).unwrap();
        Some((dir, conn, uids))
    }

    /// The titles of the dist database's records of `source_uid`, by uid.
    pub fn dist_titles(source_uid: &str) -> HashMap<String, String> {
        let dist = source_paths::bootstrap_assets_dir_from_crate().join("dist/simsapa/app-assets/appdata.sqlite3");
        let mut conn =
            SqliteConnection::establish(&crate::bootstrap::suttas_core::sqlite_readonly_uri(&dist).unwrap()).unwrap();
        suttas::table
            .filter(suttas::source_uid.eq(source_uid))
            .select((suttas::uid, suttas::title))
            .load::<(String, Option<String>)>(&mut conn)
            .unwrap()
            .into_iter()
            .map(|(uid, title)| (uid, title.unwrap_or_default()))
            .collect()
    }

    /// Asserts that every imported record has the title of the same uid in the
    /// dist database, or of `renamed[uid]` when the dist database still has
    /// the record under its old uid.
    pub fn assert_titles_unchanged(conn: &mut SqliteConnection, uids: &[String], source_uid: &str, renamed: &[(&str, &str)]) {
        let dist = dist_titles(source_uid);
        let mut differ = Vec::new();
        for uid in uids {
            let old_uid = renamed
                .iter()
                .find(|(new, _)| new == uid)
                .map(|(_, old)| *old)
                .filter(|old| dist.contains_key(*old))
                .unwrap_or(uid);
            let title = record_row(conn, uid).title;
            match dist.get(old_uid) {
                Some(old) if *old == title => {}
                old => differ.push(format!("{}: {:?}, the dist database has {:?} under {}", uid, title, old, old_uid)),
            }
        }
        assert!(differ.is_empty(), "{:#?}", differ);
    }

    pub struct RecordRow {
        pub title: String,
        pub sutta_ref: String,
        pub content_json: String,
        pub content_json_tmpl: String,
        pub content_plain: String,
        pub content_html: Option<String>,
        pub source_info: Option<String>,
    }

    pub fn record_row(conn: &mut SqliteConnection, uid: &str) -> RecordRow {
        let (title, sutta_ref, content_json, content_json_tmpl, content_plain, content_html, source_info): (
            Option<String>, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>,
        ) = suttas::table
            .filter(suttas::uid.eq(uid))
            .select((
                suttas::title, suttas::sutta_ref, suttas::content_json, suttas::content_json_tmpl,
                suttas::content_plain, suttas::content_html, suttas::source_info,
            ))
            .first(conn)
            .unwrap_or_else(|e| panic!("{}: {}", uid, e));
        RecordRow {
            title: title.unwrap_or_default(),
            sutta_ref,
            content_json: content_json.unwrap_or_default(),
            content_json_tmpl: content_json_tmpl.unwrap_or_default(),
            content_plain: content_plain.unwrap_or_default(),
            content_html,
            source_info,
        }
    }

    /// A `content_json` or template in document order.
    pub fn content_map(json: &str) -> IndexMap<String, String> {
        serde_json::from_str(json).unwrap()
    }
}
