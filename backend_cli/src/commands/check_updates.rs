//! `simsapa check-updates`: whether a newer `simsapa` is on npm and a newer
//! compatible database release is on the releases server, and the command
//! that installs each. Works without databases, takes no lock and writes
//! nothing. An explicit request, so it runs whatever `--no-update-check`,
//! `SIMSAPA_NO_UPDATE_CHECK` or the app's setting say. The release channel
//! is `--channel`, else `RELEASE_CHANNEL`, else the app's setting, else `main`.
//! See docs/backend-cli-and-npm-package.md.

use std::time::Duration;

use clap::{Args, ValueEnum};

use simsapa_backend::get_app_globals;
use simsapa_backend::update_checker::get_app_version;
use simsapa_backend::update_status::{
    CliUpdate, CliUpdateStatus, DatabaseUpdate, DatabaseUpdateStatus, SETUP_COMMAND, UpdateCheckConfig,
    UpdateStatus, check_updates, command_with_channel, installed_db_version, resolve_release_channel,
};

use crate::exit::{CliError, CliResult, ERROR};
use crate::output::{outln, print_json};

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseChannelArg {
    Main,
    Development,
}

impl ReleaseChannelArg {
    /// The `channel` the releases server takes.
    pub fn api_label(self) -> &'static str {
        match self {
            ReleaseChannelArg::Main => "main",
            ReleaseChannelArg::Development => "development",
        }
    }
}

#[derive(Args, Debug)]
pub struct CheckUpdatesArgs {
    /// The release channel to ask the releases server for. Defaults to
    /// RELEASE_CHANNEL, else the app's setting, else main
    #[arg(long, value_enum, value_name = "CHANNEL")]
    pub channel: Option<ReleaseChannelArg>,
}

/// Width of the label column.
const LABEL: usize = 12;

pub fn run(args: &CheckUpdatesArgs, json: bool) -> CliResult {
    let paths = &get_app_globals().paths;
    let config = UpdateCheckConfig {
        channel: args.channel.map(|c| c.api_label().to_string()),
        ..UpdateCheckConfig::from_env(TIMEOUT, true)
    };
    let status = check_updates(&config, &paths.appdata_db_path, &paths.appdata_database_url);

    if json {
        print_json(&status)?;
    } else {
        let channel_flag = config.channel.as_deref();
        let installed = installed_db_version(&paths.appdata_db_path, &paths.appdata_database_url);
        let channel = resolve_release_channel(&installed, &paths.appdata_database_url, channel_flag);
        for line in text_lines(&status, &get_app_version(), &channel, channel_flag) {
            outln!("{}", line);
        }
    }

    // The output already names what could not be checked; with --json an
    // {"error": …} document after it would make stdout two documents.
    if status.has_unknown() {
        return Err(CliError::reported(ERROR));
    }
    Ok(())
}

fn label(name: &str) -> String {
    format!("{:<LABEL$}", name)
}

fn indent() -> String {
    " ".repeat(LABEL)
}

/// `channel` is the release channel asked for, named on a line of its own
/// unless it is `main`; `channel_flag` is the `--channel` given, carried into
/// the commands the text names.
pub fn text_lines(status: &UpdateStatus, app_version: &str, channel: &str, channel_flag: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    if channel != "main" {
        lines.push(format!("{}{}", label("Channel"), channel));
    }
    lines.extend(cli_lines(&status.cli, channel_flag));
    lines.extend(database_lines(&status.database, app_version));
    lines
}

