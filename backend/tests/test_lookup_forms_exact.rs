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
//!
//! # The iti-sandhi words resolved as `<base> + iti`:
//! sqlite3 dpd.sqlite3 "SELECT lookup_key, headwords, deconstructor FROM lookup
//!   WHERE lookup_key IN ('magganti','maggaṁ','gopālikāti','gopālika','gopālikā',
//!   'idhāhanti','idhāhaṁ','vikālāyanti','vikālāyaṁ','sāyanti','sāyaṁ');"
//! # magganti [50519] (no split); maggaṁ 11 ids (no split);
//! # no rows for gopālikāti, gopālika; gopālikā [25347];
//! # no row for idhāhanti; idhāhaṁ [13708, 13709] + ["idha + ahaṁ"];
//! # no row for vikālāyanti; vikālāyaṁ ["vikālā + ayaṁ", "vikāla + ayaṁ"] (no headwords);
//! # sāyanti [62397] (no split); sāyaṁ 7 ids + ["sā + ayaṁ"].
//!
//! sqlite3 dpd.sqlite3 "SELECT id, lemma_1 FROM dpd_headwords WHERE id IN (50519, 25347);"
//! # 50519 | maggati   (the verb `magganti` belongs to)
//! # 25347 | gopālikā
//!
//! # Form grammar (`lookup.grammar`, `[lemma, class, parse]` triples):
//! sqlite3 dpd.sqlite3 "SELECT lookup_key, headwords, grammar FROM lookup
//!   WHERE lookup_key IN ('bhagavantaṁ','carissanti','bhavissanti','cittanti',
//!   'cittaṁ','maggaṁ','iti','katamo');"
//! # bhagavantaṁ: bhagavant noun masc acc sg; bhagavant adj nt nom sg,
//! #   masc acc sg, nt acc sg (4 triples, in that order)
//! # carissanti [26276..26279]: carati verb fut 3rd pl (1 triple)
//! # bhavissanti 9 ids: bhavati verb fut 3rd pl; bhavissati verb fut 3rd pl;
//! #   bhavissantī noun fem voc sg
//! # cittanti: '' (no grammar); cittaṁ: 9 triples, first citta noun nt nom sg
//! # maggaṁ: 13 triples (magga ×8, magganta prp ×4, maggi verb reflx aor 1st sg)
//! # iti: '' (no grammar)
//! # katamo [19645]: katama interr masc nom sg
//!
//! sqlite3 dpd.sqlite3 "SELECT id, lemma_1, lemma_clean, pos FROM dpd_headwords
//!   WHERE id IN (49139,49140,26276,26277,26278,26279,49534,49535,49621,49622,
//!   49623,49624,49625,49626,49628,50561,19645);"
//! # 49139 bhagavant 1 masc; 49140 bhagavant 2 adj; 26276..26279 carati 1.1, 2.1,
//! # 2.2, 2.3 pr; 49534/49535 bhavati 1/2 pr; 49621 bhavissa cond;
//! # 49622..49624 bhavissati 1/2/3 fut; 49625 bhavissanti sandhi;
//! # 49626 bhavissantī fem; 49628 bhavissaṁ fut; 50561 maggi aor;
//! # 19645 katama pron (no `pos` maps to the `interr` class)
//! # 50495 magga 1 masc; 50501 magga 7 nt
//!
//! sqlite3 dpd.sqlite3 "SELECT id, lemma_1, root_base, root_sign FROM dpd_headwords
//!   WHERE id IN (26276, 49236);"
//! # 26276 | carati 1.1 | √car + a > cara | a
//! # 49236 | bhaṇāmase  | √bhaṇ + a > bhaṇa | a
//! ```

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::types::{
    GrammarTriple, FormLookupResult, FormLookupStatus, FormMatchedAs, FormSplitKind, FormSplitSource,
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
    let res = lookup(&["", "a b", "uid:iti"]);
    assert_eq!(res[0].status, FormLookupStatus::Invalid);
    assert_eq!(res[0].note.as_deref(), Some("empty after normalisation"));
    assert_eq!(res[1].status, FormLookupStatus::Invalid);
    assert_eq!(res[1].note.as_deref(), Some("more than one word"));
    // The DPD normalisation passes a `uid:` query through for the search
    // routes; it must not read as "DPD does not know this form".
    assert_eq!(res[2].status, FormLookupStatus::Invalid);
    assert_eq!(res[2].note.as_deref(), Some("a uid query, not a word form"));
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

#[test]
#[serial]
fn test_quoted_iti_with_joined_iti_split() {
    for form in ["cittan'ti", "cittan’ti"] {
        let r = lookup_one(form);
        assert_eq!(r.status, FormLookupStatus::Found, "{}", form);
        assert_eq!(r.lookup_key, "cittanti", "{}", form);
        assert_eq!(r.headwords.len(), 10, "{}", form);
        assert_eq!(splits(&r), vec!["cittaṁ + iti"], "{}", form);
        let d = &r.deconstructions[0];
        assert_eq!(d.kind, FormSplitKind::ItiSandhi);
        assert_eq!(d.source, FormSplitSource::Dpd);
    }

    let r = lookup_one("bhavissan'ti");
    assert_eq!(r.status, FormLookupStatus::Found);
    assert_eq!(r.lookup_key, "bhavissanti");
    assert_eq!(r.headwords.len(), 9);
    assert_eq!(splits(&r), vec!["bhavissaṁ + iti"]);
    assert_eq!(r.deconstructions[0].source, FormSplitSource::Dpd);
}

#[test]
#[serial]
fn test_iti_rule_never_returns_the_verb() {
    let r = lookup_one("magga’’nti");
    assert_eq!(r.status, FormLookupStatus::Deconstructed);
    assert_eq!(r.lookup_key, "maggaṁ");
    assert!(r.headwords.is_empty());
    assert_eq!(splits(&r), vec!["maggaṁ + iti"]);

    let d = &r.deconstructions[0];
    assert_eq!(d.source, FormSplitSource::ItiRule);
    assert_eq!(d.kind, FormSplitKind::ItiSandhi);
    let words: Vec<&str> = d.components.iter().map(|c| c.word.as_str()).collect();
    assert_eq!(words, vec!["maggaṁ", "iti"]);
    assert_eq!(d.components[0].uids.len(), 11);
    assert_eq!(d.components[1].uids, vec!["13466/dpd"]);

    let all_uids: Vec<&String> = r.deconstructions.iter()
        .flat_map(|d| d.components.iter().flat_map(|c| c.uids.iter()))
        .collect();
    assert!(!all_uids.iter().any(|u| *u == "50519/dpd"), "the verb maggati must not appear");

    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["deconstructions"][0]["source"], "iti_rule");
    assert_eq!(v["deconstructions"][0]["kind"], "iti_sandhi");
}

