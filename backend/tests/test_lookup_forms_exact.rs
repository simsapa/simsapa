//! Exact DPD form lookup (`DpdDbHandle::lookup_forms_exact()`), against the
//! dist DB, read-only.
//!
//! The asserted facts were checked on `dpd_release_version = v0.4.20260501`.
//! Re-check them after a DPD update (run in `…/dist/simsapa/app-assets/`):
//!
//! ```sh
//! sqlite3 dpd.sqlite3 "SELECT lookup_key, headwords, deconstructor FROM lookup
//!   WHERE lookup_key IN ('bhagavantaṁ','atthaññe','cittanti','bhavissanti','iti',
//!   'bhante','āvuso','aññe','atthi','bhagav','atthañ','xyzqq');"
//! # bhagavantaṁ [49139, 49140]; atthaññe ["atthi + aññe"] (no headwords);
//! # cittanti 10 ids + ["cittaṁ + iti"]; bhavissanti 9 ids + ["bhavissaṁ + iti"];
//! # iti [13466]; bhante 6 ids; āvuso [13147]; aññe 7 ids; atthi 4 ids;
//! # no rows for bhagav, atthañ, xyzqq.
//!
//! sqlite3 dpd.sqlite3 "SELECT id, lemma_1, lemma_clean, quote(construction), root_key
//!   FROM dpd_headwords WHERE id IN (49139, 13466);"
//! # 49139 | bhagavant 1 | bhagavant | '√bhaj > bhag + a + vant\nbhaga + vant' | √bhaj 1
//! # 13466 | iti | iti | '' |
//!
//! sqlite3 dpd.sqlite3 "SELECT value FROM db_info WHERE key = 'dpd_release_version';"
//! # v0.4.20260501
//! ```

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::types::{
    FormLookupResult, FormLookupStatus, FormMatchedAs, FormSplitKind, FormSplitSource,
};

mod helpers;
use helpers as h;

fn lookup(forms: &[&str]) -> Vec<FormLookupResult> {
    h::app_data_setup();
    let app_data = get_app_data();
    let forms: Vec<String> = forms.iter().map(|s| s.to_string()).collect();
    let res = app_data.dbm.dpd.lookup_forms_exact(&forms).unwrap();
    assert_eq!(res.len(), forms.len());
    res
}

fn lookup_one(form: &str) -> FormLookupResult {
    lookup(&[form]).remove(0)
}

fn uids(r: &FormLookupResult) -> Vec<&str> {
    r.headwords.iter().map(|hw| hw.uid.as_str()).collect()
}

fn splits(r: &FormLookupResult) -> Vec<&str> {
    r.deconstructions.iter().map(|d| d.split.as_str()).collect()
}

#[test]
#[serial]
fn test_bhagavantam_found_with_construction() {
    let r = lookup_one("bhagavantaṁ");
    assert_eq!(r.status, FormLookupStatus::Found);
    assert_eq!(r.lookup_key, "bhagavantaṁ");
    assert_eq!(uids(&r), vec!["49139/dpd", "49140/dpd"]);

    let hw = &r.headwords[0];
    assert_eq!(hw.lemma_1, "bhagavant 1");
    assert_eq!(hw.construction, vec!["√bhaj > bhag + a + vant", "bhaga + vant"]);
    assert_eq!(hw.root_key, "√bhaj 1");
    assert_eq!(hw.matched_as, FormMatchedAs::Inflection);
    assert!(r.deconstructions.is_empty());
}

#[test]
#[serial]
fn test_niggahita_spelling_gives_same_key() {
    let r = lookup_one("bhagavantaṃ");
    assert_eq!(r.lookup_key, "bhagavantaṁ");
    assert_eq!(r.status, FormLookupStatus::Found);
}

#[test]
#[serial]
fn test_iti_is_lemma() {
    let r = lookup_one("iti");
    assert_eq!(r.status, FormLookupStatus::Found);
    assert_eq!(uids(&r), vec!["13466/dpd"]);
    assert_eq!(r.headwords[0].matched_as, FormMatchedAs::Lemma);
    assert!(r.headwords[0].construction.is_empty());
}

