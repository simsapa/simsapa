//! `POST /gloss_text` through Rocket's local client, against the dist
//! databases, read-only; skipped when they are absent. One test function, so
//! the databases and the Rocket instance are set up once.
//!
//! No test reaches an AI provider: the AI path runs a stub `GlossAiRunner`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rocket::http::{ContentType, Status};
use rocket::local::blocking::Client;

use simsapa_api::routes::gloss::GlossAiRunner;
use simsapa_api::{ApiConfig, build_rocket, init_app_globals_api};
use simsapa_backend::export_types::AiResponse;
use simsapa_backend::types::{AllParagraphsProcessingResult, ParagraphProcessingResult};

const DIST_SIMSAPA_DIR: &str = "/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa";

fn setup() {
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("SIMSAPA_DIR", DIST_SIMSAPA_DIR) };
    simsapa_backend::logger::set_host_options(simsapa_backend::logger::LoggerHostOptions {
        rotate_on_init: false,
        emit_tracing: false,
    });
    simsapa_backend::set_write_api_port_file(false);
    simsapa_backend::init_app_globals();
    simsapa_backend::init_app_data();
    init_app_globals_api();
}

fn client(with_runner: bool) -> Client {
    let mut rocket = build_rocket(ApiConfig::loopback(0)).expect("build_rocket");
    if with_runner {
        rocket = rocket.manage(GlossAiRunner::new(stub_runner));
    }
    Client::untracked(rocket).expect("rocket client")
}

/// Stands in for the AI: one translation per paragraph when `translate`.
fn stub_runner(mut result: AllParagraphsProcessingResult, texts: Vec<String>, translate: bool) -> AllParagraphsProcessingResult {
    if translate {
        for p in &mut result.paragraphs {
            p.ai_translations.push(AiResponse {
                model_name: "stub".to_string(),
                response: format!("Translation of: {}", texts[p.paragraph_index]),
                literal_translation: "Literal.".to_string(),
                is_selected: true,
            });
        }
    } else {
        result.ai_errors.push("stub ran without translate".to_string());
    }
    result
}

fn post(client: &Client, body: serde_json::Value) -> (Status, Option<ContentType>, String) {
    let resp = client.post("/gloss_text").header(ContentType::JSON).body(body.to_string()).dispatch();
    let status = resp.status();
    let ct = resp.content_type();
    (status, ct, resp.into_string().unwrap_or_default())
}

