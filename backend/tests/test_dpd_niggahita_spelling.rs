//! A word written with a final `n` for `ṁ` (`vuttan`, as in `vuttan ti`) is
//! looked up as its `ṁ` spelling before the fallback guesses, in the grouped
//! and flat DPD lookups, the deconstructor list and the gloss. Against the
//! dist DB, read-only.
//!
//! The asserted facts were checked on `dpd_release_version = v0.4.20260501`.
//! Re-check them after a DPD update (run in `…/dist/simsapa/app-assets/`):
//!
//! ```sh
//! sqlite3 dpd.sqlite3 "SELECT lookup_key, headwords FROM lookup
//!   WHERE lookup_key IN ('vuttan','vuttaṁ','kin','kiṁ','tan','bhikkhuṁ');"
//! # no row for vuttan; vuttaṁ [70033, …, 83222] (10 ids);
//! # kin: a row with no headwords and no split (English-to-Pāli data only);
//! # kiṁ [19158, 19159, 21591, 21956, 21957, 21958, 22021, 22022];
//! # tan: no headwords, roots ["√tan"]; bhikkhuṁ [49882, 49885].
//!
//! sqlite3 dpd.sqlite3 "SELECT id, lemma_1 FROM dpd_headwords WHERE id IN (70051, 21724);"
//! # 70051 | vuttanaya   (the "starts with" guess for vuttan)
//! # 21724 | kināti      (the stem match for kin)
//! ```

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::process_all_paragraphs;
use simsapa_backend::types::{AllParagraphsProcessingInput, FormLookupStatus, WordProcessingOptions};

mod helpers;
use helpers as h;

const VUTTAM: [&str; 10] = [
    "70033/dpd", "70034/dpd", "70035/dpd", "70036/dpd", "70037/dpd",
    "70038/dpd", "70039/dpd", "72473/dpd", "77977/dpd", "83222/dpd",
];

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn expected(uids: &[&str]) -> Vec<String> {
    sorted(uids.iter().map(|s| s.to_string()).collect())
}

#[test]
#[serial]
fn test_grouped_final_n_is_niggahita() {
    h::app_data_setup();
    let dpd = &get_app_data().dbm.dpd;

    let g = dpd.dpd_lookup_grouped("vuttan", false, true, false, None, None).unwrap();
    assert_eq!(g.query, "vuttan");
    assert_eq!(sorted(g.direct_uids.clone()), expected(&VUTTAM));
    assert!(!g.results.iter().any(|r| r.uid == "70051/dpd"), "vuttanaya is a guess");
    assert!(g.deconstructions.is_empty());

    // The `ṁ` spelling is tried before the stem match, which finds the verb
    // kināti for kin.
    let g = dpd.dpd_lookup_grouped("kin", false, true, false, None, None).unwrap();
    assert_eq!(
        sorted(g.direct_uids.clone()),
        expected(&["19158/dpd", "19159/dpd", "21591/dpd", "21956/dpd", "21957/dpd", "21958/dpd", "22021/dpd", "22022/dpd"])
    );
    assert!(!g.results.iter().any(|r| r.uid == "21724/dpd"), "kināti is a stem match");

    let g = dpd.dpd_lookup_grouped("bhikkhun", false, true, false, None, None).unwrap();
    assert_eq!(sorted(g.direct_uids.clone()), expected(&["49882/dpd", "49885/dpd"]));
}

#[test]
#[serial]
fn test_flat_final_n_is_niggahita() {
    h::app_data_setup();
    let dpd = &get_app_data().dbm.dpd;

    let res = dpd.dpd_lookup("vuttan", false, true, None, None).unwrap();
    let uids: Vec<String> = res.iter().map(|r| r.uid.clone()).collect();
    assert_eq!(sorted(uids), expected(&VUTTAM));
}

#[test]
#[serial]
fn test_exact_matches_are_kept() {
    h::app_data_setup();
    let dpd = &get_app_data().dbm.dpd;

    // An exact root match is a direct hit: tan stays the root √tan.
    let g = dpd.dpd_lookup_grouped("tan", false, true, false, None, None).unwrap();
    assert_eq!(g.results.len(), 1);
    assert_eq!(g.results[0].title, "√tan");

    // A word DPD knows as written is not respelt.
    let g = dpd.dpd_lookup_grouped("vuttanaya", false, true, false, None, None).unwrap();
    assert_eq!(g.direct_uids, vec!["70051/dpd".to_string()]);

    // A word whose `ṁ` spelling is unknown too gets nothing new.
    let g = dpd.dpd_lookup_grouped("xyzan", false, true, false, None, None).unwrap();
    assert!(g.results.is_empty());
}

#[test]
#[serial]
fn test_deconstructor_list_does_not_guess_final_n() {
    h::app_data_setup();
    let dpd = &get_app_data().dbm.dpd;

    // The "starts with" break-down guess gave `vutti + anta`, `vutta + anta`, …
    assert!(dpd.dpd_deconstructor_list("vuttan").is_empty());
}

#[test]
#[serial]
fn test_exact_lookup_does_not_respell() {
    h::app_data_setup();
    let dpd = &get_app_data().dbm.dpd;

    // The exact form lookup makes no guesses, so it has nothing to pre-empt.
    let r = dpd.lookup_forms_exact(&["vuttan".to_string()]).unwrap();
    assert_eq!(r[0].status, FormLookupStatus::NotFound);
}

#[test]
#[serial]
fn test_gloss_spaced_n_ti() {
    h::app_data_setup();
    let app_data = get_app_data();
    let input = AllParagraphsProcessingInput {
        paragraphs: vec!["Evaṁ vuttan ti bhikkhave.".to_string()],
        options: WordProcessingOptions {
            no_duplicates_globally: false,
            skip_common: false,
            common_words: Vec::new(),
            existing_global_stems: Default::default(),
            existing_paragraph_unrecognized: Default::default(),
            existing_global_unrecognized: Vec::new(),
        },
    };
    let res = process_all_paragraphs(&input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap();
    let word = res.paragraphs[0]
        .words_data
        .iter()
        .find(|w| w.original_word == "vuttan")
        .expect("vuttan is glossed");
    assert_eq!(sorted(word.direct_uids.clone()), expected(&VUTTAM));
    assert_eq!(word.stem, "vutta 1.1");
}
