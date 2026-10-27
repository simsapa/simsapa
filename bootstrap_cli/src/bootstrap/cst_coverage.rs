//! Which records a CST source file gets, and how it is cut into divisions.
//!
//! Two independent questions decide this, and conflating them is a data-quality
//! defect:
//!
//! 1. **Is the file's parse trustworthy?** [`VERIFIED_CST_FILES`], a
//!    hand-maintained list. A file that is not on it never contributes fragment
//!    records, however well mapped it looks.
//! 2. **How completely do the fragments cover it?** The coverage class, computed
//!    from `fragments.sqlite3`. It only ever applies *within* the verified set.
//!
//! ## Verification: why the list is hand-maintained and cannot be computed
//!
//! `tipitaka-xml-parser` is a work in progress. It produces wrong splits and
//! wrong uid assignments on files whose structure it does not yet handle, and
//! nothing in `fragments.sqlite3` records whether a given file's output was
//! found to be correct. The evidence, measured over the fragments database:
//!
//! - **`frag_review` is not a verdict.** `checked` means a human eyeballed that
//!   fragment so the parser could move past a stuck or edge-case state; its
//!   *absence* means the fragment never needed scrutiny, not that it is wrong.
//!   `agent-checked` is the same pass made by an agent, and `moved` marks an
//!   empty record. Only 511 of 7,104 Sutta fragments carry any review value,
//!   spread over 32 of the 52 files, and **no file** has all of its fragments
//!   marked.
//! - **The quality signal lives in the parser, not in the data.**
//!   `tipitaka-xml-parser/src/web/validation.rs` runs nine checks over the
//!   parsed fragments -- `missing_sc_code`, `unattached_text`,
//!   `missing_sc_sutta`, `sutta_review_status`, `xml_reconstruction`,
//!   `sc_code_sequence`, `code_uniqueness`, `cst_sc_range_consistency` and
//!   `sc_code_not_in_arangodb` -- and reports per file whether its fragments are
//!   sound. A file joins the list below once it passes those and its output has
//!   been reviewed. The verdict is not written back into the fragments database,
//!   so this list is the only record of it.
//!
//! **Therefore: do not replace this list with a computed predicate.** "Every
//! fragment has an `sc_code` and the spans are gapless" describes *coverage*,
//! which a confidently wrong parse satisfies just as easily as a correct one. An
//! earlier revision of this module did exactly that and admitted 3,055 fragments
//! from 20 unverified files into the shipped app.
//!
//! ## The verified list names source files; the toc names divisions
//!
//! [`VERIFIED_CST_FILES`] carries the file names the **parser** read, which are
//! the `romn/` source names and the `cst_file` values (measured: all 52 entries
//! appear in all three of `cst_file`, `romn/` and the toc set, with no
//! mismatch in either direction). The navigation tree, by contrast, works in the
//! **division** files the CST toc links -- `s0101a.att0.xml`,
//! `s0101a.att1.xml`, ... Those correlate by construction: a source file's
//! divisions are exactly the leaves of its `<name>.toc.xml`, which
//! [`crate::bootstrap::tipitaka_xml::toc_name_for`] derives from the name. No
//! separate mapping table is needed while the naming holds; if the two ever
//! diverge, that derivation is the single place to replace.
//!
//! ## The coverage classes
//!
//! | Class | Predicate | Records produced |
//! |---|---|---|
//! | [`CoverageClass::FullyMapped`] | verified, **and** every `frag_type = 'Sutta'` fragment has an `sc_code`, **and** they leave no interior gap | fragment records only |
//! | [`CoverageClass::PartiallyMapped`] | verified, and some but not all fragments carry an `sc_code` | division records **plus** the mapped fragment records |
//! | [`CoverageClass::Unmapped`] | verified, but no fragments carry an `sc_code` | division records only |
//! | [`CoverageClass::Unverified`] | not on the list | division records only |
//! | [`CoverageClass::Unsplittable`] | the division split failed its cross-check | the whole-file record, as the fallback |
//!
//! Within the verified set the class is still **computed**, so adding mappings
//! to an already-verified file moves it between classes with no code change.
//!
//! A fragment with `frag_review = 'moved'` is left out of every count above. It
//! is an empty, zero-width record whose text was folded into a neighbour, and
//! the parser clears its `sc_code` deliberately -- so it is not an unmapped
//! fragment, and counting it as one would keep its file out of the fully mapped
//! class for a record that carries none of the file's text.
//!
//! ## Why a coarser record is dropped where a finer one covers the file
//!
//! The same Pāli text is never stored twice. Duplication is a search defect
//! before it is a size problem: the whole-file records were 74 MB of the
//! `suttas` table's 163 MB of `content_plain`, indexed by a *trigram* FTS5 table
//! and by an 841 MB Tantivy index, and a duplicated text returns two hits for
//! every query that matches it.
//!
//! ## Gaplessness
//!
//! Part of the fully-mapped predicate, because the risk being guarded against is
//! losing text the reader could otherwise find. A file whose fragments are all
//! mapped but leave an interior hole is **not** fully covered and is treated as
//! partially mapped.

