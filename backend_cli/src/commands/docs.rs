//! `simsapa docs`: print or save the CLI and API reference,
//! `docs/simsapa-cli-and-localhost-api.md`, compiled into the binary so an
//! agent reads the reference that matches the installed version. Works without
//! databases.

use std::path::PathBuf;

use clap::Args;
use serde::Serialize;

use crate::exit::{CliError, CliResult};
use crate::output::{out, outln, print_json};

pub const DOC_FILE_NAME: &str = "simsapa-cli-and-localhost-api.md";
pub const DOC_MARKDOWN: &str = include_str!("../../../docs/simsapa-cli-and-localhost-api.md");

#[derive(Args, Debug)]
pub struct DocsArgs {
    /// Save the reference to this file (overwriting it) instead of printing it
    #[arg(long, short = 'o', value_name = "FILE_PATH")]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct DocsSaved {
    name: &'static str,
    path: String,
    bytes: usize,
}

#[derive(Debug, Serialize)]
struct DocsPrinted {
    name: &'static str,
    markdown: &'static str,
}

pub fn run(args: &DocsArgs, json: bool) -> CliResult {
    match &args.output {
        Some(path) => {
            std::fs::write(path, DOC_MARKDOWN)
                .map_err(|e| CliError::error(format!("Could not write {}: {}", path.display(), e)))?;
            if json {
                print_json(&DocsSaved {
                    name: DOC_FILE_NAME,
                    path: path.to_string_lossy().into_owned(),
                    bytes: DOC_MARKDOWN.len(),
                })
            } else {
                outln!("Saved {} to {}", DOC_FILE_NAME, path.display());
                Ok(())
            }
        }
        None => {
            if json {
                print_json(&DocsPrinted { name: DOC_FILE_NAME, markdown: DOC_MARKDOWN })
            } else {
                out!("{}", DOC_MARKDOWN);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled doc is read by agents outside this repository, so it must
    /// not point into it. Implementation notes belong in
    /// docs/localhost-api-implementation.md.
    #[test]
    fn test_bundled_doc_has_no_repo_references() {
        let needles = [
            "](./", "](../", "CLAUDE.md", "AGENTS.md", "PROJECT_MAP",
            "backend/src", "api/src", "bridges/src", "backend_cli/", "bootstrap_cli/",
            "cpp/", "src-ts/", "tasks/", "docs/", ".rs`", ".qml`", ".cpp`",
        ];
        let mut found = Vec::new();
        for (n, line) in DOC_MARKDOWN.lines().enumerate() {
            for needle in needles {
                if line.contains(needle) {
                    found.push(format!("line {}: {:?} in: {}", n + 1, needle, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "repo references in {}:\n{}", DOC_FILE_NAME, found.join("\n"));
    }
}
