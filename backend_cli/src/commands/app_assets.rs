//! `simsapa app-assets`: where the databases, indexes and the rest of the app
//! assets are stored and how much space they take; with `--remove`, delete
//! them after confirmation. Works without databases.
//!
//! The app-assets folder holds the user data too (settings, bookmarks,
//! imported books and dictionaries, gloss and prompt history, chanting
//! recordings), so removing it is the full reset. The data folder itself is
//! kept: it holds `log.txt` and the CLI's write lock.
//!
//! `appdata.sqlite3` is deleted first: its presence is what "set up" means to
//! the CLI and the GUI, so a removal that stops part-way (an error, Ctrl-C)
//! leaves a folder that reads as not set up, never a set-up one with
//! databases missing.

use std::fs;
use std::io::{self, IsTerminal};
use std::path::Path;

use clap::Args;
use serde::Serialize;

use simsapa_backend::errln;
use simsapa_backend::get_app_globals;
use simsapa_backend::logger;
use simsapa_backend::storage_diagnostics::format_bytes;

use crate::exit::{CliError, CliResult};
use crate::output::{outln, print_json};
use crate::setup::confirm;

const REMOVE_COMMAND: &str = "simsapa app-assets --remove";

/// The user data an upgrade exported and has not imported yet.
const IMPORT_ME: &str = "import-me";

#[derive(Args, Debug)]
pub struct AppAssetsArgs {
    /// Delete the app assets (databases, indexes and your user data) after
    /// confirmation
    #[arg(long)]
    pub remove: bool,

    /// With --remove: delete without asking
    #[arg(long, short = 'y', requires = "remove")]
    pub yes: bool,
}

#[derive(Debug, Serialize)]
pub struct AssetEntry {
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct FolderInfo {
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct AppAssetsInfo {
    pub data_folder: String,
    pub app_assets: FolderInfo,
    /// The top-level entries of the app-assets folder, by name.
    pub entries: Vec<AssetEntry>,
    /// Partial downloads left by an interrupted `setup`; removed with the
    /// app assets.
    pub download_temp: FolderInfo,
    /// Set by `--remove` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<bool>,
}

/// The size of a file, or of everything under a folder. Symbolic links are
/// counted as themselves, never followed; what cannot be read counts as 0.
fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    match fs::read_dir(path) {
        Ok(entries) => entries.flatten().map(|e| size_of(&e.path())).sum(),
        Err(_) => 0,
    }
}

/// A symbolic link to the folder is followed here, so that the total is the
/// sum of the entries `read_dir` lists through it.
fn folder_info(path: &Path) -> FolderInfo {
    let exists = matches!(path.try_exists(), Ok(true));
    let size_bytes = match fs::read_dir(path) {
        Ok(entries) if exists => entries.flatten().map(|e| size_of(&e.path())).sum(),
        _ => 0,
    };
    FolderInfo { path: path.to_string_lossy().into_owned(), exists, size_bytes }
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false)
}

/// Delete `appdata.sqlite3` and its journal files, then the rest.
fn remove_app_assets(dir: &Path) -> io::Result<()> {
    for name in ["appdata.sqlite3", "appdata.sqlite3-wal", "appdata.sqlite3-shm", "appdata.sqlite3-journal"] {
        let path = dir.join(name);
        if matches!(fs::symlink_metadata(&path), Ok(m) if !m.is_dir()) {
            fs::remove_file(&path)?;
        }
    }
    fs::remove_dir_all(dir)
}

