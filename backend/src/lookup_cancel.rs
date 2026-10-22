//! Cancellation of superseded DPD lookups.
//!
//! A caller that starts lookups one after another (the WordSummary panel, the
//! Dictionary search tab) holds an `Arc<AtomicU64>` generation counter. Before
//! it starts a new lookup it bumps the counter, then takes a token for the new
//! lookup with `LookupCancel::new()`. Every earlier token now reports
//! cancelled, and the grouped lookup stops at its next check point with a
//! `LookupCancelled` error. A running SQL statement is not interrupted.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A token that tells a lookup whether a newer one has superseded it.
#[derive(Clone, Debug)]
pub struct LookupCancel {
    generation: Option<Arc<AtomicU64>>,
    started_at: u64,
}

impl LookupCancel {
    /// A token that is never cancelled, for callers that never supersede a
    /// lookup (API routes, the Gloss tab, tests).
    pub fn none() -> Self {
        LookupCancel { generation: None, started_at: 0 }
    }

    /// A token for a lookup started now. It is cancelled once `counter` moves
    /// past its current value. Bump the counter before calling this, not after.
    pub fn new(counter: &Arc<AtomicU64>) -> Self {
        LookupCancel {
            generation: Some(Arc::clone(counter)),
            // Relaxed: the value is only compared, never used to order other memory.
            started_at: counter.load(Ordering::Relaxed),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        match &self.generation {
            Some(counter) => counter.load(Ordering::Relaxed) != self.started_at,
            None => false,
        }
    }

    /// `Err(LookupCancelled)` if a newer lookup has started.
    pub fn check(&self) -> anyhow::Result<()> {
        if self.is_cancelled() {
            Err(anyhow::Error::new(LookupCancelled))
        } else {
            Ok(())
        }
    }
}

/// The error a cancelled lookup returns. It is expected, so callers log it at
/// `info` and show nothing. Test for it with `is_lookup_cancelled()`.
#[derive(Debug)]
pub struct LookupCancelled;

impl fmt::Display for LookupCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lookup cancelled: a newer lookup superseded it")
    }
}

impl std::error::Error for LookupCancelled {}

pub fn is_lookup_cancelled(e: &anyhow::Error) -> bool {
    e.downcast_ref::<LookupCancelled>().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_never_cancelled() {
        let token = LookupCancel::none();
        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());
    }

    #[test]
    fn new_is_cancelled_after_the_counter_is_bumped() {
        let counter = Arc::new(AtomicU64::new(0));
        counter.fetch_add(1, Ordering::Relaxed);
        let token = LookupCancel::new(&counter);
        assert!(!token.is_cancelled());

        counter.fetch_add(1, Ordering::Relaxed);
        assert!(token.is_cancelled());

        // The newer lookup's token is current.
        assert!(!LookupCancel::new(&counter).is_cancelled());
    }

    #[test]
    fn check_error_is_recognised() {
        let counter = Arc::new(AtomicU64::new(0));
        let token = LookupCancel::new(&counter);
        counter.fetch_add(1, Ordering::Relaxed);

        let err = token.check().unwrap_err();
        assert!(is_lookup_cancelled(&err));
        assert!(!is_lookup_cancelled(&anyhow::anyhow!("some other error")));
        // Still recognised after context is added by a caller.
        assert!(is_lookup_cancelled(&err.context("dpd_lookup_grouped")));
    }
}
