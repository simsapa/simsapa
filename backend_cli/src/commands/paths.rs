//! `simsapa paths`: where the data folder and its databases are, and which of
//! them exist. Works without databases.

use std::path::Path;

use serde::Serialize;

use simsapa_backend::get_app_globals;

use crate::exit::CliResult;
use crate::output::print_json;

#[derive(Debug, Serialize)]
pub struct PathEntry {
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Serialize)]
pub struct ApiPortFile {
    pub path: String,
    pub exists: bool,
    /// The port recorded in the file, if it holds one.
    pub port: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct DatabasePaths {
    pub appdata: PathEntry,
    pub dictionaries: PathEntry,
    pub dpd: PathEntry,
}

#[derive(Debug, Serialize)]
pub struct PathsInfo {
    pub data_folder: PathEntry,
    pub app_assets: PathEntry,
    pub databases: DatabasePaths,
    pub index: PathEntry,
    pub api_port_file: ApiPortFile,
    pub log_file: PathEntry,
}

fn entry(p: &Path) -> PathEntry {
    PathEntry {
        path: p.to_string_lossy().into_owned(),
        exists: matches!(p.try_exists(), Ok(true)),
    }
}

pub fn paths_info() -> PathsInfo {
    let paths = &get_app_globals().paths;
    let port_path = &paths.simsapa_api_port_path;
    let port = std::fs::read_to_string(port_path)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok());

    PathsInfo {
        data_folder: entry(&paths.simsapa_dir),
        app_assets: entry(&paths.app_assets_dir),
        databases: DatabasePaths {
            appdata: entry(&paths.appdata_db_path),
            dictionaries: entry(&paths.dict_db_path),
            dpd: entry(&paths.dpd_db_path),
        },
        index: entry(&paths.index_dir),
        api_port_file: ApiPortFile {
            path: port_path.to_string_lossy().into_owned(),
            exists: matches!(port_path.try_exists(), Ok(true)),
            port,
        },
        log_file: entry(&paths.simsapa_dir.join("log.txt")),
    }
}

pub fn run(json: bool) -> CliResult {
    let info = paths_info();
    if json {
        return print_json(&info);
    }

    let mark = |e: &PathEntry| if e.exists { "" } else { "  (missing)" };
    let rows: [(&str, &PathEntry); 7] = [
        ("Data folder", &info.data_folder),
        ("App assets", &info.app_assets),
        ("appdata.sqlite3", &info.databases.appdata),
        ("dictionaries.sqlite3", &info.databases.dictionaries),
        ("dpd.sqlite3", &info.databases.dpd),
        ("Fulltext index", &info.index),
        ("Log file", &info.log_file),
    ];
    for (label, e) in rows {
        println!("{:<22} {}{}", label, e.path, mark(e));
    }

    let port_file = &info.api_port_file;
    let port_note = match (port_file.exists, port_file.port) {
        (false, _) => "  (missing)".to_string(),
        (true, Some(port)) => format!("  (port {})", port),
        (true, None) => "  (no port recorded)".to_string(),
    };
    println!("{:<22} {}{}", "api-port.txt", port_file.path, port_note);

    Ok(())
}
