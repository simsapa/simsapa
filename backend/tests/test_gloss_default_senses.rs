//! The built-in default senses (`assets/gloss-default-senses.json`) against
//! the installed `dpd.sqlite3`: every rule names an existing headword with the
//! stated lemma, and every form rule is the default the gloss pipeline gives
//! that form on its own (no cache, no AI). A failure is a rule to review
//! against the DPD release, never one to drop silently.

mod helpers;
use helpers as h;

use std::collections::HashMap;

use serial_test::serial;
use simsapa_backend::db::dpd::headword_row_fields_by_ids;
use simsapa_backend::get_app_data;
use simsapa_backend::gloss_default_senses::{DefaultSensesFile, GLOSS_DEFAULT_SENSES_JSON};
use simsapa_backend::gloss_sense_ranking::rank_paragraph_default_senses;
use simsapa_backend::helpers::{extract_words_with_context, process_word_for_glossing};
use simsapa_backend::types::{GlossFormat, WordInfo, WordProcessingOptions, WordProcessingResult};

fn rules() -> DefaultSensesFile {
    serde_json::from_str(GLOSS_DEFAULT_SENSES_JSON).unwrap()
}

/// The default sense (headword id) of each token of `passage`.
fn default_ids(passage: &str) -> Vec<Option<i32>> {
    let dpd = &get_app_data().dbm.dpd;
    let options = WordProcessingOptions {
        no_duplicates_globally: false,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format: GlossFormat::WordInSentence,
    };
    let mut words: Vec<Option<WordProcessingResult>> = extract_words_with_context(passage)
        .iter()
        .map(|w| {
            let info = WordInfo { word: w.clean_word.clone(), sentence: w.context_snippet.clone(), original_word: w.original_word.clone() };
            process_word_for_glossing(&info, &options, dpd, None).unwrap()
        })
        .collect();
    rank_paragraph_default_senses(dpd, &mut words, passage).unwrap();
    words
        .iter()
        .map(|w| match w {
            Some(WordProcessingResult::Recognized(pw)) => pw
                .results
                .get(pw.selected_index as usize)
                .and_then(|r| r.uid.strip_suffix("/dpd"))
                .and_then(|id| id.parse().ok()),
            _ => None,
        })
        .collect()
}

#[test]
#[serial]
fn test_default_sense_rules_name_existing_headwords() {
    h::app_data_setup();
    let file = rules();
    let all: Vec<_> = file.forms.iter().chain(&file.homonyms).chain(&file.components).collect();
    let ids: Vec<i32> = all.iter().map(|r| r.headword_id().expect("selected_uid")).collect();
    let mut conn = get_app_data().dbm.dpd.get_conn().unwrap();
    let found = headword_row_fields_by_ids(&mut conn, &ids).unwrap();

    let mut problems = Vec::new();
    for (r, id) in all.iter().zip(&ids) {
        match found.get(id) {
            None => problems.push(format!("{}: no headword {}", r.selected_lemma, r.selected_uid)),
            Some(hw) if hw.lemma_1 != r.selected_lemma => {
                problems.push(format!("{}: headword {} is now '{}'", r.selected_lemma, r.selected_uid, hw.lemma_1))
            }
            Some(_) => {}
        }
    }
    assert!(problems.is_empty(), "gloss-default-senses.json:\n{}", problems.join("\n"));
}

#[test]
#[serial]
fn test_form_rules_are_the_gloss_default() {
    h::app_data_setup();
    let mut problems = Vec::new();
    for r in rules().forms {
        let got = default_ids(&r.word);
        if got != vec![r.headword_id()] {
            problems.push(format!("{}: expected {} ({}), got {:?}", r.word, r.selected_uid, r.selected_lemma, got));
        }
    }
    assert!(problems.is_empty(), "form rules the gloss does not apply:\n{}", problems.join("\n"));
}

#[test]
#[serial]
fn test_homonym_rules_in_a_sentence() {
    h::app_data_setup();
    // dhammaṁ → dhamma 1.03, kāyassa bhedā → bheda 1, paraṁ (form rule).
    let ids = default_ids("bhagavā dhammaṁ deseti. kāyassa bhedā paraṁ maraṇā.");
    assert_eq!(ids[1], Some(34628));
    assert_eq!(ids[4], Some(78718));
    assert_eq!(ids[5], Some(43071));
}

/// A DPD example that the paragraph holds names the editor's sense for this
/// passage, and a form rule does not override it: `dukkhaṁ` is `dukkha 3` by
/// its form rule, but in `dukkha 4`'s example (MN 10) it stays `dukkha 4`.
/// A short passage that merely occurs inside some example is not protected.
#[test]
#[serial]
fn test_strong_example_match_beats_a_form_rule() {
    h::app_data_setup();
    let ids = default_ids(
        "Katamañca, bhikkhave, dukkhaṁ? Yaṁ kho, bhikkhave, kāyikaṁ dukkhaṁ kāyikaṁ asātaṁ \
         kāyasamphassajaṁ dukkhaṁ asātaṁ vedayitaṁ, idaṁ vuccati, bhikkhave, dukkhaṁ.",
    );
    assert_eq!(ids[2], Some(32878));
    assert_eq!(default_ids("dukkhaṁ"), vec![Some(32877)]);
    assert_eq!(default_ids("bhikkhave dukkhaṁ"), vec![Some(49885), Some(32877)]);
}

