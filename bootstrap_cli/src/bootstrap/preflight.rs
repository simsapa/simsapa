//! Preflight check for `bootstrap`: every external source the selected steps
//! will read, and the local SuttaCentral ArangoDB, checked before anything is
//! deleted or written.
//!
//! Without it a missing input surfaces only when its step is reached, often
//! after an hour of work, and after `clean_and_create_folders()` has already
//! emptied `dist/`. The preflight runs before that and reports **every**
//! problem at once, so one run is enough to see what is missing.
//!
//! A source is *required* when its step stops the bootstrap without it, and
//! *optional* when the step logs a warning and skips. Optional sources are
//! listed up front too, because a skipped source is a release without that
//! content, and the warning is otherwise buried mid-log.
//!
//! **Keep [`bootstrap_sources`] in step with `bootstrap()`**: a new import that
//! reads from `bootstrap-assets-resources/` gets an entry here, built from the
//! same `source_paths` constant the step uses, never a string literal.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use simsapa_backend::logger;

use super::source_paths as paths;
use super::suttacentral;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    File,
    Dir,
}

/// One external input, as a path relative to `bootstrap-assets-resources/`,
/// built from the `source_paths` constants the import step also uses.
#[derive(Debug, Clone)]
pub struct Source {
    pub what: &'static str,
    pub rel_path: PathBuf,
    pub kind: SourceKind,
    pub required: bool,
}

fn required(what: &'static str, rel_path: impl Into<PathBuf>, kind: SourceKind) -> Source {
    Source { what, rel_path: rel_path.into(), kind, required: true }
}

fn optional(what: &'static str, rel_path: impl Into<PathBuf>, kind: SourceKind) -> Source {
    Source { what, rel_path: rel_path.into(), kind, required: false }
}

/// Which bootstrap steps will run, as decided by the command-line flags.
#[derive(Debug, Clone, Copy)]
pub struct PreflightSteps {
    pub appdata: bool,
    pub dpd: bool,
    /// The SuttaCentral per-language imports.
    pub sc_languages: bool,
    /// The Hungarian import from Buddha Ujja.
    pub hungarian: bool,
}

impl PreflightSteps {
    /// The steps `bootstrap()` runs for these command-line flags.
    /// `only_languages` holds lowercased codes.
    ///
    /// The SuttaCentral language step is selected only when `only_languages`
    /// names a code the ArangoDB list can return: `--only-languages hu` imports
    /// no SuttaCentral language, so it needs neither ArangoDB nor sc-data.
    pub fn from_flags(
        skip_appdata: bool,
        skip_dpd: bool,
        skip_languages: bool,
        only_languages: Option<&[String]>,
    ) -> Self {
        let selects = |pred: &dyn Fn(&str) -> bool| {
            only_languages.is_none_or(|langs| langs.iter().any(|l| pred(l)))
        };
        PreflightSteps {
            appdata: !skip_appdata,
            dpd: !skip_dpd,
            sc_languages: !skip_languages
                && selects(&|l| !suttacentral::LANGUAGES_NOT_IN_LIST.contains(&l)),
            hungarian: !skip_languages && selects(&|l| l == "hu"),
        }
    }

    /// ArangoDB is read by the en/pli SuttaCentral import, the SC navigation
    /// spine (titles from `names`) and the per-language imports.
    pub fn needs_arangodb(&self) -> bool {
        self.appdata || self.sc_languages
    }
}