fn cli_lines(cli: &CliUpdate, channel_flag: Option<&str>) -> Vec<String> {
    let head = label("simsapa");
    let latest = cli.latest.as_deref().unwrap_or("?");
    match cli.status {
        CliUpdateStatus::UpdateAvailable => {
            let mut lines = vec![format!("{}{} → {} available", head, cli.current, latest)];
            if let Some(command) = &cli.command {
                lines.push(format!("{}{}", indent(), command));
            }
            if cli.requires_new_database {
                lines.push(format!(
                    "{}After updating, run `{}` to replace the databases with ones the new version can read.",
                    indent(),
                    command_with_channel(SETUP_COMMAND, channel_flag)
                ));
            }
            lines
        }
        CliUpdateStatus::UpToDate => vec![format!("{}{}, up to date", head, cli.current)],
        CliUpdateStatus::Unknown => vec![format!(
            "{}{}, could not check: {}",
            head,
            cli.current,
            cli.error.as_deref().unwrap_or("unknown error")
        )],
        CliUpdateStatus::NotApplicable => vec![format!("{}{}", head, cli.current)],
        CliUpdateStatus::Checking => vec![format!("{}{}, checking", head, cli.current)],
        CliUpdateStatus::Disabled => vec![format!("{}{}, not checked", head, cli.current)],
    }
}

