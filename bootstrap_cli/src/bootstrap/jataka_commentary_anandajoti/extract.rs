//! The epub as Nipātas and Jātakas, the notes, and the defect checks.
//! Rendering is in `page.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use scraper::ElementRef;

use super::page::{render_jataka, Page};
use super::{page_uid, Manifest, ManifestJataka, JATAKA_COUNT, MANIFEST_FILE, REPORT_FILE};
use crate::bootstrap::anandajoti_epub::epub::{EpubFiles, TocEntry};
use crate::bootstrap::anandajoti_epub::report::{write_output, Report};
use crate::bootstrap::anandajoti_epub::source::{
    body_blocks, collapse_whitespace, find_notes_files, parse_files, parse_notes, push_text_without_refs,
    text_without_refs, Block, Notes, ParsedFile,
};

/// The gray spans that hold page markers (`[1.1]`, `{1.95}`).
pub const PAGE_MARKER_CLASSES: [&str; 3] = ["t18", "t35", "t46"];

static RE_JATAKA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Ja\s+(\d+)\s+(.+)$").unwrap());
static RE_BOOK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Book ([IVXL]+)\.\s+(\S+)\s*(.*)$").unwrap());
static RE_RANGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\((\d+)-(\d+)\)\s*$").unwrap());
/// The section label of a heading: a verse count `(1s)`, `(Pak)`, `(Mahānipāta)`.
static RE_LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\((?:\d+s|Pak|Mahānipāta)\)").unwrap());

/// A section of the book, from its `Book <Roman>.` heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nipata {
    pub number: u32,
    pub roman: String,
    /// "Dukanipāta"
    pub name: String,
    /// The number range in the heading, "(464-473)", from Book XII on.
    pub range: Option<(u32, u32)>,
    pub file: String,
}

pub struct Section<'a> {
    pub heading: Block<'a>,
    /// The heading text, note refs left out.
    pub title: String,
}

pub enum Item<'a> {
    Block(Block<'a>),
    Section(Section<'a>),
}

pub struct Jataka<'a> {
    pub number: u32,
    /// "The Birth Story about Being Inexperienced"
    pub title: String,
    /// "Aṇḍabhūtajātaka"
    pub title_pali: String,
    pub heading: Block<'a>,
    /// Index into [`Book::nipatas`].
    pub nipata: Option<usize>,
    pub items: Vec<Item<'a>>,
}

impl Jataka<'_> {
    pub fn sections(&self) -> impl Iterator<Item = &Section<'_>> {
        self.items.iter().filter_map(|item| match item {
            Item::Section(section) => Some(section),
            Item::Block(_) => None,
        })
    }

    fn has_text(&self) -> bool {
        self.items.iter().any(|item| match item {
            Item::Block(block) => block.el.text().any(|t| !t.trim().is_empty()),
            Item::Section(_) => false,
        })
    }
}

pub struct Book<'a> {
    pub nipatas: Vec<Nipata>,
    pub jatakas: Vec<Jataka<'a>>,
    pub notes: Notes<'a>,
    /// The notes referenced from any content file, front matter included.
    pub referenced: BTreeSet<(String, String)>,
    /// Each Jātaka heading as the book prints it, for the `toc.ncx` check:
    /// `(number, file, text)`.
    pub heading_texts: Vec<(u32, String, String)>,
}

pub fn roman_number(roman: &str) -> Option<u32> {
    let value = |c| match c {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        _ => None,
    };
    let digits: Vec<u32> = roman.chars().map(value).collect::<Option<_>>()?;
    let mut total = 0;
    for (i, d) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|next| next > d) {
            total -= *d as i64;
        } else {
            total += *d as i64;
        }
    }
    u32::try_from(total).ok().filter(|n| *n > 0)
}

/// The heading text before and after its `<br>`, note refs and page markers
/// left out, whitespace collapsed.
fn split_at_br(el: &ElementRef) -> (String, String) {
    let (mut before, mut after) = (String::new(), String::new());
    let mut seen_br = false;
    for child in el.children() {
        if child.value().as_element().is_some_and(|e| e.name() == "br") {
            seen_br = true;
            continue;
        }
        let out = if seen_br { &mut after } else { &mut before };
        push_text_without_refs(child, &PAGE_MARKER_CLASSES, out);
    }
    (collapse_whitespace(&before), collapse_whitespace(&after))
}