fn fixture_passage(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../backend/tests/data/dpd_analysis/fixtures")
        .join(name)
        .join("study.md");
    let md = std::fs::read_to_string(path).unwrap();
    md.lines()
        .skip(1)
        .take_while(|l| !l.starts_with("### "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// The Headwords snapshot file's form: pretty JSON with the global stems in
/// a stable order (as `backend/tests/test_gloss_headwords_snapshot.rs`).
fn stable_pretty(result: &AllParagraphsProcessingResult) -> String {
    #[derive(serde::Serialize)]
    struct StableAll<'a> {
        success: bool,
        paragraphs: &'a [ParagraphProcessingResult],
        global_unrecognized_words: &'a [String],
        updated_global_stems: BTreeMap<&'a String, &'a bool>,
    }
    serde_json::to_string_pretty(&StableAll {
        success: result.success,
        paragraphs: &result.paragraphs,
        global_unrecognized_words: &result.global_unrecognized_words,
        updated_global_stems: result.updated_global_stems.iter().collect(),
    })
    .unwrap()
        + "\n"
}

fn assert_error(client: &Client, body: serde_json::Value, status: Status, needle: &str) {
    let (s, _, text) = post(client, body.clone());
    assert_eq!(s, status, "{body}: {text}");
    assert!(text.contains(needle), "{body}: {text}");
}

#[test]
fn test_gloss_text_route() {
    if !matches!(Path::new(DIST_SIMSAPA_DIR).join("app-assets/appdata.sqlite3").try_exists(), Ok(true)) {
        eprintln!("skipped: no dist databases at {}", DIST_SIMSAPA_DIR);
        return;
    }
    setup();
    let c = client(false);

    // Default: Word in Sentence, each paragraph with its rows, one per token.
    let (s, ct, body) = post(&c, serde_json::json!({ "text": "te jhāyino sātatikā\n\nte jhāyino" }));
    assert_eq!(s, Status::Ok);
    assert_eq!(ct, Some(ContentType::JSON));
    assert!(body.starts_with(r#"{"success":true,"paragraphs":[{"paragraph_index":0,"words_data":"#), "{body}");
    let result: AllParagraphsProcessingResult = serde_json::from_str(&body).unwrap();
    assert_eq!(result.paragraphs.len(), 2);
    let words: Vec<&str> = result.paragraphs[0].rows.iter().map(|r| r.word.as_str()).collect();
    assert_eq!(words, ["te", "jhāyino", "sātatikā"]);
    assert_eq!(result.paragraphs[1].rows.len(), 2);
    assert!(!body.contains("\"ai_translations\"") && !body.contains("\"ai_errors\""));

    // Headwords: the response is the Headwords snapshot, with none of the
    // Word in Sentence fields.
    let (s, _, body) = post(&c, serde_json::json!({
        "text": fixture_passage("DHP211"),
        "options": { "gloss_format": "headwords" },
    }));
    assert_eq!(s, Status::Ok);
    assert!(!body.contains("\"rows\"") && !body.contains("\"variants\""));
    let result: AllParagraphsProcessingResult = serde_json::from_str(&body).unwrap();
    let snapshot = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../backend/tests/data/gloss_headwords_snapshot/DHP211.json");
    assert_eq!(stable_pretty(&result), std::fs::read_to_string(snapshot).unwrap());

    // Exact mode still works with no gloss_format.
    let (s, _, body) = post(&c, serde_json::json!({ "text": "te jhāyino", "options": { "exact": true } }));
    assert_eq!(s, Status::Ok);
    assert!(body.contains("\"tokens\"") || body.contains("\"paragraphs\""), "{body}");

    // A reference: dhp23 is the verse inside dhp21-32/pli/ms.
    let (s, _, body) = post(&c, serde_json::json!({ "reference": "dhp23" }));
    assert_eq!(s, Status::Ok, "{body}");
    let result: AllParagraphsProcessingResult = serde_json::from_str(&body).unwrap();
    assert_eq!(result.paragraphs.len(), 1);
    assert_eq!(result.paragraphs[0].rows[0].word, "Te");
    assert!(result.paragraphs[0].rows.iter().any(|r| r.word == "daḷhaparakkamā"));

    // The analysis document in each output format.
    let (s, ct, md) = post(&c, serde_json::json!({ "reference": "dhp23", "options": { "output_format": "markdown" } }));
    assert_eq!(s, Status::Ok);
    assert_eq!(ct.map(|c| c.to_string()), Some("text/markdown; charset=utf-8".to_string()));
    assert!(md.starts_with("# Gloss of: dhp23 (dhp21-32/pli/ms)\n"), "{md}");
    assert!(md.contains("### Word-by-Word Analysis"), "{md}");
    assert!(md.contains("| ID | Word in Sentence | Grammar | Contextual | Definition | Construction | Root | Conf. |"), "{md}");
    assert!(md.contains("N: Not checked"), "{md}");
    assert!(!md.contains("### English Translation"), "{md}");
    let (s, ct, html) = post(&c, serde_json::json!({ "text": "te jhāyino", "options": { "output_format": "html" } }));
    assert_eq!(s, Status::Ok);
    assert_eq!(ct, Some(ContentType::HTML));
    assert!(html.contains("<table"), "{html}");
    let (s, ct, org) = post(&c, serde_json::json!({ "text": "te jhāyino", "options": { "output_format": "org" } }));
    assert_eq!(s, Status::Ok);
    assert_eq!(ct, Some(ContentType::Plain));
    assert!(org.starts_with("* Gloss of: te jhāyino") || org.contains("Gloss of: te jhāyino"), "{org}");

    // Bad combinations.
    let bad = Status::BadRequest;
    assert_error(&c, serde_json::json!({ "text": "te", "reference": "dhp23" }), bad, "not both");
    assert_error(&c, serde_json::json!({ "options": {} }), bad, "missing field");
    assert_error(&c, serde_json::json!({ "text": "te", "paragraphs": "1" }), bad, "need a reference");
    assert_error(&c, serde_json::json!({ "text": "te", "options": { "translate": true } }), bad, "translate needs ai");
    assert_error(&c, serde_json::json!({ "text": "te", "options": { "exact": true, "gloss_format": "word_in_sentence" } }), bad, "exact");
    assert_error(&c, serde_json::json!({ "text": "te", "options": { "exact": true, "output_format": "markdown" } }), bad, "exact");
    assert_error(&c, serde_json::json!({ "text": "te", "options": { "gloss_format": "x" } }), bad, "gloss_format");
    assert_error(&c, serde_json::json!({ "text": "te", "options": { "output_format": "docx" } }), bad, "output_format");
    assert_error(&c, serde_json::json!({ "reference": "dhp23", "edition": "pts" }), bad, "edition");
    assert_error(&c, serde_json::json!({ "reference": "dhp23", "paragraphs": "2" }), bad, "out of range");
    assert_error(&c, serde_json::json!({ "reference": "zz999" }), Status::NotFound, "no Pāli text");
    // No runner in this server.
    assert_error(&c, serde_json::json!({ "text": "te", "options": { "ai": true } }), bad, "ai not available in this server");

    // With a runner: its result is the response, and its translations reach
    // the document.
    let c = client(true);
    let (s, _, body) = post(&c, serde_json::json!({ "text": "te jhāyino", "options": { "ai": true } }));
    assert_eq!(s, Status::Ok);
    assert!(body.ends_with(r#""ai_errors":["stub ran without translate"]}"#), "{body}");
    let (s, _, md) = post(&c, serde_json::json!({
        "text": "te jhāyino", "options": { "ai": true, "translate": true, "output_format": "markdown" },
    }));
    assert_eq!(s, Status::Ok);
    assert!(md.contains("### English Translation"), "{md}");
    assert!(md.contains("**Translation:** Translation of: te jhāyino"), "{md}");
    assert!(md.contains("**Literal Translation:** Literal."), "{md}");
}
