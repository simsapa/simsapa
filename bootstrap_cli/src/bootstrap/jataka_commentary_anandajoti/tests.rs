//! Fixture and real-data tests of the extraction.
//!
//! The fixture, `tests/data/jataka-commentary-anandajoti/Fixture-Jataka.epub/`,
//! is an unpacked epub: front matter and a Nidānakathā stub that stay off the
//! pages, Book I with Ja 1 (a note in its heading, a page marker, a verse, a
//! link to another file) and Ja 2, which crosses a file boundary and has two
//! sections with numbered verses and a span class the extraction does not
//! map, then the end of the book, a volume heading and a Preface that stay
//! off the pages, and Book II with Ja 3. Its expected output is `expected/`
//! beside it. After an intended change of the output, re-write it with
//! `JA_COMM_BLESS=1 cargo test jataka_commentary` and review the diff.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use scraper::{Html, Selector};

use super::extract::{extract, write_extraction, Extraction};
use super::{MANIFEST_FILE, REPORT_FILE};
use crate::bootstrap::anandajoti_epub::epub::EpubFiles;
use crate::bootstrap::source_paths;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/jataka-commentary-anandajoti")
}

fn fixture_extraction() -> Extraction {
    let epub = EpubFiles::from_dir(&fixture_dir().join("Fixture-Jataka.epub")).unwrap();
    extract(&epub).unwrap()
}

/// The output files of an extraction, by name.
fn output_files(extraction: &Extraction) -> BTreeMap<String, String> {
    let dir = tempfile::tempdir().unwrap();
    write_extraction(extraction, dir.path()).unwrap();
    std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (path.file_name().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&path).unwrap())
        })
        .collect()
}

/// Problems with a page's in-page links: an `href` that leads outside the
/// page, one with no matching `id`, and duplicate ids.
fn link_problems(html: &str) -> Vec<String> {
    let doc = Html::parse_fragment(html);
    let mut problems = Vec::new();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for el in doc.select(&Selector::parse("[id]").unwrap()) {
        let id = el.value().attr("id").unwrap().to_string();
        if !ids.insert(id.clone()) {
            problems.push(format!("duplicate id {}", id));
        }
    }
    for el in doc.select(&Selector::parse("[href]").unwrap()) {
        let href = el.value().attr("href").unwrap();
        match href.strip_prefix('#') {
            Some(target) if ids.contains(target) => {}
            Some(target) => problems.push(format!("href #{} has no matching id", target)),
            None => problems.push(format!("href {} leads outside the page", href)),
        }
    }
    problems
}

#[test]
fn the_fixture_output_is_the_expected_output() {
    let files = output_files(&fixture_extraction());
    let expected_dir = fixture_dir().join("expected");
    if std::env::var("JA_COMM_BLESS").is_ok() {
        std::fs::create_dir_all(&expected_dir).unwrap();
        for (name, text) in &files {
            std::fs::write(expected_dir.join(name), text).unwrap();
        }
    }
    let expected: BTreeMap<String, String> = std::fs::read_dir(&expected_dir)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (path.file_name().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&path).unwrap())
        })
        .collect();
    assert_eq!(files.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>());
    for (name, text) in &files {
        assert!(text == &expected[name], "{} differs from expected/{}", name, name);
    }
}

#[test]
fn two_runs_give_the_same_bytes() {
    assert_eq!(output_files(&fixture_extraction()), output_files(&fixture_extraction()));
}

#[test]
fn the_front_matter_the_prefaces_and_the_book_headings_make_no_page() {
    let extraction = fixture_extraction();
    let uids: Vec<&str> = extraction.manifest.jatakas.iter().map(|j| j.uid.as_str()).collect();
    assert_eq!(uids, ["ja1.att/en/anandajoti", "ja2.att/en/anandajoti", "ja3.att/en/anandajoti"]);
    for page in &extraction.pages {
        for text in ["front matter", "Nidānakathā is not", "preface stays off", "Praised be", "Volume II", "End of the First Book"] {
            assert!(!page.html.contains(text), "{} contains {}", page.file_name, text);
        }
    }
}

#[test]
fn the_manifest_has_the_titles_the_nipata_and_the_verse_count() {
    let manifest = fixture_extraction().manifest;
    let ja1 = &manifest.jatakas[0];
    assert_eq!(ja1.title, "The Birth Story about What Is Unquestionable");
    assert_eq!(ja1.title_pali, "Apaṇṇakajātaka");
    assert_eq!((ja1.nipata_number, ja1.nipata.as_str(), ja1.verse_count), (1, "Ekanipāta", 1));
    let ja2 = &manifest.jatakas[1];
    assert_eq!((ja2.nipata_number, ja2.verse_count), (1, 2));
    let ja3 = &manifest.jatakas[2];
    assert_eq!((ja3.nipata_number, ja3.nipata.as_str()), (2, "Dukanipāta"));
    assert_eq!(ja3.title, "The Birth Story about the Merchant");
}

#[test]
fn every_link_on_every_fixture_page_leads_to_an_id_on_it() {
    let extraction = fixture_extraction();
    for page in &extraction.pages {
        let problems = link_problems(&page.html);
        assert!(problems.is_empty(), "{}: {:?}", page.file_name, problems);
    }
    // The link to another file is plain text.
    assert!(extraction.pages[0].html.contains("see the next file &amp; more."));
}