struct HeadingParts {
    number: u32,
    title: String,
    title_pali: String,
    /// The heading as printed (label included), for the `toc.ncx` check.
    printed: String,
}

fn parse_jataka_heading(el: &ElementRef) -> Option<HeadingParts> {
    let (before, after) = split_at_br(el);
    let printed = collapse_whitespace(&format!("{} {}", before, after));
    let strip = |text: &str| collapse_whitespace(&RE_LABEL.replace_all(text, ""));
    let (before, after) = (strip(&before), strip(&after));
    let caps = RE_JATAKA.captures(&before)?;
    Some(HeadingParts { number: caps[1].parse().ok()?, title_pali: caps[2].to_string(), title: after, printed })
}

pub fn read_book<'a>(files: &'a [ParsedFile], report: &mut Report) -> Book<'a> {
    let note_files = find_notes_files(files);
    let notes = parse_notes(files, &note_files, report);

    let mut referenced = BTreeSet::new();
    for file in files.iter().filter(|f| !note_files.contains(&f.name)) {
        for node in file.doc.root_element().descendants() {
            if let Some(el) = ElementRef::wrap(node)
                && el.value().name() == "a"
                && let Some(target) = el.value().attr("href").and_then(|href| notes.target_of(href))
            {
                referenced.insert(target);
            }
        }
    }

    let mut nipatas: Vec<Nipata> = Vec::new();
    let mut jatakas: Vec<Jataka<'a>> = Vec::new();
    let mut heading_texts = Vec::new();
    let mut current: Option<usize> = None;
    for file in files.iter().filter(|f| !note_files.contains(&f.name)) {
        for block in body_blocks(file, report) {
            let tag = block.tag();
            match tag {
                "h3" => {
                    let text = collapse_whitespace(&text_without_refs(*block.el, &PAGE_MARKER_CLASSES));
                    if !text.starts_with("Ja ") && !text.starts_with("Ja\u{a0}") {
                        current = None;
                        continue;
                    }
                    match parse_jataka_heading(&block.el) {
                        Some(parts) => {
                            heading_texts.push((parts.number, file.name.clone(), parts.printed));
                            jatakas.push(Jataka {
                                number: parts.number,
                                title: parts.title,
                                title_pali: parts.title_pali,
                                heading: block,
                                nipata: nipatas.len().checked_sub(1),
                                items: Vec::new(),
                            });
                            current = Some(jatakas.len() - 1);
                        }
                        None => {
                            report.defect(format!("{}: heading \"{}\" is not \"Ja <number> <name>\"", file.name, text));
                            current = None;
                        }
                    }
                }
                "h1" | "h2" | "h5" => {
                    current = None;
                    if tag == "h2" {
                        let (before, after) = split_at_br(&block.el);
                        let text = collapse_whitespace(&format!("{} {}", before, after));
                        if text.starts_with("Book ") {
                            match RE_BOOK.captures(&text).and_then(|c| Some((roman_number(&c[1])?, c))) {
                                Some((number, caps)) => nipatas.push(Nipata {
                                    number,
                                    roman: caps[1].to_string(),
                                    name: caps[2].to_string(),
                                    range: RE_RANGE
                                        .captures(&text)
                                        .and_then(|r| Some((r[1].parse().ok()?, r[2].parse().ok()?))),
                                    file: file.name.clone(),
                                }),
                                None => report.defect(format!("{}: Book heading \"{}\" is not read", file.name, text)),
                            }
                        }
                    }
                }
                "h4" => {
                    if let Some(i) = current {
                        let title = collapse_whitespace(&text_without_refs(*block.el, &PAGE_MARKER_CLASSES));
                        jatakas[i].items.push(Item::Section(Section { heading: block, title }));
                    }
                }
                _ => {
                    if let Some(i) = current {
                        jatakas[i].items.push(Item::Block(block));
                    }
                }
            }
        }
    }
    Book { nipatas, jatakas, notes, referenced, heading_texts }
}

