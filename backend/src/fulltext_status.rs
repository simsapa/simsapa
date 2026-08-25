//! Honest reporting of whether fulltext search actually works right now.
//!
//! One reporting user's storage-diagnostics run said **0 sutta / 0 dictionary /
//! 0 library** indexes open with six recorded failures, while the very same run
//! opened all six through [`crate::search::lenient_directory`] and searched them
//! successfully. The app called itself *"Fulltext searcher initialized"*
//! throughout, `/health` reported `fulltext_searcher_ready: true`, and every
//! search the user ran came back silently empty. Nothing in the product said
//! otherwise.
//!
//! This module is the single place that turns the two facts already recorded at
//! searcher-open time — the per-area counts (`FulltextIndexCounts`) and the
//! per-directory failures (`crate::searcher_open_failures`) — into one verdict
//! and one sentence. The search UI's empty state, the Database Validation row
//! and the `/health` route all read it, so they cannot disagree with each other
//! or drift apart.
//!
//! See `docs/fulltext-index-storage-and-file-locking.md`.
//!
//! ## The wording rules, which are requirements and not style
//!
//! No user-facing string here may contain `flock`, `ENOSYS`, `Tantivy`,
//! `META_LOCK`, `mmap` or `FUSE`, and none may say **"SD card"**: the only
//! affected volume ever actually measured is a ChromeOS/ARCVM external volume
//! over FUSE, and the one before that was a portable SD card. They are two
//! instances of one class, so the user-facing term is *"this storage location"*.
//! `no_jargon_in_user_facing_strings` pins this.
//!
//! Raw error text is deliberately **not** passed through to the user: a tantivy
//! lock error reads `Failed to acquire Lockfile: …/.tantivy-meta.lock … code:
//! 38 … Function not implemented`, which is every banned word at once. The raw
//! strings stay in the log and in the storage diagnostics, where they belong.

use crate::search::searcher::FulltextIndexCounts;

/// What to tell the user about fulltext search, and whether it is working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FulltextStatus {
    /// Whether fulltext search can return results at all.
    pub is_valid: bool,
    /// One or two sentences, in plain language. Safe to show anywhere.
    pub message: String,
    /// The distinguishing state, for callers that want to branch rather than
    /// print. This is the `StartupDbReport` principle: "the files are not there"
    /// and "the files are there and would not open" are different diagnoses and
    /// must never be conflated.
    pub state: FulltextState,
    pub counts: FulltextIndexCounts,
    /// How many index directories failed to open. Zero in every state except
    /// [`FulltextState::CouldNotOpen`] (and even there it can be zero if the
    /// directories held no per-language subdirectories at all).
    pub failure_count: usize,
}

/// The four distinguishable states, in the order they are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FulltextState {
    /// No searcher has been built this session. "Not measured", never "no
    /// failures" — the failure list is only ever written while a searcher is
    /// being built.
    NotOpenedYet,
    /// No index directory exists. The user has not built or downloaded an
    /// index; nothing is broken.
    FilesNotFound,
    /// Index directories exist, but not one index could be opened. This is the
    /// state the whole fix exists for.
    CouldNotOpen,
    /// At least one index is open and searchable.
    Ready,
}

/// Classify one recorded open failure into plain language.
///
/// The input is a tantivy error string. Only the *shape* of it is used; the
/// text itself never reaches the user (see the module docs).
fn plain_reason_for(error: &str) -> &'static str {
    let lowered = error.to_lowercase();

    if lowered.contains("lock") {
        "This storage location does not support the file locking the search index needs."
    } else if lowered.contains("permission") || lowered.contains("denied") {
        "Simsapa does not have permission to read the search index files."
    } else if lowered.contains("no such file") || lowered.contains("not found") {
        "Some search index files are missing."
    } else if lowered.contains("space") {
        "There is not enough free space on this storage location."
    } else {
        "The search index files could not be read from this storage location."
    }
}

