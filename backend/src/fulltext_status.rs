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
    /// Whether everything that should have opened, opened.
    ///
    /// **Not the same question as `state`,** and the difference is deliberate:
    /// `Ready` means fulltext search can return results at all, while this is
    /// false the moment *any* index directory failed. A partly-open index is
    /// both — search works, and something is wrong — and Database Validation
    /// must not print "All checks passed" over it.
    pub is_valid: bool,
    /// One or two sentences, in plain language. Safe to show anywhere.
    pub message: String,
    /// The distinguishing state, for callers that want to branch rather than
    /// print. This is the `StartupDbReport` principle: "the files are not there"
    /// and "the files are there and would not open" are different diagnoses and
    /// must never be conflated.
    ///
    /// `Ready` says nothing about whether *every* index opened — read
    /// `failure_count`, or [`area_state`] for one area, for that.
    pub state: FulltextState,
    pub counts: FulltextIndexCounts,
    /// How many index directories failed to open. Zero in every state except
    /// [`FulltextState::CouldNotOpen`] (and even there it can be zero if the
    /// directories held no per-language subdirectories at all).
    pub failure_count: usize,
    /// The one plain-language cause behind those failures, or `""` when there
    /// were none.
    ///
    /// Kept as a field so a **per-area** message can be composed from the same
    /// sentence (see [`area_message`]) without the caller holding the raw
    /// failure list — and so no caller is ever tempted to compose its own
    /// wording out of the counts. Every user-facing string this feature can
    /// emit is written in this file, which is what `no_jargon_in_user_facing_strings`
    /// is able to check.
    pub reason: &'static str,
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
            reason: "",
        };
    };

    if counts.total_opened() > 0 {
        // Something opened, so fulltext search works — but "works" and "is
        // fine" are different claims, and reporting the second when only the
        // first is true is the whole failure mode this module exists to remove.
        // A partly-open index is a green "All checks passed" in Database
        // Validation next to searches that silently return nothing.
        let reason = reason_or_empty(failures);
        let message = if failures.is_empty() {
            format!("OK — {} open.", describe_counts(&counts))
        } else {
            format!(
                "{} open. Some search index files could not be opened. {}",
                describe_counts(&counts),
                reason
            )
        };

        return FulltextStatus {
            is_valid: failures.is_empty(),
            message,
            state: FulltextState::Ready,
            counts,
            failure_count: failures.len(),
            reason,
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
            reason: "",
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
        reason: reason_or_empty(failures),
    }
}

/// [`plain_reason_for_all`], but `""` for an empty failure list rather than the
/// generic sentence — the difference between "nothing failed" and "something
/// failed and we cannot say what".
fn reason_or_empty(failures: &[(String, String)]) -> &'static str {
    if failures.is_empty() {
        ""
    } else {
        plain_reason_for_all(failures)
    }
}

/// The current verdict, read from the process-global searcher and failure list.
pub fn current_status() -> FulltextStatus {
    build_status(crate::fulltext_index_counts(), &crate::searcher_open_failures())
}

/// The verdict for **one** search area.
///
/// The whole-app `state` above is `Ready` as soon as *anything* opened, which is
/// the right answer for "does fulltext search work at all" and the wrong one for
/// "why did the search I just ran come back empty". A user whose sutta indexes
/// open and whose dictionary indexes all fail was told "No results found." —
/// the same silent empty result the whole feature exists to remove, just
/// narrowed to one area.
///
/// The rules are the whole-app ones applied to one area's counts:
/// something open → `Ready`; nothing open and no directory → `FilesNotFound`;
/// nothing open, directory present, and at least one index in **this area**
/// failed → `CouldNotOpen`. An area with a directory holding no per-language
/// subdirectory at all is `FilesNotFound`, not a fault.
pub fn area_state(area: &crate::search::searcher::FulltextAreaStatus) -> FulltextState {
    if area.opened > 0 {
        FulltextState::Ready
    } else if area.dir_present && area.failed > 0 {
        FulltextState::CouldNotOpen
    } else {
        FulltextState::FilesNotFound
    }
}

