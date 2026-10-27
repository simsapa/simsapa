//! The read-only commands, through the built binary, against the dist
//! databases. The commands write nothing themselves; when nothing else is
//! using the folder, each first runs the full startup preamble, as opening the
//! app would. Skipped when the dist databases are absent.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::io::Write;

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
    c
}

fn simsapa(args: &[&str]) -> Output {
    command(args).output().expect("failed to run simsapa")
}

fn simsapa_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = command(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run simsapa");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_ok(out: &Output) {
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(out));
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let s = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(s.trim()).unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}): {s:?}"))
}

/// The stored `content_json` of a sutta, read directly.
fn stored_column(uid: &str, column: &str) -> String {
    let db = Path::new(DIST_SIMSAPA_DIR).join("app-assets/appdata.sqlite3");
    let out = Command::new("sqlite3")
        .arg("-readonly")
        .arg(&db)
        .arg(format!("SELECT {} FROM suttas WHERE uid = '{}';", column, uid))
        .output()
        .expect("sqlite3 is needed for this test");
    let mut s = String::from_utf8(out.stdout).unwrap();
    // sqlite3 ends the value with a newline.
    assert!(s.ends_with('\n'));
    s.pop();
    s
}

#[test]
fn test_search_suttas_json() {
    if !have_dist() { return; }
    let out = simsapa(&["search", "suttas", "anicca", "--json"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert!(doc["hits"].as_i64().unwrap() > 0);
    assert!(!doc["results"].as_array().unwrap().is_empty());
    assert!(doc.get("deconstructor").is_none());
}

#[test]
fn test_search_text_has_no_html() {
    if !have_dist() { return; }
    let out = simsapa(&["search", "suttas", "anicca", "--page-len", "5"]);
    assert_ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().next().unwrap().contains("(page 1/"), "{text}");
    assert!(!text.contains("<span"), "{text}");
    // Not a terminal: the matches are marked **…**.
    assert!(text.contains("**"), "{text}");
}

#[test]
fn test_search_unknown_mode_exits_1() {
    if !have_dist() { return; }
    let out = simsapa(&["--json", "search", "suttas", "anicca", "--mode", "Bogus"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout_json(&out)["error"], "Unknown mode: 'Bogus'");
}

#[test]
fn test_search_dict_has_deconstructor() {
    if !have_dist() { return; }
    let out = simsapa(&["search", "dict", "atthaññe", "--json"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert!(doc["deconstructor"].as_array().is_some_and(|d| !d.is_empty()), "{doc}");
}

#[test]
fn test_query_and_count() {
    if !have_dist() { return; }
    let out = simsapa(&["query", "words", "dhammacakka", "--count"]);
    assert_ok(&out);
    let n: usize = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    assert!(n > 0);
}

#[test]
fn test_fulltext_search_json() {
    if !have_dist() { return; }
    let out = simsapa(&["fulltext-search", "anicca", "--limit", "3", "--json"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert!(doc["total"].as_u64().unwrap() > 0);
    assert_eq!(doc["results"].as_array().unwrap().len(), 3);
}

#[test]
fn test_word_resolves_human_form() {
    if !have_dist() { return; }
    let out = simsapa(&["word", "dhamma 1.01", "--json"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert_eq!(doc.as_array().unwrap().len(), 1);
    assert_eq!(doc[0]["uid"], "dhamma-1-01/dpd");
}

#[test]
fn test_word_miss_exits_4() {
    if !have_dist() { return; }
    let out = simsapa(&["word", "nosuchword"]);
    assert_eq!(out.status.code(), Some(4));
    assert!(out.stdout.is_empty());
    assert!(stderr(&out).contains("word not found: nosuchword"));
}

#[test]
fn test_word_text() {
    if !have_dist() { return; }
    let out = simsapa(&["word", "34626/dpd"]);
    assert_ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with("34626/dpd\n"), "{text}");
    assert!(text.contains("Lemma: dhamma 1.01"), "{text}");
    assert!(!text.contains('<'), "{text}");
}

#[test]
fn test_sutta_text() {
    if !have_dist() { return; }
    let out = simsapa(&["sutta", "mn1"]);
    assert_ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("Mūlapariyāyasutta"));
    assert_eq!(lines.next(), Some("mn1/pli/ms"));
    assert!(text.contains("Evaṁ me sutaṁ"));
    assert!(!text.contains('<'), "HTML left in the text output");
}

#[test]
fn test_sutta_html_resolves_verse_reference() {
    if !have_dist() { return; }
    let out = simsapa(&["sutta", "dhp182", "--html"]);
    assert_ok(&out);
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("<!doctype html>"));
}

#[test]
fn test_sutta_miss_exits_4() {
    if !have_dist() { return; }
    let out = simsapa(&["sutta", "nosuchsutta99"]);
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn test_sutta_formats_conflict() {
    let out = simsapa(&["sutta", "mn1", "--html", "--text"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn test_sutta_content_json_is_the_stored_column() {
    if !have_dist() { return; }
    let out = simsapa(&["sutta", "mn1", "--content-json"]);
    assert_ok(&out);
    assert_eq!(stderr(&out).lines().last(), Some("mn1/pli/ms"));
    let mut stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.pop(), Some('\n'));
    assert_eq!(stdout, stored_column("mn1/pli/ms", "content_json"));
}

#[test]
fn test_sutta_content_json_misses_exit_4() {
    if !have_dist() { return; }
    let out = simsapa(&["sutta", "dn1/pli/cst", "--content-json"]);
    assert_eq!(out.status.code(), Some(4));
    assert!(stderr(&out).contains("dn1/pli/cst has no SuttaCentral JSON content"));

    // A missing translation never falls back to the Pāli.
    let out = simsapa(&["--json", "sutta", "mn1/en/nosuchauthor", "--content-json"]);
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(stdout_json(&out)["error"], "sutta not found: mn1/en/nosuchauthor");
}

#[test]
fn test_sutta_bilara() {
    if !have_dist() { return; }
    let out = simsapa(&["sutta", "mn1/en/sujato", "--bilara"]);
    assert_ok(&out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(doc["comments"]["uid"], "mn1/en/sujato");
    assert_eq!(doc["variants"]["uid"], "mn1/pli/ms");
    // `content` is the stored column, byte for byte.
    assert!(stdout.contains(&stored_column("mn1/en/sujato", "content_json")));

    let out = simsapa(&["sutta", "dn1/pli/cst", "--bilara"]);
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn test_gloss_compound_has_deconstruction() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--json", "Te jānanti atthaññe āvāsikā bhikkhū"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert_eq!(doc["success"], true);
    let words = doc["paragraphs"][0]["words_data"].as_array().unwrap();
    let compound = words.iter().find(|w| w["original_word"] == "atthaññe").unwrap();
    assert!(compound["direct_uids"].as_array().unwrap().is_empty());
    assert_eq!(compound["deconstructions"].as_array().unwrap().len(), 1);

    let out = simsapa(&["gloss", "Te jānanti atthaññe āvāsikā bhikkhū"]);
    assert_ok(&out);
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.starts_with("atthaññe")).unwrap();
    assert!(line.ends_with("[atthi + aññe]"), "{line}");
    assert!(!text.contains("<span"));
}

#[test]
fn test_gloss_reads_stdin() {
    if !have_dist() { return; }
    let out = simsapa_stdin(&["gloss", "--json", "-"], "evaṁ me sutaṁ\n\nbhikkhave\n");
    assert_ok(&out);
    assert_eq!(stdout_json(&out)["paragraphs"].as_array().unwrap().len(), 2);
}

#[test]
fn test_gloss_exact_tokens_in_order() {
    if !have_dist() { return; }
    let out = simsapa(&["gloss", "--exact", "--json", "evaṁ me sutaṁ ekaṁ samayaṁ"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert_eq!(doc["exact"], true);
    let tokens = doc["paragraphs"][0]["tokens"].as_array().unwrap();
    let indexes: Vec<u64> = tokens.iter().map(|t| t["token_index"].as_u64().unwrap()).collect();
    assert_eq!(indexes, vec![0, 1, 2, 3, 4]);
}

#[test]
fn test_gloss_exact_rejects_skip_common() {
    if !have_dist() { return; }
    let out = simsapa(&["--json", "gloss", "--exact", "--skip-common", "evaṁ"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout_json(&out)["error"].as_str().unwrap().contains("skip_common"));
}

#[test]
fn test_lookup_found_then_not_found() {
    if !have_dist() { return; }
    let out = simsapa(&["lookup", "--json", "bhagavantaṁ", "bhagav"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert!(doc["dpd_version"].is_string());
    let statuses: Vec<&str> = doc["results"].as_array().unwrap().iter().map(|r| r["status"].as_str().unwrap()).collect();
    assert_eq!(statuses, vec!["found", "not_found"]);
}

#[test]
fn test_lookup_stdin_keeps_empty_lines() {
    if !have_dist() { return; }
    let out = simsapa_stdin(&["lookup", "--json", "-"], "bhagavantaṁ\n\nbhagav\n");
    assert_ok(&out);
    let doc = stdout_json(&out);
    let statuses: Vec<&str> = doc["results"].as_array().unwrap().iter().map(|r| r["status"].as_str().unwrap()).collect();
    assert_eq!(statuses, vec!["found", "invalid", "not_found"]);
}

#[test]
fn test_stats_json() {
    if !have_dist() { return; }
    let out = simsapa(&["stats", "--json"]);
    assert_ok(&out);
    let doc = stdout_json(&out);
    assert!(doc["Total rows in suttas"].as_str().unwrap().parse::<i64>().unwrap() > 0);
}

#[test]
fn test_read_only_commands_need_databases() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        vec!["search", "suttas", "x"],
        vec!["query", "suttas", "x"],
        vec!["fulltext-search", "x"],
        vec!["word", "x"],
        vec!["sutta", "x"],
        vec!["gloss", "x"],
        vec!["lookup", "x"],
        vec!["stats"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_simsapa"))
            .arg("--simsapa-dir")
            .arg(tmp.path())
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
    }
}

/// Runs `simsapa` with its stdout pipe closed before it writes anything, as
/// `simsapa … | head` does once `head` has read enough.
fn simsapa_closed_stdout(args: &[&str]) -> Output {
    let mut child = command(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run simsapa");
    // The process opens the databases before it prints, so the read end is
    // closed long before the first write.
    drop(child.stdout.take());
    child.wait_with_output().unwrap()
}

#[test]
fn test_closed_stdout_is_not_a_panic() {
    if !have_dist() { return; }
    for args in [
        &["search", "suttas", "anicca", "--json"][..],
        &["search", "suttas", "anicca"][..],
        &["word", "dhamma 1.01"][..],
    ] {
        let out = simsapa_closed_stdout(args);
        let err = stderr(&out);
        assert!(!err.contains("panicked"), "{args:?} panicked: {err}");
        assert!(!err.contains("Broken pipe"), "{args:?} reported the closed pipe: {err}");
        assert_eq!(out.status.code(), Some(0), "{args:?}: {err}");
    }
}

#[test]
fn test_failed_stdout_write_exits_1() {
    if !have_dist() { return; }
    let Ok(dev_full) = std::fs::OpenOptions::new().write(true).open("/dev/full") else { return; };
    let out = command(&["search", "suttas", "anicca", "--json"])
        .stdout(dev_full)
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run simsapa");
    let err = stderr(&out);
    assert!(!err.contains("panicked"), "panicked: {err}");
    assert!(err.contains("failed writing to stdout"), "stderr: {err}");
    assert_eq!(out.status.code(), Some(1), "stderr: {err}");
}