#[test]
#[serial]
fn test_iti_rule_picks_the_long_vowel_base() {
    let r = lookup_one("gopālikā’ti");
    assert_eq!(r.status, FormLookupStatus::Deconstructed);
    assert_eq!(r.lookup_key, "gopālikā");
    assert_eq!(splits(&r), vec!["gopālikā + iti"]);
    assert_eq!(r.deconstructions[0].components[0].uids, vec!["25347/dpd"]);
}

#[test]
#[serial]
fn test_iti_rule_appends_iti_to_the_base_splits() {
    let r = lookup_one("idhāhan’ti");
    assert_eq!(r.lookup_key, "idhāhaṁ");
    assert_eq!(splits(&r), vec!["idhāhaṁ + iti", "idha + ahaṁ + iti"]);
    let words: Vec<&str> = r.deconstructions[1].components.iter().map(|c| c.word.as_str()).collect();
    assert_eq!(words, vec!["idha", "ahaṁ", "iti"]);
    assert!(r.deconstructions.iter().all(|d| d.source == FormSplitSource::ItiRule));

    // A base with splits and no headwords gets no bare `<base> + iti`.
    let r = lookup_one("vikālāyan’ti");
    assert_eq!(r.lookup_key, "vikālāyaṁ");
    assert_eq!(splits(&r), vec!["vikālā + ayaṁ + iti", "vikāla + ayaṁ + iti"]);
}

