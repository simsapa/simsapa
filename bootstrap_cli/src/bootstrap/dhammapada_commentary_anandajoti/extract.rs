//! The epub as a structure of vaggas, stories and sub-stories, the notes, and
//! the defect checks. Rendering is in `page.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use ego_tree::NodeRef;
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};

use super::epub::{EpubFiles, TocEntry};
use super::page::{render_vagga, Page};
use super::{Manifest, ManifestVagga, MANIFEST_FILE, REPORT_FILE, SC_VAGGA_RANGES};

/// The gray spans that hold page markers (`{1.3}`, `[28.146]`).
pub const PAGE_MARKER_CLASSES: [&str; 5] = ["t10", "t17", "t21", "t25", "t27"];

static RE_VAGGA_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\. ").unwrap());
static RE_STORY_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\.(\d+) ").unwrap());
static RE_VERSE_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Dhp (\d+)(?:-(\d+))?$").unwrap());
static RE_NOTE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\{(\d+)\}$").unwrap());
static RE_COUNT_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^In the (.+?) (?:there are|are) ([a-z]+(?:-[a-z]+)?)(?: stories)?\.$").unwrap()
});
static RE_TOTAL_STORIES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d+) stories\b").unwrap());

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

pub struct Vagga<'a> {
    pub number: u32,
    /// "1. The Chapter about the Pairs"
    pub title: String,
    /// "Yamakavagga"
    pub title_pali: String,
    pub heading: Block<'a>,
    /// Blocks between the vagga heading and its first story.
    pub intro: Vec<Block<'a>>,
    pub stories: Vec<Story<'a>>,
}

pub struct Story<'a> {
    pub vagga_number: u32,
    pub number: u32,
    /// The heading text, note refs and page markers left out.
    pub title: String,
    pub heading: Block<'a>,
    pub pali_title: Option<Block<'a>>,
    pub meta: Vec<Block<'a>>,
    /// The metadata line naming the verses, verbatim: "Dhp 54-55".
    pub verse_line: Option<String>,
    pub verses: Vec<u32>,
    pub body: Vec<BodyItem<'a>>,
}

impl Story<'_> {
    /// "1.1"
    pub fn label(&self) -> String {
        format!("{}.{}", self.vagga_number, self.number)
    }
    pub fn substories(&self) -> impl Iterator<Item = &SubStory<'_>> {
        self.body.iter().filter_map(|item| match item {
            BodyItem::SubStory(sub) => Some(sub),
            BodyItem::Block(_) => None,
        })
    }
}