use std::collections::HashMap;

use anyhow::{Context, Result};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use tipitaka_xml_parser::fragments_schema::xml_fragments;

/// CST source files whose parse has been verified -- see this module's header
/// for what "verified" means and why it cannot be computed.
///
/// **This is a quality gate, not a coverage predicate.** Only files listed here
/// contribute fragment records; everything else is imported as divisions of the
/// source XML, which never depends on the parser's sutta splitting.
///
/// Adding an entry is a deliberate act: run `tipitaka-xml-parser`'s validations
/// over the file, review the result, then add it here and re-run the CST import
/// and the Pāli fulltext index rebuild.
static VERIFIED_CST_FILES: [&str; 52] = [
    // Dīgha Nikāya
    "s0101a.att.xml",
    "s0101m.mul.xml",
    "s0101t.tik.xml",
    "s0102a.att.xml",
    "s0102m.mul.xml",
    "s0102t.tik.xml",
    "s0103a.att.xml",
    "s0103m.mul.xml",
    "s0103t.tik.xml",

    // Majjhima Nikāya
    "s0201a.att.xml",
    "s0201m.mul.xml",
    "s0201t.tik.xml",
    "s0202a.att.xml",
    "s0202m.mul.xml",
    "s0202t.tik.xml",
    "s0203a.att.xml",
    "s0203m.mul.xml",
    "s0203t.tik.xml",

    // Saṁyutta Nikāya
    "s0301a.att.xml",
    "s0301m.mul.xml",
    "s0301t.tik.xml",
    "s0302a.att.xml",
    "s0302m.mul.xml",
    "s0302t.tik.xml",
    "s0303a.att.xml",
    "s0303m.mul.xml",
    "s0303t.tik.xml",
    "s0304a.att.xml",
    "s0304m.mul.xml",
    "s0304t.tik.xml",
    "s0305a.att.xml",
    "s0305m.mul.xml",
    "s0305t.tik.xml",

    // Aṅguttara Nikāya
    "s0401m.mul.xml",
    "s0402m1.mul.xml",
    "s0402m2.mul.xml",
    "s0402m3.mul.xml",
    "s0403m1.mul.xml",
    "s0403m2.mul.xml",
    "s0403m3.mul.xml",
    "s0404m1.mul.xml",
    "s0404m2.mul.xml",
    "s0404m3.mul.xml",
    "s0404m4.mul.xml",
    "s0401a.att.xml",
    "s0402a.att.xml",
    "s0403a.att.xml",
    "s0404a.att.xml",
    "s0401t.tik.xml",
    "s0402t.tik.xml",
    "s0403t.tik.xml",
    "s0404t.tik.xml",
];


