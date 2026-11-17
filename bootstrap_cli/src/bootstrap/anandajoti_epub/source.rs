//! The content files as one stream of block elements, and the notes files.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use anyhow::Result;
use ego_tree::NodeRef;
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};

use super::epub::EpubFiles;
use super::report::Report;

static RE_NOTE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\{(\d+)\}$").unwrap());

/// One block element of a content file.
#[derive(Clone, Copy)]
pub struct Block<'a> {
    pub file: &'a str,
    pub el: ElementRef<'a>,
}

impl<'a> Block<'a> {
    pub fn tag(&self) -> &'a str {
        self.el.value().name()
    }
    pub fn class(&self) -> &'a str {
        self.el.value().attr("class").unwrap_or("")
    }
}

/// A note of the notes file.
pub struct Note<'a> {
    pub number: u32,
    /// Its paragraphs. In the first, the leading anchor and back-link are
    /// skipped: rendering starts at child node `first_paragraph_start`.
    pub paragraphs: Vec<ElementRef<'a>>,
    pub first_paragraph_start: usize,
}

/// The notes, keyed by `(file, anchor id)`, the target of a note ref.
pub struct Notes<'a> {
    pub by_target: BTreeMap<(String, String), Note<'a>>,
    /// The files the note refs point to (`notes.html`).
    pub files: BTreeSet<String>,
}

impl Notes<'_> {
    /// `(file, anchor)` of a note ref's `href`, if it points into a notes
    /// file.
    pub fn target_of(&self, href: &str) -> Option<(String, String)> {
        let (file, anchor) = href.split_once('#')?;
        self.files.contains(file).then(|| (file.to_string(), anchor.to_string()))
    }
}

/// Whether `el` is a page marker span.
pub fn is_page_marker(el: &ElementRef, page_marker_classes: &[&str]) -> bool {
    el.value().name() == "span" && page_marker_classes.contains(&el.value().attr("class").unwrap_or(""))
}

/// The link text of a note ref, `{4}` → 4.
pub fn note_ref_number(el: &ElementRef) -> Option<u32> {
    let text: String = el.text().collect();
    RE_NOTE_NUMBER.captures(text.trim()).and_then(|c| c[1].parse().ok())
}

/// Whether `el` is a note ref: a link whose text is `{N}`. Used before the
/// notes files are known; afterwards [`Notes::target_of`] decides.
pub fn looks_like_note_ref(el: &ElementRef) -> bool {
    el.value().name() == "a" && el.value().attr("href").is_some() && note_ref_number(el).is_some()
}

/// Collapse whitespace (the no-break space included) and trim.
pub fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text of `node`, with note refs and page markers left out and `<br>`
/// read as a space, whitespace collapsed.
pub fn text_without_refs(node: NodeRef<Node>, page_marker_classes: &[&str]) -> String {
    let mut out = String::new();
    push_text_without_refs(node, page_marker_classes, &mut out);
    collapse_whitespace(&out)
}

fn push_text_without_refs(node: NodeRef<Node>, page_marker_classes: &[&str], out: &mut String) {
    match node.value() {
        Node::Text(text) => out.push_str(text),
        Node::Element(el) => {
            if el.name() == "br" {
                out.push(' ');
                return;
            }
            if let Some(element) = ElementRef::wrap(node)
                && (looks_like_note_ref(&element) || is_page_marker(&element, page_marker_classes))
            {
                return;
            }
            for child in node.children() {
                push_text_without_refs(child, page_marker_classes, out);
            }
        }
        _ => {}
    }
}

/// The parsed content files, in spine order.
pub struct ParsedFile {
    pub name: String,
    pub doc: Html,
}

pub fn parse_files(epub: &EpubFiles) -> Result<Vec<ParsedFile>> {
    epub.spine()?
        .into_iter()
        .map(|name| {
            let text = epub.get(&name)?;
            Ok(ParsedFile { doc: Html::parse_document(text), name })
        })
        .collect()
}

static BODY: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body").unwrap());

