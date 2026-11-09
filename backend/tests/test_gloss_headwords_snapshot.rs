//! Snapshot of the Headwords gloss format: the serialized
//! `AllParagraphsProcessingResult` (and one `SingleParagraphProcessingResult`)
//! for fixed passages, compared byte for byte with the files in
//! `tests/data/gloss_headwords_snapshot/`. It guards the rule that the
//! Headwords output of `/gloss_text`, `simsapa gloss` and the Gloss tab does
//! not change when the pipeline gains other formats.
//!
//! The output depends on the dist databases as well as on the code: the DPD
//! release in `dpd.sqlite3` and the built-in word-selection rows in
//! `appdata.sqlite3`. After a DPD data upgrade or a new built-in data bank, a
//! difference here is expected; check it, then regenerate the files with
//!
//! ```sh
//! cd backend && SIMSAPA_UPDATE_SNAPSHOTS=1 cargo test --test test_gloss_headwords_snapshot
//! ```
//!
//! A missing snapshot file is written on the first run, and the test passes.

#[path = "helpers/dpd_report.rs"]
mod dpd_report;

mod helpers;
use helpers as h;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{process_all_paragraphs, process_single_paragraph};
use simsapa_backend::types::{
    AllParagraphsProcessingInput, AllParagraphsProcessingResult, GlossFormat, ParagraphProcessingResult,
    ProcessedWord, SingleParagraphProcessingInput, WordProcessingOptions,
};

const FIXTURES: [&str; 5] = ["TH215", "MN41_p2", "SN15.1_p2", "DHP211", "AN3.33_p1"];

const COMMON_WORDS_JSON: &str = include_str!("../../assets/common-words.json");

fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/gloss_headwords_snapshot")
}

fn options(no_duplicates_globally: bool, skip_common: bool) -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally,
        skip_common,
        common_words: if skip_common {
            serde_json::from_str(COMMON_WORDS_JSON).unwrap()
        } else {
            Vec::new()
        },
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format: GlossFormat::Headwords,
    }
}

fn fixture_passage(name: &str) -> String {
    dpd_report::read_report(&dpd_report::fixture_report_path(name)).passage
}

/// Compare `value`'s pretty JSON with the snapshot file `name`, writing it when
/// missing or when `SIMSAPA_UPDATE_SNAPSHOTS` is set.
fn check_snapshot<T: serde::Serialize>(name: &str, value: &T) {
    let actual = serde_json::to_string_pretty(value).unwrap() + "\n";
    let path = snapshot_dir().join(format!("{}.json", name));
    let update = std::env::var_os("SIMSAPA_UPDATE_SNAPSHOTS").is_some();
    let exists = path.try_exists().unwrap_or(false);
    if update || !exists {
        std::fs::create_dir_all(snapshot_dir()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap();
    if expected != actual {
        let first_diff = expected
            .lines()
            .zip(actual.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("line {}:\n  expected: {}\n  actual:   {}", i + 1, a, b))
            .unwrap_or_else(|| "lengths differ".to_string());
        panic!("Headwords snapshot {} changed, first difference at {}", path.display(), first_diff);
    }
}

/// `AllParagraphsProcessingResult` with `updated_global_stems` in a stable
/// order (it is a `HashMap`); every other field serializes as the API does.
#[derive(serde::Serialize)]
struct StableAll<'a> {
    success: bool,
    paragraphs: &'a [ParagraphProcessingResult],
    global_unrecognized_words: &'a [String],
    updated_global_stems: BTreeMap<&'a String, &'a bool>,
}

fn stable(result: &AllParagraphsProcessingResult) -> StableAll<'_> {
    StableAll {
        success: result.success,
        paragraphs: &result.paragraphs,
        global_unrecognized_words: &result.global_unrecognized_words,
        updated_global_stems: result.updated_global_stems.iter().collect(),
    }
}

fn gloss_all(paragraphs: Vec<String>, options: WordProcessingOptions) -> AllParagraphsProcessingResult {
    let app_data = get_app_data();
    let input = AllParagraphsProcessingInput { paragraphs, options };
    process_all_paragraphs(&input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap()
}

#[test]
#[serial]
fn test_headwords_snapshot_fixture_passages() {
    h::app_data_setup();
    for name in FIXTURES {
        let result = gloss_all(vec![fixture_passage(name)], options(false, false));
        check_snapshot(name, &stable(&result));
    }
}

#[test]
#[serial]
fn test_headwords_snapshot_no_duplicates_globally() {
    h::app_data_setup();
    // AN3.33 and MN41 share many words (`atha kho … yena bhagavā …`), so the
    // second paragraph loses its repeats.
    let paragraphs = vec![fixture_passage("AN3.33_p1"), fixture_passage("MN41_p2")];
    let result = gloss_all(paragraphs, options(true, false));
    check_snapshot("no_duplicates_globally", &stable(&result));
}

#[test]
#[serial]
fn test_headwords_snapshot_skip_common() {
    h::app_data_setup();
    let result = gloss_all(vec![fixture_passage("SN15.1_p2")], options(false, true));
    check_snapshot("skip_common", &stable(&result));
}

/// The Gloss tab's per-paragraph "Update Gloss" path, which seeds the global
/// de-dup map from the paragraphs before it.
#[test]
#[serial]
fn test_headwords_snapshot_single_paragraph_with_existing_stems() {
    h::app_data_setup();
    let first = gloss_all(vec![fixture_passage("AN3.33_p1")], options(true, false));

    let mut opts = options(true, false);
    opts.existing_global_stems = first.updated_global_stems.clone();
    let input = SingleParagraphProcessingInput {
        paragraph_text: fixture_passage("MN41_p2"),
        options: opts,
    };
    let app_data = get_app_data();
    let result = process_single_paragraph(1, &input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap();
    #[derive(serde::Serialize)]
    struct StableSingle<'a> {
        success: bool,
        paragraph_index: usize,
        words_data: &'a [ProcessedWord],
        unrecognized_words: &'a [String],
        updated_global_stems: BTreeMap<&'a String, &'a bool>,
    }
    let value = StableSingle {
        success: result.success,
        paragraph_index: result.paragraph_index,
        words_data: &result.words_data,
        unrecognized_words: &result.unrecognized_words,
        updated_global_stems: result.updated_global_stems.iter().collect(),
    };
    check_snapshot("single_paragraph_existing_stems", &value);
}