/// What to say about one area, or `""` when there is nothing to say.
///
/// **An empty string is the instruction to stay silent**, and the caller needs
/// no other rule — every decision about when this feature speaks is made here.
/// There are three cases and only two of them produce a sentence:
///
/// - nothing opened and something failed → the index could not be opened;
/// - **something opened and something else failed** → the results are
///   incomplete. This one is easy to miss and was: an area is not all-or-nothing
///   (`suttas/en` opens while `suttas/pli` fails), and a user searching Pāli
///   would otherwise be told "No results found." while English worked fine;
/// - nothing failed → silent, whether the area is open or simply not downloaded.
///   A user who never downloaded a language has no fault to report, and telling
///   them their index is broken would send them to Rebuild Search Index for
///   nothing (FR-26).
pub fn area_message(
    area: &crate::search::searcher::FulltextAreaStatus,
    reason: &str,
) -> String {
    if area.failed == 0 {
        return String::new();
    }

    let headline = if area.opened == 0 {
        "The search index could not be opened."
    } else {
        "Some of the search index could not be opened, so these results may be incomplete."
    };

    if reason.is_empty() {
        return headline.to_string();
    }
    format!("{headline} {reason}")
}

/// Escape one string for a JSON double-quoted value.
///
/// Applied to **every** string this module writes into JSON, not only the
/// whole-app message. All of them are written in this file today and none
/// contains a quote or a backslash, so this changes no output — but "it happens
/// to be safe" is a property of the current wording, not of the code, and the
/// failure it guards against is silent: `JSON.parse` throws in QML, the `catch`
/// clears the message, and the search-index problem this feature exists to
/// report goes back to being invisible.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