/// The block elements of a file's `<body>`. Text directly in the body that
/// is not whitespace is reported.
pub fn body_blocks<'a>(file: &'a ParsedFile, report: &mut Report) -> Vec<Block<'a>> {
    let Some(body) = file.doc.select(&BODY).next() else {
        return Vec::new();
    };
    let mut blocks = Vec::new();
    for child in body.children() {
        match child.value() {
            Node::Element(_) => blocks.push(Block { file: &file.name, el: ElementRef::wrap(child).unwrap() }),
            Node::Text(text) if !text.trim().is_empty() => {
                report.defect(format!("{}: text outside any paragraph: {:?}", file.name, collapse_whitespace(text)));
            }
            _ => {}
        }
    }
    blocks
}

/// The head of a note paragraph: it opens (whitespace aside) with an empty
/// anchor and a `{N}` link. Gives the anchor id, N, and the index of the
/// child node after the link.
pub fn note_head(p: &ElementRef) -> Option<(String, u32, usize)> {
    let mut elements = p.children().enumerate().filter(|(_, node)| match node.value() {
        Node::Text(text) => !text.trim().is_empty(),
        _ => true,
    });
    let (_, anchor) = elements.next()?;
    let (link_index, link) = elements.next()?;
    let (anchor, link) = (ElementRef::wrap(anchor)?, ElementRef::wrap(link)?);
    let id = anchor.value().attr("id")?;
    let is_head = anchor.value().name() == "a"
        && anchor.value().attr("href").is_none()
        && looks_like_note_ref(&link);
    is_head.then(|| (id.to_string(), note_ref_number(&link).unwrap(), link_index + 1))
}

/// Whether most of a file's paragraphs are note heads: the notes file, as
/// against a content file that its back-links point to.
pub fn is_notes_file(file: &ParsedFile) -> bool {
    static P: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body p").unwrap());
    let (mut heads, mut paragraphs) = (0, 0);
    for p in file.doc.select(&P) {
        paragraphs += 1;
        if note_head(&p).is_some() {
            heads += 1;
        }
    }
    paragraphs > 0 && heads * 2 > paragraphs
}

/// Read the notes files: one `<p>` per note, opening with an anchor and a
/// `{N}` back-link; a `<p>` without them continues the note before it.
pub fn parse_notes<'a>(files: &'a [ParsedFile], note_files: &BTreeSet<String>, report: &mut Report) -> Notes<'a> {
    let mut by_target: BTreeMap<(String, String), Note<'a>> = BTreeMap::new();
    for file in files.iter().filter(|f| note_files.contains(&f.name)) {
        let mut last: Option<(String, String)> = None;
        for block in body_blocks(file, report) {
            if block.tag() != "p" {
                continue;
            }
            match note_head(&block.el) {
                Some((anchor, number, start)) => {
                    let key = (file.name.clone(), anchor);
                    if by_target.contains_key(&key) {
                        report.defect(format!("{}: note anchor {} appears twice", file.name, key.1));
                    }
                    by_target.insert(
                        key.clone(),
                        Note { number, paragraphs: vec![block.el], first_paragraph_start: start },
                    );
                    last = Some(key);
                }
                None => match last.as_ref().and_then(|key| by_target.get_mut(key)) {
                    Some(note) => note.paragraphs.push(block.el),
                    None => report.defect(format!("{}: a paragraph before the first note", file.name)),
                },
            }
        }
    }
    Notes { by_target, files: note_files.clone() }
}


/// The notes files: those the `{N}` links point to, and whose paragraphs
/// are notes. The notes' back-links are `{N}` links too, to content files.
pub fn find_notes_files(files: &[ParsedFile]) -> BTreeSet<String> {
    let mut targets: BTreeSet<String> = BTreeSet::new();
    for file in files {
        for node in file.doc.root_element().descendants() {
            if let Some(el) = ElementRef::wrap(node)
                && looks_like_note_ref(&el)
                && let Some((target, _)) = el.value().attr("href").unwrap().split_once('#')
                && !target.is_empty()
                && target != file.name
            {
                targets.insert(target.to_string());
            }
        }
    }
    files
        .iter()
        .filter(|f| targets.contains(&f.name) && is_notes_file(f))
        .map(|f| f.name.clone())
        .collect()
}