/// The sources the selected steps read, in the order the bootstrap reads them.
pub fn bootstrap_sources(steps: &PreflightSteps) -> Vec<Source> {
    use SourceKind::{Dir, File};
    let mut sources = Vec::new();
    let sc_data = Path::new(paths::SC_DATA_DIR);
    let cst_xml = Path::new(paths::CST_XML_DIR);

    if steps.appdata || steps.sc_languages {
        // ArangoDB stores the bilara file paths, read from this checkout.
        sources.push(required("SuttaCentral sc-data checkout", sc_data.join(paths::SC_BILARA_DATA_SUBDIR), Dir));
    }

    if steps.appdata {
        sources.extend([
            required("SuttaCentral super-tree (navigation spine)", paths::sc_super_tree_file(sc_data), File),
            required("SuttaCentral blurbs (navigation spine)", sc_data.join(paths::SC_BLURBS_SUBFILE), File),
            optional("SuttaCentral parallels (navigation spine)", sc_data.join(paths::SC_PARALLELS_SUBFILE), File),
            required("CST fragments DB (OpenTipitaka export)", paths::CST_FRAGMENTS_DB, File),
            required("CST romn source XML", paths::cst_source_dir(cst_xml), Dir),
            required("CST tipitaka.org toc files", paths::cst_site_dir(cst_xml), Dir),
            required("CST tipitaka.org cscd toc files", paths::cst_toc_dir(cst_xml), Dir),
            required("dhammatalks.org suttas", paths::DHAMMATALKS_SUTTAS_DIR, Dir),
            required("suttas-core DB", paths::SUTTAS_CORE_DB, File),
            optional("Dhammapada Tipitaka.net exported DB", paths::DHAMMAPADA_TIPITAKA_NET_DB, File),
            optional("Nyanadipa translations", paths::NYANADIPA_DIR, Dir),
            optional("Dhammapada Munindo", paths::DHAMMAPADA_MUNINDO_DIR, Dir),
            optional("Library imports TOML", paths::LIBRARY_IMPORTS_TOML, File),
            optional("Library imports books folder", paths::LIBRARY_IMPORTS_BOOKS_DIR, Dir),
            optional("Chanting practice", paths::CHANTING_PRACTICE_DIR, Dir),
            optional("Gloss data bank", paths::GLOSS_DATA_CACHE_DIR, Dir),
        ]);
    }

    if steps.dpd {
        sources.extend([
            required("DPD StarDict export", paths::DPD_STARDICT_DIR, Dir),
            required("DPD database (extracted dpd.db)", paths::DPD_DB, File),
            required("DPPN database", paths::DPPN_DB, File),
        ]);
    }

    if steps.hungarian {
        sources.push(optional("Buddha Ujja DB (Hungarian)", paths::BUDDHA_UJJA_DB, File));
    }

    sources
}

/// What is wrong with a source, or `None` when it is present and of the
/// expected kind. Follows symlinks (`current/` is one).
fn source_problem(path: &Path, kind: SourceKind) -> Option<String> {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Some("not found".to_string()),
        Err(e) => return Some(format!("cannot be checked: {}", e)),
    }
    match fs::metadata(path) {
        Ok(meta) => match kind {
            SourceKind::Dir if !meta.is_dir() => Some("is not a directory".to_string()),
            SourceKind::File if !meta.is_file() => Some("is not a file".to_string()),
            _ => None,
        },
        Err(e) => Some(format!("cannot be read: {}", e)),
    }
}

#[derive(Debug, Default)]
pub struct PreflightReport {
    /// One line per required source that is missing, or per failed service.
    pub errors: Vec<String>,
    /// One line per optional source that is missing.
    pub warnings: Vec<String>,
}

pub fn check_sources(bootstrap_assets_dir: &Path, sources: &[Source]) -> PreflightReport {
    let mut report = PreflightReport::default();
    for source in sources {
        let path: PathBuf = bootstrap_assets_dir.join(&source.rel_path);
        if let Some(problem) = source_problem(&path, source.kind) {
            let line = format!("{}: {} {}", source.what, path.display(), problem);
            if source.required {
                report.errors.push(line);
            } else {
                report.warnings.push(format!("{} (step will be skipped)", line));
            }
        }
    }
    report
}

