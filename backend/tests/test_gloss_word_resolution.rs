// Integration tests for gloss word-selection resolution: cache + set-phrase
// lookups applied during gloss processing (see docs/gloss-ai-word-selection.md).
//
// Uses the real DPD database for lookups (via app_data_setup) and a throwaway
// temp appdata DB for the cache/phrase tables, seeded with the embedded
// gloss-phrase-selections.json — never the real appdata DB.

use std::collections::HashMap;

use serial_test::serial;
use simsapa_backend::get_app_data;
use simsapa_backend::db::{DatabaseHandle, APPDATA_MIGRATIONS};
use simsapa_backend::db::appdata::AppdataDbHandle;
use simsapa_backend::helpers::{
    extract_words_with_context, gloss_option_uid_matches, process_word_for_glossing,
    GlossResolutionData,
};
use simsapa_backend::types::{ProcessedWord, WordInfo, WordProcessingOptions, WordProcessingResult};
use diesel_migrations::MigrationHarness;

mod helpers;
use helpers as h;

const ARAME_SENTENCE: &str =
    "Ekaṁ samayaṁ bhagavā sāvatthiyaṁ viharati jetavane anāthapiṇḍikassa ārāme.";

fn temp_appdata() -> AppdataDbHandle {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "simsapa_gloss_resolution_test_{}_{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let url = path.to_string_lossy().to_string();
    let handle = DatabaseHandle::new(&url).expect("create temp appdata handle");
    let mut conn = handle.get_conn().expect("get temp appdata conn");
    conn.run_pending_migrations(APPDATA_MIGRATIONS)
        .expect("run appdata migrations on temp db");
    handle
}

fn default_options() -> WordProcessingOptions {
    WordProcessingOptions {
        no_duplicates_globally: false,
        skip_common: false,
        common_words: Vec::new(),
        existing_global_stems: HashMap::new(),
        existing_paragraph_unrecognized: HashMap::new(),
        existing_global_unrecognized: Vec::new(),
    }
}

/// Process a paragraph the same way the bridge does: extract words with
/// context, pre-fetch resolution data from the given appdata handle (None =
/// no resolution, as for callers without a DB), gloss each word.
fn gloss_paragraph(text: &str, appdata: Option<&AppdataDbHandle>) -> Vec<ProcessedWord> {
    let app_data = get_app_data();
    let words_with_context = extract_words_with_context(text);
    let resolution_data = appdata.map(|db| GlossResolutionData::fetch(db, &words_with_context));

    let options = default_options();
    let mut paragraph_shown_stems = HashMap::new();
    let mut global_stems = HashMap::new();
    let mut out = Vec::new();

    for wc in &words_with_context {
        let word_info = WordInfo {
            word: wc.clean_word.clone(),
            sentence: wc.context_snippet.clone(),
        };
        let res = process_word_for_glossing(
            &word_info,
            &mut paragraph_shown_stems,
            &mut global_stems,
            false,
            &options,
            &app_data.dbm.dpd,
            resolution_data.as_ref(),
        )
        .expect("word processing");
        if let Some(WordProcessingResult::Recognized(pw)) = res {
            out.push(pw);
        }
    }
    out
}

fn find_word<'a>(words: &'a [ProcessedWord], original_word: &str) -> &'a ProcessedWord {
    words
        .iter()
        .find(|w| w.original_word == original_word)
        .unwrap_or_else(|| panic!("'{}' should be among the glossed words", original_word))
}