#[test]
#[serial]
fn test_prefix_is_not_found() {
    let r = lookup_one("bhagav");
    assert_eq!(r.status, FormLookupStatus::NotFound);
    assert!(r.headwords.is_empty());
    assert!(r.deconstructions.is_empty());
}

#[test]
#[serial]
fn test_no_prefix_guess_split() {
    let r = lookup_one("atthañ");
    assert!(!splits(&r).contains(&"atthaṁ + ca"), "got {:?}", splits(&r));
    assert_eq!(r.status, FormLookupStatus::NotFound);
}

#[test]
#[serial]
fn test_atthanne_deconstructed() {
    let r = lookup_one("atthaññe");
    assert_eq!(r.status, FormLookupStatus::Deconstructed);
    assert!(r.headwords.is_empty());
    assert_eq!(splits(&r), vec!["atthi + aññe"]);

    let d = &r.deconstructions[0];
    assert_eq!(d.source, FormSplitSource::Dpd);
    assert_eq!(d.kind, FormSplitKind::Sandhi);
    let words: Vec<&str> = d.components.iter().map(|c| c.word.as_str()).collect();
    assert_eq!(words, vec!["atthi", "aññe"]);
    assert_eq!(d.components[0].uids, vec!["2736/dpd", "2737/dpd", "2738/dpd", "2757/dpd"]);
    assert!(!d.components[1].uids.is_empty());
}

#[test]
#[serial]
fn test_common_words_found() {
    for form in ["bhante", "āvuso"] {
        let r = lookup_one(form);
        assert_eq!(r.status, FormLookupStatus::Found, "{}", form);
    }
    assert_eq!(uids(&lookup_one("āvuso")), vec!["13147/dpd"]);
    assert_eq!(lookup_one("bhante").headwords.len(), 6);
}

#[test]
#[serial]
fn test_unknown_is_not_found() {
    let r = lookup_one("xyzqq");
    assert_eq!(r.status, FormLookupStatus::NotFound);
    assert_eq!(r.note, None);
}

#[test]
#[serial]
fn test_invalid_forms() {
    let res = lookup(&["", "a b"]);
    assert_eq!(res[0].status, FormLookupStatus::Invalid);
    assert_eq!(res[0].note.as_deref(), Some("empty after normalisation"));
    assert_eq!(res[1].status, FormLookupStatus::Invalid);
    assert_eq!(res[1].note.as_deref(), Some("more than one word"));
}

#[test]
#[serial]
fn test_order_and_duplicates_kept() {
    let res = lookup(&["iti", "xyzqq", "iti"]);
    let inputs: Vec<&str> = res.iter().map(|r| r.input.as_str()).collect();
    assert_eq!(inputs, vec!["iti", "xyzqq", "iti"]);
    assert_eq!(res[0].status, FormLookupStatus::Found);
    assert_eq!(res[1].status, FormLookupStatus::NotFound);
    assert_eq!(res[2], res[0]);
}

#[test]
#[serial]
fn test_empty_batch() {
    assert!(lookup(&[]).is_empty());
}

#[test]
#[serial]
fn test_serialised_names() {
    let r = lookup_one("atthaññe");
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["status"], "deconstructed");
    assert_eq!(v["deconstructions"][0]["kind"], "sandhi");
    assert_eq!(v["deconstructions"][0]["source"], "dpd");
    assert!(v.get("note").is_none(), "note must be skipped when None");

    let r = lookup_one("bhagavantaṁ");
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["headwords"][0]["matched_as"], "inflection");
    let v = serde_json::to_value(lookup_one("xyzqq")).unwrap();
    assert_eq!(v["status"], "not_found");
}

#[test]
#[serial]
fn test_dpd_release_version() {
    h::app_data_setup();
    let app_data = get_app_data();
    let v = app_data.dbm.dpd.dpd_release_version().expect("dpd_release_version");
    assert!(v.starts_with('v'), "got {}", v);
    // The cached second read gives the same answer.
    assert_eq!(app_data.dbm.dpd.dpd_release_version(), Some(v));
}