/// Whether a source file's parse has been verified ([`VERIFIED_CST_FILES`]).
pub fn is_verified(cst_file: &str) -> bool {
    VERIFIED_CST_FILES.contains(&cst_file)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoverageClass {
    FullyMapped,
    PartiallyMapped,
    Unmapped,
    /// The parse has not been verified, so no fragment record is imported from
    /// it whatever its mapping looks like. Divisions of the source XML are
    /// imported instead, which do not depend on the parser's sutta splitting.
    Unverified,
    /// Not derivable from the fragments: assigned by the import when the
    /// division split failed its per-file cross-check.
    Unsplittable,
}

impl CoverageClass {
    pub fn name(&self) -> &'static str {
        match self {
            CoverageClass::FullyMapped => "fully mapped",
            CoverageClass::PartiallyMapped => "partially mapped",
            CoverageClass::Unmapped => "unmapped",
            CoverageClass::Unverified => "unverified",
            CoverageClass::Unsplittable => "unsplittable",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileCoverage {
    pub mapped: usize,
    pub total: usize,
    /// False when the Sutta fragments leave an interior hole in the file.
    pub gapless: bool,
    pub class: CoverageClass,
}

/// Classify every file that has `frag_type = 'Sutta'` fragments.
///
/// A file absent from the result has no fragments at all and is
/// [`CoverageClass::Unmapped`] — [`coverage_of`] is what applies that default,
/// so no caller has to remember it.
pub fn compute_coverage(frag_conn: &mut SqliteConnection) -> Result<HashMap<String, FileCoverage>> {
    let rows: Vec<(String, Option<String>, Option<String>, i32, i32, i32, i32)> = xml_fragments::table
        .filter(xml_fragments::frag_type.eq("Sutta"))
        .select((
            xml_fragments::cst_file,
            xml_fragments::sc_code,
            xml_fragments::frag_review,
            xml_fragments::start_line,
            xml_fragments::start_char,
            xml_fragments::end_line,
            xml_fragments::end_char,
        ))
        .order((xml_fragments::cst_file.asc(), xml_fragments::start_line.asc()))
        .load(frag_conn)
        .context("Failed to read fragments for the coverage classification")?;

    let mut per_file: HashMap<String, Vec<FragmentSpan>> = HashMap::new();
    for (file, sc_code, frag_review, start_line, start_char, end_line, end_char) in rows {
        // The file gets an entry even when every fragment of it is moved.
        let spans = per_file.entry(file).or_default();
        let is_empty = start_line == end_line && start_char == end_char;
        if is_moved_fragment(frag_review.as_deref(), is_empty) {
            continue;
        }
        spans.push(FragmentSpan { mapped: sc_code.is_some(), start_line, end_line });
    }

    Ok(per_file
        .into_iter()
        .map(|(file, spans)| {
            let coverage = classify_spans(is_verified(&file), spans);
            (file, coverage)
        })
        .collect())
}

/// The `frag_review` value of a fragment whose text was folded into a
/// neighbour.
///
/// Such a record is empty and zero-width, and the parser clears its `sc_code`
/// on purpose: the neighbour carries both the text and the uid. It is therefore
/// **not** an unmapped fragment and takes no part in the classification.
/// Counting it keeps a file out of the fully mapped class for a record that
/// holds none of the file's text, and the whole file is then stored twice.
pub const MOVED_REVIEW: &str = "moved";

/// Whether a fragment is a moved record to leave out of the classification.
///
/// The mark alone is not enough. A record marked moved that still spans text
/// is counted like any other fragment: it has no `sc_code`, so its file falls
/// back to the partially mapped class and keeps its divisions, and the text
/// stays findable. Trusting the mark there would drop that text from the app
/// without a word.
fn is_moved_fragment(frag_review: Option<&str>, is_empty: bool) -> bool {
    frag_review == Some(MOVED_REVIEW) && is_empty
}

/// One text-carrying `frag_type = 'Sutta'` fragment, as the classification
/// sees it.
#[derive(Debug, Clone, Copy)]
struct FragmentSpan {
    mapped: bool,
    start_line: i32,
    end_line: i32,
}

fn classify_spans(verified: bool, mut spans: Vec<FragmentSpan>) -> FileCoverage {
    spans.sort_by_key(|span| span.start_line);

    let total = spans.len();
    let mapped = spans.iter().filter(|span| span.mapped).count();
    // A hole between two consecutive fragments is text no fragment record
    // carries, so the fragments do not cover the file even if all of them
    // are mapped.
    let gapless = spans.windows(2).all(|w| w[1].start_line <= w[0].end_line + 1);

    // The verification gate comes first: an unverified parse never yields a
    // fragment record, however completely mapped it looks.
    let class = if !verified {
        CoverageClass::Unverified
    } else if total > 0 && mapped == total && gapless {
        CoverageClass::FullyMapped
    } else if mapped > 0 {
        CoverageClass::PartiallyMapped
    } else {
        CoverageClass::Unmapped
    };

    FileCoverage { mapped, total, gapless, class }
}

/// The coverage of one file, defaulting to unmapped when it has no fragments.
pub fn coverage_of(coverage: &HashMap<String, FileCoverage>, file: &str) -> FileCoverage {
    coverage.get(file).cloned().unwrap_or(FileCoverage {
        mapped: 0,
        total: 0,
        gapless: true,
        // No fragments at all: divisions either way, but the class still says
        // which of the two reasons applies.
        class: if is_verified(file) {
            CoverageClass::Unmapped
        } else {
            CoverageClass::Unverified
        },
    })
}

/// One division of a source file: the XML that belongs to a single toc leaf.
#[derive(Debug, Clone)]
pub struct Division {
    /// The division file name the toc leaf names, e.g. `s0305m.mul0.xml`.
    pub division_file: String,
    /// The toc leaf's `text=`, which is the division's displayed title.
    pub title: String,
    /// The division's XML — the body content, not a whole document.
    /// `xml_to_html` is event-driven and needs no document root.
    pub xml: String,
}

#[derive(Debug)]
pub enum SplitError {
    /// The toc leaf count and the chapter-marker count disagree by more than
    /// the one leading division, so which text belongs to which leaf cannot be
    /// decided. Reported, never guessed.
    CountMismatch { leaves: usize, chapters: usize },
    /// The file has no `<body>`, or the split produced nothing.
    NoBody,
}

impl std::fmt::Display for SplitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SplitError::CountMismatch { leaves, chapters } => write!(
                f,
                "{} toc leaves but {} chapter markers (+/-1 expected)",
                leaves, chapters
            ),
            SplitError::NoBody => write!(f, "no <body> element, or the split produced nothing"),
        }
    }
}