/// The defect checks that need the whole book, each naming the Jātaka and
/// the file. Nothing is repaired.
pub fn check_book(book: &Book, toc: &[TocEntry], report: &mut Report) {
    let file_of: BTreeMap<usize, &str> =
        book.jatakas.iter().enumerate().map(|(i, j)| (i, j.heading.file)).collect();

    let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
    let mut previous: Option<u32> = None;
    for (i, jataka) in book.jatakas.iter().enumerate() {
        if let Some(&first) = seen.get(&jataka.number) {
            report.defect(format!(
                "{}: Ja {} appears twice (first in {}); the second is not written",
                file_of[&i], jataka.number, file_of[&first]
            ));
        } else {
            seen.insert(jataka.number, i);
        }
        if let Some(prev) = previous
            && jataka.number != prev + 1
            && seen.get(&jataka.number) == Some(&i)
        {
            report.defect(format!(
                "{}: Ja {} follows Ja {}",
                file_of[&i], jataka.number, prev
            ));
        }
        previous = Some(jataka.number);
        if jataka.number == 0 || jataka.number > JATAKA_COUNT {
            report.defect(format!("{}: Ja {} is outside 1–{}", file_of[&i], jataka.number, JATAKA_COUNT));
        }
        if jataka.nipata.is_none() {
            report.defect(format!("{}: Ja {} is before any Book heading; not written", file_of[&i], jataka.number));
        }
        if !jataka.has_text() {
            report.defect(format!("{}: Ja {} has no body text", file_of[&i], jataka.number));
        }
    }
    let missing: Vec<u32> = (1..=JATAKA_COUNT).filter(|n| !seen.contains_key(n)).collect();
    if !missing.is_empty() {
        report.defect(format!("Ja {} missing from the book", ranges_text(&missing)));
    }

    // The Book numbers run 1, 2, 3, …
    for (i, nipata) in book.nipatas.iter().enumerate() {
        if nipata.number != i as u32 + 1 {
            report.defect(format!("{}: Book {} is the {}th Book heading", nipata.file, nipata.roman, i + 1));
        }
    }
    // The number range some Book headings carry.
    for (index, nipata) in book.nipatas.iter().enumerate() {
        let Some((first, last)) = nipata.range else { continue };
        let numbers: Vec<u32> = book.jatakas.iter().filter(|j| j.nipata == Some(index)).map(|j| j.number).collect();
        let (min, max) = (numbers.iter().min(), numbers.iter().max());
        let as_stated = min == Some(&first) && max == Some(&last) && numbers.len() as u32 == last - first + 1;
        if !as_stated {
            report.defect(format!(
                "{}: Book {} states Ja {}–{}, but holds {} Jātakas ({})",
                nipata.file,
                nipata.roman,
                first,
                last,
                numbers.len(),
                if numbers.is_empty() { "none".to_string() } else { ranges_text(&numbers) }
            ));
        }
    }

    check_toc(book, toc, report);

    let mut unreferenced: Vec<(u32, &(String, String))> = book
        .notes
        .by_target
        .iter()
        .filter(|(target, _)| !book.referenced.contains(*target))
        .map(|(target, note)| (note.number, target))
        .collect();
    unreferenced.sort();
    for (number, (file, anchor)) in unreferenced {
        report.defect(format!("{}: note {{{}}} (#{}) is never referenced", file, number, anchor));
    }
}

/// The Jātaka headings against the `Ja N …` entries of `toc.ncx`.
fn check_toc(book: &Book, toc: &[TocEntry], report: &mut Report) {
    static RE_TOC_JA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^Ja\s+(\d+)\b").unwrap());
    let mut entries: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for entry in toc {
        if let Some(caps) = RE_TOC_JA.captures(collapse_whitespace(&entry.label).as_str())
            && let Ok(n) = caps[1].parse()
        {
            entries.entry(n).or_default().push(collapse_whitespace(&entry.label));
        }
    }
    let mut in_book: BTreeSet<u32> = BTreeSet::new();
    for (number, file, text) in &book.heading_texts {
        in_book.insert(*number);
        match entries.get(number) {
            None => report.defect(format!("{}: Ja {} is not in toc.ncx", file, number)),
            Some(labels) if !labels.contains(text) => report.defect(format!(
                "{}: Ja {} heading \"{}\" differs from toc.ncx \"{}\"",
                file, number, text, labels[0]
            )),
            Some(_) => {}
        }
    }
    for (number, labels) in &entries {
        if !in_book.contains(number) {
            report.defect(format!("toc.ncx: Ja {} (\"{}\") is not in the content files", number, labels[0]));
        }
    }
}

