//! Fixture and real-data tests of the extraction.
//!
//! The fixture, `tests/data/dhammapada-commentary-anandajoti/Fixture-Comm.epub/`,
//! is an unpacked epub: one vagga whose first story crosses a file boundary
//! and has two sub-stories (one with a note ref in its heading), a second
//! story, a note referenced twice and running over two paragraphs, page
//! markers (one inside a verse line), a link to another file, and front
//! matter and a Conclusion that stay off the page. Its expected output is
//! `expected/` beside it. After an intended change of the output, re-write
//! it with `DHP_COMM_BLESS=1 cargo test dhammapada_commentary` and review the
//! diff.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use scraper::{Html, Selector};

use crate::bootstrap::anandajoti_epub::epub::EpubFiles;
use super::extract::{extract, write_extraction, Extraction};
use super::{MANIFEST_FILE, REPORT_FILE, SC_VAGGA_RANGES};
use crate::bootstrap::source_paths;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/dhammapada-commentary-anandajoti")
}

fn fixture_extraction() -> Extraction {
    let epub = EpubFiles::from_dir(&fixture_dir().join("Fixture-Comm.epub")).unwrap();
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
    if std::env::var("DHP_COMM_BLESS").is_ok() {
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
fn every_link_on_the_fixture_page_leads_to_an_id_on_it() {
    let extraction = fixture_extraction();
    assert_eq!(extraction.pages.len(), 1);
    let problems = link_problems(&extraction.pages[0].html);
    assert!(problems.is_empty(), "{:?}", problems);
    // The link to another file is plain text.
    assert!(extraction.pages[0].html.contains("as told in the Prologue."));
}

/// The text of the `<section>` with this id.
fn section(html: &str, id: &str) -> String {
    let start = html.find(&format!("<section class=\"story\" id=\"{}\">", id)).unwrap();
    let end = start + html[start..].find("</section>").unwrap();
    html[start..end].to_string()
}

#[test]
fn a_story_that_crosses_a_file_boundary_is_whole_and_in_order() {
    let html = fixture_extraction().pages.remove(0).html;
    let story = section(&html, "story-1-1");
    let order = [
        "1.1 The Story about the Elder Cakkhupāla",
        "Cakkhupālattheravatthu",
        "Mind precedes thoughts",
        "id=\"story-1-1-1a\"",
        "In times long past",
        "id=\"story-1-1-1b\"",
        "The story goes on in the next file",
    ];
    let positions: Vec<usize> = order.iter().map(|text| story.find(text).unwrap_or_else(|| panic!("{} missing", text))).collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{:?}", positions);
    assert!(!section(&html, "story-1-2").contains("The story goes on"));
    // Front matter and Conclusion stay off the page.
    assert!(!html.contains("front matter is not"));
    assert!(!html.contains("Having shown"));
}

#[test]
fn each_substory_list_links_to_its_headings() {
    let html = fixture_extraction().pages.remove(0).html;
    let story = section(&html, "story-1-1");
    let doc = Html::parse_fragment(&story);
    let links: Vec<String> = doc
        .select(&Selector::parse("nav.substory-toc a").unwrap())
        .map(|a| a.value().attr("href").unwrap().trim_start_matches('#').to_string())
        .collect();
    let headings: Vec<String> = doc
        .select(&Selector::parse("h4.substory-heading").unwrap())
        .map(|h| h.value().attr("id").unwrap().to_string())
        .collect();
    assert_eq!(links, ["story-1-1-1a", "story-1-1-1b"]);
    assert_eq!(links, headings);
    // The list text has no note ref.
    assert!(story.contains("<li><a href=\"#story-1-1-1a\">1a Story of the Past: The Wicked Physician</a></li>"));
    // A story with no sub-stories has no list.
    assert!(!section(&html, "story-1-2").contains("substory-toc"));
}

#[test]
fn the_notes_keep_the_book_numbers() {
    let html = fixture_extraction().pages.remove(0).html;
    assert!(html.contains("<li id=\"note-1\" value=\"1\">"));
    assert!(html.contains("<li id=\"note-2\" value=\"2\">"));
    // Referenced twice: the second ref has its own id, the back-link goes to
    // the first.
    assert!(html.contains("id=\"note-ref-2\" href=\"#note-2\""));
    assert!(html.contains("id=\"note-ref-2-2\" href=\"#note-2\""));
    assert!(html.contains("into a second paragraph. <a class=\"footnote-backref\" href=\"#note-ref-2\">"));
    // The notes of the front matter and the Conclusion are not on the page.
    assert!(!html.contains("note-3"));
    assert!(!html.contains("note-4"));
}

#[test]
fn the_fixture_report_names_its_range_as_not_an_sc_range() {
    let extraction = fixture_extraction();
    assert!(
        extraction
            .report_text
            .contains("the stories of vagga 1 cover Dhp 1-2, which is not its SuttaCentral range (1-20)"),
        "{}",
        extraction.report_text
    );
    assert_eq!(extraction.manifest.vaggas[0].uid, "dhp1-20.att/en/anandajoti");
}

/// The real epub, when the bootstrap assets are checked out.
#[test]
fn test_real_data_dhammapada_commentary_extract() {
    let epub_path = source_paths::bootstrap_assets_dir_from_crate().join(source_paths::dhammapada_commentary_epub_file());
    if !matches!(epub_path.try_exists(), Ok(true)) {
        return;
    }
    let extraction = extract(&EpubFiles::from_zip(&epub_path).unwrap()).unwrap();
    // Defects are the source's: shown, not failed on.
    println!("{}", extraction.report_text);

    assert_eq!(extraction.pages.len(), 26);
    assert_eq!(extraction.story_count, 305);
    let ranges: Vec<(u32, u32)> = extraction.manifest.vaggas.iter().map(|v| (v.verse_start, v.verse_end)).collect();
    assert_eq!(ranges, SC_VAGGA_RANGES);
    for page in &extraction.pages {
        let problems = link_problems(&page.html);
        assert!(problems.is_empty(), "{}: {:?}", page.file_name, problems);
    }
    assert!(extraction.report_text.contains("Markup the extraction does not map (rendered as its text):\n(none)"));
    let files = output_files(&extraction);
    assert!(files.contains_key(MANIFEST_FILE) && files.contains_key(REPORT_FILE));
}