// PRD test case 1: in "anāthapiṇḍikassa ārāme" the seeded set phrase resolves
// ārāme to the monastery (ārāma-4/dpd) with no cache row and no AI request.
#[test]
#[serial]
fn test_seeded_phrase_resolves_arame() {
    h::app_data_setup();
    let db = temp_appdata();
    db.seed_gloss_phrase_selections().expect("seed phrases");

    let words = gloss_paragraph(ARAME_SENTENCE, Some(&db));
    let arame = find_word(&words, "ārāme");

    assert!(
        arame.results.len() > 1,
        "ārāme should be ambiguous (multiple DPD entries), got {}",
        arame.results.len()
    );
    assert_eq!(arame.resolution.as_deref(), Some("built-in-phrase-match"));
    // Gloss options carry the numeric dpd_headwords uid; the curated phrase
    // stores the lemma-based form "ārāma-4/dpd" — they refer to the same entry.
    assert!(gloss_option_uid_matches(
        &arame.results[arame.selected_index as usize],
        "ārāma-4/dpd",
    ));
    assert_eq!(arame.results[arame.selected_index as usize].word, "ārāma 4");
}

// Without resolution data (no appdata handle) nothing is resolved.
#[test]
#[serial]
fn test_no_resolution_data_leaves_words_unresolved() {
    h::app_data_setup();

    let words = gloss_paragraph(ARAME_SENTENCE, None);
    let arame = find_word(&words, "ārāme");

    assert_eq!(arame.resolution, None);
    assert_eq!(arame.selected_index, 0);
}

// A user cache row beats the seeded phrase; a stale-uid user row is ignored
// and resolution falls through to the phrase.
#[test]
#[serial]
fn test_user_cache_beats_phrase_and_stale_uid_falls_through() {
    h::app_data_setup();
    let db = temp_appdata();
    db.seed_gloss_phrase_selections().expect("seed phrases");

    // First pass: get the word key inputs (context hash) from processing itself.
    let words = gloss_paragraph(ARAME_SENTENCE, Some(&db));
    let arame = find_word(&words, "ārāme");
    let context_hash = arame.context_hash.clone();
    assert!(!context_hash.is_empty());

    // Pick a different valid option than the phrase's ārāma-4/dpd.
    let other_uid = arame
        .results
        .iter()
        .map(|r| r.uid.clone())
        .find(|uid| uid != "ārāma-4/dpd")
        .expect("ārāme has another option");

    db.upsert_gloss_word_cache("ārāme", &context_hash, ARAME_SENTENCE, &other_uid, "user-selected")
        .expect("upsert user row");

    let words = gloss_paragraph(ARAME_SENTENCE, Some(&db));
    let arame = find_word(&words, "ārāme");
    assert_eq!(arame.resolution.as_deref(), Some("user-selected"));
    assert_eq!(arame.results[arame.selected_index as usize].uid, other_uid);

    // Replace with a stale uid (as if dictionary data changed): the user row
    // is ignored and the phrase resolves again.
    db.upsert_gloss_word_cache("ārāme", &context_hash, ARAME_SENTENCE, "gone-uid/dpd", "user-selected")
        .expect("upsert stale user row");

    let words = gloss_paragraph(ARAME_SENTENCE, Some(&db));
    let arame = find_word(&words, "ārāme");
    assert_eq!(arame.resolution.as_deref(), Some("built-in-phrase-match"));
    // Gloss options carry the numeric dpd_headwords uid; the curated phrase
    // stores the lemma-based form "ārāma-4/dpd" — they refer to the same entry.
    assert!(gloss_option_uid_matches(
        &arame.results[arame.selected_index as usize],
        "ārāma-4/dpd",
    ));
    assert_eq!(arame.results[arame.selected_index as usize].word, "ārāma 4");
}

