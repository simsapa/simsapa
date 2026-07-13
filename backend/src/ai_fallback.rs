//! The sequential fallback/retry walk over the "Fallback sequence" model list.
//!
//! See `docs/ai-model-management-and-fallback.md`. This module holds the pure
//! decision logic — which model to try next, when to skip a provider, when to
//! start a backoff round, when to give up — with the actual network request,
//! the sleeping and the cancellation check injected as closures, so the walk
//! is unit-testable without network, Qt or clocks. The Qt-facing engine that
//! feeds it real requests lives in `bridges/src/prompt_manager.rs`.
//!
//! Walk semantics (FR-D2–D8):
//!
//! - Only `enabled` entries of the sequence are considered; the usage lists
//!   hold only models of enabled providers (see `app_data.rs` reconcile), so
//!   there is no separate provider-enabled check here.
//! - With `auto_fallback` off only the *first* enabled entry is used.
//! - A retryable error (`is_retryable()`) moves to the next model; an
//!   `auth`/`quota_exceeded` error skips every remaining model of that
//!   provider for the whole run; `model_not_found` skips that model for the
//!   whole run; any other non-retryable error fails the run immediately.
//! - When a round exhausts the sequence without success and `auto_retry` is
//!   on, the walk sleeps and re-runs: up to [`MAX_RETRY_ROUNDS`] retry rounds
//!   with [`RETRY_DELAYS_SECS`] backoff.
//! - Cancellation is checked before each model attempt and after each backoff
//!   sleep; a cancelled walk returns [`WalkOutcome::Cancelled`] and the caller
//!   emits no further signals.

use std::collections::HashSet;

use crate::ai_error::{AiErrorKind, AiRequestError};
use crate::app_settings::ModelUsageEntry;

/// Retry rounds after the initial pass (FR-D3).
pub const MAX_RETRY_ROUNDS: usize = 5;
/// Backoff before retry round 1..=5.
pub const RETRY_DELAYS_SECS: [u64; MAX_RETRY_ROUNDS] = [10, 20, 30, 40, 50];

/// The message for an empty / fully-disabled sequence (FR-D7).
pub const NO_MODELS_ENABLED_MSG: &str =
    "No models enabled for sequential fallback — check AI Models settings";

#[derive(Debug, Clone, PartialEq)]
pub enum WalkOutcome {
    /// A model answered; carries the provider/model that produced the response.
    Success {
        provider: String,
        model: String,
        response: String,
    },
    /// The walk gave up; carries the last (or only) classified error.
    Failed(AiRequestError),
    /// The run was cancelled or superseded: the caller must emit nothing.
    Cancelled,
}

/// A progress event the caller renders for the user (FR-D6).
#[derive(Debug, Clone, PartialEq)]
pub enum WalkProgress {
    /// About to send the request to this model.
    Trying { provider: String, model: String },
    /// The attempt failed; the walk continues (with the next model, or a
    /// retry round). Terminal failures are not reported here — they arrive
    /// as [`WalkOutcome::Failed`].
    AttemptFailed { error: AiRequestError },
    /// Sleeping `delay_secs` before retry round `round` (1-based).
    /// `last_error` is the failure that exhausted the previous round, so the
    /// caller can report *why* the requests are being retried.
    RetryRound {
        round: usize,
        delay_secs: u64,
        last_error: Option<AiRequestError>,
    },
}

