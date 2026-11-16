// Tests of `gloss_agent_apply::apply_agent_answers` for items built with
// Word in Sentence rows: a one-option (reading-only) item writes nothing, a
// component item of a compound headword writes an ordinary component entry.
// The items-without-rows behaviour is covered by bootstrap_cli's
// `gloss_agent_check` tests.
//
// Runs against the live databases (SIMSAPA_DIR from .env), since every
// selected uid is checked with `AppData::resolve_word_uid`. The uids are real
// records: "34626/dpd" (dhamma 1.01), "18134/dpd" / "18135/dpd" (evaṁ 1 / 2).

mod helpers;
use helpers::app_data_setup;

use simsapa_backend::get_app_data;
use simsapa_backend::gloss_agent_apply::{apply_agent_answers, parse_agent_item_id, AgentItemKind};
use simsapa_backend::helpers::{parse_word_selection_response, WordSelectionParseMode};

/// A session with one paragraph of three words: `dhammaṁ` with one sense
/// and several readings (a reading-only item), `evaṁ` with two senses, and
/// `dhammevaṁ` standing in for a compound headword whose components are
/// asked for.
fn session_content() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "app_version": "0.4.4",
        "exported_at": "2026-11-16T10:00:00Z",
        "format": "simsapa-gloss-session",
        "format_version": 1,
        "session": {
            "no_duplicates_globally": false,
            "skip_common": false,
            "text": "Dhammaṁ evaṁ dhammevaṁ.",
            "paragraphs": [{
                "selected_ai_tab": 0,
                "source_uid": "mn1/pli/ms",
                "text": "Dhammaṁ evaṁ dhammevaṁ.",
                "translations": [],
                "words": [
                    {
                        "context_hash": "hash-dhamma",
                        "example_sentence": "<b>Dhammaṁ</b> evaṁ dhammevaṁ.",
                        "original_word": "dhammaṁ",
                        "resolution": null,
                        "selected_index": 0,
                        "stem": "dhamma 1.01",
                        "results": [
                            {"uid": "34626/dpd", "word": "dhamma 1.01", "summary": "masc. nature"}
                        ]
                    },
                    {
                        "context_hash": "hash-evam",
                        "example_sentence": "Dhammaṁ <b>evaṁ</b> dhammevaṁ.",
                        "original_word": "evaṁ",
                        "resolution": null,
                        "selected_index": 0,
                        "stem": "evaṁ 1",
                        "results": [
                            {"uid": "18134/dpd", "word": "evaṁ 1", "summary": "thus"},
                            {"uid": "18135/dpd", "word": "evaṁ 2", "summary": "yes"}
                        ]
                    },
                    {
                        "context_hash": "hash-compound",
                        "example_sentence": "Dhammaṁ evaṁ <b>dhammevaṁ</b>.",
                        "original_word": "dhammevaṁ",
                        "resolution": null,
                        "selected_index": 0,
                        "stem": "dhammevaṁ",
                        "results": [
                            {"uid": "34626/dpd", "word": "dhamma 1.01", "summary": "the headword"}
                        ]
                    }
                ]
            }]
        },
        "word_cache": []
    }))
    .unwrap()
}

/// The items as `build_word_selection_items` builds them with rows: a
/// reading-only item (one option with `grammar_options`), a sense item, and a
/// component item of a compound headword (`component_word`, the token's
/// construction as `deconstructions`).
fn items() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "id": "p0w0", "word": "dhammaṁ", "context": "<b>Dhammaṁ</b> evaṁ dhammevaṁ.",
            "options": [{"uid": "34626/dpd", "word": "dhamma 1.01", "summary": "masc. nature",
                         "grammar_options": ["masc acc sg of dhamma", "nt nom sg of dhamma"]}]
        }),
        serde_json::json!({
            "id": "p0w1", "word": "evaṁ", "context": "Dhammaṁ <b>evaṁ</b> dhammevaṁ.",
            "options": [{"uid": "18134/dpd", "word": "evaṁ 1", "summary": "thus"},
                        {"uid": "18135/dpd", "word": "evaṁ 2", "summary": "yes"}]
        }),
        serde_json::json!({
            "id": "p0w2c1", "word": "dhammevaṁ", "component_word": "evaṁ",
            "deconstructions": ["dhamma + evaṁ"],
            "context": "Dhammaṁ evaṁ <b>dhammevaṁ</b>.",
            "options": [{"uid": "18134/dpd", "word": "evaṁ 1", "summary": "thus"},
                        {"uid": "18135/dpd", "word": "evaṁ 2", "summary": "yes"}]
        }),
    ]
}