fn entries_of(dir: &Path) -> Vec<AssetEntry> {
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    let mut entries: Vec<AssetEntry> = read
        .flatten()
        .map(|e| {
            let path = e.path();
            AssetEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
                size_bytes: size_of(&path),
            }
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

pub fn app_assets_info() -> AppAssetsInfo {
    let paths = &get_app_globals().paths;
    AppAssetsInfo {
        data_folder: paths.simsapa_dir.to_string_lossy().into_owned(),
        app_assets: folder_info(&paths.app_assets_dir),
        entries: entries_of(&paths.app_assets_dir),
        download_temp: folder_info(&paths.download_temp_folder),
        removed: None,
    }
}

fn print_info(info: &AppAssetsInfo) {
    outln!("{:<22} {}", "Data folder", info.data_folder);
    if !info.app_assets.exists {
        outln!("{:<22} {}  (missing)", "App assets", info.app_assets.path);
    } else {
        outln!("{:<22} {}  ({})", "App assets", info.app_assets.path, format_bytes(info.app_assets.size_bytes));
        for e in &info.entries {
            let name = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
            outln!("  {:<28} {:>10}", name, format_bytes(e.size_bytes));
        }
    }
    if info.download_temp.exists {
        outln!(
            "{:<22} {}  ({})",
            "Partial downloads",
            info.download_temp.path,
            format_bytes(info.download_temp.size_bytes)
        );
    }
}

pub fn run(args: &AppAssetsArgs, json: bool) -> CliResult {
    if args.remove {
        return remove(args, json);
    }
    let info = app_assets_info();
    if json {
        return print_json(&info);
    }
    print_info(&info);
    Ok(())
}

/// The caller holds the write lock, and has checked that no server uses the
/// folder.
fn remove(args: &AppAssetsArgs, json: bool) -> CliResult {
    let mut info = app_assets_info();
    let paths = &get_app_globals().paths;

    if !info.app_assets.exists && !info.download_temp.exists {
        info.removed = Some(false);
        if json {
            return print_json(&info);
        }
        outln!("Nothing to remove: {} does not exist.", info.app_assets.path);
        return Ok(());
    }

    // Removing a link would leave the data where it is, and report it gone.
    for dir in [&paths.app_assets_dir, &paths.download_temp_folder] {
        if is_symlink(dir) {
            let target = fs::read_link(dir).map(|t| t.display().to_string()).unwrap_or_else(|_| "?".to_string());
            return Err(CliError::error(format!(
                "{} is a symbolic link to {}. Remove the folder it points to yourself, then the link.",
                dir.display(),
                target
            )));
        }
    }
    // The data folder falls back to "." when it cannot be resolved; never
    // delete an `app-assets` folder relative to the current directory.
    if !paths.app_assets_dir.is_absolute() {
        return Err(CliError::error(format!(
            "The data folder could not be resolved (got {}). Pass --simsapa-dir with the full path.",
            paths.app_assets_dir.display()
        )));
    }

    // Before printing anything: refuse early, as setup does.
    if !args.yes && !io::stdin().is_terminal() {
        return Err(CliError::error(format!("Run `{} --yes` to confirm non-interactively.", REMOVE_COMMAND)));
    }

    let total = info.app_assets.size_bytes + info.download_temp.size_bytes;
    errln!("This deletes ({} in all):", format_bytes(total));
    if info.app_assets.exists {
        errln!("  {}", info.app_assets.path);
    }
    if info.download_temp.exists {
        errln!("  {}", info.download_temp.path);
    }
    errln!(
        "These hold the databases, the fulltext indexes and your user data: settings, bookmarks, \
         imported books and dictionaries, gloss and prompt history, chanting recordings. \
         It cannot be undone."
    );
    if info.entries.iter().any(|e| e.name == IMPORT_ME) {
        errln!(
            "{} holds user data exported for a database upgrade that has not been imported yet. \
             It is deleted too.",
            paths.app_assets_dir.join(IMPORT_ME).display()
        );
    }
    if !args.yes && !confirm("Remove the app assets?", REMOVE_COMMAND)? {
        return Err(CliError::error("Nothing was removed."));
    }

    if matches!(paths.app_assets_dir.try_exists(), Ok(true)) {
        remove_app_assets(&paths.app_assets_dir).map_err(|e| {
            CliError::error(format!(
                "Failed to remove {}: {}. It is partly removed; close any program using it and run `{}` again.",
                paths.app_assets_dir.display(),
                e,
                REMOVE_COMMAND
            ))
        })?;
        logger::info(&format!("app-assets --remove: removed {}", paths.app_assets_dir.display()));
    }
    if matches!(paths.download_temp_folder.try_exists(), Ok(true)) {
        fs::remove_dir_all(&paths.download_temp_folder).map_err(|e| {
            CliError::error(format!("Failed to remove {}: {}", paths.download_temp_folder.display(), e))
        })?;
        logger::info(&format!("app-assets --remove: removed {}", paths.download_temp_folder.display()));
    }

    info.removed = Some(true);
    if json {
        return print_json(&info);
    }
    outln!("Removed the app assets from {}.", info.data_folder);
    outln!("Run `simsapa setup` to download the databases again.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_of_counts_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), [0u8; 10]).unwrap();
        fs::create_dir_all(dir.path().join("sub/deeper")).unwrap();
        fs::write(dir.path().join("sub/deeper/b"), [0u8; 32]).unwrap();
        assert_eq!(size_of(dir.path()), 42);
        assert_eq!(size_of(&dir.path().join("missing")), 0);
    }

    #[cfg(unix)]
    #[test]
    fn test_folder_info_follows_a_symlinked_folder() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("real")).unwrap();
        fs::write(dir.path().join("real/a"), [0u8; 9]).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        let info = folder_info(&dir.path().join("link"));
        assert!(info.exists);
        assert_eq!(info.size_bytes, 9);
        assert!(is_symlink(&dir.path().join("link")));
    }

    #[test]
    fn test_remove_app_assets_removes_appdata_and_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("app-assets");
        fs::create_dir_all(assets.join("index")).unwrap();
        fs::write(assets.join("appdata.sqlite3"), [0u8; 1]).unwrap();
        fs::write(assets.join("appdata.sqlite3-wal"), [0u8; 1]).unwrap();
        fs::write(assets.join("dpd.sqlite3"), [0u8; 1]).unwrap();
        remove_app_assets(&assets).unwrap();
        assert!(!assets.try_exists().unwrap());
    }

    #[test]
    fn test_entries_are_sorted_by_name() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("dpd.sqlite3"), [0u8; 3]).unwrap();
        fs::create_dir(dir.path().join("index")).unwrap();
        fs::write(dir.path().join("appdata.sqlite3"), [0u8; 5]).unwrap();
        let names: Vec<_> = entries_of(dir.path()).into_iter().map(|e| (e.name, e.is_dir, e.size_bytes)).collect();
        assert_eq!(
            names,
            vec![
                ("appdata.sqlite3".to_string(), false, 5),
                ("dpd.sqlite3".to_string(), false, 3),
                ("index".to_string(), true, 0),
            ]
        );
    }
}
