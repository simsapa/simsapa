//! `/gloss_text` exact mode (`helpers::process_all_paragraphs_exact()`) against
//! the dist DB, read-only, and the request validators of `POST /lookup_forms`
//! and of exact mode (`simsapa_backend::lookup_forms`).
//!
//! The DPD facts are the ones recorded in `test_lookup_forms_exact.rs`.

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::helpers::{extract_words_with_context, process_all_paragraphs_exact};
use simsapa_backend::lookup_forms::{
    validate_gloss_exact_options, validate_lookup_forms_request, MAX_LOOKUP_FORMS,
};
use simsapa_backend::types::{ExactGlossResult, FormLookupStatus, FormSplitSource};

mod helpers;
use helpers as h;

fn gloss_exact(paragraphs: &[&str]) -> ExactGlossResult {
    h::app_data_setup();
    let app_data = get_app_data();
    let paragraphs: Vec<String> = paragraphs.iter().map(|s| s.to_string()).collect();
    process_all_paragraphs_exact(&paragraphs, &app_data.dbm.dpd).unwrap()
}

fn forms(n: usize) -> Vec<String> {
    vec!["iti".to_string(); n]
}

#[test]
fn test_lookup_forms_request_missing_forms() {
    assert_eq!(
        validate_lookup_forms_request(None, None),
        Err("missing field: forms".to_string())
    );
    assert_eq!(
        validate_lookup_forms_request(None, Some(true)),
        Err("missing field: forms".to_string())
    );
}

#[test]
fn test_lookup_forms_request_exact_false() {
    assert_eq!(
        validate_lookup_forms_request(Some(&forms(1)), Some(false)),
        Err("only exact lookups are supported; use /dict_combined_search for fuzzy matching".to_string())
    );
    assert_eq!(validate_lookup_forms_request(Some(&forms(1)), Some(true)), Ok(()));
    assert_eq!(validate_lookup_forms_request(Some(&forms(1)), None), Ok(()));
}

#[test]
fn test_lookup_forms_request_empty_list_is_valid() {
    assert_eq!(validate_lookup_forms_request(Some(&Vec::new()), None), Ok(()));
}

#[test]
fn test_lookup_forms_request_batch_limit() {
    assert_eq!(MAX_LOOKUP_FORMS, 5000);
    assert_eq!(validate_lookup_forms_request(Some(&forms(5000)), None), Ok(()));
    assert_eq!(
        validate_lookup_forms_request(Some(&forms(5001)), None),
        Err("too many forms: 5001 (max 5000)".to_string())
    );
}

#[test]
fn test_gloss_exact_options_defaults_accepted() {
    assert_eq!(validate_gloss_exact_options(None, None, None), Ok(()));
    assert_eq!(
        validate_gloss_exact_options(Some(false), Some(false), Some(&Vec::new())),
        Ok(())
    );
}

#[test]
fn test_gloss_exact_options_conflicts_rejected() {
    let err = validate_gloss_exact_options(Some(true), None, None).unwrap_err();
    assert!(err.contains("no_duplicates_globally"), "{}", err);

    let err = validate_gloss_exact_options(None, Some(true), None).unwrap_err();
    assert!(err.contains("skip_common"), "{}", err);

    let words = vec!["ca".to_string()];
    let err = validate_gloss_exact_options(None, None, Some(&words)).unwrap_err();
    assert!(err.contains("common_words"), "{}", err);

    let err = validate_gloss_exact_options(Some(true), Some(true), Some(&words)).unwrap_err();
    assert!(
        err.contains("no_duplicates_globally") && err.contains("skip_common") && err.contains("common_words"),
        "{}",
        err
    );
}

#[test]
#[serial]
fn test_exact_gloss_every_token_in_order() {
    let line = "Evaṁ bhante, iti āvuso xyzqq bhante bhavissan'ti.";
    let res = gloss_exact(&[line]);

    assert!(res.success);
    assert!(res.exact);
    assert!(res.dpd_version.as_deref().is_some_and(|v| v.starts_with('v')));
    assert_eq!(res.paragraphs.len(), 1);

    let tokens = &res.paragraphs[0].tokens;
    assert_eq!(tokens.len(), extract_words_with_context(line).len());
    for (i, t) in tokens.iter().enumerate() {
        assert_eq!(t.token_index, i);
    }

    let words: Vec<&str> = tokens.iter().map(|t| t.original_word.as_str()).collect();
    assert_eq!(words, vec!["Evaṁ", "bhante", "iti", "āvuso", "xyzqq", "bhante", "bhavissan'ti"]);

    // The repeated word is not deduplicated.
    assert_eq!(tokens[1].lookup.status, FormLookupStatus::Found);
    assert_eq!(tokens[5].lookup.status, FormLookupStatus::Found);

    // An unknown word is kept, not dropped.
    assert_eq!(tokens[4].lookup.status, FormLookupStatus::NotFound);

    assert_eq!(tokens[2].lookup.status, FormLookupStatus::Found);
    assert_eq!(tokens[3].lookup.status, FormLookupStatus::Found);

    // The quoted iti word stays whole: its headwords and the iti split.
    let bh = &tokens[6];
    assert_eq!(bh.clean_word, "bhavissaṁ");
    assert_eq!(bh.lookup.input, "bhavissan'ti");
    assert_eq!(bh.lookup.lookup_key, "bhavissanti");
    assert_eq!(bh.lookup.status, FormLookupStatus::Found);
    assert!(!bh.lookup.headwords.is_empty());
    assert!(bh
        .lookup
        .deconstructions
        .iter()
        .any(|d| d.split == "bhavissaṁ + iti" && d.source == FormSplitSource::Dpd));
}