pub enum BodyItem<'a> {
    Block(Block<'a>),
    SubStory(SubStory<'a>),
}

pub struct SubStory<'a> {
    pub heading: Block<'a>,
    /// The heading text, note refs and page markers left out.
    pub title: String,
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

/// The defects found in the source, and the markup the extraction does not
/// map. Lines, in the order found.
#[derive(Default)]
pub struct Report {
    pub defects: Vec<String>,
    pub unmapped: BTreeMap<String, usize>,
}

impl Report {
    pub fn defect(&mut self, line: String) {
        self.defects.push(line);
    }
    pub fn unmapped(&mut self, what: String) {
        *self.unmapped.entry(what).or_default() += 1;
    }
}

/// Whether `el` is a page marker span.
pub fn is_page_marker(el: &ElementRef) -> bool {
    el.value().name() == "span" && PAGE_MARKER_CLASSES.contains(&el.value().attr("class").unwrap_or(""))
}

/// The link text of a note ref, `{4}` → 4.
pub fn note_ref_number(el: &ElementRef) -> Option<u32> {
    let text: String = el.text().collect();
    RE_NOTE_NUMBER.captures(text.trim()).and_then(|c| c[1].parse().ok())
}

/// Whether `el` is a note ref: a link whose text is `{N}`. Used before the
/// notes files are known; afterwards [`Notes::target_of`] decides.
fn looks_like_note_ref(el: &ElementRef) -> bool {
    el.value().name() == "a" && el.value().attr("href").is_some() && note_ref_number(el).is_some()
}

/// Collapse whitespace (the no-break space included) and trim.
pub fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text of `node`, with note refs and page markers left out and `<br>`
/// read as a space, whitespace collapsed.
pub fn text_without_refs(node: NodeRef<Node>) -> String {
    let mut out = String::new();
    push_text_without_refs(node, &mut out);
    collapse_whitespace(&out)
}

fn push_text_without_refs(node: NodeRef<Node>, out: &mut String) {
    match node.value() {
        Node::Text(text) => out.push_str(text),
        Node::Element(el) => {
            if el.name() == "br" {
                out.push(' ');
                return;
            }
            if let Some(element) = ElementRef::wrap(node)
                && (looks_like_note_ref(&element) || is_page_marker(&element))
            {
                return;
            }
            for child in node.children() {
                push_text_without_refs(child, out);
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
fn body_blocks<'a>(file: &'a ParsedFile, report: &mut Report) -> Vec<Block<'a>> {
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

/// What a heading of the content files is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingKind {
    Vagga,
    Story,
    PaliTitle,
    SubStory,
    /// Front matter, the Conclusion, or an unnumbered `<h1>`.
    Other,
}

/// A heading, for the cross-check with `toc.ncx`.
pub struct HeadingRecord {
    pub file: String,
    /// The heading's own id and its descendants' anchor ids.
    pub ids: Vec<String>,
    pub kind: HeadingKind,
    pub text: String,
    /// "vagga 1, story 1.1"
    pub context: String,
}

pub struct Book<'a> {
    pub vaggas: Vec<Vagga<'a>>,
    pub notes: Notes<'a>,
    /// The blocks after the last vagga (the Conclusion).
    pub after: Vec<Block<'a>>,
    pub headings: Vec<HeadingRecord>,
}

/// The head of a note paragraph: it opens (whitespace aside) with an empty
/// anchor and a `{N}` link. Gives the anchor id, N, and the index of the
/// child node after the link.
fn note_head(p: &ElementRef) -> Option<(String, u32, usize)> {
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
fn is_notes_file(file: &ParsedFile) -> bool {
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
fn parse_notes<'a>(files: &'a [ParsedFile], note_files: &BTreeSet<String>, report: &mut Report) -> Notes<'a> {
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

/// Split a vagga heading into the English title and the Pāli name: at the
/// `<br>`, or, without one, at the last ", " when the rest ends in "vagga".
fn split_vagga_heading(el: &ElementRef, file: &str, report: &mut Report) -> (String, String) {
    let mut before = String::new();
    let mut after = String::new();
    let mut seen_br = false;
    for child in el.children() {
        if let Some(child_el) = ElementRef::wrap(child)
            && child_el.value().name() == "br"
        {
            seen_br = true;
            continue;
        }
        let text = text_without_refs(child);
        let target = if seen_br { &mut after } else { &mut before };
        if !text.is_empty() {
            if !target.is_empty() {
                target.push(' ');
            }
            target.push_str(&text);
        }
    }
    let before = collapse_whitespace(&before);
    let after = collapse_whitespace(&after);
    if seen_br {
        return (before.trim_end_matches(',').trim_end().to_string(), after);
    }
    match before.rsplit_once(", ") {
        Some((english, pali)) if pali.ends_with("vagga") => {
            report.defect(format!(
                "{}: vagga heading \"{}\" has no line break between the English and the Pāli name; split at the last \", \"",
                file, before
            ));
            (english.to_string(), pali.to_string())
        }
        _ => {
            report.defect(format!("{}: vagga heading \"{}\" has no Pāli name", file, before));
            (before, String::new())
        }
    }
}

/// The ids of `el` and of its descendants.
fn ids_in(el: &ElementRef) -> Vec<String> {
    let mut ids: Vec<String> = el.value().attr("id").map(|id| vec![id.to_string()]).unwrap_or_default();
    for node in el.descendants() {
        if let Some(d) = ElementRef::wrap(node)
            && node.id() != el.id()
            && let Some(id) = d.value().attr("id")
        {
            ids.push(id.to_string());
        }
    }
    ids
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Stage {
    FrontMatter,
    Vaggas,
    After,
}

/// Walk the content files as one stream of blocks, and switch vagga, story
/// and sub-story on each heading.
pub fn read_book<'a>(files: &'a [ParsedFile], report: &mut Report) -> Book<'a> {
    // The notes files: those the `{N}` links point to, and whose paragraphs
    // are notes. The notes' back-links are `{N}` links too, to content files.
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
    let note_files: BTreeSet<String> = files
        .iter()
        .filter(|f| targets.contains(&f.name) && is_notes_file(f))
        .map(|f| f.name.clone())
        .collect();
    let notes = parse_notes(files, &note_files, report);

    let mut vaggas: Vec<Vagga<'a>> = Vec::new();
    let mut after: Vec<Block<'a>> = Vec::new();
    let mut headings: Vec<HeadingRecord> = Vec::new();
    let mut stage = Stage::FrontMatter;
    // The last block was the story heading or its Pāli title, so that `p7`
    // paragraphs are metadata.
    let mut in_story_head = false;

    for file in files.iter().filter(|f| !note_files.contains(&f.name)) {
        for block in body_blocks(file, report) {
            let tag = block.tag();
            let is_heading = matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
            let text = if is_heading { text_without_refs(*block.el) } else { String::new() };
            let mut kind = HeadingKind::Other;

            match (tag, stage) {
                ("h1", _) => {
                    in_story_head = false;
                    match RE_VAGGA_HEADING.captures(&text) {
                        Some(c) if stage != Stage::After => {
                            let number: u32 = c[1].parse().unwrap();
                            let expected = vaggas.last().map(|v| v.number + 1).unwrap_or(1);
                            if number != expected {
                                report.defect(format!("{}: vagga {} follows vagga {}", file.name, number, expected - 1));
                            }
                            let (title, title_pali) = split_vagga_heading(&block.el, &file.name, report);
                            vaggas.push(Vagga { number, title, title_pali, heading: block, intro: Vec::new(), stories: Vec::new() });
                            stage = Stage::Vaggas;
                            kind = HeadingKind::Vagga;
                        }
                        _ => {
                            if stage == Stage::Vaggas {
                                stage = Stage::After;
                            }
                            if stage == Stage::After {
                                after.push(block);
                            }
                        }
                    }
                }
                (_, Stage::FrontMatter) => {}
                (_, Stage::After) => after.push(block),
                ("h2", Stage::Vaggas) => {
                    let vagga = vaggas.last_mut().unwrap();
                    match RE_STORY_HEADING.captures(&text) {
                        Some(c) => {
                            let (v, n): (u32, u32) = (c[1].parse().unwrap(), c[2].parse().unwrap());
                            if v != vagga.number {
                                report.defect(format!("{}: story {}.{} is in vagga {}", file.name, v, n, vagga.number));
                            }
                            let expected = vagga.stories.last().map(|s| s.number + 1).unwrap_or(1);
                            if n != expected {
                                report.defect(format!("{}: story {}.{} follows story {}.{}", file.name, v, n, vagga.number, expected - 1));
                            }
                            vagga.stories.push(Story {
                                vagga_number: vagga.number,
                                number: n,
                                title: text.clone(),
                                heading: block,
                                pali_title: None,
                                meta: Vec::new(),
                                verse_line: None,
                                verses: Vec::new(),
                                body: Vec::new(),
                            });
                            in_story_head = true;
                            kind = HeadingKind::Story;
                        }
                        None => {
                            report.defect(format!("{}: unnumbered <h2> \"{}\" inside vagga {}", file.name, text, vagga.number));
                            push_body(vagga, BodyItem::Block(block));
                            in_story_head = false;
                        }
                    }
                }
                ("h3", Stage::Vaggas) if in_story_head && vaggas.last().unwrap().stories.last().unwrap().pali_title.is_none() => {
                    vaggas.last_mut().unwrap().stories.last_mut().unwrap().pali_title = Some(block);
                    kind = HeadingKind::PaliTitle;
                }
                ("p", Stage::Vaggas) if in_story_head && matches!(block.class(), "p7" | "p14") => {
                    vaggas.last_mut().unwrap().stories.last_mut().unwrap().meta.push(block);
                }
                ("h4", Stage::Vaggas) => {
                    in_story_head = false;
                    let vagga = vaggas.last_mut().unwrap();
                    if vagga.stories.is_empty() {
                        report.defect(format!("{}: sub-story \"{}\" before the first story of vagga {}", file.name, text, vagga.number));
                    }
                    push_body(vagga, BodyItem::SubStory(SubStory { heading: block, title: text.clone() }));
                    kind = HeadingKind::SubStory;
                }
                (_, Stage::Vaggas) => {
                    in_story_head = false;
                    push_body(vaggas.last_mut().unwrap(), BodyItem::Block(block));
                }
            }

            if is_heading {
                let context = match (kind, vaggas.last()) {
                    (HeadingKind::Other, _) | (_, None) => String::new(),
                    (_, Some(vagga)) => match vagga.stories.last() {
                        Some(story) if kind != HeadingKind::Vagga => format!("vagga {}, story {}", vagga.number, story.label()),
                        _ => format!("vagga {}", vagga.number),
                    },
                };
                headings.push(HeadingRecord { file: file.name.clone(), ids: ids_in(&block.el), kind, text, context });
            }
        }
    }

    for vagga in &mut vaggas {
        for story in &mut vagga.stories {
            read_story_verses(story, report);
        }
    }

    Book { vaggas, notes, after, headings }
}

/// A block or sub-story of a vagga goes to its last story, or before its
/// first one.
fn push_body<'a>(vagga: &mut Vagga<'a>, item: BodyItem<'a>) {
    match vagga.stories.last_mut() {
        Some(story) => story.body.push(item),
        None => match item {
            BodyItem::Block(block) => vagga.intro.push(block),
            BodyItem::SubStory(sub) => vagga.intro.push(sub.heading),
        },
    }
}

fn read_story_verses(story: &mut Story, report: &mut Report) {
    let lines: Vec<String> = story
        .meta
        .iter()
        .map(|block| text_without_refs(*block.el))
        .filter(|text| text.starts_with("Dhp "))
        .collect();
    match lines.as_slice() {
        [] => report.defect(format!("{}: story {} has no Dhp line", story.heading.file, story.label())),
        [line, rest @ ..] => {
            if !rest.is_empty() {
                report.defect(format!("{}: story {} has {} Dhp lines; the first is used", story.heading.file, story.label(), lines.len()));
            }
            match RE_VERSE_LINE.captures(line) {
                Some(c) => {
                    let start: u32 = c[1].parse().unwrap();
                    let end: u32 = c.get(2).map(|m| m.as_str().parse().unwrap()).unwrap_or(start);
                    if end < start {
                        report.defect(format!("{}: story {}: reversed verse range \"{}\"", story.heading.file, story.label(), line));
                    }
                    story.verses = (start..=end).collect();
                    story.verse_line = Some(line.clone());
                }
                None => report.defect(format!("{}: story {}: unreadable verse line \"{}\"", story.heading.file, story.label(), line)),
            }
        }
    }
}

/// "fourteen" → 14, "twenty-three" → 23.
pub fn number_from_words(words: &str) -> Option<u32> {
    const UNITS: [&str; 20] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
        "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
    ];
    const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    let parts: Vec<&str> = words.split('-').collect();
    let unit = |w: &str| UNITS.iter().position(|u| *u == w).map(|n| n as u32);
    let tens = |w: &str| TENS.iter().position(|t| !t.is_empty() && *t == w).map(|n| n as u32 * 10);
    match parts.as_slice() {
        [w] => unit(w).or_else(|| tens(w)),
        [t, u] => Some(tens(t)? + unit(u).filter(|n| (1..10).contains(n))?),
        _ => None,
    }
}

