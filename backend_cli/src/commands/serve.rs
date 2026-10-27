//! `simsapa serve`: the shared routes of the localhost API
//! (`simsapa_api::build_rocket()`), without the GUI, AI and static-asset routes.
//!
//! The port is resolved before anything creates an `AppGlobals`, because both
//! globals (the backend's, created by `init_globals()`, and `simsapa_api`'s)
//! read `API_PORT` once, and the backend's `api_url` is baked into every
//! rendered page. Only `simsapa_api`'s globals write `api-port.txt`, after the
//! in-use check, so a refused `serve` never overwrites a running server's port.
//!
//! Rocket handles Ctrl-C itself (a graceful shutdown); no `ctrlc` handler is
//! installed here. See docs/backend-cli-and-npm-package.md.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use clap::Args;
use rocket::fairing::AdHoc;
use rocket::error::ErrorKind;

use simsapa_api::{ApiConfig, build_rocket, set_update_status, spawn_background_update_check};
use simsapa_backend::errln;
use simsapa_backend::update_checker::get_app_version;
use simsapa_backend::update_status::{UpdateStatus, implicit_check_disabled};
use simsapa_backend::{dotenv_c, find_port_set_env, get_app_data, get_app_globals, logger};

use crate::exit::{CliError, CliResult};
use crate::in_use::WriteLock;
use crate::output::outln;

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// The port to listen on. Defaults to API_PORT, else the first free port
    /// from 4848.
    #[arg(long)]
    pub port: Option<u16>,

    /// The IP address to listen on. 127.0.0.1 is reachable only from this
    /// machine; 0.0.0.0 (or ::) from anywhere, e.g. from outside a container.
    /// The API has no authentication.
    #[arg(long, default_value_t = IpAddr::V4(Ipv4Addr::LOCALHOST))]
    pub host: IpAddr,
}

/// The server configuration. A non-loopback bind does not mount `/shutdown`,
/// so another machine cannot stop the server.
pub fn serve_config(host: IpAddr, port: u16) -> ApiConfig {
    ApiConfig {
        host,
        port,
        mount_shutdown: host.is_loopback(),
    }
}

/// The port to serve on: `--port`, else what `find_port_set_env()` picks
/// (`API_PORT` when it is free, else the first free port from 4848).
/// `find` returns `None` when no port is free.
pub fn resolve_port(flag: Option<u16>, find: impl FnOnce() -> Option<u16>) -> Option<u16> {
    flag.or_else(find)
}

/// Resolve the port and set `API_PORT`. Runs before `init_globals()`; loads
/// `.env` / `config.txt` first, so an `API_PORT` set there is honoured.
pub fn prepare_port(args: &ServeArgs) -> CliResult {
    dotenv_c();
    let port = resolve_port(args.port, || {
        if find_port_set_env() {
            std::env::var("API_PORT").ok().and_then(|p| p.parse::<u16>().ok())
        } else {
            None
        }
    })
    .ok_or_else(|| CliError::error("No free port found from 4848 up. Pass one with --port."))?;
    // SAFETY: called from main before any other thread exists.
    unsafe { std::env::set_var("API_PORT", port.to_string()) };
    Ok(())
}

/// `api-port.txt` as it was before `serve` wrote its own port, so a server
/// that never started does not leave the file naming a port nobody listens on.
pub struct PortFileBackup {
    path: std::path::PathBuf,
    /// `None`: there was no file (or it could not be read).
    before: Option<Vec<u8>>,
}

impl PortFileBackup {
    /// Read the file. Call before `init_app_globals_api()` writes it.
    pub fn read() -> Self {
        let path = get_app_globals().paths.simsapa_api_port_path.clone();
        let before = std::fs::read(&path).ok();
        PortFileBackup { path, before }
    }