/// Cut one `romn/` source file into the divisions its toc names.
///
/// **The chapter markup gives the boundaries; the toc leaf count decides
/// whether the head before the first marker is a division of its own.**
/// Measured across all 217 files: 169 have exactly as many leaves as
/// `rend="chapter"` markers, 47 have one more, and one (`abh05t.nrf.xml`, 5
/// leaves against 6 markers) has one fewer. The 47 open with a titled division
/// the chapter markers do not open — `abh01a.att.xml` begins with
/// `Ganthārambhakathā`, which is precisely its first toc leaf.
///
/// Reading the leading division out of the markup alone does **not** work, and
/// this is why the leaf count decides it: eleven of the 169 have real body text
/// before their first chapter marker — preamble gāthās in `s0508m.mul.xml`,
/// `e0201n.nrf.xml` and others — which the toc folds into the first division
/// rather than listing separately. So when the counts are equal that head is
/// **prepended to the first division**, never dropped and never made a division
/// the toc has no leaf for.
///
/// A file whose counts differ by anything else is a [`SplitError`], reported
/// and imported whole instead.
pub fn split_into_divisions(
    xml: &str,
    leaves: &[crate::bootstrap::navigation_tree::cst_spine::TocLeaf],
) -> Result<Vec<Division>, SplitError> {
    let pieces = split_body_at_chapters(xml)?;
    let (head, chapters) = pieces;

    let head_is_own_division = match leaves.len().checked_sub(chapters.len()) {
        Some(0) => false,
        Some(1) => true,
        _ => {
            return Err(SplitError::CountMismatch {
                leaves: leaves.len(),
                chapters: chapters.len(),
            })
        }
    };

    let mut parts: Vec<String> = Vec::with_capacity(leaves.len());
    if head_is_own_division {
        parts.push(head);
        parts.extend(chapters);
    } else {
        let mut chapters = chapters;
        if let Some(first) = chapters.first_mut() {
            *first = format!("{}{}", head, first);
        } else {
            return Err(SplitError::NoBody);
        }
        parts.extend(chapters);
    }

    if parts.len() != leaves.len() {
        return Err(SplitError::CountMismatch {
            leaves: leaves.len(),
            chapters: parts.len(),
        });
    }

    Ok(leaves
        .iter()
        .zip(parts)
        .map(|(leaf, xml)| Division {
            division_file: leaf.division_file.clone(),
            title: leaf.title.clone(),
            xml,
        })
        .collect())
}

