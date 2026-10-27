//! Exit codes and the one place a command's result is printed and turned into
//! the process exit status.

use std::fmt;

use simsapa_backend::errln;

use crate::output::{outln, stdout_write_failed};

/// The command succeeded.
pub const OK: i32 = 0;
/// Any error without a code of its own, including a command-line usage error.
pub const ERROR: i32 = 1;
/// The databases are not set up in the data folder.
pub const DATABASES_MISSING: i32 = 2;
/// The data folder is in use by the GUI app or another `simsapa` process.
pub const IN_USE: i32 = 3;
/// An unknown uid or reference.
pub const NOT_FOUND: i32 = 4;
/// Stopped by Ctrl-C.
pub const INTERRUPTED: i32 = 130;

/// The text `--help` ends with.
pub const EXIT_CODES_HELP: &str = "\
Exit codes:
  0    success
  1    error
  2    the databases are not set up (run `simsapa setup`)
  3    the data folder is in use by the Simsapa app or another simsapa command
  4    not found (unknown uid)
  130  interrupted";

#[derive(Debug)]
pub struct CliError {
    pub code: i32,
    pub message: String,
    /// The command has already printed its result, which carries the error;
    /// `finish()` prints nothing more.
    pub reported: bool,
}

pub type CliResult = Result<(), CliError>;

impl CliError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        CliError { code, message: message.into(), reported: false }
    }

    /// Exit with `code` after the command printed its own result (in
    /// `--json` mode a document of its own, which an `{"error": …}` document
    /// would follow as a second one).
    pub fn reported(code: i32) -> Self {
        CliError { code, message: String::new(), reported: true }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(ERROR, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(NOT_FOUND, message)
    }

    pub fn interrupted_with(message: impl Into<String>) -> Self {
        Self::new(INTERRUPTED, message)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        CliError::error(format!("{:#}", e))
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::error(e.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::error(e.to_string())
    }
}

/// The `--json` error document.
pub fn json_error_document(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

/// Print a failed result and exit with its code. With `--json` the error is
/// `{"error": "<message>"}` on stdout, so stdout always holds one JSON
/// document; otherwise the message goes to stderr. A command that succeeded
/// but could not write its output (other than to a closed pipe, see
/// `output::write_stdout`) exits 1.
pub fn finish(result: CliResult, json: bool) -> ! {
    match result {
        Ok(()) if stdout_write_failed() => std::process::exit(ERROR),
        Ok(()) => std::process::exit(OK),
        Err(e) if e.reported => std::process::exit(e.code),
        Err(e) => {
            if json {
                outln!("{}", json_error_document(&e.message));
            } else if e.code == ERROR {
                errln!("Error: {}", e.message);
            } else {
                errln!("{}", e.message);
            }
            std::process::exit(e.code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_error_document_shape() {
        let doc: serde_json::Value = serde_json::from_str(&json_error_document("a \"quoted\" message")).unwrap();
        assert_eq!(doc, serde_json::json!({ "error": "a \"quoted\" message" }));
    }

    #[test]
    fn test_anyhow_error_keeps_context_chain() {
        let e = anyhow::anyhow!("inner").context("outer");
        let c = CliError::from(e);
        assert_eq!(c.code, ERROR);
        assert_eq!(c.message, "outer: inner");
    }
}