#[test]
#[serial]
fn test_exact_gloss_punctuation_and_capitals() {
    // `original_word` comes without the surrounding punctuation and quote
    // marks, and a capital is lowercased by the DPD normalisation, so no
    // stripping is needed before the lookup.
    let line = "“Bhavissan’ti, cittan’ti.” ‘Maggan’ti?";
    let res = gloss_exact(&[line]);
    let tokens = &res.paragraphs[0].tokens;

    let got: Vec<(&str, &str)> = tokens
        .iter()
        .map(|t| (t.original_word.as_str(), t.lookup.lookup_key.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("Bhavissan’ti", "bhavissanti"),
            ("cittan’ti", "cittanti"),
            ("Maggan’ti", "maggaṁ"),
        ]
    );
    assert_eq!(tokens[0].lookup.status, FormLookupStatus::Found);
    assert_eq!(tokens[1].lookup.status, FormLookupStatus::Found);
    // `magganti` is the verb; the quoted form is maggaṁ + iti.
    assert_eq!(tokens[2].lookup.status, FormLookupStatus::Deconstructed);
    assert_eq!(tokens[2].lookup.deconstructions[0].split, "maggaṁ + iti");
    assert_eq!(tokens[2].lookup.deconstructions[0].source, FormSplitSource::ItiRule);
}

#[test]
#[serial]
fn test_exact_gloss_paragraph_indexes() {
    let res = gloss_exact(&["iti bhante", "āvuso iti xyzqq"]);
    assert_eq!(res.paragraphs.len(), 2);
    assert_eq!(res.paragraphs[0].paragraph_index, 0);
    assert_eq!(res.paragraphs[1].paragraph_index, 1);

    let idx: Vec<Vec<usize>> = res
        .paragraphs
        .iter()
        .map(|p| p.tokens.iter().map(|t| t.token_index).collect())
        .collect();
    assert_eq!(idx, vec![vec![0, 1], vec![0, 1, 2]]);
    assert_eq!(res.paragraphs[1].tokens[2].lookup.input, "xyzqq");
}

#[test]
#[serial]
fn test_exact_gloss_no_paragraphs() {
    let res = gloss_exact(&[]);
    assert!(res.success);
    assert!(res.paragraphs.is_empty());
}

#[test]
#[serial]
fn test_exact_gloss_unquoted_iti_stays_whole() {
    // dpd: abhinandunti [7791] + ["abhinanduṁ + iti"].
    let res = gloss_exact(&["Attamanā te bhikkhū bhagavato bhāsitaṁ abhinandunti. Pāsarāsisuttaṁ niṭṭhitaṁ."]);
    let tokens = &res.paragraphs[0].tokens;
    assert!(!tokens.iter().any(|t| t.clean_word == "ti"));

    let t = tokens.iter().find(|t| t.original_word == "abhinandunti").unwrap();
    assert_eq!(t.lookup.lookup_key, "abhinandunti");
    assert_eq!(t.lookup.status, FormLookupStatus::Found);
    assert!(t.lookup.deconstructions.iter().any(|d| d.split == "abhinanduṁ + iti"));
}

#[test]
#[serial]
fn test_exact_gloss_token_carries_form_grammar() {
    let res = gloss_exact(&["Evaṁ bhagavantaṁ vadati."]);
    let token = res.paragraphs[0].tokens.iter().find(|t| t.clean_word == "bhagavantaṁ").unwrap();
    assert_eq!(token.lookup.form_grammar_raw.len(), 4);
    let hw = token.lookup.headwords.iter().find(|hw| hw.uid == "49139/dpd").unwrap();
    assert_eq!(hw.form_grammar, ["masc acc sg"]);
}
