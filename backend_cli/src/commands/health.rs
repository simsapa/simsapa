//! `simsapa health`: the document `GET /health` returns, computed in-process
//! through the same `simsapa_api::health_info()`.
//!
//! One difference by necessity: a server opens the fulltext indexes on its
//! first fulltext search, while this command opens them before reporting, so
//! `fulltext_searcher_ready` and `fulltext` say whether search will work rather
//! than "not opened yet". `api_port` is the port this process would use, not
//! that of a running server.
//!
//! `updates` is checked inline, on its own thread so it overlaps with opening
//! the indexes, with a short timeout; offline is not an error here. The check
//! is skipped by `--no-update-check`, `SIMSAPA_NO_UPDATE_CHECK` or the app's
//! "Notify about Simsapa updates" setting. See
//! docs/backend-cli-and-npm-package.md.

use std::time::Duration;

use simsapa_api::{HealthFulltextArea, HealthInfo, health_info, init_app_globals_api};
use simsapa_backend::update_checker::get_app_version;
use simsapa_backend::update_status::{
    CliUpdateStatus, DatabaseUpdateStatus, DisabledReason, UpdateCheckConfig, UpdateCheckState, UpdateStatus,
    check_updates, implicit_check_disabled,
};
use simsapa_backend::{get_app_data, get_app_globals, init_fulltext_searcher};

use crate::exit::CliResult;
use crate::output::{outln, print_json};

/// Per request: `health` must stay prompt offline.
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

/// Requires the preamble to have run.
pub fn run(json: bool, no_update_check: bool) -> CliResult {
    init_app_globals_api();

    let disabled = implicit_check_disabled(no_update_check, get_app_data().get_notify_about_simsapa_updates());
    // Started before the indexes are opened, so the two overlap.
    let check = match disabled {
        Some(_) => None,
        None => {
            let paths = &get_app_globals().paths;
            let (db_path, db_url) = (paths.appdata_db_path.clone(), paths.appdata_database_url.clone());
            let config = UpdateCheckConfig::from_env(UPDATE_CHECK_TIMEOUT, true);
            Some(std::thread::spawn(move || check_updates(&config, &db_path, &db_url)))
        }
    };

    init_fulltext_searcher();

    let mut info = health_info(&get_app_data().dbm);
    info.updates = match (disabled, check) {
        (Some(reason), _) => UpdateStatus::disabled(reason, &get_app_version(), true),
        (None, Some(handle)) => handle.join().unwrap_or_else(|_| {
            // The check panicked: report it as not made rather than fail `health`.
            let mut s = UpdateStatus::checking(&get_app_version(), true);
            s.state = UpdateCheckState::Checked;
            s.cli.status = CliUpdateStatus::Unknown;
            s.cli.error = Some("the update check failed".to_string());
            s.database.status = DatabaseUpdateStatus::Unknown;
            s.database.error = Some("the update check failed".to_string());
            s
        }),
        (None, None) => unreachable!("a check is started whenever it is not disabled"),
    };
    if json {
        print_json(&info)
    } else {
        print_text(&info);
        Ok(())
    }
}

fn count(n: Option<i64>) -> String {
    n.map(|n| n.to_string()).unwrap_or_else(|| "unknown".to_string())
}

fn area_line(name: &str, a: &HealthFulltextArea) -> String {
    let mut line = format!("  {:<8} {} ({} open, {} failed)", name, a.state, a.opened, a.failed);
    if !a.message.is_empty() {
        line.push_str(" — ");
        line.push_str(&a.message);
    }
    line
}

fn print_text(h: &HealthInfo) {
    outln!("Simsapa {}", h.app_version);
    outln!("DPD release      {}", h.dpd_version.as_deref().unwrap_or("unknown"));
    outln!("SuttaCentral     {}", h.sc_data_version.as_ref().map(|v| v.display()).unwrap_or_else(|| "unknown".to_string()));
    outln!("appdata          {}", h.db_paths.appdata);
    outln!("dictionaries     {}", h.db_paths.dictionaries);
    outln!("dpd              {}", h.db_paths.dpd);
    outln!("Suttas           {}", count(h.counts.suttas));
    outln!("Dictionary words {}", count(h.counts.dict_words));
    outln!("DPD headwords    {}", count(h.counts.dpd_headwords));
    outln!("Sutta languages  {}", h.sutta_languages.join(", "));
    outln!("Dictionaries     {}", h.dict_sources.len());
    let mut fulltext = format!("Fulltext search  {}", h.fulltext.state);
    if !h.fulltext.message.is_empty() {
        fulltext.push_str(" — ");
        fulltext.push_str(&h.fulltext.message);
    }
    outln!("{}", fulltext);
    outln!("{}", area_line("suttas", &h.fulltext.sutta));
    outln!("{}", area_line("dict", &h.fulltext.dict));
    outln!("{}", area_line("library", &h.fulltext.library));
    outln!("Updates          {}", updates_line(&h.updates));
}