/// The Conclusion's story counts: (chapter name, count) per vagga line, and
/// the total.
pub struct ConclusionCounts {
    pub vaggas: Vec<(String, u32)>,
    pub total: Option<u32>,
}

pub fn conclusion_counts(after: &[Block]) -> ConclusionCounts {
    let mut vaggas = Vec::new();
    let mut total = None;
    for block in after {
        let text = text_without_refs(*block.el);
        if let Some(c) = RE_COUNT_LINE.captures(&text)
            && let Some(n) = number_from_words(&c[2])
        {
            vaggas.push((c[1].to_string(), n));
        } else if total.is_none()
            && let Some(c) = RE_TOTAL_STORIES.captures(&text)
        {
            total = c[1].parse().ok();
        }
    }
    ConclusionCounts { vaggas, total }
}

/// "1. The Chapter about the Pairs" and "Chapter about the Pairs" name the
/// same chapter.
fn same_chapter_name(vagga_title: &str, conclusion_name: &str) -> bool {
    let norm = |s: &str| {
        let s = RE_VAGGA_HEADING.replace(s, "").to_lowercase();
        s.strip_prefix("the ").map(str::to_string).unwrap_or(s)
    };
    norm(vagga_title) == norm(conclusion_name)
}

/// The verses a vagga's stories cover, first to last.
pub fn story_verse_range(vagga: &Vagga) -> Option<(u32, u32)> {
    let verses = vagga.stories.iter().flat_map(|s| s.verses.iter().copied());
    verses.clone().min().zip(verses.max())
}