/// `1, 3–5, 9`
fn ranges_text(numbers: &[u32]) -> String {
    let mut sorted = numbers.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && sorted[j + 1] == sorted[j] + 1 {
            j += 1;
        }
        parts.push(if j > i { format!("{}–{}", sorted[i], sorted[j]) } else { sorted[i].to_string() });
        i = j + 1;
    }
    parts.join(", ")
}

/// What one extraction produced.
pub struct Extraction {
    pub pages: Vec<Page>,
    pub manifest: Manifest,
    pub report_text: String,
    pub defect_count: usize,
}

/// Run the extraction in memory.
pub fn extract(epub: &EpubFiles) -> Result<Extraction> {
    let files = parse_files(epub)?;
    let toc = epub.toc()?;
    let mut report = Report::default();
    let book = read_book(&files, &mut report);
    check_book(&book, &toc, &mut report);

    let mut pages: Vec<Page> = Vec::new();
    let mut written: BTreeSet<u32> = BTreeSet::new();
    for jataka in &book.jatakas {
        let Some(index) = jataka.nipata else { continue };
        if !written.insert(jataka.number) {
            continue;
        }
        pages.push(render_jataka(jataka, &book.nipatas[index], &book.notes, &mut report));
    }

    let manifest = Manifest {
        epub_file: epub.file_name.clone(),
        epub_sha256: epub.sha256.clone(),
        jatakas: pages
            .iter()
            .map(|page| ManifestJataka {
                uid: page_uid(page.number),
                file: page.file_name.clone(),
                jataka_number: page.number,
                title: page.title.clone(),
                title_pali: page.title_pali.clone(),
                nipata_number: page.nipata_number,
                nipata: page.nipata.clone(),
                verse_count: page.verse_count,
            })
            .collect(),
    };

    let section_count: usize = book.jatakas.iter().map(|j| j.sections().count()).sum();
    let verse_count: usize = pages.iter().map(|p| p.verse_count).sum();
    let note_ref_count: usize = pages.iter().map(|p| p.note_ref_count).sum();
    let note_count: usize = pages.iter().map(|p| p.note_count).sum();
    let summary = format!(
        "{} Jātakas, {} sections, {} verses, {} note references to {} notes",
        pages.len(),
        section_count,
        verse_count,
        note_ref_count,
        note_count
    );
    let report_text = report.render("The Birth Stories (Jātaka)", &epub.file_name, &epub.sha256, &summary);
    Ok(Extraction { pages, manifest, report_text, defect_count: report.defects.len() })
}

/// Write the pages, `manifest.json` and `extract-report.txt` to `out_dir`.
pub fn write_extraction(extraction: &Extraction, out_dir: &Path) -> Result<()> {
    let pages: Vec<(&str, &str)> =
        extraction.pages.iter().map(|p| (p.file_name.as_str(), p.html.as_str())).collect();
    write_output(out_dir, &pages, &extraction.manifest, MANIFEST_FILE, &extraction.report_text, REPORT_FILE)
}

/// The `jataka-commentary-extract` command.
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
    fn roman_numbers() {
        assert_eq!(roman_number("I"), Some(1));
        assert_eq!(roman_number("IV"), Some(4));
        assert_eq!(roman_number("XIX"), Some(19));
        assert_eq!(roman_number("XXII"), Some(22));
        assert_eq!(roman_number("Q"), None);
    }

    #[test]
    fn ranges_are_collapsed() {
        assert_eq!(ranges_text(&[1, 2, 3, 5, 9, 10]), "1–3, 5, 9–10");
    }

    #[test]
    fn the_section_label_is_not_part_of_the_title() {
        let strip = |t: &str| collapse_whitespace(&RE_LABEL.replace_all(t, ""));
        assert_eq!(strip("The Birth Story about the (Clever) Monkey (4s)"), "The Birth Story about the (Clever) Monkey");
        assert_eq!(strip("Setaketujātaka (6s)"), "Setaketujātaka");
        assert_eq!(strip("The Short Birth Story about the Ruddy Geese (Pak)"), "The Short Birth Story about the Ruddy Geese");
        assert_eq!(strip("The Birth Story about the Great Black (Hound)"), "The Birth Story about the Great Black (Hound)");
        assert_eq!(strip("The Birth Story about Vessantara (Mahānipāta)"), "The Birth Story about Vessantara");
    }
}
