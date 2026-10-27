//! `simsapa serve` against the dist databases, through the built binary: the
//! listening line, a shared route, a GUI and an AI route (404), the exit-3
//! refusal of a writer while serving, and a clean exit after `GET /shutdown`.
//! A non-loopback bind is checked for its warning and the missing `/shutdown`,
//! then stopped with SIGINT.
//!
//! `serve` writes the dist folder's `api-port.txt`; the guard puts back what was
//! there. Skipped when the dist databases are absent or the folder is in use.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

const DIST_SIMSAPA_DIR: &str = "/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa";

/// One server at a time: a second `serve` on the same folder exits 3.
static SERVE_LOCK: Mutex<()> = Mutex::new(());

/// Restores `api-port.txt` and kills the server if a test failed before
/// stopping it.
struct ServeGuard {
    child: Child,
    port_file: PathBuf,
    port_file_before: Option<Vec<u8>>,
}

impl Drop for ServeGuard {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        match &self.port_file_before {
            Some(bytes) => {
                let _ = fs::write(&self.port_file, bytes);
            }
            None => {
                let _ = fs::remove_file(&self.port_file);
            }
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn dist_available() -> bool {
    if !matches!(Path::new(DIST_SIMSAPA_DIR).join("app-assets/appdata.sqlite3").try_exists(), Ok(true)) {
        eprintln!("skipped: no dist databases at {}", DIST_SIMSAPA_DIR);
        return false;
    }
    true
}

/// Start `serve` and wait for its first stdout line. `None` when it exited
/// first; the guard then carries its exit status and stderr.
fn start_serve(args: &[&str]) -> (ServeGuard, Option<String>) {
    let port_file = Path::new(DIST_SIMSAPA_DIR).join("api-port.txt");
    let port_file_before = fs::read(&port_file).ok();

    let mut child = Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .arg("--simsapa-dir")
        .arg(DIST_SIMSAPA_DIR)
        .arg("serve")
        .args(args)
        .env_remove("API_PORT")
        // No test reaches npm or the releases server.
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run simsapa serve");

    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx.recv_timeout(Duration::from_secs(120)).ok().filter(|l| !l.is_empty());

    (ServeGuard { child, port_file, port_file_before }, line)
}

fn stderr_of(guard: &mut ServeGuard) -> String {
    let mut s = String::new();
    if let Some(mut e) = guard.child.stderr.take() {
        let _ = e.read_to_string(&mut s);
    }
    s
}

fn get(url: &str) -> (u16, String) {
    let r = reqwest::blocking::Client::new()
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    let status = r.status().as_u16();
    (status, r.text().unwrap_or_default())
}

fn wait_exit(guard: &mut ServeGuard) -> Option<i32> {
    for _ in 0..100 {
        if let Ok(Some(status)) = guard.child.try_wait() {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

#[test]
fn test_serve_on_loopback() {
    let _serial = SERVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !dist_available() {
        return;
    }
    let port = free_port();
    let (mut guard, line) = start_serve(&["--port", &port.to_string()]);
    let Some(line) = line else {
        let code = guard.child.wait().ok().and_then(|s| s.code());
        let err = stderr_of(&mut guard);
        if code == Some(3) {
            eprintln!("skipped: the dist folder is in use: {err}");
            return;
        }
        panic!("serve exited with {code:?} before listening: {err}");
    };
    let base = format!("http://127.0.0.1:{port}");
    assert_eq!(line.trim(), format!("Simsapa API listening on {base}"));

    assert_eq!(fs::read_to_string(&guard.port_file).unwrap().trim(), port.to_string());

    let (status, body) = get(&format!("{base}/health"));
    assert_eq!(status, 200);
    let health: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(health["api_port"], port);
    assert!(health["db_paths"]["appdata"].as_str().unwrap().starts_with(DIST_SIMSAPA_DIR));

    assert_eq!(get(&format!("{base}/open_sutta_window/mn1")).0, 404);
    assert_eq!(get(&format!("{base}/ai_provider_key")).0, 404);
    assert_eq!(get(&format!("{base}/word_selection_ws")).0, 404);

    // A writer is refused while the folder is served.
    let out = Command::new(env!("CARGO_BIN_EXE_simsapa"))
        .args(["--simsapa-dir", DIST_SIMSAPA_DIR, "index", "delete", "--area", "library", "--lang", "zz"])
        .env_remove("API_PORT")
        // No test reaches npm or the releases server.
        .env("SIMSAPA_NO_UPDATE_CHECK", "1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    assert_eq!(get(&format!("{base}/shutdown")).0, 200);
    assert_eq!(wait_exit(&mut guard), Some(0));
}

#[cfg(unix)]
#[test]
fn test_serve_on_all_interfaces_warns_and_has_no_shutdown() {
    let _serial = SERVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !dist_available() {
        return;
    }
    let port = free_port();
    let (mut guard, line) = start_serve(&["--port", &port.to_string(), "--host", "0.0.0.0", "--json"]);
    let Some(line) = line else {
        let code = guard.child.wait().ok().and_then(|s| s.code());
        let err = stderr_of(&mut guard);
        if code == Some(3) {
            eprintln!("skipped: the dist folder is in use: {err}");
            return;
        }
        panic!("serve exited with {code:?} before listening: {err}");
    };
    let doc: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(doc["url"], format!("http://0.0.0.0:{port}"));
    assert_eq!(doc["port"], port);

    let base = format!("http://127.0.0.1:{port}");
    assert_eq!(get(&format!("{base}/health")).0, 200);
    assert_eq!(get(&format!("{base}/shutdown")).0, 404);

    let pid = guard.child.id().to_string();
    assert!(Command::new("kill").args(["-INT", &pid]).status().unwrap().success());
    assert_eq!(wait_exit(&mut guard), Some(0));
    let err = stderr_of(&mut guard);
    assert!(err.contains("without\nauthentication"), "stderr: {err}");
}

#[test]
fn test_serve_on_a_busy_port_keeps_the_port_file() {
    let _serial = SERVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !dist_available() {
        return;
    }
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();

    let (mut guard, line) = start_serve(&["--port", &port.to_string()]);
    assert_eq!(line, None, "serve must not claim to listen on a busy port");
    let code = guard.child.wait().ok().and_then(|s| s.code());
    let err = stderr_of(&mut guard);
    if code == Some(3) {
        eprintln!("skipped: the dist folder is in use: {err}");
        return;
    }
    assert_eq!(code, Some(1), "stderr: {err}");
    assert!(err.contains("Could not listen on"), "stderr: {err}");

    // Checked before the guard restores it: serve itself put it back.
    assert_eq!(fs::read(&guard.port_file).ok(), guard.port_file_before);
}