/// A vagga's verse range, which names its page: the SuttaCentral range of its
/// number, so that the page has the uid of the CST commentary record of the
/// same vagga even when a story's verse line is wrong (that is reported).
/// Past the 26 SC vaggas, the range its stories cover.
pub fn vagga_range(vagga: &Vagga) -> Option<(u32, u32)> {
    vagga
        .number
        .checked_sub(1)
        .and_then(|i| SC_VAGGA_RANGES.get(i as usize).copied())
        .or_else(|| story_verse_range(vagga))
}

/// Collapse sorted numbers into "1-3, 7".
fn ranges_text(numbers: &[u32]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < numbers.len() {
        let start = numbers[i];
        let mut end = start;
        while i + 1 < numbers.len() && numbers[i + 1] == end + 1 {
            i += 1;
            end = numbers[i];
        }
        parts.push(if start == end { start.to_string() } else { format!("{}-{}", start, end) });
        i += 1;
    }
    parts.join(", ")
}

/// The FR 17 checks, and the cross-check with `toc.ncx`.
pub fn check_book(book: &Book, toc: &[TocEntry], report: &mut Report) {
    // Verse coverage, over the SC verse numbering.
    let last_verse = SC_VAGGA_RANGES.last().unwrap().1;
    let mut covering: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for vagga in &book.vaggas {
        for story in &vagga.stories {
            for v in &story.verses {
                covering.entry(*v).or_default().push(story.label());
            }
        }
    }
    let uncovered: Vec<u32> = (1..=last_verse).filter(|v| !covering.contains_key(v)).collect();
    if !uncovered.is_empty() && !book.vaggas.is_empty() {
        report.defect(format!("Verses covered by no story: Dhp {}", ranges_text(&uncovered)));
    }
    for (v, stories) in &covering {
        if stories.len() > 1 {
            report.defect(format!("Dhp {} is covered by more than one story: {}", v, stories.join(", ")));
        }
        if *v > last_verse {
            report.defect(format!("Dhp {} (story {}) is past the last verse, {}", v, stories.join(", "), last_verse));
        }
    }

    for vagga in &book.vaggas {
        let sc = vagga.number.checked_sub(1).and_then(|i| SC_VAGGA_RANGES.get(i as usize).copied());
        if let Some(range) = story_verse_range(vagga)
            && Some(range) != sc
        {
            report.defect(format!(
                "{}: the stories of vagga {} cover Dhp {}-{}, which is not its SuttaCentral range{}",
                vagga.heading.file,
                vagga.number,
                range.0,
                range.1,
                sc.map(|(s, e)| format!(" ({}-{})", s, e)).unwrap_or_default()
            ));
        }
        if let Some((start, end)) = sc {
            for story in &vagga.stories {
                let outside: Vec<u32> = story.verses.iter().copied().filter(|v| *v < start || *v > end).collect();
                if !outside.is_empty() {
                    report.defect(format!(
                        "{}: story {} has Dhp {}, outside vagga {}'s range {}-{}",
                        story.heading.file,
                        story.label(),
                        ranges_text(&outside),
                        vagga.number,
                        start,
                        end
                    ));
                }
            }
        }
    }

    // Story counts against the Conclusion.
    let counts = conclusion_counts(&book.after);
    if counts.vaggas.len() != book.vaggas.len() {
        report.defect(format!(
            "The Conclusion gives {} vagga story counts, for {} vaggas",
            counts.vaggas.len(),
            book.vaggas.len()
        ));
    }
    for (vagga, (name, count)) in book.vaggas.iter().zip(&counts.vaggas) {
        if !same_chapter_name(&vagga.title, name) {
            report.defect(format!(
                "The Conclusion's count line for vagga {} names \"{}\", the heading \"{}\"",
                vagga.number, name, vagga.title
            ));
        }
        if *count as usize != vagga.stories.len() {
            report.defect(format!(
                "Vagga {} has {} stories, the Conclusion says {}",
                vagga.number,
                vagga.stories.len(),
                count
            ));
        }
    }
    let story_total: usize = book.vaggas.iter().map(|v| v.stories.len()).sum();
    match counts.total {
        Some(total) if total as usize != story_total => {
            report.defect(format!("The book has {} stories, the Conclusion says {}", story_total, total))
        }
        None if !book.vaggas.is_empty() => report.defect("The Conclusion gives no total story count".to_string()),
        _ => {}
    }

    check_toc(book, toc, report);
}