fn apply(answers: serde_json::Value) -> simsapa_backend::gloss_agent_apply::AgentApplyOutcome {
    app_data_setup();
    let items = items();
    let items_json = serde_json::to_string(&items).unwrap();
    let entries =
        parse_word_selection_response(&answers.to_string(), &items_json, WordSelectionParseMode::Strict).unwrap();
    apply_agent_answers(&session_content(), &items, &entries, get_app_data()).unwrap()
}

fn answers() -> serde_json::Value {
    serde_json::json!({ "selections": [
        {"id": "p0w0", "word": "dhamma 1.01", "grammar_index": 1, "contextual_meaning": "the teaching"},
        {"id": "p0w1", "word": "evaṁ 1"},
        {"id": "p0w2c1", "word": "evaṁ 2", "confidence": "review", "note": "unsure"}
    ]})
}

#[test]
fn test_one_option_item_writes_nothing() {
    let outcome = apply(answers());
    assert_eq!(outcome.reading_only, 1);
    assert_eq!(outcome.confirmed, 1);
    assert_eq!(outcome.review, 1);

    let cache = outcome.envelope["word_cache"].as_array().unwrap();
    assert!(
        cache.iter().all(|e| e["context_hash"] != "hash-dhamma"),
        "a reading-only answer must write no word_cache entry: {cache:?}"
    );
    // The word itself is untouched: no reading, no changed selection.
    let word = &outcome.envelope["session"]["paragraphs"][0]["words"][0];
    assert_eq!(word["selected_index"], 0);
    assert!(word.get("reading").is_none());
    // No exported_at, and nothing of the reading anywhere in the cache.
    assert!(outcome.envelope.get("exported_at").is_none());
    assert!(!outcome.envelope["word_cache"].to_string().contains("the teaching"));
}

#[test]
fn test_compound_headword_component_entry() {
    let outcome = apply(answers());
    let cache = outcome.envelope["word_cache"].as_array().unwrap();
    assert_eq!(cache.len(), 2);

    // Sorted by (word, context_hash): the component row ("evaṁ",
    // "hash-compound") before the sense row ("evaṁ", "hash-evam").
    let component = &cache[0];
    assert_eq!(component["word"], "evaṁ");
    assert_eq!(component["context_hash"], "hash-compound");
    assert_eq!(component["context_snippet"], "Dhammaṁ evaṁ <b>dhammevaṁ</b>.");
    assert_eq!(component["selected_uid"], "18135/dpd");
    assert_eq!(component["selected_lemma"], "evaṁ 2");
    assert_eq!(component["origin"], "built-in-agent-checked");
    assert_eq!(component["confidence"], "review");
    assert_eq!(component["note"], "unsure");
    assert!(component.get("deconstruction").is_none());

    let compound = &outcome.envelope["session"]["paragraphs"][0]["words"][2];
    assert_eq!(compound["component_selected_uids"]["evaṁ"], "18135/dpd");
    // The compound's own sense is not changed by a component answer.
    assert_eq!(compound["selected_index"], 0);

    let sense = &cache[1];
    assert_eq!(sense["word"], "evaṁ");
    assert_eq!(sense["context_hash"], "hash-evam");
    assert_eq!(sense["selected_uid"], "18134/dpd");
    assert!(sense.get("confidence").is_none());
}

#[test]
fn test_agent_item_ids() {
    assert_eq!(parse_agent_item_id("p0w2c1").unwrap(), (0, 2, AgentItemKind::Component));
    assert_eq!(parse_agent_item_id("p3w4d").unwrap(), (3, 4, AgentItemKind::Breakdown));
    assert!(parse_agent_item_id("p0w").is_err());
}