fn database_lines(db: &DatabaseUpdate, app_version: &str) -> Vec<String> {
    let head = label("Database");
    let current = db.current.as_deref().unwrap_or("version unknown");
    let command_line = |lines: &mut Vec<String>| {
        if let Some(command) = &db.command {
            lines.push(format!("{}{}", indent(), command));
        }
    };
    match db.status {
        DatabaseUpdateStatus::UpdateAvailable => {
            let mut lines = vec![format!(
                "{}{} → {} available",
                head,
                current,
                db.latest.as_deref().unwrap_or("?")
            )];
            command_line(&mut lines);
            if let Some(notes) = db.release_notes.as_deref().filter(|n| !n.trim().is_empty()) {
                lines.push(String::new());
                lines.push("Release notes:".to_string());
                lines.push(String::new());
                lines.push(notes.trim_end().to_string());
            }
            lines
        }
        DatabaseUpdateStatus::UpToDate => vec![format!("{}{}, up to date", head, current)],
        DatabaseUpdateStatus::Incompatible => {
            let mut lines = vec![format!(
                "{}{}, not compatible with simsapa {} (setup replaces the databases)",
                head, current, app_version
            )];
            command_line(&mut lines);
            lines
        }
        DatabaseUpdateStatus::NotInstalled => {
            let mut lines = vec![format!("{}not installed", head)];
            command_line(&mut lines);
            lines
        }
        DatabaseUpdateStatus::Unknown => vec![format!(
            "{}{}, could not check: {}",
            head,
            current,
            db.error.as_deref().unwrap_or("unknown error")
        )],
        DatabaseUpdateStatus::Checking => vec![format!("{}{}, checking", head, current)],
        DatabaseUpdateStatus::Disabled => vec![format!("{}not checked", head)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(cli: serde_json::Value, database: serde_json::Value) -> UpdateStatus {
        serde_json::from_value(serde_json::json!({
            "state": "checked", "disabled_reason": null, "checked_at": "2026-10-27T15:03:49Z",
            "cli": cli, "database": database,
        }))
        .unwrap()
    }

    fn cli(status: &str, latest: Option<&str>, requires_new_database: bool, error: Option<&str>) -> serde_json::Value {
        let command = (status == "update_available").then_some("npm install -g @simsapa/cli@latest");
        serde_json::json!({
            "status": status, "current": "1.1.0-alpha.3", "latest": latest,
            "requires_new_database": requires_new_database, "command": command, "error": error,
        })
    }

    fn db(status: &str, current: Option<&str>, latest: Option<&str>, command: Option<&str>, notes: Option<&str>, error: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "status": status, "current": current, "latest": latest,
            "command": command, "release_notes": notes, "error": error,
        })
    }

    #[test]
    fn test_text_update_available_and_up_to_date() {
        let s = status(
            cli("update_available", Some("1.1.0-alpha.4"), false, None),
            db("up_to_date", Some("1.1.0"), Some("1.1.0"), None, None, None),
        );
        assert_eq!(
            text_lines(&s, "1.1.0-alpha.3", "main", None),
            vec![
                "simsapa     1.1.0-alpha.3 → 1.1.0-alpha.4 available",
                "            npm install -g @simsapa/cli@latest",
                "Database    1.1.0, up to date",
            ]
        );
    }

    #[test]
    fn test_text_names_a_channel_other_than_main() {
        let s = status(
            cli("update_available", Some("1.2.0"), true, None),
            db("update_available", Some("1.1.0"), Some("1.1.2"), Some("simsapa db-upgrade --channel development"), None, None),
        );
        assert_eq!(
            text_lines(&s, "1.1.0-alpha.3", "development", Some("development")),
            vec![
                "Channel     development",
                "simsapa     1.1.0-alpha.3 → 1.2.0 available",
                "            npm install -g @simsapa/cli@latest",
                "            After updating, run `simsapa setup --channel development` to replace the databases with ones the new version can read.",
                "Database    1.1.0 → 1.1.2 available",
                "            simsapa db-upgrade --channel development",
            ]
        );
        // From RELEASE_CHANNEL or the setting: named, but no flag to carry.
        assert_eq!(
            text_lines(&s, "1.1.0-alpha.3", "development", None)[3],
            "            After updating, run `simsapa setup` to replace the databases with ones the new version can read."
        );
    }

    #[test]
    fn test_text_requires_new_database() {
        let s = status(
            cli("update_available", Some("1.2.0"), true, None),
            db("up_to_date", Some("1.1.0"), Some("1.1.0"), None, None, None),
        );
        let lines = text_lines(&s, "1.1.0-alpha.3", "main", None);
        assert_eq!(
            lines[2],
            "            After updating, run `simsapa setup` to replace the databases with ones the new version can read."
        );
    }

    #[test]
    fn test_text_database_update_with_release_notes() {
        let s = status(
            cli("up_to_date", Some("1.1.0-alpha.3"), false, None),
            db("update_available", Some("1.1.0"), Some("1.1.2"), Some("simsapa db-upgrade"), Some("- New suttas\n- Fixes\n"), None),
        );
        assert_eq!(
            text_lines(&s, "1.1.0-alpha.3", "main", None),
            vec![
                "simsapa     1.1.0-alpha.3, up to date",
                "Database    1.1.0 → 1.1.2 available",
                "            simsapa db-upgrade",
                "",
                "Release notes:",
                "",
                "- New suttas\n- Fixes",
            ]
        );
    }

    #[test]
    fn test_text_unknown_not_installed_incompatible() {
        let s = status(
            cli("unknown", None, false, Some("could not reach registry.npmjs.org: timed out")),
            db("not_installed", None, None, Some("simsapa setup"), None, None),
        );
        assert_eq!(
            text_lines(&s, "1.1.0-alpha.3", "main", None),
            vec![
                "simsapa     1.1.0-alpha.3, could not check: could not reach registry.npmjs.org: timed out",
                "Database    not installed",
                "            simsapa setup",
            ]
        );

        let s = status(
            cli("up_to_date", Some("1.1.0-alpha.3"), false, None),
            db("incompatible", Some("1.0.4"), None, Some("simsapa setup"), None, None),
        );
        let lines = text_lines(&s, "1.1.0-alpha.3", "main", None);
        assert_eq!(lines[1], "Database    1.0.4, not compatible with simsapa 1.1.0-alpha.3 (setup replaces the databases)");
        assert_eq!(lines[2], "            simsapa setup");

        let s = status(
            cli("up_to_date", Some("1.1.0-alpha.3"), false, None),
            db("unknown", Some("1.1.0"), None, None, None, Some("could not reach x: refused")),
        );
        assert_eq!(text_lines(&s, "1.1.0-alpha.3", "main", None)[1], "Database    1.1.0, could not check: could not reach x: refused");
    }
}