/// Each `toc.ncx` entry must point to a heading with the same text, and each
/// vagga, story, Pāli title and sub-story heading must have an entry.
fn check_toc(book: &Book, toc: &[TocEntry], report: &mut Report) {
    let mut by_target: BTreeMap<(&str, Option<&str>), usize> = BTreeMap::new();
    for (i, heading) in book.headings.iter().enumerate() {
        by_target.entry((heading.file.as_str(), None)).or_insert(i);
        for id in &heading.ids {
            by_target.entry((heading.file.as_str(), Some(id.as_str()))).or_insert(i);
        }
    }
    let mut listed: BTreeSet<usize> = BTreeSet::new();
    for entry in toc {
        let Some(&i) = by_target.get(&(entry.file.as_str(), entry.fragment.as_deref())) else {
            // Only the vagga files' targets are checked: the title page has no
            // heading to point to.
            let in_vagga_file = book.headings.iter().any(|h| h.file == entry.file && h.kind != HeadingKind::Other);
            if !in_vagga_file {
                continue;
            }
            report.defect(format!(
                "toc.ncx entry \"{}\" points to {}{}, which is not a heading",
                entry.label,
                entry.file,
                entry.fragment.as_ref().map(|f| format!("#{}", f)).unwrap_or_default()
            ));
            continue;
        };
        listed.insert(i);
        let heading = &book.headings[i];
        if heading.kind == HeadingKind::Other {
            continue;
        }
        if collapse_whitespace(&entry.label) != heading.text {
            report.defect(format!(
                "{}: {} heading \"{}\" is \"{}\" in toc.ncx",
                heading.file, heading.context, heading.text, entry.label
            ));
        }
    }
    for (i, heading) in book.headings.iter().enumerate() {
        if heading.kind != HeadingKind::Other && !listed.contains(&i) {
            report.defect(format!("{}: {} heading \"{}\" is not in toc.ncx", heading.file, heading.context, heading.text));
        }
    }
}