/// The single plain-language reason for a set of failures.
///
/// When several directories failed for the same reason — which is the normal
/// case, since they share one volume — say it once. When they genuinely differ,
/// fall back to the generic sentence rather than listing six variations of the
/// same sentence at the user.
fn plain_reason_for_all(failures: &[(String, String)]) -> &'static str {
    let mut reasons = failures.iter().map(|(_path, error)| plain_reason_for(error));

    match reasons.next() {
        None => "The search index could not be opened.",
        Some(first) => {
            if reasons.all(|r| r == first) {
                first
            } else {
                "The search index files could not be read from this storage location."
            }
        }
    }
}

/// Render the counts the way the Database Validation row wants them, e.g.
/// `"3 sutta, 2 dictionary, 1 library index"`. Areas holding nothing are
/// omitted, so a user with only suttas downloaded does not read `0 dictionary`
/// as a fault.
fn describe_counts(counts: &FulltextIndexCounts) -> String {
    let mut parts: Vec<String> = Vec::new();
    if counts.sutta.opened > 0 {
        parts.push(format!("{} sutta", counts.sutta.opened));
    }
    if counts.dict.opened > 0 {
        parts.push(format!("{} dictionary", counts.dict.opened));
    }
    if counts.library.opened > 0 {
        parts.push(format!("{} library", counts.library.opened));
    }

    let total: usize = counts.total_opened();
    let noun = if total == 1 { "index" } else { "indexes" };

    if parts.is_empty() {
        return format!("0 {noun}");
    }
    format!("{} {}", parts.join(", "), noun)
}

/// Build the verdict from the two facts recorded at searcher-open time.
///
/// `counts` is `None` when no searcher has been built this session.
pub fn build_status(
    counts: Option<FulltextIndexCounts>,
    failures: &[(String, String)],
) -> FulltextStatus {
    let Some(counts) = counts else {
        return FulltextStatus {
            is_valid: false,
            message: "The search index has not been opened yet.".to_string(),
            state: FulltextState::NotOpenedYet,
            counts: FulltextIndexCounts::default(),
            failure_count: 0,
        };
    };

    if counts.total_opened() > 0 {
        return FulltextStatus {
            is_valid: true,
            message: format!("OK — {} open.", describe_counts(&counts)),
            state: FulltextState::Ready,
            counts,
            failure_count: failures.len(),
        };
    }

    // Zero open. The two reasons are completely different diagnoses, and
    // conflating them is what made the reporting user's failure invisible:
    // "Query returned 0 results" reads as an empty database, not as a volume
    // that cannot be read.
    if !counts.any_dir_present() {
        return FulltextStatus {
            is_valid: false,
            message: "Fulltext index files not found. Use Rebuild Search Index to create them."
                .to_string(),
            state: FulltextState::FilesNotFound,
            counts,
            failure_count: failures.len(),
        };
    }

    let message = if failures.is_empty() {
        // The directories exist but hold no per-language index subdirectories,
        // so nothing was even attempted. Not a fault of the volume.
        "Fulltext index files not found. Use Rebuild Search Index to create them.".to_string()
    } else {
        format!(
            "The search index could not be opened. {}",
            plain_reason_for_all(failures)
        )
    };

    FulltextStatus {
        is_valid: false,
        // An index directory that exists but is empty is the "not found" state,
        // whatever the directory tree looks like from outside.
        state: if failures.is_empty() {
            FulltextState::FilesNotFound
        } else {
            FulltextState::CouldNotOpen
        },
        message,
        counts,
        failure_count: failures.len(),
    }
}

/// The current verdict, read from the process-global searcher and failure list.
pub fn current_status() -> FulltextStatus {
    build_status(crate::fulltext_index_counts(), &crate::searcher_open_failures())
}