/// The exact `<base> + iti` rule is a copy of `dpd_lookup_grouped_iti_split()`'s.
/// For words whose base has an exact lookup row, the grouped lookup's base read
/// is exact too, so the two must give the same splits in the same order.
#[test]
#[serial]
fn test_iti_rule_matches_grouped_lookup() {
    h::app_data_setup();
    let app_data = get_app_data();
    let dpd = &app_data.dbm.dpd;

    for word in ["magga’’nti", "gopālikā’ti", "idhāhan’ti", "vikālāyan’ti", "sāyan’ti"] {
        let exact = dpd.lookup_forms_exact(&[word.to_string()]).unwrap().remove(0);
        assert!(
            exact.deconstructions.iter().all(|d| d.source == FormSplitSource::ItiRule),
            "{} should resolve by the iti rule", word
        );
        let grouped = dpd.dpd_lookup_grouped(word, false, true, true, None, None).unwrap();
        let grouped_splits: Vec<&str> = grouped.deconstructions.iter().map(|d| d.words_joined.as_str()).collect();
        assert_eq!(splits(&exact), grouped_splits, "{}", word);
    }
}

fn triple(lemma: &str, class: &str, parse: &str) -> GrammarTriple {
    [lemma.to_string(), class.to_string(), parse.to_string()]
}

fn form_grammar<'a>(r: &'a FormLookupResult, uid: &str) -> &'a [String] {
    &r.headwords.iter().find(|hw| hw.uid == uid).unwrap_or_else(|| panic!("no headword {uid}")).form_grammar
}

#[test]
#[serial]
fn test_form_grammar_bhagavantam() {
    let r = lookup_one("bhagavantaṁ");
    assert_eq!(r.form_grammar_raw, vec![
        triple("bhagavant", "noun", "masc acc sg"),
        triple("bhagavant", "adj", "nt nom sg"),
        triple("bhagavant", "adj", "masc acc sg"),
        triple("bhagavant", "adj", "nt acc sg"),
    ]);
    assert_eq!(form_grammar(&r, "49139/dpd"), ["masc acc sg"]);
    assert_eq!(form_grammar(&r, "49140/dpd"), ["nt nom sg", "masc acc sg", "nt acc sg"]);
}

#[test]
#[serial]
fn test_form_grammar_each_homonym_gets_the_entry() {
    let r = lookup_one("carissanti");
    assert_eq!(uids(&r), vec!["26276/dpd", "26277/dpd", "26278/dpd", "26279/dpd"]);
    for hw in &r.headwords {
        assert_eq!(hw.form_grammar, ["fut 3rd pl"], "{}", hw.lemma_1);
    }
}

#[test]
#[serial]
fn test_form_grammar_bhavissanti() {
    let r = lookup_one("bhavissanti");
    for uid in ["49534/dpd", "49535/dpd", "49622/dpd", "49623/dpd", "49624/dpd"] {
        assert_eq!(form_grammar(&r, uid), ["fut 3rd pl"], "{uid}");
    }
    assert_eq!(form_grammar(&r, "49626/dpd"), ["fem voc sg"]);
    // bhavissa (cond), the sandhi headword and bhavissaṁ have no entry.
    for uid in ["49621/dpd", "49625/dpd", "49628/dpd"] {
        assert!(form_grammar(&r, uid).is_empty(), "{uid}");
    }
}