/// What one extraction produced.
pub struct Extraction {
    pub pages: Vec<Page>,
    pub manifest: Manifest,
    pub report_text: String,
    pub story_count: usize,
    pub defect_count: usize,
}

/// Run the extraction in memory.
pub fn extract(epub: &EpubFiles) -> Result<Extraction> {
    let files = parse_files(epub)?;
    let toc = epub.toc()?;
    let mut report = Report::default();
    let book = read_book(&files, &mut report);
    check_book(&book, &toc, &mut report);

    let mut pages = Vec::new();
    for vagga in &book.vaggas {
        let Some((start, end)) = vagga_range(vagga) else {
            report.defect(format!("{}: vagga {} has no verses and no SuttaCentral range; not written", vagga.heading.file, vagga.number));
            continue;
        };
        pages.push(render_vagga(vagga, start, end, &book.notes, &mut report));
    }

    let manifest = Manifest {
        epub_file: epub.file_name.clone(),
        epub_sha256: epub.sha256.clone(),
        vaggas: pages
            .iter()
            .map(|page| ManifestVagga {
                uid: page.uid.clone(),
                file: page.file_name.clone(),
                vagga_number: page.vagga_number,
                title: page.title.clone(),
                title_pali: page.title_pali.clone(),
                verse_start: page.verse_start,
                verse_end: page.verse_end,
                story_count: page.story_count,
            })
            .collect(),
    };

    let story_count: usize = book.vaggas.iter().map(|v| v.stories.len()).sum();
    let substory_count: usize = book.vaggas.iter().flat_map(|v| &v.stories).map(|s| s.substories().count()).sum();
    let note_ref_count: usize = pages.iter().map(|p| p.note_ref_count).sum();
    let note_count: usize = pages.iter().map(|p| p.note_count).sum();

    let mut text = String::new();
    text.push_str("Dhamma Verses Commentary: extraction report\n");
    text.push_str(&format!("epub: {} (SHA-256 {})\n\n", epub.file_name, epub.sha256));
    text.push_str("Defects in the source (reported, not repaired):\n");
    if report.defects.is_empty() {
        text.push_str("(none)\n");
    }
    for line in &report.defects {
        text.push_str(&format!("- {}\n", line));
    }
    text.push_str("\nMarkup the extraction does not map (rendered as its text):\n");
    if report.unmapped.is_empty() {
        text.push_str("(none)\n");
    }
    for (what, count) in &report.unmapped {
        text.push_str(&format!("- {} ({}×)\n", what, count));
    }
    text.push_str(&format!(
        "\nSummary: {} vaggas, {} stories, {} sub-stories, {} note references to {} notes, {} defects, {} unmapped\n",
        pages.len(),
        story_count,
        substory_count,
        note_ref_count,
        note_count,
        report.defects.len(),
        report.unmapped.len()
    ));

    Ok(Extraction { pages, manifest, report_text: text, story_count, defect_count: report.defects.len() })
}

