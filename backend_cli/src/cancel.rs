//! Ctrl-C for the commands whose work checks a cancel flag (`setup`, the
//! StarDict imports). Never for `serve`: Rocket handles SIGINT itself.
//!
//! The first Ctrl-C sets the flag, and the work stops at its next check point
//! and cleans up. Some steps have no check point (the dictionary-index update
//! after an import), so a second Ctrl-C exits at once, as an unhandled one
//! would.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::exit::{CliError, INTERRUPTED};

pub fn install_ctrlc_handler() -> Result<Arc<AtomicBool>, CliError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    ctrlc::set_handler(move || {
        if flag.swap(true, Ordering::SeqCst) {
            std::process::exit(INTERRUPTED);
        }
        eprintln!("\nStopping… (press Ctrl-C again to quit immediately)");
    })
    .map_err(|e| CliError::error(format!("Cannot install the Ctrl-C handler: {}", e)))?;
    Ok(cancel)
}
