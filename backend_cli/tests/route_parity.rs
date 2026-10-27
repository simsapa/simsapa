//! `--json` output against the shared routes' responses: each command's
//! stdout must be the same bytes as the matching route's body, served by
//! `simsapa_api::build_rocket()` through Rocket's local client in this
//! process. Read-only against the dist databases; skipped when they are absent.

use std::path::Path;
use std::process::Command;

use rocket::http::{ContentType, Status};
use rocket::local::blocking::Client;

use simsapa_api::{ApiConfig, build_rocket, init_app_globals_api};

const DIST_SIMSAPA_DIR: &str = "/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa";

fn cli_stdout(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(DIST_SIMSAPA_DIR)
        .args(args)
        .env_remove("API_PORT")
        // No test reaches npm or the releases server.
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to run simsapa");
    assert_eq!(out.status.code(), Some(0), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    let mut s = String::from_utf8(out.stdout).unwrap();
    assert_eq!(s.pop(), Some('\n'), "{args:?}: stdout should end with one newline");
    s
}

fn client() -> Client {
    // SAFETY: set before any thread of this test reads the environment.
    unsafe { std::env::set_var("SIMSAPA_DIR", DIST_SIMSAPA_DIR) };
    // Don't rotate the dist folder's log.txt, as the CLI doesn't.
    simsapa_backend::logger::set_host_options(simsapa_backend::logger::LoggerHostOptions {
        rotate_on_init: false,
        emit_tracing: false,
    });
    simsapa_backend::set_write_api_port_file(false);
    simsapa_backend::init_app_globals();
    simsapa_backend::init_app_data();
    init_app_globals_api();
    let rocket = build_rocket(ApiConfig::loopback(0)).expect("build_rocket");
    Client::untracked(rocket).expect("rocket client")
}

fn post_json(client: &Client, path: &str, body: serde_json::Value) -> String {
    let resp = client.post(path).header(ContentType::JSON).body(body.to_string()).dispatch();
    assert_eq!(resp.status(), Status::Ok, "{path}");
    resp.into_string().unwrap()
}

fn get(client: &Client, path: &str) -> String {
    let resp = client.get(path).dispatch();
    assert_eq!(resp.status(), Status::Ok, "{path}");
    resp.into_string().unwrap()
}

/// One test, so the databases and the Rocket instance are set up once.
#[test]
fn test_json_output_matches_the_routes() {
    if !matches!(Path::new(DIST_SIMSAPA_DIR).join("app-assets/appdata.sqlite3").try_exists(), Ok(true)) {
        eprintln!("skipped: no dist databases at {}", DIST_SIMSAPA_DIR);
        return;
    }
    let client = client();

    assert_eq!(
        cli_stdout(&["lookup", "--json", "bhagavantaṁ", "bhagav"]),
        post_json(&client, "/lookup_forms", serde_json::json!({ "forms": ["bhagavantaṁ", "bhagav"] })),
        "lookup vs /lookup_forms",
    );

    let text = "Te jānanti atthaññe āvāsikā bhikkhū";
    assert_eq!(
        cli_stdout(&["gloss", "--json", text]),
        post_json(&client, "/gloss_text", serde_json::json!({ "text": text })),
        "gloss vs /gloss_text",
    );
    assert_eq!(
        cli_stdout(&["gloss", "--exact", "--json", text]),
        post_json(&client, "/gloss_text", serde_json::json!({ "text": text, "options": { "exact": true } })),
        "gloss --exact vs /gloss_text exact",
    );

    assert_eq!(
        cli_stdout(&["search", "dict", "atthaññe", "--json"]),
        post_json(&client, "/search", serde_json::json!({ "query_text": "atthaññe", "search_area": "Dictionary", "page_num": 0, "page_len": 20 })),
        "search dict vs /search",
    );
    assert_eq!(
        cli_stdout(&["search", "suttas", "anicca", "--mode", "Contains Match", "--lang", "pli", "--json"]),
        post_json(&client, "/search", serde_json::json!({ "query_text": "anicca", "search_area": "Suttas", "mode": "Contains Match", "suttas_lang": "pli", "page_num": 0, "page_len": 20 })),
        "search suttas vs /search",
    );

    assert_eq!(
        cli_stdout(&["word", "dhamma 1.01", "--json"]),
        get(&client, "/word.json?uid=dhamma%201.01"),
        "word vs /word.json",
    );

    assert_eq!(
        cli_stdout(&["sutta", "mn1", "--content-json"]),
        get(&client, "/sutta.json?uid=mn1"),
        "sutta --content-json vs /sutta.json",
    );
    assert_eq!(
        cli_stdout(&["sutta", "mn1/en/sujato", "--bilara"]),
        get(&client, "/sutta_bilara.json?uid=mn1/en/sujato"),
        "sutta --bilara vs /sutta_bilara.json",
    );
}
