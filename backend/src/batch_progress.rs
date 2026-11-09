//! Progress and cancel for the long batched steps of the DPD import (the
//! StarDict parse, the HTML conversions, the bold-definition and
//! construction-summary columns, the niggahīta pass).
//!
//! The bootstrap passes [`BatchProgress::none()`]. The DPD upgrade pipeline
//! passes a reporter, so a step of several minutes shows rows done instead of
//! looking like a hang, and a cancel flag checked between batches.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// The error a step returns when its cancel flag was set between batches.
/// Callers tell it from a failure with `anyhow::Error::is::<Cancelled>()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// A progress reporter and a cancel flag, both optional.
#[derive(Clone, Copy, Default)]
pub struct BatchProgress<'a> {
    report: Option<&'a dyn Fn(u64, u64)>,
    cancel: Option<&'a AtomicBool>,
}

impl<'a> BatchProgress<'a> {
    /// No reporting, never cancelled.
    pub fn none() -> Self {
        Self::default()
    }

    /// `report(done, total)`: `total == 0` means the size is unknown.
    pub fn new(report: &'a dyn Fn(u64, u64), cancel: &'a AtomicBool) -> Self {
        Self { report: Some(report), cancel: Some(cancel) }
    }

    pub fn report(&self, done: u64, total: u64) {
        if let Some(f) = self.report {
            f(done, total);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    /// `Err(Cancelled)` when the cancel flag is set.
    pub fn check_cancel(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() { Err(Cancelled) } else { Ok(()) }
    }

    /// The cancel flag itself, for code that takes a plain `&AtomicBool`.
    pub fn cancel_flag(&self) -> Option<&'a AtomicBool> {
        self.cancel
    }
}