/// Split a document's `<body>` at every `rend="chapter"` element, returning the
/// head (everything before the first marker) and one piece per marker.
///
/// Each piece is **re-balanced**: any element still open at a boundary is
/// closed at the end of the outgoing piece and re-opened at the start of the
/// incoming one, so every piece is well-formed on its own. A `<div>` that was
/// opened immediately before the marker with nothing but whitespace in between
/// is trimmed off the outgoing piece instead of being closed as an empty
/// element — that is the common shape (`<div type="samyutta">` wrapping a
/// `<head rend="chapter">`) and leaving it in would end each division with an
/// empty div belonging to the next one.
fn split_body_at_chapters(xml: &str) -> Result<(String, Vec<String>), SplitError> {
    let body_open = find_body_open(xml).ok_or(SplitError::NoBody)?;
    let body_end = xml[body_open..]
        .find("</body>")
        .map(|i| body_open + i)
        .unwrap_or(xml.len());
    let body = &xml[body_open..body_end];

    // Each entry is one open element: where its start tag begins and ends in
    // the current piece, and the tag itself for re-opening.
    struct Open {
        tag: String,
        begins: usize,
        ends: usize,
    }

    let mut stack: Vec<Open> = Vec::new();
    let mut current = String::new();
    let mut head: Option<String> = None;
    let mut chapters: Vec<String> = Vec::new();
    let mut cursor = 0usize;

    for tag in iter_tags(body) {
        // Everything between the previous tag and this one is content.
        current.push_str(&body[cursor..tag.start]);
        cursor = tag.end;

        if tag.kind == TagKind::Start && is_chapter_tag(&body[tag.start..tag.end]) {
            // Close the piece: drop the elements that were opened with nothing
            // but whitespace since, and close the rest.
            let mut keep_open = stack.len();
            while keep_open > 0 {
                let open = &stack[keep_open - 1];
                if current[open.ends..].trim().is_empty() {
                    current.truncate(open.begins);
                    keep_open -= 1;
                } else {
                    break;
                }
            }
            for open in stack[..keep_open].iter().rev() {
                current.push_str(&format!("</{}>", element_name(&open.tag)));
            }

            let finished = std::mem::take(&mut current);
            match head {
                None => head = Some(finished),
                Some(_) => chapters.push(finished),
            }

            // Re-open the ancestors in the new piece.
            for open in stack.iter_mut() {
                open.begins = current.len();
                current.push_str(&open.tag);
                open.ends = current.len();
            }
        }

        let raw = &body[tag.start..tag.end];
        match tag.kind {
            TagKind::Start => {
                let begins = current.len();
                current.push_str(raw);
                stack.push(Open { tag: raw.to_string(), begins, ends: current.len() });
            }
            TagKind::End => {
                current.push_str(raw);
                stack.pop();
            }
            TagKind::Other => current.push_str(raw),
        }
    }

    current.push_str(&body[cursor..]);
    for open in stack.iter().rev() {
        current.push_str(&format!("</{}>", element_name(&open.tag)));
    }

    match head {
        None => Err(SplitError::NoBody),
        Some(head) => {
            chapters.push(current);
            Ok((head, chapters))
        }
    }
}