// ai and built-in cache rows resolve with their origin when no phrase matches.
#[test]
#[serial]
fn test_ai_and_built_in_cache_rows_resolve() {
    h::app_data_setup();

    // A sentence without any seeded set phrase around the target word.
    let sentence = "Bhagavā bhikkhūnaṁ dhammaṁ deseti.";

    for origin in ["ai-selected", "built-in-human-checked"] {
        let db = temp_appdata();

        let words = gloss_paragraph(sentence, Some(&db));
        let target = find_word(&words, "dhammaṁ");
        assert!(target.results.len() > 1, "dhammaṁ should be ambiguous");
        assert_eq!(target.resolution, None, "no cache row yet");

        let selected_uid = target.results[1].uid.clone();
        db.upsert_gloss_word_cache("dhammaṁ", &target.context_hash, sentence, &selected_uid, origin)
            .expect("upsert cache row");

        let words = gloss_paragraph(sentence, Some(&db));
        let target = find_word(&words, "dhammaṁ");
        assert_eq!(target.resolution.as_deref(), Some(origin));
        assert_eq!(target.results[target.selected_index as usize].uid, selected_uid);
    }
}

// Unambiguous words (a single DPD entry) never get a resolution even when a
// cache row exists for their (word, context) key.
#[test]
#[serial]
fn test_unambiguous_words_are_not_resolved() {
    h::app_data_setup();
    let db = temp_appdata();

    let words = gloss_paragraph(ARAME_SENTENCE, Some(&db));
    for pw in &words {
        if pw.results.len() == 1 {
            assert_eq!(
                pw.resolution, None,
                "unambiguous '{}' must not carry a resolution",
                pw.original_word
            );
        }
    }
}


// Restored-session annotation: resolution / selected_index / context_hash are
// re-derived from the current cache + phrase tables, not trusted from the
// serialized session JSON (see annotate_gloss_words_json).
#[test]
#[serial]
fn test_annotate_gloss_words_json_rederives_state() {
    use simsapa_backend::helpers::annotate_gloss_words_json;

    h::app_data_setup();
    let db = temp_appdata();
    db.seed_gloss_phrase_selections().expect("seed phrases");

    // Simulate an old (pre-feature) session: gloss without resolution data,
    // then strip context_hash/resolution as an old session JSON would lack them,
    // and plant a stale annotation that must be cleared.
    let words = gloss_paragraph(ARAME_SENTENCE, None);
    let mut values: Vec<serde_json::Value> = words
        .iter()
        .map(|w| serde_json::to_value(w).unwrap())
        .collect();
    for v in values.iter_mut() {
        let obj = v.as_object_mut().unwrap();
        obj.remove("context_hash");
        obj.remove("resolution");
        // A property the app does not know about must survive the round trip.
        obj.insert("custom_field".to_string(), serde_json::json!("kept"));
    }
    // Stale annotation on the first word (no cache row backs it).
    values[0]
        .as_object_mut()
        .unwrap()
        .insert("resolution".to_string(), serde_json::json!("ai-selected"));

    let input = serde_json::to_string(&values).unwrap();
    let annotated = annotate_gloss_words_json(&db, &input).expect("annotate");
    let out: Vec<serde_json::Value> = serde_json::from_str(&annotated).unwrap();
    assert_eq!(out.len(), values.len());

    let arame = out
        .iter()
        .find(|v| v["original_word"] == "ārāme")
        .expect("ārāme present");
    assert_eq!(arame["resolution"], "built-in-phrase-match");
    let hash = arame["context_hash"].as_str().unwrap().to_string();
    assert_eq!(hash.len(), 64, "context_hash filled in for old sessions");
    let idx = arame["selected_index"].as_i64().unwrap() as usize;
    assert_eq!(arame["results"][idx]["word"], "ārāma 4");
    assert_eq!(arame["custom_field"], "kept");

    // Ambiguous words without any backing row get resolution: null (stale
    // annotations cleared); every word got a context_hash.
    for v in &out {
        assert!(v["context_hash"].as_str().unwrap().len() == 64);
        let results_len = v["results"].as_array().map(|a| a.len()).unwrap_or(0);
        if results_len > 1 && v["original_word"] != "ārāme" {
            assert!(v["resolution"].is_null(), "stale/unbacked resolution must be null for {}", v["original_word"]);
        }
    }

    // A user cache row written after the session was saved wins on restore.
    let arame_pw = find_word(&words, "ārāme");
    let other_uid = arame_pw
        .results
        .iter()
        .find(|r| !gloss_option_uid_matches(r, "ārāma-4/dpd"))
        .map(|r| r.uid.clone())
        .expect("another option");
    db.upsert_gloss_word_cache("ārāme", &hash, ARAME_SENTENCE, &other_uid, "user-selected")
        .expect("upsert user row");

    let annotated = annotate_gloss_words_json(&db, &input).expect("annotate");
    let out: Vec<serde_json::Value> = serde_json::from_str(&annotated).unwrap();
    let arame = out
        .iter()
        .find(|v| v["original_word"] == "ārāme")
        .expect("ārāme present");
    assert_eq!(arame["resolution"], "user-selected");
    let idx = arame["selected_index"].as_i64().unwrap() as usize;
    assert_eq!(arame["results"][idx]["uid"], serde_json::json!(other_uid.clone()));
}

