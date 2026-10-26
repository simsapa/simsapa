//! A sutta's Bilara JSON (`sutta_json::get_sutta_content_json()`, behind
//! `GET /sutta.json`) and its companion records
//! (`sutta_json::get_sutta_bilara_json()`, behind `GET /sutta_bilara.json`),
//! against the dist DB, read-only.
//!
//! Re-check the facts after a re-bootstrap (run in `…/dist/simsapa/app-assets/`):
//!
//! ```sh
//! sqlite3 appdata.sqlite3 "SELECT uid, length(content_json) FROM suttas WHERE uid IN
//!   ('mn1/pli/ms','dhp179-196/pli/ms','sn45.92-95/pli/ms','sn45.92-95/en/sujato','dn1/pli/cst');"
//! ```
//!
//! `dn1/pli/cst` has a NULL `content_json`; the others have one.
//!
//! ```sh
//! sqlite3 appdata.sqlite3 "SELECT sutta_uid FROM sutta_variants WHERE sutta_uid LIKE 'mn1/%';
//!   SELECT sutta_uid FROM sutta_comments WHERE sutta_uid LIKE 'mn1/%';"
//! ```
//!
//! `mn1/pli/ms` has variant readings, `mn1/en/sujato` has comments.

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::sutta_json::{get_sutta_bilara_json, get_sutta_content_json, SuttaBilaraJson, SuttaContentJson};

mod helpers;
use helpers as h;

fn content(uid: &str) -> SuttaContentJson {
    h::app_data_setup();
    get_sutta_content_json(&get_app_data().dbm.appdata, uid)
}

fn found_uid(uid: &str) -> String {
    match content(uid) {
        SuttaContentJson::Found { uid, .. } => uid,
        other => panic!("{uid}: expected Found, got {other:?}"),
    }
}

#[test]
#[serial]
fn test_content_json_is_the_stored_column_verbatim() {
    h::app_data_setup();
    let app_data = get_app_data();
    let stored = app_data.dbm.appdata.get_sutta("mn1/pli/ms").unwrap().content_json.unwrap();
    match get_sutta_content_json(&app_data.dbm.appdata, "mn1/pli/ms") {
        SuttaContentJson::Found { uid, content_json } => {
            assert_eq!(uid, "mn1/pli/ms");
            // Verbatim, so the segment order is the file's order.
            assert_eq!(content_json, stored);
            let parsed: serde_json::Value = serde_json::from_str(&content_json).unwrap();
            assert_eq!(parsed["mn1:0.1"], "Majjhima Nikāya 1 ");
            // Text order, not sorted order: a sorted object would put
            // "mn1:10.1" right after "mn1:1.6", before "mn1:2.1".
            let pos = |key: &str| content_json.find(&format!("\"{}\"", key)).unwrap();
            assert!(pos("mn1:1.6") < pos("mn1:2.1"));
            assert!(pos("mn1:2.1") < pos("mn1:10.1"));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
#[serial]
fn test_bare_code_reads_pli_ms() {
    assert_eq!(found_uid("mn1"), "mn1/pli/ms");
}

#[test]
#[serial]
fn test_translation_uid() {
    assert_eq!(found_uid("sn45.92-95/en/sujato"), "sn45.92-95/en/sujato");
}

#[test]
#[serial]
fn test_verse_reference() {
    assert_eq!(found_uid("dhp182"), "dhp179-196/pli/ms");
}

#[test]
#[serial]
fn test_reference_inside_a_range_keeps_lang_and_author() {
    assert_eq!(found_uid("sn45.92/pli/ms"), "sn45.92-95/pli/ms");
    assert_eq!(found_uid("sn45.92/en/sujato"), "sn45.92-95/en/sujato");
}

#[test]
#[serial]
fn test_html_only_sutta_has_no_content_json() {
    assert_eq!(content("dn1/pli/cst"), SuttaContentJson::NoContentJson { uid: "dn1/pli/cst".to_string() });
}

#[test]
#[serial]
fn test_missing_translation_does_not_fall_back_to_pali() {
    assert_eq!(content("mn1/en/nosuchauthor"), SuttaContentJson::NotFound);
}

#[test]
#[serial]
fn test_unknown_and_empty_uid() {
    assert_eq!(content("xyz99/pli/ms"), SuttaContentJson::NotFound);
    assert_eq!(content(""), SuttaContentJson::NotFound);
}

fn bilara(uid: &str) -> (String, String) {
    h::app_data_setup();
    match get_sutta_bilara_json(&get_app_data().dbm.appdata, uid) {
        SuttaBilaraJson::Found { uid, json } => (uid, json),
        other => panic!("{uid}: expected Found, got {other:?}"),
    }
}

#[test]
#[serial]
fn test_bilara_pali_has_variants_and_template() {
    let (uid, json) = bilara("mn1");
    assert_eq!(uid, "mn1/pli/ms");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["uid"], "mn1/pli/ms");
    assert_eq!(v["content"]["mn1:0.1"], "Majjhima Nikāya 1 ");
    assert!(v["html_template"]["mn1:0.2"].as_str().unwrap().contains("sutta-title"));
    assert_eq!(v["variants"]["uid"], "mn1/pli/ms");
    assert!(v["variants"]["content"]["mn1:3.2"].as_str().unwrap().contains("paṭhaviṁ"));
    assert!(v["comments"].is_null());
    assert!(v["glosses"].is_null());
}

#[test]
#[serial]
fn test_bilara_content_is_embedded_verbatim() {
    h::app_data_setup();
    let stored = get_app_data().dbm.appdata.get_sutta("mn1/pli/ms").unwrap().content_json.unwrap();
    let (_, json) = bilara("mn1/pli/ms");
    // Embedded unchanged, so the segment order is the file's order.
    assert!(json.contains(&format!("\"content\":{}", stored)));
}

#[test]
#[serial]
fn test_bilara_translation_has_comments_and_the_root_variants() {
    let (uid, json) = bilara("mn1/en/sujato");
    assert_eq!(uid, "mn1/en/sujato");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["language"], "en");
    assert_eq!(v["source_uid"], "sujato");
    assert_eq!(v["comments"]["uid"], "mn1/en/sujato");
    assert!(v["comments"]["content"]["mn1:1.1"].as_str().unwrap().contains("Brahmajālasutta"));
    // The variant readings are the Pāli root text's, labelled as such.
    assert_eq!(v["variants"]["uid"], "mn1/pli/ms");
}

#[test]
#[serial]
fn test_bilara_errors_match_sutta_json() {
    h::app_data_setup();
    let appdata = &get_app_data().dbm.appdata;
    assert!(matches!(get_sutta_bilara_json(appdata, "dn1/pli/cst"),
                     SuttaBilaraJson::NoContentJson { uid } if uid == "dn1/pli/cst"));
    assert!(matches!(get_sutta_bilara_json(appdata, "mn1/en/nosuchauthor"), SuttaBilaraJson::NotFound));
}