/// Curation aid, not a check: glosses long suttas with AI off and prints the
/// ambiguous forms by frequency with their default sense, to find forms that
/// need a rule in `assets/gloss-default-senses.json`. Run with
/// `cargo test --test test_gloss_default_senses survey_default_senses -- --ignored --nocapture`.
/// `SURVEY_REFS` (comma-separated references, e.g. `dhp1-20/pli/ms`) replaces
/// the suttas; `SURVEY_WORDS` (comma-separated word keys) prints every
/// occurrence of those words with its sentence and default instead.
#[test]
#[ignore]
#[serial]
fn survey_default_senses() {
    use simsapa_backend::gloss_source::{resolve_gloss_source, PaliEdition};
    h::app_data_setup();
    let app_data = get_app_data();
    let dpd = &app_data.dbm.dpd;
    let default_refs = "mn10,dn2,dn16,mn26,mn118,sn56.11,sn22.59,an3.65,mn1,mn2,dn22,sn12.2";
    let refs_var = std::env::var("SURVEY_REFS").unwrap_or_else(|_| default_refs.to_string());
    let refs: Vec<&str> = refs_var.split(',').map(str::trim).filter(|r| !r.is_empty()).collect();
    let words_var = std::env::var("SURVEY_WORDS").unwrap_or_default();
    let focus: Vec<String> = words_var.split(',').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect();
    let options = WordProcessingOptions {
        no_duplicates_globally: false,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format: GlossFormat::WordInSentence,
    };
    // word → (count, default lemma, its meaning, other senses)
    let mut seen: HashMap<String, (usize, String, Vec<String>)> = HashMap::new();
    let mut conn = dpd.get_conn().unwrap();
    for r in refs {
        let Ok(src) = resolve_gloss_source(&app_data.dbm.appdata, r, PaliEdition::Default, None) else {
            println!("skipped {}", r);
            continue;
        };
        for para in &src.paragraphs {
            let mut words: Vec<Option<WordProcessingResult>> = extract_words_with_context(para)
                .iter()
                .map(|w| {
                    let info = WordInfo { word: w.clean_word.clone(), sentence: w.context_snippet.clone(), original_word: w.original_word.clone() };
                    process_word_for_glossing(&info, &options, dpd, None).unwrap()
                })
                .collect();
            rank_paragraph_default_senses(dpd, &mut words, para).unwrap();
            for w in words.iter().flatten() {
                let WordProcessingResult::Recognized(pw) = w else { continue };
                let n = if pw.direct_uids.is_empty() { pw.results.len() } else { pw.results.iter().filter(|r| pw.direct_uids.contains(&r.uid)).count() };
                if n < 2 {
                    continue;
                }
                let key = simsapa_backend::helpers::gloss_cache_word_key(&pw.original_word);
                if !focus.is_empty() {
                    if focus.contains(&key) {
                        let chosen = pw.results.get(pw.selected_index as usize).map(|r| format!("{} {}", r.uid, r.word)).unwrap_or_default();
                        println!("{} [{}] {} => {}\n    {}", key, r, pw.original_word, chosen, pw.example_sentence.replace('\n', " "));
                    }
                    continue;
                }
                let e = seen.entry(key).or_insert_with(|| {
                    let ids: Vec<i32> = pw.results[..n].iter().filter_map(|r| r.uid.strip_suffix("/dpd")?.parse().ok()).collect();
                    let hws = headword_row_fields_by_ids(&mut conn, &ids).unwrap();
                    let show = |id: &i32| hws.get(id).map(|h| format!("{} {} '{}'", id, h.lemma_1, h.meaning_1.chars().take(40).collect::<String>())).unwrap_or_default();
                    let chosen: Option<i32> = pw.results.get(pw.selected_index as usize).and_then(|r| r.uid.strip_suffix("/dpd")?.parse().ok());
                    let others = ids.iter().filter(|i| Some(**i) != chosen).map(show).collect();
                    (0, chosen.as_ref().map(show).unwrap_or_default(), others)
                });
                e.0 += 1;
            }
        }
    }
    let mut rows: Vec<_> = seen.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    for (word, (count, chosen, others)) in rows.iter().take(250) {
        println!("{:4} {:16} => {}\n       others: {}", count, word, chosen, others.join(" | "));
    }
}

/// A set phrase overrides a form rule: `kathaṁ` defaults to `kathaṁ 1`
/// "how?", but in `sammodanīyaṁ kathaṁ` it is `kathā 1` "talk" (the phrase
/// rule in `assets/gloss-phrase-selections.json`, seeded into appdata).
#[test]
#[serial]
fn test_phrase_rule_overrides_form_rule() {
    use simsapa_backend::helpers::process_all_paragraphs;
    use simsapa_backend::types::AllParagraphsProcessingInput;
    h::app_data_setup();
    let app_data = get_app_data();
    let options = WordProcessingOptions {
        no_duplicates_globally: false,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
        gloss_format: GlossFormat::WordInSentence,
    };
    let input = AllParagraphsProcessingInput {
        paragraphs: vec![
            "sammodanīyaṁ kathaṁ sāraṇīyaṁ vītisāretvā ekamantaṁ nisīdi".to_string(),
            "kathaṁ nu kho bhante".to_string(),
        ],
        options,
    };
    let result = process_all_paragraphs(&input, &app_data.dbm.appdata, &app_data.dbm.dpd).unwrap();
    let kathaṁ = |p: usize| {
        result.paragraphs[p]
            .words_data
            .iter()
            .find(|w| w.original_word == "kathaṁ")
            .map(|w| (w.results[w.selected_index as usize].uid.clone(), w.resolution.clone()))
            .unwrap()
    };
    assert_eq!(kathaṁ(0), ("19850/dpd".to_string(), Some("built-in-phrase-match".to_string())));
    assert_eq!(kathaṁ(1), ("19830/dpd".to_string(), None));
}
