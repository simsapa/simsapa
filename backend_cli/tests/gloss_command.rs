//! `simsapa gloss`: references, gloss formats, output formats, the AI-off
//! path and the exit codes, through the built binary against the dist
//! databases. No test makes a network call: they pass `--no-ai`, or check
//! first that the dist settings enable no AI provider. Skipped when the dist
//! databases are absent.

use std::path::Path;
use std::process::{Command, Output, Stdio};

const DIST_SIMSAPA_DIR: &str = "/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa";

fn have_dist() -> bool {
    let ok = matches!(Path::new(DIST_SIMSAPA_DIR).join("app-assets/appdata.sqlite3").try_exists(), Ok(true));
    if !ok {
        eprintln!("skipped: no dist databases at {}", DIST_SIMSAPA_DIR);
    }
    ok
}

fn command(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_simsapa"));
    c.arg("--simsapa-dir").arg(DIST_SIMSAPA_DIR).args(args).env_remove("API_PORT");
    c.env("SIMSAPA_NO_UPDATE_CHECK", "1");
    c
}

fn simsapa(args: &[&str]) -> Output {
    command(args).output().expect("failed to run simsapa")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_ok(out: &Output) {
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(out));
}

/// Whether the dist settings have an AI model in the Fallback sequence; a
/// test without `--no-ai` would then make a network request.
fn dist_has_ai_model() -> bool {
    let db = Path::new(DIST_SIMSAPA_DIR).join("app-assets/appdata.sqlite3");
    let out = Command::new("sqlite3")
        .arg("-readonly")
        .arg(&db)
        .arg("SELECT value FROM app_settings WHERE key = 'app_settings';")
        .output()
        .expect("sqlite3 is needed for this test");
    let settings: serde_json::Value = serde_json::from_slice(&out.stdout).expect("app_settings is JSON");
    settings["ai_fallback_sequence"].as_array().is_some_and(|a| !a.is_empty())
}

#[test]
fn test_dhp23_markdown_without_ai() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--no-ai", "--format", "markdown", "dhp23"]);
    assert_ok(&out);
    let doc = stdout(&out);
    assert!(doc.starts_with("# Gloss of: dhp23 (dhp21-32/pli/ms)\n"), "{doc}");
    assert!(doc.contains("### Word-by-Word Analysis"), "{doc}");
    assert!(doc.contains("| ID | Word in Sentence | Grammar | Meaning | Construction | Root | Conf. |"), "{doc}");
    assert!(doc.contains("| 62145 | sātatikā | adj, masc nom pl of sātatika |"), "{doc}");
    assert!(doc.contains("| 32201 | - daḷha | pp |"), "{doc}");
    assert!(doc.contains("- N: Not checked"), "{doc}");
    // The verse's own text, not the vagga's other verses.
    assert!(!doc.contains("Appamādo"), "{doc}");
    assert!(!doc.contains("### English Translation"), "{doc}");
    assert!(!stderr(&out).contains("Waiting for AI response"));
}

#[test]
fn test_markdown_is_the_default_when_piped() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--no-ai", "dhp23"]);
    assert_ok(&out);
    assert!(stdout(&out).starts_with("# Gloss of: dhp23"));
}

#[test]
fn test_ai_off_without_a_provider() {
    if !have_dist() { return; }
    if dist_has_ai_model() {
        eprintln!("skipped: the dist settings enable an AI model");
        return;
    }
    let out = simsapa(&["gloss", "--format", "markdown", "dhp23"]);
    assert_ok(&out);
    assert!(stderr(&out).contains("no AI model is enabled"), "stderr: {}", stderr(&out));
    let doc = stdout(&out);
    assert!(doc.contains("No AI provider is enabled, so senses marked N were not checked"), "{doc}");
    assert!(!doc.contains("### English Translation"));
}

#[test]
fn test_text_format_has_grammar_and_legend() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--no-ai", "--format", "text", "dhp23"]);
    assert_ok(&out);
    let text = stdout(&out);
    assert!(text.starts_with("dhp23 (dhp21-32/pli/ms)\n"), "{text}");
    let line = text.lines().find(|l| l.starts_with("sātatikā")).unwrap();
    assert!(line.contains("adj, masc nom pl of sātatika"), "{line}");
    assert!(text.lines().any(|l| l.starts_with("- daḷha")), "{text}");
    assert!(text.contains("N: Not checked"), "{text}");
}

#[test]
fn test_headwords_json_is_the_headwords_result() {
    if !have_dist() { return; }
    let text = "evaṁ me sutaṁ ekaṁ samayaṁ bhagavā";
    let out = simsapa(&["gloss", "--json", "--no-ai", "--gloss-format", "headwords", text]);
    assert_ok(&out);
    let doc: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    let p = &doc["paragraphs"][0];
    assert!(p.get("rows").is_none(), "a Headwords result has no rows");
    assert!(!p["words_data"].as_array().unwrap().is_empty());

    let out = simsapa(&["gloss", "--json", "--no-ai", text]);
    assert_ok(&out);
    let doc: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(doc["paragraphs"][0]["rows"].as_array().unwrap().len(), 6);
}

#[test]
fn test_session_json_export() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--no-ai", "--format", "json", "dhp23"]);
    assert_ok(&out);
    let doc: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(doc["format"], "simsapa-gloss-session");
    assert_eq!(doc["session"]["gloss_format"], "word_in_sentence");
    assert!(!doc["session"]["paragraphs"][0]["rows"].as_array().unwrap().is_empty());
}

#[test]
fn test_docx_to_a_file() {
    if !have_dist() { return; }
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("dhp23.docx");
    let out = simsapa(&["gloss", "--no-ai", "--output", path.to_str().unwrap(), "dhp23"]);
    assert_ok(&out);
    assert!(stdout(&out).is_empty());
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"PK"), "a DOCX is a zip archive");
}

#[test]
fn test_paragraph_selection() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--no-ai", "--paragraphs", "2", "sn1.71"]);
    assert_ok(&out);
    assert!(stdout(&out).starts_with("# Gloss of: sn1.71 ¶2 ("), "{}", stdout(&out));
}

#[test]
fn test_refused_combinations() {
    if !have_dist() { return; }
    for args in [
        &["gloss", "--no-ai", "--format", "docx", "dhp23"][..],
        &["gloss", "--no-ai", "--json", "--format", "json", "dhp23"][..],
        &["gloss", "--no-ai", "--exact", "--gloss-format", "word-in-sentence", "evaṁ"][..],
        &["gloss", "--no-ai", "--paragraphs", "1", "evaṁ me sutaṁ"][..],
    ] {
        let out = simsapa(args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", stderr(&out));
    }
}

#[test]
fn test_unknown_reference_is_not_found() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--no-ai", "dn999"]);
    assert_eq!(out.status.code(), Some(4), "stderr: {}", stderr(&out));
}

#[test]
fn test_closed_stdout_is_not_a_panic() {
    if !have_dist() { return; }
    for args in [&["gloss", "--no-ai", "dhp23"][..], &["gloss", "--no-ai", "--format", "text", "dhp23"][..]] {
        let mut child = command(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        drop(child.stdout.take());
        let out = child.wait_with_output().unwrap();
        let err = stderr(&out);
        assert!(!err.contains("panicked"), "{args:?} panicked: {err}");
        assert_eq!(out.status.code(), Some(0), "{args:?}: {err}");
    }
}
