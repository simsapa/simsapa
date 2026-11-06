//! Where each bootstrap input lives under `bootstrap-assets-resources/`.
//!
//! The single place these paths are written: the import steps, the CLI
//! subcommands that re-run one step, the preflight check
//! (`preflight::bootstrap_sources()`) and the tests that read the real
//! checkout all build their paths from these, so the preflight checks exactly
//! the path the step reads.
//!
//! Constants named `*_DIR` / `*_FILE` / `*_DB` without `SUB` are relative to
//! the bootstrap assets folder; `*_SUBDIR` / `*_SUBFILE` ones are relative to
//! the source folder named in their doc comment, because the CLI subcommands
//! take that folder as an argument.

use std::path::{Path, PathBuf};

/// The bootstrap assets folder, relative to the `bootstrap_cli/` crate folder
/// (the working directory of `cargo run` and `cargo test`).
pub const BOOTSTRAP_ASSETS_DIR_FROM_CRATE: &str = "../../bootstrap-assets-resources";

/// The bootstrap assets folder, resolved from this crate's manifest folder.
/// For tests and CLI defaults; `bootstrap()` resolves it from the working
/// directory instead.
pub fn bootstrap_assets_dir_from_crate() -> PathBuf {
    simsapa_backend::normalize_lexically(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(BOOTSTRAP_ASSETS_DIR_FROM_CRATE))
}

// === SuttaCentral ===

/// The local `sc-data` checkout.
pub const SC_DATA_DIR: &str = "sc-data";
/// Under [`SC_DATA_DIR`]: the bilara texts, whose paths ArangoDB stores.
pub const SC_BILARA_DATA_SUBDIR: &str = "sc_bilara_data";
/// Under [`SC_DATA_DIR`]: the tree files, `<root>/<collection>-tree.json`.
pub const SC_TREE_SUBDIR: &str = "structure/tree";
/// Under [`SC_TREE_SUBDIR`]: the navigation spine's roots and grouping levels.
pub const SC_SUPER_TREE_FILE_NAME: &str = "super-tree.json";
/// Under [`SC_DATA_DIR`]: the navigation spine's blurbs.
pub const SC_BLURBS_SUBFILE: &str = "additional-info/blurbs.json";
/// Under [`SC_DATA_DIR`]: parallels, for textless Pātimokkha leaves.
pub const SC_PARALLELS_SUBFILE: &str = "relationship/parallels.json";

pub fn sc_super_tree_file(sc_data_dir: &Path) -> PathBuf {
    sc_data_dir.join(SC_TREE_SUBDIR).join(SC_SUPER_TREE_FILE_NAME)
}

// === CST (tipitaka.org) ===

/// The tipitaka.org CST checkout, the `--cst-xml-dir` of the CLI.
pub const CST_XML_DIR: &str = "tipitaka-org-vri-cst/tipitaka-xml";
/// Under [`CST_XML_DIR`]: the `romn/` source XML files.
pub const CST_SOURCE_SUBDIR: &str = "romn";
/// Under [`CST_XML_DIR`]: the tipitaka.org site folder, holding `toc1`–`toc4`.
pub const CST_SITE_SUBDIR: &str = "tipitaka.org/romn";
/// Under [`CST_SITE_SUBDIR`]: one `<name>.toc.xml` per source file.
pub const CST_TOC_SUBDIR: &str = "cscd";
/// The OpenTipitaka export written by tipitaka-xml-parser.
pub const CST_FRAGMENTS_DB: &str = "tipitaka-xml-data/fragments.sqlite3";

pub fn cst_source_dir(cst_xml_dir: &Path) -> PathBuf {
    cst_xml_dir.join(CST_SOURCE_SUBDIR)
}

pub fn cst_site_dir(cst_xml_dir: &Path) -> PathBuf {
    cst_xml_dir.join(CST_SITE_SUBDIR)
}

pub fn cst_toc_dir(cst_xml_dir: &Path) -> PathBuf {
    cst_site_dir(cst_xml_dir).join(CST_TOC_SUBDIR)
}