/// Both parts of the update check in one line.
pub fn updates_line(u: &UpdateStatus) -> String {
    if u.state == UpdateCheckState::Disabled {
        let why = match u.disabled_reason {
            Some(DisabledReason::Flag) => "--no-update-check",
            Some(DisabledReason::Environment) => "SIMSAPA_NO_UPDATE_CHECK",
            Some(DisabledReason::Setting) | None => "turned off in Simsapa's settings",
        };
        return format!("not checked ({})", why);
    }
    if u.state == UpdateCheckState::Checking {
        return "checking".to_string();
    }
    if u.cli.status == CliUpdateStatus::Unknown && u.database.status == DatabaseUpdateStatus::Unknown {
        return "unknown (offline)".to_string();
    }

    let with_command = |text: String, command: &Option<String>| match command {
        Some(c) => format!("{} ({})", text, c),
        None => text,
    };
    let mut parts = Vec::new();
    let cli = &u.cli;
    match cli.status {
        CliUpdateStatus::UpdateAvailable => {
            let mut part = with_command(
                format!("simsapa {} available", cli.latest.as_deref().unwrap_or("?")),
                &cli.command,
            );
            if cli.requires_new_database {
                part.push_str(", then simsapa setup");
            }
            parts.push(part);
        }
        CliUpdateStatus::UpToDate => parts.push("simsapa up to date".to_string()),
        CliUpdateStatus::Unknown => parts.push("simsapa unknown".to_string()),
        CliUpdateStatus::NotApplicable | CliUpdateStatus::Checking | CliUpdateStatus::Disabled => {}
    }
    let db = &u.database;
    parts.push(match db.status {
        DatabaseUpdateStatus::UpdateAvailable => with_command(
            format!("database {} available", db.latest.as_deref().unwrap_or("?")),
            &db.command,
        ),
        DatabaseUpdateStatus::UpToDate => "database up to date".to_string(),
        DatabaseUpdateStatus::Incompatible => with_command("database not compatible".to_string(), &db.command),
        DatabaseUpdateStatus::NotInstalled => with_command("database not installed".to_string(), &db.command),
        DatabaseUpdateStatus::Unknown => "database unknown".to_string(),
        DatabaseUpdateStatus::Checking => "database checking".to_string(),
        DatabaseUpdateStatus::Disabled => "database not checked".to_string(),
    });
    parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use simsapa_backend::update_status::{NPM_INSTALL_COMMAND, DB_UPGRADE_COMMAND};

    fn checked() -> UpdateStatus {
        let mut s = UpdateStatus::checking("1.1.0-alpha.3", true);
        s.state = UpdateCheckState::Checked;
        s
    }

    #[test]
    fn test_updates_line_disabled() {
        for (reason, text) in [
            (DisabledReason::Flag, "not checked (--no-update-check)"),
            (DisabledReason::Environment, "not checked (SIMSAPA_NO_UPDATE_CHECK)"),
            (DisabledReason::Setting, "not checked (turned off in Simsapa's settings)"),
        ] {
            assert_eq!(updates_line(&UpdateStatus::disabled(reason, "1.1.0", true)), text);
        }
    }

    #[test]
    fn test_updates_line_offline() {
        let mut s = checked();
        s.cli.status = CliUpdateStatus::Unknown;
        s.database.status = DatabaseUpdateStatus::Unknown;
        assert_eq!(updates_line(&s), "unknown (offline)");
    }

    #[test]
    fn test_updates_line_cli_update_database_up_to_date() {
        let mut s = checked();
        s.cli.status = CliUpdateStatus::UpdateAvailable;
        s.cli.latest = Some("1.1.0-alpha.4".to_string());
        s.cli.command = Some(NPM_INSTALL_COMMAND.to_string());
        s.database.status = DatabaseUpdateStatus::UpToDate;
        assert_eq!(
            updates_line(&s),
            "simsapa 1.1.0-alpha.4 available (npm install -g @simsapa/cli@latest); database up to date"
        );

        s.cli.requires_new_database = true;
        assert!(updates_line(&s).contains("@latest), then simsapa setup; database"));
    }

    #[test]
    fn test_updates_line_database_update_cli_unknown() {
        let mut s = checked();
        s.cli.status = CliUpdateStatus::Unknown;
        s.database.status = DatabaseUpdateStatus::UpdateAvailable;
        s.database.latest = Some("1.1.1".to_string());
        s.database.command = Some(DB_UPGRADE_COMMAND.to_string());
        assert_eq!(updates_line(&s), "simsapa unknown; database 1.1.1 available (simsapa db-upgrade)");
    }
}