#[test]
#[serial]
fn test_form_grammar_maggam_verb_headword() {
    let r = lookup_one("maggaṁ");
    assert_eq!(r.form_grammar_raw.len(), 13);
    assert_eq!(r.form_grammar_raw[12], triple("maggi", "verb", "reflx aor 1st sg"));
    // maggi is a headword of the row (aor → verb), so it gets the entry.
    assert_eq!(form_grammar(&r, "50561/dpd"), ["reflx aor 1st sg"]);
    // Noun entries go by gender: masc magga 1 does not get magga 7's nt forms.
    assert_eq!(form_grammar(&r, "50495/dpd"), ["masc acc sg"]);
    assert_eq!(form_grammar(&r, "50501/dpd"), ["nt nom sg", "nt acc sg", "nt voc sg"]);
}

#[test]
#[serial]
fn test_form_grammar_unmapped_class_only_in_raw() {
    // `interr` maps to no `pos`: the entry stays in the raw list only.
    let r = lookup_one("katamo");
    assert_eq!(r.form_grammar_raw, vec![triple("katama", "interr", "masc nom sg")]);
    assert_eq!(uids(&r), vec!["19645/dpd"]);
    assert!(form_grammar(&r, "19645/dpd").is_empty());
}

#[test]
#[serial]
fn test_form_grammar_joined_iti_and_components() {
    let r = lookup_one("cittan'ti");
    assert_eq!(r.lookup_key, "cittanti");
    assert!(r.form_grammar_raw.is_empty());
    for hw in &r.headwords {
        assert!(hw.form_grammar.is_empty(), "{}", hw.lemma_1);
    }
    let d = r.deconstructions.iter().find(|d| d.split == "cittaṁ + iti").unwrap();
    assert_eq!(d.components[0].word, "cittaṁ");
    assert_eq!(d.components[0].form_grammar_raw.len(), 9);
    assert!(d.components[0].form_grammar_raw.contains(&triple("citta", "noun", "nt nom sg")));
    assert_eq!(d.components[1].word, "iti");
    assert!(d.components[1].form_grammar_raw.is_empty());
}

#[test]
#[serial]
fn test_form_grammar_iti_rule() {
    let r = lookup_one("magga’’nti");
    assert_eq!(r.lookup_key, "maggaṁ");
    assert!(r.form_grammar_raw.is_empty());
    let d = r.deconstructions.iter().find(|d| d.split == "maggaṁ + iti").unwrap();
    assert_eq!(d.source, FormSplitSource::ItiRule);
    let maggam = lookup_one("maggaṁ");
    assert_eq!(d.components[0].form_grammar_raw, maggam.form_grammar_raw);
    assert_eq!(d.components[0].form_grammar_raw.len(), 13);
    assert!(d.components[1].form_grammar_raw.is_empty());
}

#[test]
#[serial]
fn test_form_grammar_empty_for_invalid_and_unknown() {
    let res = lookup(&["", "xyzqq"]);
    for r in &res {
        assert!(r.form_grammar_raw.is_empty(), "{:?}", r.input);
    }
}

#[test]
#[serial]
fn test_root_base_and_sign() {
    let r = lookup_one("carati");
    let hw = r.headwords.iter().find(|hw| hw.uid == "26276/dpd").unwrap();
    assert_eq!(hw.root_base, "√car + a > cara");
    assert_eq!(hw.root_sign, "a");

    let r = lookup_one("bhaṇāmase");
    let hw = r.headwords.iter().find(|hw| hw.uid == "49236/dpd").unwrap();
    assert_eq!(hw.root_base, "√bhaṇ + a > bhaṇa");
    assert_eq!(hw.root_sign, "a");
}

#[test]
#[serial]
fn test_form_grammar_serialised_as_arrays() {
    let r = lookup_one("bhagavantaṁ");
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["form_grammar_raw"][0], serde_json::json!(["bhagavant", "noun", "masc acc sg"]));
    assert_eq!(v["headwords"][0]["form_grammar"], serde_json::json!(["masc acc sg"]));
    assert_eq!(v["headwords"][0]["root_base"], serde_json::json!(""));
}