/// The offset just past the `<body …>` start tag.
fn find_body_open(xml: &str) -> Option<usize> {
    let start = xml.find("<body")?;
    let close = xml[start..].find('>')? + start;
    Some(close + 1)
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum TagKind {
    Start,
    End,
    /// Self-closing, a comment, or a processing instruction — anything that
    /// does not change the open-element stack.
    Other,
}

struct Tag {
    start: usize,
    end: usize,
    kind: TagKind,
}

/// Walk a fragment's `<…>` tags. This is a scanner and not an XML parser
/// because the pieces are re-emitted **verbatim**: entities, `xml:space`
/// content and the CST's own whitespace all have to survive the split
/// byte-for-byte, and a parse-then-serialize round trip does not guarantee
/// that. Nothing here interprets the markup beyond opening and closing.
fn iter_tags(text: &str) -> Vec<Tag> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let Some(rel) = text[i..].find('<') else { break };
        let start = i + rel;
        let Some(rel_end) = text[start..].find('>') else { break };
        let end = start + rel_end + 1;

        let inner = &text[start..end];
        let kind = if inner.starts_with("</") {
            TagKind::End
        } else if inner.starts_with("<!") || inner.starts_with("<?") || inner.ends_with("/>") {
            TagKind::Other
        } else {
            TagKind::Start
        };

        out.push(Tag { start, end, kind });
        i = end;
    }

    out
}