    /// Put back the previous contents, or remove the file if there was none.
    pub fn restore(&self) {
        let result = match &self.before {
            Some(bytes) => std::fs::write(&self.path, bytes),
            None => match std::fs::remove_file(&self.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        };
        if let Err(e) = result {
            logger::warn(&format!("simsapa serve: could not restore {}: {}", self.path.display(), e));
        }
    }
}

/// The URL a client uses. An IPv6 address is bracketed.
pub fn listening_url(host: IpAddr, port: u16) -> String {
    format!("http://{}", SocketAddr::new(host, port))
}

fn non_loopback_warning(host: IpAddr, port: u16) -> String {
    format!(
        "Warning: the Simsapa API is listening on {} without\n\
         authentication. Anyone who can reach this machine on that port can use it.\n\
         That includes changing this folder's display settings (POST /save_sutta_display_settings)\n\
         and writing to its log (POST /logger). GET /shutdown is not available; stop it with Ctrl-C.",
        SocketAddr::new(host, port)
    )
}

/// Run the server until Ctrl-C or `GET /shutdown`. Requires the full preamble
/// and `init_app_globals_api()`. `port_file` is restored when the server does
/// not start. `no_update_check` is `--no-update-check`: see
/// `implicit_check_disabled()`. `_lock` is the CLI write lock, held while
/// serving: the in-use check cannot see a server bound to a specific
/// non-loopback address, and the lock still keeps other `simsapa` writers out.
pub fn run(
    args: &ServeArgs,
    json: bool,
    no_update_check: bool,
    _lock: Option<WriteLock>,
    port_file: PortFileBackup,
) -> CliResult {
    let result = serve(args, json, no_update_check);
    if result.is_err() {
        port_file.restore();
    }
    result
}

fn serve(args: &ServeArgs, json: bool, no_update_check: bool) -> CliResult {
    let port: u16 = std::env::var("API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| CliError::error("API_PORT is not set to a port number."))?;
    let config = serve_config(args.host, port);
    let url = listening_url(config.host, config.port);

    if !config.host.is_loopback() {
        errln!("{}", non_loopback_warning(config.host, config.port));
    }

    let rocket = build_rocket(config.clone())
        .map_err(|e| CliError::error(format!("Could not start the API server: {:#}", e)))?;

    let listening_line = if json {
        serde_json::json!({ "url": url, "host": config.host.to_string(), "port": config.port }).to_string()
    } else {
        format!("Simsapa API listening on {}", url)
    };
    let log_line = format!("simsapa serve: listening on {}", url);

    // One update check per start, for the `updates` field of `/health`.
    let disabled = implicit_check_disabled(no_update_check, get_app_data().get_notify_about_simsapa_updates());
    if let Some(reason) = disabled {
        set_update_status(UpdateStatus::disabled(reason, &get_app_version(), true));
    }

    // Printed once Rocket has bound, so a bind failure is not preceded by a
    // false "listening" line. The update check starts there too, on a thread
    // of its own (it must not block the runtime).
    let rocket = rocket.attach(AdHoc::on_liftoff("Listening line", move |_| {
        Box::pin(async move {
            outln!("{}", listening_line);
            logger::info(&log_line);
            if disabled.is_none() {
                spawn_background_update_check();
            }
        })
    }));

    // Rocket builds and owns the tokio runtime; `main` stays synchronous.
    let launched = rocket::execute(rocket.launch());

    match launched {
        Ok(_) => {
            logger::info("simsapa serve: stopped");
            Ok(())
        }
        Err(e) => {
            let message = match e.kind() {
                ErrorKind::Bind(io) => format!("Could not listen on {}: {}", SocketAddr::new(config.host, config.port), io),
                _ => format!("The API server failed: {}", e),
            };
            Err(CliError::error(message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn ip(s: &str) -> IpAddr {
        IpAddr::from_str(s).unwrap()
    }

    #[test]
    fn test_loopback_hosts_mount_shutdown() {
        for h in ["127.0.0.1", "::1"] {
            let c = serve_config(ip(h), 4848);
            assert!(c.mount_shutdown, "{h}");
            assert_eq!(c.port, 4848);
        }
    }

    #[test]
    fn test_non_loopback_hosts_do_not_mount_shutdown() {
        for h in ["0.0.0.0", "192.168.1.5", "::"] {
            assert!(!serve_config(ip(h), 4848).mount_shutdown, "{h}");
        }
    }

    #[test]
    fn test_resolve_port() {
        assert_eq!(resolve_port(Some(5000), || Some(4848)), Some(5000));
        assert_eq!(resolve_port(None, || Some(4849)), Some(4849));
        assert_eq!(resolve_port(None, || None), None);
    }

    #[test]
    fn test_listening_url_brackets_ipv6() {
        assert_eq!(listening_url(ip("127.0.0.1"), 4848), "http://127.0.0.1:4848");
        assert_eq!(listening_url(ip("::1"), 4848), "http://[::1]:4848");
    }

    #[derive(clap::Parser, Debug)]
    struct TestCli {
        #[command(flatten)]
        serve: ServeArgs,
    }

    fn parse(args: &[&str]) -> Result<TestCli, clap::Error> {
        use clap::Parser;
        TestCli::try_parse_from(std::iter::once("serve").chain(args.iter().copied()))
    }

    #[test]
    fn test_host_flag_parsing() {
        assert_eq!(parse(&[]).unwrap().serve.host, ip("127.0.0.1"));
        assert_eq!(parse(&["--host", "0.0.0.0"]).unwrap().serve.host, ip("0.0.0.0"));
        assert_eq!(parse(&["--host", "::"]).unwrap().serve.host, ip("::"));
        assert!(parse(&["--host", "localhost"]).is_err());
        assert!(parse(&["--host", "example.org"]).is_err());
        assert!(parse(&["--port", "70000"]).is_err());
        assert_eq!(parse(&["--port", "5000"]).unwrap().serve.port, Some(5000));
    }
}