// PRD acceptance cases, end-to-end at the backend level:
// (1) covered by test_seeded_phrase_resolves_arame — a phrase-resolved word
//     carries resolution "built-in-phrase-match", which the QML payload builder excludes, so
//     zero AI requests are issued for it;
// (2) "manobhāvanīyā bhikkhū" resolves bhikkhū via the seeded phrase, and a
//     mock AI response round-trips into an ai cache row for another word;
// (3) re-glossing the unchanged text resolves from the ai cache (word carries
//     a resolution, hence excluded from the automatic AI payload);
// (4) an ai write never downgrades a user row (forced-pass survival).
#[test]
#[serial]
fn test_prd_cases_phrase_ai_cache_and_user_survival() {
    use simsapa_backend::helpers::{gloss_cache_word_key, parse_word_selection_response};

    h::app_data_setup();
    let db = temp_appdata();
    db.seed_gloss_phrase_selections().expect("seed phrases");

    // (2) PRD test sentence: bhikkhū resolves via the phrase to the monk entry.
    let sentence = "Paṭisallīnā manobhāvanīyā bhikkhū.";
    let words = gloss_paragraph(sentence, Some(&db));
    let bhikkhu = find_word(&words, "bhikkhū");
    assert!(bhikkhu.results.len() > 1, "bhikkhū should be ambiguous");
    assert_eq!(bhikkhu.resolution.as_deref(), Some("built-in-phrase-match"));
    assert!(gloss_option_uid_matches(
        &bhikkhu.results[bhikkhu.selected_index as usize],
        "bhikkhu/dpd",
    ));

    // (2b) Mock AI selection for an ambiguous word in a sentence without a
    // phrase rule, exercising the same parse/validate/apply path as QML.
    let sentence2 = "Bhagavā bhikkhūnaṁ dhammaṁ deseti.";
    let words = gloss_paragraph(sentence2, Some(&db));
    let target = find_word(&words, "dhammaṁ");
    assert!(target.results.len() > 1);
    assert_eq!(target.resolution, None);

    let options: Vec<serde_json::Value> = target
        .results
        .iter()
        .map(|r| serde_json::json!({ "uid": r.uid, "word": r.word, "summary": "" }))
        .collect();
    let items = serde_json::json!([{
        "id": "p0w2",
        "word": target.original_word,
        "context": target.example_sentence,
        "options": options,
    }]);
    let chosen_uid = target.results[1].uid.clone();
    let response = format!(r#"{{"selections": [{{"id": "p0w2", "uid": "{}"}}]}}"#, chosen_uid);
    let pairs = parse_word_selection_response(&response, &items.to_string()).expect("valid response");
    assert_eq!(pairs, vec![("p0w2".to_string(), chosen_uid.clone())]);

    // Apply like SuttaBridge.save_gloss_word_cache does (key-normalized word,
    // hash from the snippet).
    assert!(db
        .upsert_gloss_word_cache(
            &gloss_cache_word_key(&target.original_word),
            &target.context_hash,
            &target.example_sentence,
            &chosen_uid,
            "ai-selected",
        )
        .expect("upsert ai row"));

    // (3) Re-gloss of the unchanged text resolves from the ai cache; the
    // resolution excludes the word from the automatic AI payload.
    let words = gloss_paragraph(sentence2, Some(&db));
    let target = find_word(&words, "dhammaṁ");
    assert_eq!(target.resolution.as_deref(), Some("ai-selected"));
    assert_eq!(target.results[target.selected_index as usize].uid, chosen_uid);

    // (4) The user saves a different choice; a later (forced-pass) ai write
    // must not downgrade it.
    let user_uid = target
        .results
        .iter()
        .map(|r| r.uid.clone())
        .find(|uid| *uid != chosen_uid)
        .expect("another option");
    assert!(db
        .upsert_gloss_word_cache(
            &gloss_cache_word_key(&target.original_word),
            &target.context_hash,
            &target.example_sentence,
            &user_uid,
            "user-selected",
        )
        .expect("upsert user row"));
    assert!(!db
        .upsert_gloss_word_cache(
            &gloss_cache_word_key(&target.original_word),
            &target.context_hash,
            &target.example_sentence,
            &chosen_uid,
            "ai-selected",
        )
        .expect("ai upsert attempt"));

    let words = gloss_paragraph(sentence2, Some(&db));
    let target = find_word(&words, "dhammaṁ");
    assert_eq!(target.resolution.as_deref(), Some("user-selected"));
    assert_eq!(target.results[target.selected_index as usize].uid, user_uid);
}

// Iti-sandhi quote variants and punctuation changes:
// "Diṭṭhaṁ vo, bhikkhave, caraṇaṁ nāma cittan”ti?" glosses
// as diṭṭhaṁ / vo / bhikkhave / caraṇaṁ / nāma / cittaṁ — the ”ti is absorbed
// by iti-sandhi handling — and a cache row saved from one edition's context
// window resolves the same word in editions with straight quote marks
// (cittan'ti), changed commas, or a missing quote mark (cittanti — the target
// word itself then differs, but the shared window still resolves the
// sentence's other words).
#[test]
#[serial]
fn test_iti_sandhi_quote_variants_share_cache() {
    use simsapa_backend::helpers::gloss_cache_word_key;

    h::app_data_setup();
    let db = temp_appdata();

    let smart = "Diṭṭhaṁ vo, bhikkhave, caraṇaṁ nāma cittan”ti?";
    let straight = "Diṭṭhaṁ vo, bhikkhave, caraṇaṁ nāma cittan'ti?";
    let commas_changed = "Diṭṭhaṁ vo bhikkhave, caraṇaṁ nāma cittan”ti.";
    let bare = "Diṭṭhaṁ vo bhikkhave caraṇaṁ nāma cittanti?";

    // Screenshot expectation: the glossed word list ("ti" is not glossed).
    let words = gloss_paragraph(smart, Some(&db));
    let list: Vec<&str> = words.iter().map(|w| w.original_word.as_str()).collect();
    assert_eq!(list, ["diṭṭhaṁ", "vo", "bhikkhave", "caraṇaṁ", "nāma", "cittaṁ"]);

    let citta = find_word(&words, "cittaṁ");
    assert!(citta.results.len() > 1, "cittaṁ should be ambiguous");
    // The screenshot's selection: "citta 2.3" (nt, painting; picture).
    let citta_23_uid = citta
        .results
        .iter()
        .find(|r| r.word == "citta 2.3")
        .map(|r| r.uid.clone())
        .expect("citta 2.3 should be among the options");

    // Save a user row from the smart-quote edition's window.
    assert!(db
        .upsert_gloss_word_cache(
            &gloss_cache_word_key(&citta.original_word),
            &citta.context_hash,
            &citta.example_sentence,
            &citta_23_uid,
            "user-selected",
        )
        .expect("upsert user row"));

    // The straight-quote and comma-variant editions produce the same window
    // hash and hit the saved row.
    for variant in [straight, commas_changed] {
        let words_v = gloss_paragraph(variant, Some(&db));
        let citta_v = find_word(&words_v, "cittaṁ");
        assert_eq!(
            citta_v.context_hash, citta.context_hash,
            "window hash differs for variant: {}", variant,
        );
        assert_eq!(citta_v.resolution.as_deref(), Some("user-selected"), "no cache hit for: {}", variant);
        assert_eq!(citta_v.results[citta_v.selected_index as usize].word, "citta 2.3");
    }

    // Missing-quote edition: cittanti is one written word there, but the
    // normalized window is identical — a row saved for another word of the
    // sentence (vo) resolves across all quote forms.
    let vo = find_word(&words, "vo");
    assert!(vo.results.len() > 1, "vo should be ambiguous");
    let vo_uid = vo.results.last().unwrap().uid.clone();
    assert!(db
        .upsert_gloss_word_cache(
            &gloss_cache_word_key(&vo.original_word),
            &vo.context_hash,
            &vo.example_sentence,
            &vo_uid,
            "user-selected",
        )
        .expect("upsert vo row"));

    let words_bare = gloss_paragraph(bare, Some(&db));
    let vo_bare = find_word(&words_bare, "vo");
    assert_eq!(
        vo_bare.context_hash, vo.context_hash,
        "vo window hash differs between the quoted and bare iti-sandhi editions",
    );
    assert_eq!(vo_bare.resolution.as_deref(), Some("user-selected"));
    assert_eq!(vo_bare.results[vo_bare.selected_index as usize].uid, vo_uid);
}

// Bootstrap data-bank import parity (docs/gloss-ai-word-selection.md): a
// confirmed session-export entry imported as a built-in row — as
// `import-gloss-data` and the bootstrap do — must resolve during gloss
// processing. The hashes come from the app's own gloss run, so this asserts
// the export -> import -> re-gloss hash parity end to end.
#[test]
#[serial]
fn test_imported_builtin_row_resolves_during_glossing() {
    use simsapa_backend::helpers::{import_gloss_word_cache_entries, GlossWordCacheExportEntry};

    h::app_data_setup();
    let db = temp_appdata();

    // First gloss pass with no resolution data: as the exporting user's app
    // would compute the words (context windows + hashes).
    let words = gloss_paragraph(ARAME_SENTENCE, None);
    let arame = find_word(&words, "ārāme");
    assert!(arame.results.len() > 1, "ārāme should be ambiguous");
    assert!(arame.resolution.is_none());

    // The confirmed selection, as a session-export word_cache entry. The
    // bootstrap import writes it as origin "built-in-human-checked".
    let selected = arame
        .results
        .iter()
        .find(|r| gloss_option_uid_matches(r, "ārāma-4/dpd"))
        .expect("ārāma-4 should be among the options");
    let entries = vec![GlossWordCacheExportEntry {
        word: arame.original_word.clone(),
        context_hash: arame.context_hash.clone(),
        context_snippet: arame.example_sentence.clone(),
        selected_uid: selected.uid.clone(),
        origin: "built-in-human-checked".to_string(),
    }];
    let (imported, skipped) = import_gloss_word_cache_entries(&db, &entries);
    assert_eq!((imported, skipped), (1, 0));

    // Re-gloss the same passage against the imported bank: the row resolves
    // (hash parity between the exporting and importing gloss runs).
    let words2 = gloss_paragraph(ARAME_SENTENCE, Some(&db));
    let arame2 = find_word(&words2, "ārāme");
    assert_eq!(arame2.context_hash, arame.context_hash, "hash parity");
    assert_eq!(arame2.resolution.as_deref(), Some("built-in-human-checked"));
    assert!(gloss_option_uid_matches(
        &arame2.results[arame2.selected_index as usize],
        "ārāma-4/dpd"
    ));
}