impl FulltextStatus {
    /// The project's convention for structured bridge results: a JSON string.
    ///
    /// Hand-rolled rather than derived, because `FulltextIndexCounts` lives in
    /// the search module and gains nothing else from a serde dependency. The
    /// shape is small and is consumed by `/health`, the Database Validation row
    /// and `FulltextResults.qml`.
    pub fn to_json(&self) -> String {
        let escaped = json_escape(&self.message);
        format!(
            concat!(
                r#"{{"is_valid":{},"state":"{}","message":"{}","failure_count":{},"#,
                r#""sutta":{{"opened":{},"dir_present":{},"failed":{},"state":"{}","message":"{}"}},"#,
                r#""dict":{{"opened":{},"dir_present":{},"failed":{},"state":"{}","message":"{}"}},"#,
                r#""library":{{"opened":{},"dir_present":{},"failed":{},"state":"{}","message":"{}"}}}}"#,
            ),
            self.is_valid,
            self.state.as_str(),
            escaped,
            self.failure_count,
            self.counts.sutta.opened,
            self.counts.sutta.dir_present,
            self.counts.sutta.failed,
            area_state(&self.counts.sutta).as_str(),
            json_escape(&area_message(&self.counts.sutta, self.reason)),
            self.counts.dict.opened,
            self.counts.dict.dir_present,
            self.counts.dict.failed,
            area_state(&self.counts.dict).as_str(),
            json_escape(&area_message(&self.counts.dict, self.reason)),
            self.counts.library.opened,
            self.counts.library.dir_present,
            self.counts.library.failed,
            area_state(&self.counts.library).as_str(),
            json_escape(&area_message(&self.counts.library, self.reason)),
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

    /// An area that opened nothing is given one failure when its directory is
    /// present, which is what the searcher records for a directory it tried and
    /// could not open. `area_state` needs the two apart.
    fn area(opened: usize, dir_present: bool) -> FulltextAreaStatus {
        FulltextAreaStatus {
            opened,
            dir_present,
            failed: if opened == 0 && dir_present { 1 } else { 0 },
        }
    }

    fn counts(sutta: usize, dict: usize, library: usize, dirs: bool) -> FulltextIndexCounts {
        FulltextIndexCounts {
            sutta: area(sutta, dirs),
            dict: area(dict, dirs),
            library: area(library, dirs),
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
            let status = build_status(
                Some(counts(0, 0, 0, true)),
                &[("suttas/en".to_string(), raw.to_string())],
            );
            // The per-area sentence is shown in the search results panel and is
            // just as user-facing as the whole-app one.
            messages.push(area_message(&status.counts.sutta, status.reason));
            messages.push(status.message);
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
        assert!(
            json.contains(r#""sutta":{"opened":3,"dir_present":true,"failed":0,"state":"ready","message":""}"#),
            "got {json}"
        );
        assert!(
            json.contains(r#""library":{"opened":1,"dir_present":true,"failed":0,"state":"ready","message":""}"#),
            "got {json}"
        );
    }

    /// Every string this module writes into JSON goes through [`json_escape`],
    /// not only the whole-app `message`.
    ///
    /// The consumer is `JSON.parse` in QML inside a `try`, so a stray quote does
    /// not throw an error anyone sees — it lands in the `catch`, the message is
    /// cleared, and the broken search index goes back to being invisible. That
    /// is the exact failure this whole module exists to remove, so the escaping
    /// is pinned rather than left resting on today's wording.
    #[test]
    fn every_json_string_is_escaped() {
        assert_eq!(json_escape(r#"a "quoted" \ word"#), r#"a \"quoted\" \\ word"#);
        assert_eq!(json_escape("plain"), "plain");

        // And the real document parses. Serde is already a dependency of this
        // crate, so the round-trip is free and catches a malformed field this
        // file's own `concat!` template could introduce.
        let status = build_status(
            Some(counts(0, 0, 0, true)),
            &[lock_failure("/vol/index/suttas/pli")],
        );
        let parsed: serde_json::Value = serde_json::from_str(&status.to_json())
            .expect("to_json must produce parseable JSON");
        assert_eq!(parsed["state"], "could_not_open");
        assert_eq!(parsed["sutta"]["state"], "could_not_open");
        assert!(
            parsed["sutta"]["message"]
                .as_str()
                .is_some_and(|m| !m.is_empty()),
            "the failed area must carry its own sentence: {parsed}"
        );
    }

    /// The case the per-area block exists for: one area open, another failed.
    /// The whole-app verdict is `ready` — fulltext search does work — and a
    /// search of the failed area must still say so instead of "No results".
    #[test]
    fn a_partly_open_index_reports_per_area() {
        let counts = FulltextIndexCounts {
            sutta: area(3, true),
            dict: area(0, true),
            library: area(0, false),
        };
        let status = build_status(Some(counts), &[lock_failure("/vol/index/dict_words/pli")]);

        // Search works, and something is wrong. Both are true, and the two
        // fields say so separately — `state` is what the search UI branches on,
        // `is_valid` is what stops Database Validation printing "All checks
        // passed" over a broken dictionary index.
        assert_eq!(status.state, FulltextState::Ready);
        assert!(!status.is_valid, "a failed index is not a clean bill of health");
        assert!(
            status.message.contains("could not be opened"),
            "the whole-app message must not read as OK: {}",
            status.message
        );

        assert_eq!(area_state(&status.counts.sutta), FulltextState::Ready);
        assert_eq!(area_state(&status.counts.dict), FulltextState::CouldNotOpen);
        // Never downloaded, never attempted: not a fault, and silent.
        assert_eq!(area_state(&status.counts.library), FulltextState::FilesNotFound);

        assert!(area_message(&status.counts.sutta, status.reason).is_empty());
        assert!(area_message(&status.counts.library, status.reason).is_empty());

        let dict_message = area_message(&status.counts.dict, status.reason);
        assert!(
            dict_message.starts_with("The search index could not be opened."),
            "got {dict_message}"
        );
        assert!(
            dict_message.contains("does not support the file locking"),
            "the area message must carry the cause, not just the fact: {dict_message}"
        );
    }

    /// An area is not all-or-nothing either: `suttas/en` can open while
    /// `suttas/pli` fails. The area's own state is `Ready` — English searches
    /// work — but a Pāli search comes back empty, and saying nothing there is
    /// the same silent-empty defect one level down.
    #[test]
    fn an_area_with_some_indexes_open_and_some_failed_still_speaks() {
        let counts = FulltextIndexCounts {
            sutta: FulltextAreaStatus { opened: 1, dir_present: true, failed: 1 },
            dict: area(2, true),
            library: area(0, false),
        };
        let status = build_status(Some(counts), &[lock_failure("/vol/index/suttas/pli")]);

        assert_eq!(area_state(&status.counts.sutta), FulltextState::Ready);

        let message = area_message(&status.counts.sutta, status.reason);
        assert!(
            message.contains("may be incomplete"),
            "a partly-open area must say results are incomplete, not that nothing opened: {message}"
        );
        assert!(
            message.contains("does not support the file locking"),
            "and it must still carry the cause: {message}"
        );

        // The fully-open area stays silent.
        assert!(area_message(&status.counts.dict, status.reason).is_empty());
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