#[test]
fn the_jataka_crossing_a_file_is_whole_and_in_order() {
    let html = fixture_extraction().pages.remove(1).html;
    let order = [
        "Ja 2 begins at the end of this file.",
        "Ja 2 goes on in this file.",
        "id=\"ja-2-the-first-part\"",
        "First part text.",
        "id=\"ja-2-v1\"",
        "id=\"ja-2-the-second-part\"",
        "Second part text.",
    ];
    let positions: Vec<usize> = order.iter().map(|text| html.find(text).unwrap_or_else(|| panic!("{} missing", text))).collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{:?}", positions);
}

#[test]
fn the_section_list_links_to_the_section_headings_and_has_no_note_ref() {
    let html = fixture_extraction().pages.remove(1).html;
    let doc = Html::parse_fragment(&html);
    let links: Vec<String> = doc
        .select(&Selector::parse("nav.section-toc a").unwrap())
        .map(|a| a.value().attr("href").unwrap().trim_start_matches('#').to_string())
        .collect();
    let headings: Vec<String> = doc
        .select(&Selector::parse("h3.section-heading").unwrap())
        .map(|h| h.value().attr("id").unwrap().to_string())
        .collect();
    assert_eq!(links, ["ja-2-the-first-part", "ja-2-the-second-part"]);
    assert_eq!(links, headings);
    assert!(html.contains("<li><a href=\"#ja-2-the-second-part\">The Second Part</a></li>"));
    // A Jātaka without sections has no list.
    assert!(!fixture_extraction().pages.remove(0).html.contains("section-toc"));
}

#[test]
fn the_notes_keep_the_book_numbers_and_the_heading_note_stays() {
    let html = fixture_extraction().pages.remove(0).html;
    assert!(html.contains("<li id=\"note-2\" value=\"2\">"));
    assert!(html.contains("<li id=\"note-3\" value=\"3\">"));
    // The note of the heading is on the heading.
    assert!(html.contains("Apaṇṇakajātaka<sup class=\"footnote-reference\"><a id=\"note-ref-2\" href=\"#note-2\">2</a></sup><br>"));
    // The note of the front matter is not on the page, and a note running
    // over two paragraphs keeps both.
    assert!(!html.contains("note-1\""));
    assert!(html.contains("Note 3 text.</p><p>Note 3 runs into a second paragraph. <a class=\"footnote-backref\" href=\"#note-ref-3\">"));
}

#[test]
fn page_markers_are_spans_also_when_split_over_two_spans() {
    let extraction = fixture_extraction();
    assert!(extraction.pages[0].html.contains("<span class=\"pts-ref\">[1.1] {1.95}</span>"));
    assert!(extraction.pages[1].html.contains("<span class=\"pts-ref\">{1.9</span><span class=\"pts-ref\">6}</span>"));
}

#[test]
fn verses_get_anchors_and_the_classes_are_semantic() {
    let html = fixture_extraction().pages.remove(0).html;
    assert!(html.contains("<a id=\"ja-1-v1\"></a>\n<p class=\"verse\">1. Apaṇṇakaṁ ṭhānam-eke,</p>"));
    assert!(html.contains("<p class=\"verse-translation\">"));
    assert!(html.contains("<p class=\"verse-quote\">"));
    assert!(html.contains("<p class=\"translator-note\">In the present a layman"));
    assert!(html.contains("<p class=\"list-line translator-note\">"));
    assert!(html.contains("<i>In the past</i>"));
}

#[test]
fn the_report_names_the_planted_and_the_natural_defects() {
    let extraction = fixture_extraction();
    let report = &extraction.report_text;
    // Only three Jātakas in the fixture.
    assert!(report.contains("- Ja 4–547 missing from the book"), "{}", report);
    assert!(report.contains("- <span class=\"t99\"> (1×)"), "{}", report);
    assert!(report.contains("Summary: 3 Jātakas, 2 sections, 3 verses, 5 note references to 5 notes, 1 defects, 1 unmapped"), "{}", report);
}

/// The real epub, when the bootstrap assets are checked out.
#[test]
fn test_real_data_jataka_commentary_extract() {
    let epub_path = source_paths::bootstrap_assets_dir_from_crate().join(source_paths::jataka_commentary_epub_file());
    if !matches!(epub_path.try_exists(), Ok(true)) {
        return;
    }
    let extraction = extract(&EpubFiles::from_zip(&epub_path).unwrap()).unwrap();
    // Defects are the source's: shown, not failed on.
    println!("{}", extraction.report_text);

    let numbers: Vec<u32> = extraction.manifest.jatakas.iter().map(|j| j.jataka_number).collect();
    assert_eq!(numbers, (1..=547).collect::<Vec<u32>>());
    for page in &extraction.pages {
        let problems = link_problems(&page.html);
        assert!(problems.is_empty(), "{}: {:?}", page.file_name, problems);
    }
    assert!(extraction.report_text.contains("Markup the extraction does not map (rendered as its text):\n(none)"));
    let files = output_files(&extraction);
    assert!(files.contains_key(MANIFEST_FILE) && files.contains_key(REPORT_FILE));
    assert_eq!(files.len(), 549);
}