fn element_name(tag: &str) -> &str {
    let rest = tag.trim_start_matches('<');
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// A chapter marker is written as `<p rend="chapter">` in most files and as
/// `<head rend="chapter">` in the `<div>`-structured ones; both are boundaries.
fn is_chapter_tag(tag: &str) -> bool {
    let name = element_name(tag);
    (name == "p" || name == "head") && tag.contains("rend=\"chapter\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::navigation_tree::cst_spine::TocLeaf;

    fn leaf(file: &str, title: &str) -> TocLeaf {
        TocLeaf { title: title.to_string(), division_file: file.to_string() }
    }

    const DIV_STRUCTURED: &str = r#"<TEI.2><text><body xml:space="preserve">
<p rend="centre">Namo tassa</p>
<p rend="nikaya">Saṃyuttanikāyo</p>
<div id="sn5" type="book">
<head rend="book">Mahāvaggo</head>
<div id="sn5_1" type="samyutta">
<head rend="chapter">1. Maggasaṃyuttaṃ</head>
<p rend="bodytext">one</p>
</div>
<div id="sn5_2" type="samyutta">
<head rend="chapter">2. Bojjhaṅgasaṃyuttaṃ</head>
<p rend="bodytext">two</p>
</div>
</div>
</body></text></TEI.2>"#;

    fn span(mapped: bool, start_line: i32, end_line: i32) -> FragmentSpan {
        FragmentSpan { mapped, start_line, end_line }
    }

    #[test]
    fn classifies_by_mapping_and_gaplessness_within_the_verified_set() {
        let full = vec![span(true, 1, 10), span(true, 11, 20)];
        assert_eq!(classify_spans(true, full.clone()).class, CoverageClass::FullyMapped);
        // The gate comes first, whatever the mapping looks like.
        assert_eq!(classify_spans(false, full).class, CoverageClass::Unverified);

        let partial = vec![span(true, 1, 10), span(false, 11, 20)];
        assert_eq!(classify_spans(true, partial).class, CoverageClass::PartiallyMapped);

        let holed = vec![span(true, 1, 10), span(true, 15, 20)];
        let coverage = classify_spans(true, holed);
        assert!(!coverage.gapless);
        assert_eq!(coverage.class, CoverageClass::PartiallyMapped);

        assert_eq!(classify_spans(true, vec![span(false, 1, 10)]).class, CoverageClass::Unmapped);
        assert_eq!(classify_spans(true, Vec::new()).class, CoverageClass::Unmapped);
    }

    #[test]
    fn only_an_empty_moved_record_is_left_out() {
        assert!(is_moved_fragment(Some("moved"), true));
        // Marked moved but still carrying text: counted, so nothing is lost.
        assert!(!is_moved_fragment(Some("moved"), false));
        assert!(!is_moved_fragment(Some("checked"), true));
        assert!(!is_moved_fragment(None, true));
    }

    #[test]
    fn splits_at_chapter_markers_and_rebalances_each_piece() {
        let (head, chapters) = split_body_at_chapters(DIV_STRUCTURED).unwrap();
        assert_eq!(chapters.len(), 2);

        // The head keeps the front matter and closes nothing it did not open.
        assert!(head.contains("Namo tassa"));
        assert!(head.contains("Mahāvaggo"));
        // The samyutta div opened immediately before the marker is trimmed off
        // rather than left behind as an empty element.
        assert!(!head.contains("sn5_1"), "empty wrapper div left in the head: {}", head);
        // The book div was open, so it is closed here and re-opened below.
        assert!(head.trim_end().ends_with("</div>"));

        assert!(chapters[0].contains("sn5_1"));
        assert!(chapters[0].contains("1. Maggasaṃyuttaṃ"));
        assert!(chapters[0].contains("one"));
        assert!(!chapters[0].contains("two"));
        assert!(chapters[0].starts_with("<div id=\"sn5\""), "ancestors not re-opened");

        assert!(chapters[1].contains("2. Bojjhaṅgasaṃyuttaṃ"));
        assert!(chapters[1].contains("two"));
    }

    /// 169 of the 217 files have as many leaves as chapter markers, so their
    /// front matter belongs to the first division rather than to one of its own.
    #[test]
    fn head_is_prepended_when_the_toc_has_no_leaf_for_it() {
        let leaves = vec![leaf("x0.xml", "1. Maggasaṃyuttaṃ"), leaf("x1.xml", "2. Bojjhaṅgasaṃyuttaṃ")];
        let divisions = split_into_divisions(DIV_STRUCTURED, &leaves).unwrap();

        assert_eq!(divisions.len(), 2);
        assert_eq!(divisions[0].division_file, "x0.xml");
        assert!(divisions[0].xml.contains("Namo tassa"), "head was dropped");
        assert!(divisions[0].xml.contains("1. Maggasaṃyuttaṃ"));
        assert!(!divisions[1].xml.contains("Namo tassa"));
    }

    /// The 47 files whose toc does list the front matter get it as division 0.
    #[test]
    fn head_is_its_own_division_when_the_toc_lists_it() {
        let leaves = vec![
            leaf("x0.xml", "Ganthārambhakathā"),
            leaf("x1.xml", "1. Maggasaṃyuttaṃ"),
            leaf("x2.xml", "2. Bojjhaṅgasaṃyuttaṃ"),
        ];
        let divisions = split_into_divisions(DIV_STRUCTURED, &leaves).unwrap();

        assert_eq!(divisions.len(), 3);
        assert_eq!(divisions[0].title, "Ganthārambhakathā");
        assert!(divisions[0].xml.contains("Namo tassa"));
        assert!(divisions[1].xml.contains("1. Maggasaṃyuttaṃ"));
    }

    #[test]
    fn a_count_that_differs_by_more_than_one_is_an_error_not_a_guess() {
        let leaves = vec![leaf("x0.xml", "only one")];
        let err = split_into_divisions(DIV_STRUCTURED, &leaves).unwrap_err();
        assert!(matches!(err, SplitError::CountMismatch { leaves: 1, chapters: 2 }));
    }

    #[test]
    fn splits_flat_paragraph_files() {
        let xml = r#"<body xml:space="preserve">
<p rend="centre">Namo</p>
<p rend="book">Visuddhimaggo</p>
<p rend="chapter">12. Iddhividhaniddeso</p>
<p rend="bodytext">FIRST</p>
<p rend="chapter">13. Abhiññāniddeso</p>
<p rend="bodytext">SECOND</p>
</body>"#;
        let (head, chapters) = split_body_at_chapters(xml).unwrap();
        assert!(head.contains("Visuddhimaggo"));
        assert_eq!(chapters.len(), 2);
        assert!(chapters[0].contains("FIRST") && !chapters[0].contains("SECOND"));
        assert!(chapters[1].contains("SECOND") && !chapters[1].contains("FIRST"));
    }

    #[test]
    fn a_file_with_no_body_is_reported() {
        assert!(matches!(split_body_at_chapters("<TEI.2></TEI.2>"), Err(SplitError::NoBody)));
    }
}
