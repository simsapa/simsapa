//! The `updates` field of `GET /health`: the last update-check result of this
//! process, held in memory. `/health` only reads it (a read lock and a clone),
//! so its response time does not depend on the network.
//!
//! `simsapa serve` fills it with one background check per start
//! ([`spawn_background_update_check`]); the GUI's server makes no request of
//! its own and fills it from the GUI's own update check. Nothing refreshes it
//! periodically. See docs/localhost-api-implementation.md.

use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use simsapa_backend::logger::info;
use simsapa_backend::update_checker::get_app_version;
use simsapa_backend::update_status::{UpdateCheckConfig, UpdateStatus, check_updates};

use crate::get_app_globals_api;

/// Per request, for the background check.
pub const BACKGROUND_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

pub struct UpdateCache {
    status: RwLock<Option<UpdateStatus>>,
    spawned: AtomicBool,
}

impl UpdateCache {
    pub const fn new() -> Self {
        UpdateCache { status: RwLock::new(None), spawned: AtomicBool::new(false) }
    }

    pub fn set(&self, status: UpdateStatus) {
        // A poisoned lock still holds a whole value: every write replaces it.
        *self.status.write().unwrap_or_else(|e| e.into_inner()) = Some(status);
    }

    /// Set only when nothing is stored yet, so an initial `checking` written
    /// late does not replace a finished check.
    pub fn set_if_unset(&self, status: UpdateStatus) {
        let mut guard = self.status.write().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(status);
        }
    }

    /// The stored result; `checking` when nothing is stored yet.
    pub fn current(&self) -> UpdateStatus {
        let stored = self.status.read().unwrap_or_else(|e| e.into_inner()).clone();
        stored.unwrap_or_else(|| UpdateStatus::checking(&get_app_version(), true))
    }

    /// Store `checking` and run the check on a `std::thread` (its requests are
    /// `reqwest::blocking`, which must not run inside an async runtime).
    /// Returns `false`, starting nothing, when a check was already started.
    pub fn spawn_check(
        &'static self,
        config: UpdateCheckConfig,
        appdata_db_path: PathBuf,
        appdata_database_url: String,
    ) -> Option<std::thread::JoinHandle<()>> {
        if self.spawned.swap(true, Ordering::SeqCst) {
            return None;
        }
        self.set(UpdateStatus::checking(&get_app_version(), config.check_cli));
        Some(std::thread::spawn(move || {
            let status = check_updates(&config, &appdata_db_path, &appdata_database_url);
            info(&format!("update check: {}", summary(&status)));
            self.set(status);
        }))
    }
}

impl Default for UpdateCache {
    fn default() -> Self {
        Self::new()
    }
}

/// One log line: each part's status and, when unknown, why.
fn summary(status: &UpdateStatus) -> String {
    let name = |v: serde_json::Result<serde_json::Value>| {
        v.ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    };
    let part = |label: &str, status: String, latest: &Option<String>, error: &Option<String>| {
        let mut s = format!("{} {}", label, status);
        if let Some(latest) = latest {
            s.push_str(&format!(" (latest {})", latest));
        }
        if let Some(error) = error {
            s.push_str(&format!(": {}", error));
        }
        s
    };
    format!(
        "{}; {}",
        part("cli", name(serde_json::to_value(status.cli.status)), &status.cli.latest, &status.cli.error),
        part("database", name(serde_json::to_value(status.database.status)), &status.database.latest, &status.database.error),
    )
}

static UPDATE_CACHE: UpdateCache = UpdateCache::new();

pub fn set_update_status(status: UpdateStatus) {
    UPDATE_CACHE.set(status);
}

/// See [`UpdateCache::set_if_unset`].
pub fn init_update_status(status: UpdateStatus) {
    UPDATE_CACHE.set_if_unset(status);
}

pub fn current_update_status() -> UpdateStatus {
    UPDATE_CACHE.current()
}

/// `simsapa serve`'s one check per start: 3 s per request, npm included.
/// Call once the server is bound; a second call does nothing. Requires
/// `init_app_globals_api()`.
pub fn spawn_background_update_check() {
    let paths = &get_app_globals_api().paths;
    let config = UpdateCheckConfig::from_env(BACKGROUND_CHECK_TIMEOUT, true);
    let _ = UPDATE_CACHE.spawn_check(config, paths.appdata_db_path.clone(), paths.appdata_database_url.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::Instant;

    use simsapa_backend::update_status::{CliUpdateStatus, DatabaseUpdateStatus, DisabledReason, UpdateCheckState};

    fn leaked() -> &'static UpdateCache {
        Box::leak(Box::new(UpdateCache::new()))
    }

    /// Accepts connections and never answers. Returns the URL.
    fn hanging_listener() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                held.push(stream);
            }
        });
        format!("http://127.0.0.1:{}/x", port)
    }

    fn config(url: &str, timeout: Duration) -> UpdateCheckConfig {
        UpdateCheckConfig {
            npm_dist_tags_url: url.to_string(),
            releases_api_url: url.to_string(),
            timeout,
            check_cli: true,
        }
    }

    #[test]
    fn test_empty_cache_is_checking() {
        let cache = leaked();
        let s = cache.current();
        assert_eq!(s.state, UpdateCheckState::Checking);
        assert_eq!(s.cli.status, CliUpdateStatus::Checking);
    }

    #[test]
    fn test_reads_do_not_wait_for_a_blocked_check() {
        let cache = leaked();
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("appdata.sqlite3");
        let url = hanging_listener();

        let handle = cache
            .spawn_check(config(&url, Duration::from_secs(2)), db_path.clone(), db_path.display().to_string())
            .expect("first spawn starts a check");

        let start = Instant::now();
        let s = cache.current();
        assert!(start.elapsed() < Duration::from_millis(100), "took {:?}", start.elapsed());
        assert_eq!(s.state, UpdateCheckState::Checking);

        // A second spawn starts nothing.
        assert!(cache.spawn_check(config(&url, Duration::from_secs(2)), db_path.clone(), String::new()).is_none());

        handle.join().unwrap();
        let s = cache.current();
        assert_eq!(s.state, UpdateCheckState::Checked);
        assert_eq!(s.cli.status, CliUpdateStatus::Unknown);
        assert_eq!(s.database.status, DatabaseUpdateStatus::NotInstalled);
    }

    #[test]
    fn test_set_if_unset_keeps_a_finished_check() {
        let cache = leaked();
        cache.set(UpdateStatus::disabled(DisabledReason::Setting, "1.1.0", false));
        cache.set_if_unset(UpdateStatus::checking("1.1.0", false));
        assert_eq!(cache.current().state, UpdateCheckState::Disabled);
    }

    #[test]
    fn test_serialised_updates_field() {
        let cache = leaked();
        cache.set(UpdateStatus::disabled(DisabledReason::Flag, "1.1.0", true));
        let v = serde_json::to_value(cache.current()).unwrap();
        assert_eq!(v["state"], "disabled");
        assert_eq!(v["disabled_reason"], "flag");
        assert_eq!(v["cli"]["status"], "disabled");
        assert_eq!(v["database"]["status"], "disabled");
        for key in ["checked_at", "cli", "database"] {
            assert!(v.get(key).is_some(), "{key}");
        }
    }
}