impl FulltextStatus {
    /// The project's convention for structured bridge results: a JSON string.
    ///
    /// Hand-rolled rather than derived, because `FulltextIndexCounts` lives in
    /// the search module and gains nothing else from a serde dependency. The
    /// shape is small and is consumed by `/health`, the Database Validation row
    /// and `FulltextResults.qml`.
    pub fn to_json(&self) -> String {
        let escaped = self.message.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
            concat!(
                r#"{{"is_valid":{},"state":"{}","message":"{}","failure_count":{},"#,
                r#""sutta":{{"opened":{},"dir_present":{}}},"#,
                r#""dict":{{"opened":{},"dir_present":{}}},"#,
                r#""library":{{"opened":{},"dir_present":{}}}}}"#,
            ),
            self.is_valid,
            self.state.as_str(),
            escaped,
            self.failure_count,
            self.counts.sutta.opened,
            self.counts.sutta.dir_present,
            self.counts.dict.opened,
            self.counts.dict.dir_present,
            self.counts.library.opened,
            self.counts.library.dir_present,
        )
    }
}

impl FulltextState {
    pub fn as_str(&self) -> &'static str {
        match self {
            FulltextState::NotOpenedYet => "not_opened_yet",
            FulltextState::FilesNotFound => "files_not_found",
            FulltextState::CouldNotOpen => "could_not_open",
            FulltextState::Ready => "ready",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::searcher::FulltextAreaStatus;

    fn counts(sutta: usize, dict: usize, library: usize, dirs: bool) -> FulltextIndexCounts {
        FulltextIndexCounts {
            sutta: FulltextAreaStatus { opened: sutta, dir_present: dirs },
            dict: FulltextAreaStatus { opened: dict, dir_present: dirs },
            library: FulltextAreaStatus { opened: library, dir_present: dirs },
        }
    }

    fn lock_failure(path: &str) -> (String, String) {
        (
            path.to_string(),
            "Failed to acquire Lockfile: .tantivy-meta.lock. \
             Some(Os { code: 38, kind: Uncategorized, message: \"Function not implemented\" })"
                .to_string(),
        )
    }

    #[test]
    fn no_searcher_is_not_measured_rather_than_healthy() {
        let status = build_status(None, &[]);
        assert!(!status.is_valid);
        assert_eq!(status.state, FulltextState::NotOpenedYet);
    }

    #[test]
    fn open_indexes_are_reported_valid_with_their_counts() {
        let status = build_status(Some(counts(3, 2, 1, true)), &[]);
        assert!(status.is_valid);
        assert_eq!(status.state, FulltextState::Ready);
        assert!(
            status.message.contains("3 sutta, 2 dictionary, 1 library indexes"),
            "got {}",
            status.message
        );
    }

    /// An area holding nothing is omitted rather than printed as `0 dictionary`
    /// — a user who downloaded only suttas has no fault to report.
    #[test]
    fn empty_areas_are_omitted_from_the_count_description() {
        let status = build_status(Some(counts(2, 0, 0, true)), &[]);
        assert!(status.is_valid);
        assert!(status.message.contains("2 sutta"), "got {}", status.message);
        assert!(!status.message.contains("dictionary"), "got {}", status.message);
    }

    #[test]
    fn one_index_is_singular() {
        let status = build_status(Some(counts(1, 0, 0, true)), &[]);
        assert!(status.message.contains("1 sutta index"), "got {}", status.message);
        assert!(!status.message.contains("indexes"), "got {}", status.message);
    }

    /// FR-30 / the `StartupDbReport` principle: an absent index directory is a
    /// different diagnosis from one that would not open, and saying the wrong
    /// one sends the user to the wrong remedy.
    #[test]
    fn absent_directories_are_not_reported_as_a_failure_to_open() {
        let status = build_status(Some(counts(0, 0, 0, false)), &[]);
        assert!(!status.is_valid);
        assert_eq!(status.state, FulltextState::FilesNotFound);
        assert!(status.message.contains("not found"), "got {}", status.message);
        assert!(
            !status.message.contains("could not be opened"),
            "an absent directory must not read as a lock failure: {}",
            status.message
        );
    }

    /// The reporting user's exact situation: the directories are there, six of
    /// them failed, and every failure has the same cause.
    #[test]
    fn present_directories_with_lock_failures_name_the_storage_location() {
        let failures: Vec<(String, String)> = ["suttas/en", "suttas/pli", "dict_words/pli"]
            .iter()
            .map(|p| lock_failure(p))
            .collect();

        let status = build_status(Some(counts(0, 0, 0, true)), &failures);
        assert!(!status.is_valid);
        assert_eq!(status.state, FulltextState::CouldNotOpen);
        assert_eq!(status.failure_count, 3);
        assert!(
            status.message.contains("does not support the file locking"),
            "got {}",
            status.message
        );
    }

    /// Directories present but empty: nothing was attempted, so this is "not
    /// found", not "would not open".
    #[test]
    fn present_but_empty_directories_are_files_not_found() {
        let status = build_status(Some(counts(0, 0, 0, true)), &[]);
        assert_eq!(status.state, FulltextState::FilesNotFound);
    }

    #[test]
    fn differing_failure_causes_fall_back_to_the_generic_sentence() {
        let failures = vec![
            lock_failure("suttas/en"),
            ("dict_words/pli".to_string(), "Permission denied (os error 13)".to_string()),
        ];
        let status = build_status(Some(counts(0, 0, 0, true)), &failures);
        assert!(
            status.message.contains("could not be read"),
            "got {}",
            status.message
        );
    }

    /// The wording rules of the module docs, enforced across every state this
    /// module can produce. A regression here ships jargon to users.
    #[test]
    fn no_jargon_in_user_facing_strings() {
        let banned = [
            "flock", "ENOSYS", "Tantivy", "tantivy", "META_LOCK", "mmap", "FUSE", "SD card",
            "errno", "Lockfile",
        ];

        let mut messages: Vec<String> = Vec::new();
        messages.push(build_status(None, &[]).message);
        messages.push(build_status(Some(counts(0, 0, 0, false)), &[]).message);
        messages.push(build_status(Some(counts(0, 0, 0, true)), &[]).message);
        messages.push(build_status(Some(counts(3, 2, 1, true)), &[]).message);
        messages.push(
            build_status(
                Some(counts(0, 0, 0, true)),
                &[lock_failure("suttas/en")],
            )
            .message,
        );
        messages.push(
            build_status(
                Some(counts(0, 0, 0, true)),
                &[
                    lock_failure("suttas/en"),
                    ("d".to_string(), "Permission denied".to_string()),
                ],
            )
            .message,
        );

        // Every plain reason the classifier can return, reached through the
        // public entry point rather than asserted on in isolation.
        for raw in [
            "Failed to acquire Lockfile",
            "Permission denied (os error 13)",
            "No such file or directory",
            "No space left on device",
            "something entirely unexpected",
        ] {
            messages.push(
                build_status(
                    Some(counts(0, 0, 0, true)),
                    &[("suttas/en".to_string(), raw.to_string())],
                )
                .message,
            );
        }

        for message in &messages {
            for word in banned {
                assert!(
                    !message.contains(word),
                    "user-facing string contains {word:?}: {message}"
                );
            }
        }
    }

    #[test]
    fn json_shape_carries_the_state_and_the_counts() {
        let status = build_status(Some(counts(3, 2, 1, true)), &[]);
        let json = status.to_json();
        assert!(json.contains(r#""is_valid":true"#), "got {json}");
        assert!(json.contains(r#""state":"ready""#), "got {json}");
        assert!(json.contains(r#""sutta":{"opened":3,"dir_present":true}"#), "got {json}");
        assert!(json.contains(r#""library":{"opened":1,"dir_present":true}"#), "got {json}");
    }

    /// The message is interpolated into JSON by hand, so a quote in it must not
    /// produce a document the QML side cannot parse.
    #[test]
    fn json_escapes_quotes_in_the_message() {
        let mut status = build_status(Some(counts(1, 0, 0, true)), &[]);
        status.message = r#"a "quoted" word and a \ backslash"#.to_string();
        let json = status.to_json();
        assert!(json.contains(r#"\"quoted\""#), "got {json}");
        assert!(json.contains(r#"\\ backslash"#), "got {json}");
    }
}