/// Walk the fallback sequence until a model answers, the walk is cancelled,
/// or the retry rounds are exhausted.
///
/// - `attempt(provider, model)` performs one request.
/// - `on_progress` receives [`WalkProgress`] events.
/// - `sleep(secs)` blocks for the backoff delay; it returns `false` when the
///   run was cancelled during the sleep (the walk then exits silently).
/// - `is_cancelled()` is polled before each attempt.
pub fn run_fallback_walk(
    entries: &[ModelUsageEntry],
    auto_fallback: bool,
    auto_retry: bool,
    attempt: &mut dyn FnMut(&str, &str) -> Result<String, AiRequestError>,
    on_progress: &mut dyn FnMut(WalkProgress),
    sleep: &mut dyn FnMut(u64) -> bool,
    is_cancelled: &mut dyn FnMut() -> bool,
) -> WalkOutcome {
    let enabled: Vec<&ModelUsageEntry> = entries.iter().filter(|e| e.enabled).collect();
    let candidates: &[&ModelUsageEntry] = if auto_fallback {
        &enabled
    } else {
        // Auto-fallback off: only the first enabled model is used (FR-D4);
        // auto-retry then re-tries that same model on the same schedule.
        &enabled[..enabled.len().min(1)]
    };

    if candidates.is_empty() {
        return WalkOutcome::Failed(AiRequestError::new(
            AiErrorKind::InvalidRequest,
            "",
            "",
            NO_MODELS_ENABLED_MSG,
        ));
    }

    // Skips persist across retry rounds: an invalid key or an exhausted hard
    // quota (skips_provider) and a 404 model (model_not_found) would repeat.
    let mut skipped_providers: HashSet<String> = HashSet::new();
    let mut skipped_models: HashSet<(String, String)> = HashSet::new();
    let mut last_error: Option<AiRequestError> = None;

    for round in 0..=MAX_RETRY_ROUNDS {
        let remaining = candidates.iter().any(|entry| {
            !skipped_providers.contains(&entry.provider)
                && !skipped_models.contains(&(entry.provider.clone(), entry.model_name.clone()))
        });
        if !remaining {
            // Every candidate is permanently skipped; retry rounds cannot help.
            break;
        }

        if round > 0 {
            if !auto_retry {
                break;
            }
            let delay_secs = RETRY_DELAYS_SECS[round - 1];
            on_progress(WalkProgress::RetryRound {
                round,
                delay_secs,
                last_error: last_error.clone(),
            });
            if !sleep(delay_secs) {
                return WalkOutcome::Cancelled;
            }
        }

        for entry in candidates {
            if skipped_providers.contains(&entry.provider)
                || skipped_models.contains(&(entry.provider.clone(), entry.model_name.clone()))
            {
                continue;
            }

            if is_cancelled() {
                return WalkOutcome::Cancelled;
            }

            on_progress(WalkProgress::Trying {
                provider: entry.provider.clone(),
                model: entry.model_name.clone(),
            });

            match attempt(&entry.provider, &entry.model_name) {
                Ok(response) => {
                    return WalkOutcome::Success {
                        provider: entry.provider.clone(),
                        model: entry.model_name.clone(),
                        response,
                    };
                }
                Err(err) => {
                    if err.skips_provider() {
                        skipped_providers.insert(entry.provider.clone());
                    } else if err.kind == AiErrorKind::ModelNotFound {
                        skipped_models
                            .insert((entry.provider.clone(), entry.model_name.clone()));
                    } else if !err.is_retryable() {
                        // Non-retryable and not a skip: report immediately (FR-D5).
                        return WalkOutcome::Failed(err);
                    }
                    on_progress(WalkProgress::AttemptFailed { error: err.clone() });
                    last_error = Some(err);
                }
            }
        }
    }

    WalkOutcome::Failed(last_error.unwrap_or_else(|| {
        AiRequestError::new(AiErrorKind::InvalidRequest, "", "", NO_MODELS_ENABLED_MSG)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(provider: &str, model: &str, enabled: bool) -> ModelUsageEntry {
        ModelUsageEntry {
            provider: provider.to_string(),
            model_name: model.to_string(),
            enabled,
        }
    }

    fn err(kind: AiErrorKind, provider: &str, model: &str) -> AiRequestError {
        AiRequestError::new(kind, provider, model, format!("{:?}", kind))
    }

    /// Drives the walk with a scripted list of per-attempt results, recording
    /// the (provider, model) order of attempts and the sleeps taken.
    struct Script {
        results: Vec<Result<String, AiRequestError>>,
        attempts: Vec<(String, String)>,
        sleeps: Vec<u64>,
        progress: Vec<WalkProgress>,
        cancel_after_attempts: Option<usize>,
        cancel_during_sleep: bool,
    }

    impl Script {
        fn new(results: Vec<Result<String, AiRequestError>>) -> Self {
            Script {
                results,
                attempts: Vec::new(),
                sleeps: Vec::new(),
                progress: Vec::new(),
                cancel_after_attempts: None,
                cancel_during_sleep: false,
            }
        }

        fn run(
            &mut self,
            entries: &[ModelUsageEntry],
            auto_fallback: bool,
            auto_retry: bool,
        ) -> WalkOutcome {
            let mut results = std::mem::take(&mut self.results).into_iter();
            let mut attempts: Vec<(String, String)> = Vec::new();
            let attempt_count = std::cell::Cell::new(0usize);
            let mut sleeps: Vec<u64> = Vec::new();
            let mut progress: Vec<WalkProgress> = Vec::new();
            let cancel_after = self.cancel_after_attempts;
            let cancel_during_sleep = self.cancel_during_sleep;

            let outcome = run_fallback_walk(
                entries,
                auto_fallback,
                auto_retry,
                &mut |provider, model| {
                    attempts.push((provider.to_string(), model.to_string()));
                    attempt_count.set(attempt_count.get() + 1);
                    results.next().expect("script ran out of attempt results")
                },
                &mut |p| progress.push(p),
                &mut |secs| {
                    sleeps.push(secs);
                    !cancel_during_sleep
                },
                &mut || cancel_after.is_some_and(|n| attempt_count.get() >= n),
            );

            self.attempts = attempts;
            self.sleeps = sleeps;
            self.progress = progress;
            outcome
        }
    }

    fn attempted_models(script: &Script) -> Vec<&str> {
        script.attempts.iter().map(|(_, m)| m.as_str()).collect()
    }

    #[test]
    fn success_on_first_model_stops_the_walk() {
        let entries = [entry("Gemini", "g1", true), entry("Mistral", "m1", true)];
        let mut s = Script::new(vec![Ok("answer".into())]);
        let outcome = s.run(&entries, true, true);
        assert_eq!(
            outcome,
            WalkOutcome::Success {
                provider: "Gemini".into(),
                model: "g1".into(),
                response: "answer".into()
            }
        );
        assert_eq!(attempted_models(&s), ["g1"]);
        assert!(s.sleeps.is_empty());
    }

    #[test]
    fn retryable_error_falls_back_to_next_model_in_order() {
        let entries = [
            entry("Gemini", "g1", true),
            entry("Gemini", "g2", true),
            entry("Mistral", "m1", true),
        ];
        let mut s = Script::new(vec![
            Err(err(AiErrorKind::RateLimited, "Gemini", "g1")),
            Err(err(AiErrorKind::Overloaded, "Gemini", "g2")),
            Ok("answer".into()),
        ]);
        let outcome = s.run(&entries, true, true);
        assert!(matches!(outcome, WalkOutcome::Success { ref model, .. } if model == "m1"));
        assert_eq!(attempted_models(&s), ["g1", "g2", "m1"]);
    }

    #[test]
    fn disabled_entries_are_not_attempted() {
        let entries = [
            entry("Gemini", "g1", false),
            entry("Mistral", "m1", true),
        ];
        let mut s = Script::new(vec![Ok("answer".into())]);
        let outcome = s.run(&entries, true, true);
        assert!(matches!(outcome, WalkOutcome::Success { ref model, .. } if model == "m1"));
        assert_eq!(attempted_models(&s), ["m1"]);
    }

    #[test]
    fn auth_error_skips_remaining_models_of_that_provider() {
        let entries = [
            entry("Gemini", "g1", true),
            entry("Gemini", "g2", true),
            entry("Mistral", "m1", true),
        ];
        let mut s = Script::new(vec![
            Err(err(AiErrorKind::Auth, "Gemini", "g1")),
            Ok("answer".into()),
        ]);
        let outcome = s.run(&entries, true, true);
        assert!(matches!(outcome, WalkOutcome::Success { ref model, .. } if model == "m1"));
        // g2 must not be attempted: the bad key would repeat.
        assert_eq!(attempted_models(&s), ["g1", "m1"]);
    }

    #[test]
    fn quota_exceeded_skips_provider_and_persists_across_retry_rounds() {
        let entries = [
            entry("OpenAI", "o1", true),
            entry("Gemini", "g1", true),
        ];
        let mut s = Script::new(vec![
            Err(err(AiErrorKind::QuotaExceeded, "OpenAI", "o1")),
            Err(err(AiErrorKind::RateLimited, "Gemini", "g1")),
            // Retry round 1: only g1 remains.
            Ok("answer".into()),
        ]);
        let outcome = s.run(&entries, true, true);
        assert!(matches!(outcome, WalkOutcome::Success { ref model, .. } if model == "g1"));
        assert_eq!(attempted_models(&s), ["o1", "g1", "g1"]);
        assert_eq!(s.sleeps, [10]);
    }

    #[test]
    fn model_not_found_skips_only_that_model() {
        let entries = [
            entry("Gemini", "g1", true),
            entry("Gemini", "g2", true),
        ];
        let mut s = Script::new(vec![
            Err(err(AiErrorKind::ModelNotFound, "Gemini", "g1")),
            Ok("answer".into()),
        ]);
        let outcome = s.run(&entries, true, true);
        assert!(matches!(outcome, WalkOutcome::Success { ref model, .. } if model == "g2"));
        assert_eq!(attempted_models(&s), ["g1", "g2"]);
    }

    #[test]
    fn non_retryable_error_fails_immediately() {
        let entries = [
            entry("Gemini", "g1", true),
            entry("Mistral", "m1", true),
        ];
        let mut s = Script::new(vec![Err(err(AiErrorKind::InvalidRequest, "Gemini", "g1"))]);
        let outcome = s.run(&entries, true, true);
        match outcome {
            WalkOutcome::Failed(e) => assert_eq!(e.kind, AiErrorKind::InvalidRequest),
            other => panic!("expected Failed, got {:?}", other),
        }
        assert_eq!(attempted_models(&s), ["g1"]);
        assert!(s.sleeps.is_empty());
    }

    #[test]
    fn exhausted_sequence_retries_with_backoff_schedule() {
        let entries = [entry("Gemini", "g1", true)];
        // 1 initial + 5 retry rounds, all rate limited.
        let results = (0..6)
            .map(|_| Err(err(AiErrorKind::RateLimited, "Gemini", "g1")))
            .collect();
        let mut s = Script::new(results);
        let outcome = s.run(&entries, true, true);
        match outcome {
            WalkOutcome::Failed(e) => assert_eq!(e.kind, AiErrorKind::RateLimited),
            other => panic!("expected Failed, got {:?}", other),
        }
        assert_eq!(attempted_models(&s), ["g1"; 6]);
        assert_eq!(s.sleeps, RETRY_DELAYS_SECS);
    }

    #[test]
    fn auto_retry_off_fails_after_one_round() {
        let entries = [entry("Gemini", "g1", true), entry("Mistral", "m1", true)];
        let mut s = Script::new(vec![
            Err(err(AiErrorKind::RateLimited, "Gemini", "g1")),
            Err(err(AiErrorKind::RateLimited, "Mistral", "m1")),
        ]);
        let outcome = s.run(&entries, true, false);
        match outcome {
            WalkOutcome::Failed(e) => {
                assert_eq!(e.kind, AiErrorKind::RateLimited);
                assert_eq!(e.provider, "Mistral");
            }
            other => panic!("expected Failed, got {:?}", other),
        }
        assert!(s.sleeps.is_empty());
    }

    #[test]
    fn auto_fallback_off_uses_only_first_enabled_model() {
        let entries = [
            entry("Gemini", "g0", false),
            entry("Gemini", "g1", true),
            entry("Mistral", "m1", true),
        ];
        let mut s = Script::new(vec![
            Err(err(AiErrorKind::RateLimited, "Gemini", "g1")),
            // Retry round 1 re-tries the same model.
            Ok("answer".into()),
        ]);
        let outcome = s.run(&entries, false, true);
        assert!(matches!(outcome, WalkOutcome::Success { ref model, .. } if model == "g1"));
        assert_eq!(attempted_models(&s), ["g1", "g1"]);
        assert_eq!(s.sleeps, [10]);
    }

    #[test]
    fn empty_or_all_disabled_sequence_fails_fast() {
        let mut s = Script::new(vec![]);
        let outcome = s.run(&[], true, true);
        match outcome {
            WalkOutcome::Failed(e) => assert_eq!(e.message, NO_MODELS_ENABLED_MSG),
            other => panic!("expected Failed, got {:?}", other),
        }

        let entries = [entry("Gemini", "g1", false)];
        let mut s = Script::new(vec![]);
        match s.run(&entries, true, true) {
            WalkOutcome::Failed(e) => assert_eq!(e.message, NO_MODELS_ENABLED_MSG),
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn all_models_permanently_skipped_fails_without_retry_rounds() {
        let entries = [
            entry("Gemini", "g1", true),
            entry("Gemini", "g2", true),
        ];
        let mut s = Script::new(vec![Err(err(AiErrorKind::Auth, "Gemini", "g1"))]);
        let outcome = s.run(&entries, true, true);
        match outcome {
            WalkOutcome::Failed(e) => assert_eq!(e.kind, AiErrorKind::Auth),
            other => panic!("expected Failed, got {:?}", other),
        }
        // No pointless backoff sleeps when nothing is left to try.
        assert!(s.sleeps.is_empty());
        assert_eq!(attempted_models(&s), ["g1"]);
    }

    #[test]
    fn cancel_before_attempt_stops_the_walk_silently() {
        let entries = [entry("Gemini", "g1", true), entry("Mistral", "m1", true)];
        let mut s = Script::new(vec![Err(err(AiErrorKind::RateLimited, "Gemini", "g1"))]);
        s.cancel_after_attempts = Some(1);
        let outcome = s.run(&entries, true, true);
        assert_eq!(outcome, WalkOutcome::Cancelled);
        assert_eq!(attempted_models(&s), ["g1"]);
    }

    #[test]
    fn cancel_during_backoff_sleep_stops_the_walk() {
        let entries = [entry("Gemini", "g1", true)];
        let mut s = Script::new(vec![Err(err(AiErrorKind::RateLimited, "Gemini", "g1"))]);
        s.cancel_during_sleep = true;
        let outcome = s.run(&entries, true, true);
        assert_eq!(outcome, WalkOutcome::Cancelled);
        assert_eq!(s.sleeps, [10]);
    }

    #[test]
    fn progress_events_report_trying_failures_and_retry_rounds() {
        let entries = [entry("Gemini", "g1", true), entry("Mistral", "m1", true)];
        let mut s = Script::new(vec![
            Err(err(AiErrorKind::RateLimited, "Gemini", "g1")),
            Err(err(AiErrorKind::RateLimited, "Mistral", "m1")),
            Ok("answer".into()),
        ]);
        let outcome = s.run(&entries, true, true);
        assert!(matches!(outcome, WalkOutcome::Success { ref model, .. } if model == "g1"));

        let kinds: Vec<&str> = s
            .progress
            .iter()
            .map(|p| match p {
                WalkProgress::Trying { .. } => "trying",
                WalkProgress::AttemptFailed { .. } => "failed",
                WalkProgress::RetryRound { .. } => "retry",
            })
            .collect();
        assert_eq!(
            kinds,
            ["trying", "failed", "trying", "failed", "retry", "trying"]
        );
        match &s.progress[4] {
            WalkProgress::RetryRound { round: 1, delay_secs: 10, last_error: Some(e) } => {
                // The reason for the retry round is carried along.
                assert_eq!(e.kind, AiErrorKind::RateLimited);
                assert_eq!(e.provider, "Mistral");
            }
            other => panic!("expected RetryRound with last_error, got {:?}", other),
        }
    }
}