/// Check every source and service the selected steps need. Missing optional
/// sources are logged as warnings; any missing required source, or an
/// unreachable ArangoDB, is an error naming all of them.
pub fn run_preflight(bootstrap_assets_dir: &Path, steps: &PreflightSteps) -> Result<()> {
    logger::info("=== Bootstrap preflight: checking sources ===");

    let sources = bootstrap_sources(steps);
    let mut report = check_sources(bootstrap_assets_dir, &sources);

    if steps.needs_arangodb() {
        match suttacentral::connect_to_arangodb() {
            Ok(_) => logger::info("ArangoDB 'suttacentral' database at http://localhost:8529 is available"),
            Err(e) => report.errors.push(format!(
                "SuttaCentral ArangoDB: 'suttacentral' database at http://localhost:8529 is not available: {:#}",
                e
            )),
        }
    }

    for line in &report.warnings {
        logger::warn(&format!("Preflight: optional source missing: {}", line));
    }

    if report.errors.is_empty() {
        logger::info(&format!(
            "Preflight passed: {} source(s) checked, {} optional missing",
            sources.len(),
            report.warnings.len()
        ));
        return Ok(());
    }

    for line in &report.errors {
        logger::error(&format!("Preflight: {}", line));
    }
    anyhow::bail!(
        "Bootstrap preflight failed, nothing was changed. {} problem(s):\n  - {}\n\
         Provide the missing sources, or skip their steps with --skip-appdata / --skip-dpd / --skip-languages.",
        report.errors.len(),
        report.errors.join("\n  - ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_steps() -> PreflightSteps {
        PreflightSteps { appdata: true, dpd: true, sc_languages: true, hungarian: true }
    }

    fn langs(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn test_from_flags_language_selection() {
        let all = PreflightSteps::from_flags(false, false, false, None);
        assert!(all.sc_languages && all.hungarian);

        let hu = langs(&["hu"]);
        let only_hu = PreflightSteps::from_flags(true, true, false, Some(&hu));
        assert!(!only_hu.sc_languages && only_hu.hungarian);
        assert!(!only_hu.needs_arangodb());

        let de = langs(&["de"]);
        let only_de = PreflightSteps::from_flags(true, true, false, Some(&de));
        assert!(only_de.sc_languages && !only_de.hungarian);

        let skipped = PreflightSteps::from_flags(false, false, true, Some(&hu));
        assert!(!skipped.sc_languages && !skipped.hungarian);
    }

    #[test]
    fn test_skipped_steps_need_no_sources() {
        let steps = PreflightSteps { appdata: false, dpd: false, sc_languages: false, hungarian: false };
        assert!(bootstrap_sources(&steps).is_empty());
        assert!(!steps.needs_arangodb());
    }

    #[test]
    fn test_dpd_only_needs_dpd_sources_and_no_arangodb() {
        let steps = PreflightSteps { appdata: false, dpd: true, sc_languages: false, hungarian: false };
        let rel_paths: Vec<_> = bootstrap_sources(&steps).into_iter().map(|s| s.rel_path).collect();
        assert!(rel_paths.contains(&PathBuf::from(paths::DPD_DB)));
        assert!(!rel_paths.iter().any(|p| p.starts_with(paths::SC_DATA_DIR)));
        assert!(!steps.needs_arangodb());
    }

    #[test]
    fn test_languages_only_needs_sc_data_and_arangodb() {
        let steps = PreflightSteps { appdata: false, dpd: false, sc_languages: true, hungarian: false };
        let rel_paths: Vec<_> = bootstrap_sources(&steps).into_iter().map(|s| s.rel_path).collect();
        assert_eq!(rel_paths, vec![Path::new(paths::SC_DATA_DIR).join(paths::SC_BILARA_DATA_SUBDIR)]);
        assert!(steps.needs_arangodb());
    }

    #[test]
    fn test_no_source_listed_twice() {
        let sources = bootstrap_sources(&all_steps());
        let mut rel_paths: Vec<_> = sources.into_iter().map(|s| s.rel_path).collect();
        rel_paths.sort();
        let before = rel_paths.len();
        rel_paths.dedup();
        assert_eq!(before, rel_paths.len());
    }

    #[test]
    fn test_check_sources_reports_missing_and_wrong_kind() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("present-dir")).unwrap();
        fs::write(dir.path().join("present-file"), b"x").unwrap();
        fs::write(dir.path().join("file-not-dir"), b"x").unwrap();

        let sources = [
            required("a", "present-dir", SourceKind::Dir),
            required("b", "present-file", SourceKind::File),
            required("c", "missing-file", SourceKind::File),
            required("d", "file-not-dir", SourceKind::Dir),
            optional("e", "missing-optional", SourceKind::Dir),
        ];
        let report = check_sources(dir.path(), &sources);

        assert_eq!(report.errors.len(), 2, "{:?}", report.errors);
        assert!(report.errors[0].starts_with("c:") && report.errors[0].ends_with("not found"));
        assert!(report.errors[1].starts_with("d:") && report.errors[1].ends_with("is not a directory"));
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].starts_with("e:"));
    }

    /// Against the real checkout: every source the full bootstrap reads is
    /// spelled correctly. Ignored by default, because it tests the state of
    /// the local assets folder, not the code (e.g. `dpd.db` deleted after a
    /// build would fail every `cargo test`). Run it after editing the list:
    /// `cargo test test_full_bootstrap_sources -- --ignored`.
    #[test]
    #[ignore]
    fn test_full_bootstrap_sources_present_in_checkout() {
        let assets = paths::bootstrap_assets_dir_from_crate();
        if !matches!(assets.try_exists(), Ok(true)) {
            return;
        }
        let required_only: Vec<Source> =
            bootstrap_sources(&all_steps()).into_iter().filter(|s| s.required).collect();
        let report = check_sources(&assets, &required_only);
        assert!(report.errors.is_empty(), "{:#?}", report.errors);
    }
}