/// The fragments DB of the bootstrap assets folder that holds `cst_xml_dir`
/// at [`CST_XML_DIR`]: the CLI's default when `--fragments-db` is not given.
pub fn cst_fragments_db_beside(cst_xml_dir: &Path) -> PathBuf {
    let mut assets_dir = cst_xml_dir.to_path_buf();
    for _ in Path::new(CST_XML_DIR).components() {
        assets_dir.push("..");
    }
    assets_dir.join(CST_FRAGMENTS_DB)
}

// === Translations ===

/// dhammatalks.org mirror (Ṭhānissaro).
pub const DHAMMATALKS_SUTTAS_DIR: &str = "dhammatalks-org/www.dhammatalks.org/suttas";
/// suttas-core; `current` is a symlink to a dated folder.
pub const SUTTAS_CORE_DB: &str = "suttas-core-db/current/suttas.sqlite";
/// Written by the `dhammapada-tipitaka-net-export` command.
pub const DHAMMAPADA_TIPITAKA_NET_DB: &str = "dhammapada-tipitaka-net/dhammapada-tipitaka-net.sqlite3";
pub const NYANADIPA_DIR: &str = "nyanadipa-translations";
pub const DHAMMAPADA_MUNINDO_DIR: &str = "dhammapada-munindo";
/// Hungarian translations.
pub const BUDDHA_UJJA_DB: &str = "buddha-ujja-sql/bu.sqlite3";

// === Library and chanting ===

pub const LIBRARY_IMPORTS_TOML: &str = "library-imports/library-imports.toml";
pub const LIBRARY_IMPORTS_BOOKS_DIR: &str = "library-imports/books";
pub const CHANTING_PRACTICE_DIR: &str = "chanting-practice";
/// Under [`CHANTING_PRACTICE_DIR`] (or the CLI's `--data-dir`).
pub const CHANTING_PRACTICE_TOML_SUBFILE: &str = "chanting-practice.toml";

// === Gloss data bank ===

/// Exported gloss session JSONs, imported as built-in word-selection rows.
pub const GLOSS_DATA_CACHE_DIR: &str = "gloss-data-cache";
/// Under [`GLOSS_DATA_CACHE_DIR`]: sessions confirmed in the Gloss UI.
pub const GLOSS_HUMAN_CHECKED_SUBDIR: &str = "human-checked";
/// Under [`GLOSS_DATA_CACHE_DIR`]: sessions confirmed by `gloss-agent-check`.
pub const GLOSS_AGENT_CHECKED_SUBDIR: &str = "agent-checked";
/// Under [`GLOSS_DATA_CACHE_DIR`]: unreviewed `gloss-corpus-explore` output,
/// never imported.
pub const GLOSS_CANDIDATES_SUBDIR: &str = "candidates";
/// The subfolders `import-gloss-data` scans besides the top level. The
/// bootstrap's "anything to import" check uses the same list.
pub const GLOSS_CHECKED_SUBDIRS: [&str; 2] = [GLOSS_HUMAN_CHECKED_SUBDIR, GLOSS_AGENT_CHECKED_SUBDIR];

/// The CLI default for the gloss commands' data-cache folder.
pub fn gloss_data_cache_dir_from_crate() -> PathBuf {
    bootstrap_assets_dir_from_crate().join(GLOSS_DATA_CACHE_DIR)
}

// === Dictionaries ===

/// The DPD StarDict export.
pub const DPD_STARDICT_DIR: &str = "dpd-db-for-bootstrap/current/dpd";
/// The extracted `dpd.db` (from `dpd.db.tar.bz2`).
pub const DPD_DB: &str = "dpd-db-for-bootstrap/current/dpd.db";
pub const DPPN_DB: &str = "dppn-anandajoti/dppn.sqlite3";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cst_fragments_db_beside_matches_assets_layout() {
        let assets = Path::new("/x/assets");
        let beside = cst_fragments_db_beside(&assets.join(CST_XML_DIR));
        assert_eq!(
            simsapa_backend::normalize_lexically(&beside),
            assets.join(CST_FRAGMENTS_DB)
        );
    }
}