/// Write the pages, `manifest.json` and `extract-report.txt` to `out_dir`.
pub fn write_extraction(extraction: &Extraction, out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("Cannot create {}", out_dir.display()))?;
    for page in &extraction.pages {
        let path = out_dir.join(&page.file_name);
        std::fs::write(&path, &page.html).with_context(|| format!("Cannot write {}", path.display()))?;
    }
    let mut manifest = serde_json::to_string_pretty(&extraction.manifest)?;
    manifest.push('\n');
    let path = out_dir.join(MANIFEST_FILE);
    std::fs::write(&path, manifest).with_context(|| format!("Cannot write {}", path.display()))?;
    let path = out_dir.join(REPORT_FILE);
    std::fs::write(&path, &extraction.report_text).with_context(|| format!("Cannot write {}", path.display()))?;
    Ok(())
}

/// The `dhammapada-commentary-extract` command.
pub fn run(epub_path: &Path, out_dir: &Path) -> Result<Extraction> {
    let epub = EpubFiles::from_zip(epub_path)?;
    let extraction = extract(&epub)?;
    write_extraction(&extraction, out_dir)?;
    Ok(extraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_words_up_to_the_forties() {
        assert_eq!(number_from_words("nine"), Some(9));
        assert_eq!(number_from_words("fourteen"), Some(14));
        assert_eq!(number_from_words("forty"), Some(40));
        assert_eq!(number_from_words("twenty-three"), Some(23));
        assert_eq!(number_from_words("twenty-zero"), None);
        assert_eq!(number_from_words("many"), None);
    }

    #[test]
    fn both_count_line_phrasings_are_read() {
        for (line, name, n) in [
            ("In the Chapter about the Pairs are fourteen stories.", "Chapter about the Pairs", "fourteen"),
            ("In the Chapter about Heedfulness there are nine.", "Chapter about Heedfulness", "nine"),
            ("In the Miscellaneous Chapter there are nine.", "Miscellaneous Chapter", "nine"),
        ] {
            let c = RE_COUNT_LINE.captures(line).unwrap();
            assert_eq!((&c[1], &c[2]), (name, n));
        }
        assert!(same_chapter_name("1. The Chapter about the Pairs", "Chapter about the Pairs"));
        assert!(same_chapter_name("21. The Miscellaneous Chapter", "Miscellaneous Chapter"));
        assert!(!same_chapter_name("25. The Chapter about Bhikkhus", "Chapter about Monastics"));
    }

    #[test]
    fn ranges_are_collapsed() {
        assert_eq!(ranges_text(&[1, 2, 3, 7, 9, 10]), "1-3, 7, 9-10");
    }
}
